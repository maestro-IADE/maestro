//! One agent per worktree, as the rest of Maestro sees it (`Agent`): a facade over its runtime (runtime.rs), the
//! PTY runtime (pty.rs: the real CLI in a terminal) or the ACP runtime (acp_runtime.rs: the chat through an
//! adapter). Either way the chat is drawn from the CLI's transcript as common events (events.rs). The facade
//! swaps one runtime for the other (the chat ↔ CLI switch, the ACP → PTY fallback) under a single-writer lock: the
//! old process is gone before the new one starts on the same conversation. Also what every runtime shares: the
//! agent's saved meta, the busy/idle state, the brief Maestro gives it, its environment.

use crate::acp_runtime::{AcpCtx, AcpRuntime, Ask};
use crate::acp_setup::{self, AcpPlan, FallbackReason, Want};
use crate::approval::Approvals;
use crate::catalog;
use crate::events::{self, ResetReason, RuntimeEvent};
use crate::pty::PtyRuntime;
use crate::runtime::{self, LaunchCtx, Prompt, PtySnapshot, Readiness, Runtime, Sent, ToolMatch};
use crate::stream::Emitter;
use crate::transcript::LiveTail;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter as _, Manager};

pub use crate::runtime::LaunchFailure;

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentState {
    Running,
    Idle,
    Terminated,
}

/// Saved per worktree so the same Claude session is resumed after a restart.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentMeta {
    pub adapter_id: String,
    pub permission_mode: String,
    pub cli_session_id: Option<String>,
    /// what the user calls this worktree (the branch is only the fallback)
    pub name: Option<String>,
    pub description: Option<String>,
    /// passed as --model on every start; None = the CLI's own default
    pub model: Option<String>,
    /// "maestro" = the project's coordinator, running on the project folder itself (main branch)
    #[serde(default)]
    pub role: Option<String>,
    /// child worktree paths: this agent's workers, one level down the project's hierarchy (canvas lines)
    #[serde(default)]
    pub workers: Vec<String>,
    /// Markdown files this agent should refer to (reference lines from Markdown blocks)
    #[serde(default)]
    pub refs: Vec<String>,
    /// the worktree checked out a branch that existed before: discarding it keeps the branch
    #[serde(default)]
    pub keep_branch: bool,
    /// Maestro only: the branch workers start from and get merged back into (by the app, after a merge-request is
    /// approved; None = the repo's default)
    #[serde(default)]
    pub base_branch: Option<String>,
    /// cached one-line summary of the latest turn, and which conversation text it was made from
    pub summary: Option<String>,
    pub summary_key: Option<String>,
    /// "cli" = the user last looked at the CLI view: the next start opens the terminal (no chat runtime)
    #[serde(default)]
    pub last_view: Option<String>,
    /// the folder was trusted in Maestro for the ACP chat (the TUI's own dialog never shows there)
    #[serde(default)]
    pub acp_trusted: bool,
}

#[derive(Clone)]
pub struct Files {
    pub meta: PathBuf,
}

impl Files {
    /// <project_dir>/agents/<worktree folder>.json
    pub fn new(project_dir: &Path, worktree_path: &str) -> Files {
        let name = Path::new(worktree_path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        Files { meta: project_dir.join("agents").join(format!("{name}.json")) }
    }

    pub fn read_meta(&self) -> Option<AgentMeta> {
        serde_json::from_str(&std::fs::read_to_string(&self.meta).ok()?).ok()
    }

    pub fn write_meta(&self, meta: &AgentMeta) -> Result<(), String> {
        std::fs::create_dir_all(self.meta.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&self.meta, serde_json::to_string_pretty(meta).unwrap()).map_err(|e| e.to_string())
    }

    /// Worker notes kept until this agent can take them (`control::worker_done`). Its own file, not a meta
    /// field: meta is rewritten whole by others (a summary call holds it for seconds) and would drop one.
    pub fn notes(&self) -> PathBuf {
        self.meta.with_extension("notes.json")
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AgentStateEvent {
    worktree_path: String,
    state: AgentState,
}

fn emit_state(app: &AppHandle, path: &str, cur: &Mutex<AgentState>, next: AgentState) {
    let mut cur = cur.lock().unwrap();
    if *cur == AgentState::Terminated || *cur == next {
        return;
    }
    *cur = next.clone();
    let _ = app.emit("agent://state", AgentStateEvent { worktree_path: path.into(), state: next });
}

/// When Maestro itself runs inside a Claude Code session (e.g. `tauri dev` started from one), that host's
/// identity, IPC socket and NO_COLOR would leak into every agent. Returns those vars; user config like
/// CLAUDE_CODE_GIT_BASH_PATH or provider switches stay.
pub fn host_session_env(vars: impl Iterator<Item = String>) -> Vec<String> {
    const KEEP: [&str; 3] = ["CLAUDE_CODE_GIT_BASH_PATH", "CLAUDE_CODE_USE_BEDROCK", "CLAUDE_CODE_USE_VERTEX"];
    let vars: Vec<String> = vars.collect();
    if !vars.iter().any(|k| k == "CLAUDECODE") {
        return vec![];
    }
    vars.into_iter()
        .filter(|k| {
            (k.starts_with("CLAUDE_CODE_") || k.starts_with("CLAUDE_AGENT_SDK_") || ["CLAUDECODE", "CLAUDE_PID", "NO_COLOR"].contains(&k.as_str()))
                && !KEEP.contains(&k.as_str())
        })
        .collect()
}

/// States the answer itself ("X는 Y다"), not a description of the conversation ("~에 대해 설명했다").
/// Part of the summary cache key, so editing it refreshes every card.
pub const SUMMARY_PROMPT: &str = "너는 작업 노트 정리기다. 입력의 '결과'에서 핵심만 뽑아 한국어 평서문 1~2문장, 합쳐서 90자 이내로 쓴다.
규칙:
- 문장의 주어는 내용(코드, 파일, 개념, 기능)이다. 사람이나 대화가 주어가 되면 안 된다.
- 다음 단어는 절대 쓰지 마라: 사용자, 에이전트, 알려줬다, 설명했다, 요청했다, 전달했다, 제공했다, 대화.
- 정보라면 그 정보 자체를 쓴다. 좋은 예: 'HTML 기본 틀은 doctype 선언, lang 속성의 html, charset·viewport·title을 담은 head, body로 이뤄진다.'
- 작업이라면 무엇이 어떻게 바뀌었고 결과(테스트 통과, 남은 문제)가 어떤지 쓴다. 좋은 예: 'auth.ts의 토큰 만료 검사를 고쳐 로그인 버그가 해결됐다. 새 테스트 3개가 모두 통과한다.'
- 마크다운, 따옴표, 머리말 없이 문장만 출력한다.";

/// Summarize with the assistant model chosen in settings (an agent whose catalog entry has `assistant`, plus a
/// model; "" = that entry's default, Haiku for Claude).
pub fn summarize(conversation: &str, agent: &str, model: &str) -> Result<String, String> {
    let Some((def, default)) = catalog::get(agent).and_then(|d| Some((d, d.features.assistant?))) else { return Err("보조 모델이 꺼져 있어요".into()) };
    let model = if model.is_empty() { default } else { model };
    let exe = crate::cli::command(def)?;
    runtime::pty_agent(agent).summarize(exe, conversation, model)
}

/// Where the `maestro` command lives for agents: `~/.maestro/bin`.
pub fn cli_dir(home: &Path) -> PathBuf {
    home.join(".maestro").join("bin")
}

/// Puts the CLI built next to this app (`maestro-cli`) into `~/.maestro/bin` as `maestro`, once per app start.
/// A copy rather than a .cmd shim so Git Bash (Claude's Bash tool on Windows) finds it too.
pub fn install_cli(home: &Path) {
    let Ok(exe) = std::env::current_exe() else { return };
    let ext = if cfg!(windows) { ".exe" } else { "" };
    let src = exe.with_file_name(format!("maestro-cli{ext}"));
    let dir = cli_dir(home);
    // fails harmlessly while an agent is running the old copy; it's replaced on a later start
    let _ = std::fs::create_dir_all(&dir).and_then(|_| std::fs::copy(&src, dir.join(format!("maestro{ext}"))));
}

/// What lets the `maestro` CLI inside an agent's terminal reach this app and say who's calling (with a token only
/// that worktree's caller header matches, `Approvals::agent_token`).
pub(crate) fn maestro_env(home: &Path, worktree_path: &str, approvals: &Approvals) -> Vec<(String, String)> {
    let sep = if cfg!(windows) { ";" } else { ":" };
    let path = format!("{}{sep}{}", cli_dir(home).display(), std::env::var("PATH").unwrap_or_default());
    vec![
        ("MAESTRO_URL".into(), approvals.addr()),
        ("MAESTRO_TOKEN".into(), approvals.agent_token(worktree_path)),
        ("MAESTRO_WORKTREE".into(), worktree_path.into()),
        ("PATH".into(), path),
    ]
}

/// Tells Claude the `maestro` command exists (appended to its system prompt).
const MAESTRO_HINT: &str = "You are running inside Maestro, a desktop app that runs several coding agents, each in its own git worktree of this project. \
A `maestro` command is on your PATH: `maestro --help` lists it. With it you can see the other worktrees and their agents (`maestro worktree list`), \
start a new worktree with its own agent for a side task (`maestro worktree create --name … --prompt …`), send another agent instructions (`maestro send --to … --text …`), \
and read its status and latest answer (`maestro read --to …`). Use it when the user asks you to delegate, parallelize, or coordinate with other agents. \
`maestro whoami` also shows your parent, your workers, and the reference documents the user linked to you on the canvas: read those documents before starting work. When the user says \"the selected ones\", \"these\", or the like, run `maestro selection` first: it lists the blocks (agents, documents, prompt blocks, and drawn structure: blocks with their notes, areas with what they hold, arrows) they selected on your canvas. \
Git in Maestro: commit only on your own worktree's branch. Never push (`git push` in any form, `gh pr create/merge`), never merge into the base branch, and never change the project folder's history. \
Finished branches reach the base branch only through Maestro: the coordinator asks with `maestro merge-request`, the user approves, and the app merges. \
When asked to bring in the base branch or resolve conflicts, merge or rebase the base branch into your own branch, inside your own worktree. \
Don't run destructive git commands (`git reset --hard`, `git clean -f`, `git branch -D`, force pushes, discarding the whole tree, removing worktrees); Maestro blocks them for Claude Code and they need the user.";

/// Extra brief for the project's Maestro agent (role "maestro"): what it can do, not how it should coordinate.
/// The how (delegate or not, how to split, which personas) is the playbooks' (`~/.maestro/playbooks`), so the
/// user can change it by editing them.
const COORDINATOR_HINT: &str = "You are this project's Maestro: the coordinator. You run on the project folder itself (its main branch), not in a worktree. \
Your workers are the worktrees linked to you (`maestro whoami` lists them; `maestro worktree list` marks them). \
`maestro worktree create --name … --prompt …` makes a new linked worker, `maestro send --to …` instructs an existing one, \
`maestro ref add --to … --path …` gives a worker a persona or document, `maestro draw --file …` draws a structure (areas, blocks, arrows) on your canvas for the user to see, `maestro read --to …` reads a worker's conversation \
(when a worker finishes a turn, Maestro also pastes a `[maestro] 워커 … 완료` note with its answer into your prompt). \
You never merge or push yourself. To get a worker's branch into the base branch: make sure its work is committed, have the worker merge (or rebase onto) the latest base branch in its own worktree and resolve conflicts there, \
verify it (build, tests), then run `maestro merge-request --to <worker> --verified \"what you checked and the result\"`. \
The user sees an approval card with the commits and the diff; on approval Maestro merges exactly the commits you asked about with `git merge --no-ff` into the project folder, which must have no uncommitted changes then. \
The outcome comes back as a `[maestro] 병합 …` note: merged, rejected, a conflict (aborted, folder unchanged), or the base branch moved since your request (update and ask again). \
Wherever a playbook says to merge (합친다, 합치기), it means this: one request at a time, in the playbook's order; after each merge, build and test in the project folder.";

/// Added to the coordinator's brief when the project's merge mode is "pr" (R6).
const PR_MODE_HINT: &str = "이 프로젝트는 PR 모드예요. 위의 --no-ff 병합 설명 대신 아래 흐름을 따라요. 합칠 때는 여전히 maestro merge-request를 써요. 사용자가 승인하면 앱이 (사용자 확인 시 커밋하고) 브랜치를 푸시하고 PR을 만들어요 (프로젝트 폴더에서 합치지 않아요). 병합은 사용자가 앱에서 merge/squash로 해요. [maestro] PR 생성 / CI 실패 / PR 병합 메모가 와요. 병합 메모에서 프로젝트 폴더를 빨리감기 했다고 할 때만 거기서 빌드·테스트하세요. '그대로예요'면 폴더는 아직 이전 기준이니, 사용자가 '기준 브랜치 받기'를 누른 뒤 하세요. CI가 실패하면 워커에게 고치게 하고 다시 merge-request하세요 (같은 PR이 갱신돼요). PR이 열린(푸시된) 워커 브랜치는 rebase·amend·reset으로 다시 쓰지 말고, 기준 브랜치는 merge로 받게 하세요. 앱은 강제 푸시를 하지 않아서 다음 승인이 거절돼요. squash로 합쳐진 워커는 원격 기준을 합치거나 새 워커로 시작하게 하세요. push, gh pr create/merge/edit/review/comment, 쓰기 gh api는 절대 직접 하지 마세요 (gh는 pr view/checks, run view 같은 읽기만 돼요).";

/// Added to a worker's brief when its project's merge mode is "pr" (R6): the app pushes the branch and never
/// force-pushes, so the base comes in by merge and a pushed branch is never rewritten.
const WORKER_PR_HINT: &str = "이 프로젝트는 PR 모드예요. 앱이 승인 뒤 이 워크트리의 브랜치를 푸시해 PR을 만들어요. 기준 브랜치를 받아 오거나 충돌을 풀 때는 위의 'merge or rebase' 대신 자기 워크트리에서 git merge <기준>만 쓰세요. 푸시된 브랜치(PR이 열린 브랜치)는 rebase·amend·reset으로 기록을 바꾸지 마세요: 앱은 강제 푸시를 하지 않아서 다음 승인이 거절돼요.";

/// The worker's project (the main checkout its `.git` belongs to, as registered in `~/.maestro/repos.json`)
/// merges through PRs.
fn worker_pr_mode(home: &Path, worktree: &str) -> bool {
    let Some(main) = crate::worktree::main_checkout(worktree) else { return false };
    let same = |r: &str| r.replace('\\', "/").trim_end_matches('/').eq_ignore_ascii_case(main.trim_end_matches('/'));
    let repos: Vec<String> = std::fs::read_to_string(home.join(".maestro").join("repos.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    // the project folder itself isn't a worker's
    repos.iter().find(|r| same(r)).is_some_and(|r| !same(worktree) && crate::project::merge_mode(home, r).is_pr())
}

/// Everything Maestro tells an agent up front: the `maestro` CLI, the coordinator role, its persona and
/// reference documents. Goes in a file: Claude's `--append-system-prompt-file` (a multi-line argument would be
/// cut at the first newline by `cmd.exe /c`), Antigravity's rules folder (`AGENTS.md`). `worktree` is the agent's
/// folder (for the coordinator, the project folder, whose merge mode it reads).
pub fn system_brief(home: &Path, meta: &AgentMeta, worktree: &str) -> String {
    let mut out = String::from(MAESTRO_HINT);
    if meta.role.as_deref() != Some("maestro") && worker_pr_mode(home, worktree) {
        out = format!("{out}\n\n{WORKER_PR_HINT}");
    }
    if meta.role.as_deref() == Some("maestro") {
        out = format!("{out}\n\n{COORDINATOR_HINT}");
        if crate::project::merge_mode(home, worktree).is_pr() {
            out = format!("{out}\n\n{PR_MODE_HINT}");
        }
        if let Some(base) = &meta.base_branch {
            out = format!("{out}\n\nBase branch: `{base}`. New workers branch off it (`maestro worktree create` does this by default); finished work reaches `{base}` only through `maestro merge-request`. Keep the project folder clean before asking.");
        }
        // the coordination procedures kept in Maestro (the sidebar's 플레이북 tab)
        let books = crate::library::scan(&crate::library::playbooks_dir(home));
        let personas = crate::library::personas_dir(home).to_string_lossy().replace('\\', "/");
        if books.is_empty() {
            out = format!("{out}\n\nThere are no playbooks: decide yourself how to coordinate. Personas for workers are in `{personas}`.");
        } else {
            let list: Vec<String> = books.iter().map(|b| format!("- {} ({})", b.title, b.path)).collect();
            out = format!(
                "{out}\n\n## Playbooks\n\nHow you coordinate (delegate or do it yourself, how to split the work, which personas the workers get) is set by these playbooks. \
Before acting on a request, read the one that fits and follow it; if none fits, follow 기본 지휘 (default.md) when it's there. \
A playbook's 구성 table names the persona files to attach to the workers you create (`~/.maestro/personas` is `{personas}`).\n\n{}",
                list.join("\n")
            );
        }
    }
    let (personas, docs): (Vec<&String>, Vec<&String>) = meta.refs.iter().partition(|r| crate::library::is_persona(home, r));
    for p in personas {
        if let Ok(text) = std::fs::read_to_string(p) {
            out = format!("{out}\n\n## Your persona (assigned in Maestro; follow it)\n\n{}", text.trim());
        }
    }
    if !docs.is_empty() {
        let list: Vec<String> = docs.iter().map(|d| format!("- {} ({d})", crate::library::title_of(Path::new(d.as_str())))).collect();
        out = format!("{out}\n\n## Reference documents linked to you (read them before starting work)\n\n{}", list.join("\n"));
    }
    out
}

/// The command a new worktree with this agent would run, for the settings tile: the real argv without the
/// per-worktree parts (session, hook settings, brief) and with the bare executable name.
pub fn launch_preview(adapter: &str, permission_mode: &str, model: &str) -> String {
    let Some(def) = catalog::get(adapter) else { return adapter.into() };
    let model = Some(model).filter(|m| !m.is_empty());
    let a = if def.launchable() { runtime::pty_agent(adapter).preview(def, permission_mode, model) } else { vec![def.bin.into()] };
    a.join(" ")
}

/// One process in `processes()`.
#[derive(Debug, Clone, PartialEq)]
pub struct Proc {
    pub pid: u32,
    /// the parent's pid when this one started: Windows never updates it, and reuses pids soon
    pub ppid: u32,
    /// the executable's name
    pub name: String,
    /// when it started, in ms since the Unix epoch (0: unknown)
    pub started: u64,
}

/// Every process; None when the list can't be read.
pub fn processes() -> Option<Vec<Proc>> {
    #[cfg(windows)]
    let out = {
        use std::os::windows::process::CommandExt;
        // the name last: it may have spaces. The System Idle Process has no creation date.
        let script = "Get-CimInstance Win32_Process | ForEach-Object { $t = if ($_.CreationDate) { ([DateTimeOffset]$_.CreationDate).ToUnixTimeMilliseconds() } else { 0 }; '{0} {1} {2} {3}' -f $_.ProcessId, $_.ParentProcessId, $t, $_.Name }";
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .output()
            .ok()?
    };
    // etime ([[dd-]hh:]mm:ss) rather than etimes: macOS's ps has only the former
    #[cfg(not(windows))]
    let out = std::process::Command::new("ps").args(["-A", "-o", "pid=,ppid=,etime=,comm="]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    #[cfg(windows)]
    let started = |t: &str| t.parse::<u64>().ok();
    #[cfg(not(windows))]
    let started = |t: &str| Some(now_ms().saturating_sub(elapsed_secs(t)? * 1000));
    let list: Vec<_> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let (pid, ppid, t) = (it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.next()?);
            Some(Proc { pid, ppid, started: started(t)?, name: it.collect::<Vec<_>>().join(" ") })
        })
        .collect();
    (!list.is_empty()).then_some(list)
}

/// Now, in `Proc::started`'s units: a runtime notes it right before it starts its process.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// `ps`'s etime, in seconds.
#[cfg_attr(windows, allow(dead_code))]
fn elapsed_secs(t: &str) -> Option<u64> {
    let (days, rest) = match t.split_once('-') {
        Some((d, r)) => (d.parse::<u64>().ok()?, r),
        None => (0, t),
    };
    let mut secs = 0;
    for part in rest.split(':') {
        secs = secs * 60 + part.parse::<u64>().ok()?;
    }
    Some(days * 86_400 + secs)
}

/// Started no earlier than `since` (ms; 0: anything counts), give or take the list's precision (`ps`: a second).
/// A process from before is someone else's, whatever its parent pid says.
pub fn born_since(p: &Proc, since: u64) -> bool {
    p.started == 0 || p.started + 1000 >= since
}

/// `pid`, or a process right below it (its parent id is still `pid` after the parent's exit), is listed. Only
/// processes started since `since` (the root's start, `Runtime::started`) count: the parent id of one that's
/// older belonged to another process before the pid was reused.
pub fn tree_listed(procs: &[Proc], pid: u32, since: u64) -> bool {
    procs.iter().any(|p| (p.pid == pid || p.ppid == pid) && born_since(p, since))
}

/// Shells the agents' tools run in. MCP servers (node, python, cmd wrappers) also live under the CLI, so only these count.
const SHELLS: [&str; 6] = ["bash.exe", "sh.exe", "powershell.exe", "pwsh.exe", "bash", "sh"];

/// Any shell among the descendants of `root` (not `root` itself), started since `since` (`tree_listed`).
fn shell_below(procs: &[Proc], root: u32, since: u64) -> bool {
    let mut stack = vec![root];
    let mut seen = std::collections::HashSet::from([root]);
    while let Some(p) = stack.pop() {
        for c @ Proc { pid, ppid, name, .. } in procs {
            if *ppid == p && born_since(c, since) && seen.insert(*pid) {
                let exe = name.rsplit(['/', '\\']).next().unwrap_or(name).to_ascii_lowercase();
                if SHELLS.contains(&exe.as_str()) {
                    return true;
                }
                stack.push(*pid);
            }
        }
    }
    false
}

/// A worktree's agent (or a settings setup terminal), whatever runtime drives it. Keeps the busy/idle state, one
/// prompt at a time, and the notes it holds for later; the runtime does the rest.
pub struct Agent {
    pub worktree_path: String,
    pub state: Arc<Mutex<AgentState>>,
    /// swapped only while `switching` is held: the chat ↔ CLI switch, the ACP → PTY fallback
    runtime: RwLock<Arc<dyn Runtime>>,
    app: AppHandle,
    /// `Files::notes` for an agent; None for setup terminals
    pub notes: Option<PathBuf>,
    /// one prompt at a time, so a worker note and a chat message can't interleave (and none lands mid-switch)
    sending: Mutex<()>,
    /// durable events seen so far, and how many there were at the last prompt: a prompt that never reached the
    /// transcript (a TUI-only slash command such as `/help`) shows no turn the interrupt could end
    activity: Arc<AtomicU64>,
    sent_at: AtomicU64,
    /// bumped whenever a runtime is replaced or stopped on purpose: a runtime's exit only marks the agent off
    /// while the generation it started in is still current
    gen: Arc<AtomicU64>,
    /// the generation whose runtime exited last, counted or not (`restore_gen`)
    exited: Arc<AtomicU64>,
    /// the single-writer lock: one start, stop, swap or fallback at a time
    switching: Mutex<()>,
    owner: Mutex<Owner>,
    /// a newer switch request supersedes one still waiting for its turn to end
    switch_seq: AtomicU64,
    /// discarded or forgotten: nothing starts again
    retired: AtomicBool,
    /// what starting a runtime again needs; None for setup terminals
    ctx: Option<SpawnCtx>,
}

struct SpawnCtx {
    home: PathBuf,
    files: Files,
}

/// The runtime that owns the conversation now, and why it isn't ACP when the chat could have been.
#[derive(Clone, Debug)]
pub struct Owner {
    pub kind: catalog::Runtime,
    pub note: Option<FallbackReason>,
}

/// What a start runs.
#[derive(Clone)]
enum Target {
    Pty,
    Acp(AcpPlan),
}

/// The view asking for the switch: the chat (ACP when it can) or the CLI (the terminal).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum View {
    Chat,
    Cli,
}

/// When the switch may happen: only if idle now, after the turn under way, or right after interrupting it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum When {
    IfIdle,
    AfterTurn,
    Interrupt,
}

impl View {
    pub fn parse(s: &str) -> Result<View, String> {
        match s {
            "chat" => Ok(View::Chat),
            "cli" => Ok(View::Cli),
            _ => Err(format!("알 수 없는 보기: {s}")),
        }
    }
}

impl When {
    pub fn parse(s: &str) -> Result<When, String> {
        match s {
            "ifIdle" => Ok(When::IfIdle),
            "afterTurn" => Ok(When::AfterTurn),
            "interrupt" => Ok(When::Interrupt),
            _ => Err(format!("알 수 없는 전환 시점: {s}")),
        }
    }
}

/// `switch_runtime`'s answer.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SwitchResult {
    /// "done" | "busy" (not now: `busy` says why) | "stuck" (the old side didn't stop: a fork can open it) |
    /// "superseded" (a newer switch came) | "off" (it went off while the switch waited: `switch_runtime` starts it
    /// on the view's runtime and answers "done", so the UI never sees this one)
    pub status: &'static str,
    pub runtime: catalog::Runtime,
    /// why the chat runs on the terminal (`FallbackReason::view`)
    pub note: Option<Value>,
    /// "turn" | "approval" | "shell"
    pub busy: Option<&'static str>,
}

/// One look at a switch.
#[derive(Debug, PartialEq)]
pub enum Step {
    /// already the target
    Done,
    Busy(&'static str),
    /// wait for the turn to end
    Wait,
    /// interrupt the turn (once), then wait
    Interrupt,
    Proceed,
}

/// The switch's decision table. `busy`: what keeps it from being quiet ("turn" or "approval").
pub fn switch_step(cur: catalog::Runtime, target: catalog::Runtime, quiet: bool, busy: &'static str, shell: bool, when: When, interrupted: bool) -> Step {
    if cur == target {
        return Step::Done;
    }
    if !quiet {
        return match when {
            When::IfIdle => Step::Busy(busy),
            When::AfterTurn => Step::Wait,
            When::Interrupt if interrupted => Step::Wait,
            When::Interrupt => Step::Interrupt,
        };
    }
    // a background Bash dies with the process: only when the user said so
    if shell && when != When::Interrupt {
        return Step::Busy("shell");
    }
    Step::Proceed
}

/// How long the agent must have shown nothing before a switch stops it: the TUI redraws within this after an Enter
/// typed straight into it, the adapter talks within this once a turn starts. Idle and silent this long is not a turn
/// that just began (the state dot only turns Running at the tail's next tick).
const SWITCH_SILENCE: Duration = Duration::from_millis(1000);

/// How long a switch waits for the turn to end.
fn wait_limit(when: When) -> Duration {
    match when {
        When::AfterTurn => Duration::from_secs(30 * 60),
        _ => Duration::from_secs(15),
    }
}

/// The runtime before the first start.
struct Off;

impl Runtime for Off {
    fn kind(&self) -> catalog::Runtime {
        catalog::Runtime::Pty
    }
    fn ready(&self) -> Result<Readiness, LaunchFailure> {
        Err(LaunchFailure::Exited { message: "에이전트가 아직 시작하지 않았어요".into() })
    }
    fn accept_trust(&self) -> Result<(), String> {
        Err("에이전트가 실행 중이 아니에요".into())
    }
    fn prompt(&self, _: &Prompt) -> Result<Sent, String> {
        Err("에이전트가 꺼져 있어요".into())
    }
    fn cancel(&self) -> Result<(), String> {
        Ok(())
    }
    fn kill(&self) {}
    fn alive(&self) -> bool {
        false
    }
    fn quiet_for(&self) -> Duration {
        Duration::ZERO
    }
    fn pid(&self) -> Option<u32> {
        None
    }
}

fn same_runtime(a: &Arc<dyn Runtime>, b: &Arc<AcpRuntime>) -> bool {
    std::ptr::eq(Arc::as_ptr(a) as *const u8, Arc::as_ptr(b) as *const u8)
}

impl Agent {
    /// A worktree's agent, started on the runtime `acp_setup::plan` picks (`want`: the CLI view asks for the
    /// terminal). A terminal that can't start is an error here; the ACP runtime starts in the background and falls
    /// back to the terminal by itself.
    pub fn spawn(app: AppHandle, home: &Path, worktree_path: &str, files: &Files, meta: AgentMeta, want: Want) -> Result<Arc<Agent>, String> {
        let def = catalog::require(&meta.adapter_id)?;
        if !def.launchable() {
            return Err(format!("{}은(는) 아직 워크트리 에이전트로 실행할 수 없어요", def.name));
        }
        let (target, kind, note) = match acp_setup::plan_for(def, home, worktree_path, want) {
            Ok(plan) => (Target::Acp(plan), catalog::Runtime::Acp, None),
            Err(r) => (Target::Pty, catalog::Runtime::Pty, Some(r)),
        };
        let agent = Arc::new(Agent {
            worktree_path: worktree_path.into(),
            state: Arc::new(Mutex::new(AgentState::Idle)),
            runtime: RwLock::new(Arc::new(Off)),
            app: app.clone(),
            notes: Some(files.notes()),
            sending: Mutex::new(()),
            activity: Arc::new(AtomicU64::new(0)),
            sent_at: AtomicU64::new(0),
            gen: Arc::new(AtomicU64::new(1)),
            exited: Arc::default(),
            switching: Mutex::new(()),
            owner: Mutex::new(Owner { kind, note: note.clone() }),
            switch_seq: AtomicU64::new(0),
            retired: AtomicBool::new(false),
            ctx: Some(SpawnCtx { home: home.into(), files: files.clone() }),
        });
        {
            let _one = agent.switching.lock().unwrap();
            let (rt, boot) = agent.start(&target, ResetReason::Restart)?;
            agent.swap(rt, kind, note);
            if let Some(b) = boot {
                b.begin();
            }
        }
        // a restart (after an exit or an away stop) would otherwise still read "off" until its first turn
        let _ = app.emit("agent://state", AgentStateEvent { worktree_path: worktree_path.into(), state: AgentState::Idle });
        Ok(agent)
    }

    /// A CLI in a terminal with no transcript behind it: settings' setup terminals (install / login).
    pub fn run(app: AppHandle, key: &str, argv: &[String], cwd: &Path, env: &[(String, String)]) -> Result<Agent, String> {
        let state = Arc::new(Mutex::new(AgentState::Idle));
        let (a, p, st) = (app.clone(), key.to_string(), state.clone());
        let on_exit = Box::new(move || emit_state(&a, &p, &st, AgentState::Terminated));
        let rt = PtyRuntime::start(app.clone(), key, argv, cwd, env, None, &[], false, on_exit)?;
        Ok(Agent {
            worktree_path: key.into(),
            state,
            runtime: RwLock::new(Arc::new(rt)),
            app,
            notes: None,
            sending: Mutex::new(()),
            activity: Arc::default(),
            sent_at: AtomicU64::new(0),
            gen: Arc::default(),
            exited: Arc::default(),
            switching: Mutex::new(()),
            owner: Mutex::new(Owner { kind: catalog::Runtime::Pty, note: None }),
            switch_seq: AtomicU64::new(0),
            retired: AtomicBool::new(false),
            ctx: None,
        })
    }

    /// The runtime now (a clone: a swap doesn't pull it from under a caller).
    fn rt(&self) -> Arc<dyn Runtime> {
        self.runtime.read().unwrap().clone()
    }

    /// Every durable event moves the state dot first, then goes out; a finished turn is reported to whoever leads
    /// this agent (delivering blocks until the parent's prompt is up, so on its own thread).
    fn observer(&self) -> impl Fn(&RuntimeEvent) + Send + Sync + 'static {
        let (a, p, st, seen) = (self.app.clone(), self.worktree_path.clone(), self.state.clone(), self.activity.clone());
        move |ev| {
            seen.fetch_add(1, Ordering::SeqCst);
            // a permission card whose call ran (or whose turn ended) was answered elsewhere
            match ev {
                RuntimeEvent::ToolFinished { turn_id, tool_call_id, .. } => a.state::<Approvals>().settle(&p, turn_id, Some(tool_call_id)),
                RuntimeEvent::TurnEnded { turn_id, .. } => a.state::<Approvals>().settle(&p, turn_id, None),
                _ => {}
            }
            if let Some(next) = events::status_after(ev) {
                let done = events::finishes_turn(&st.lock().unwrap(), ev);
                emit_state(&a, &p, &st, next);
                if done {
                    let (a, p) = (a.clone(), p.clone());
                    thread::spawn(move || crate::control::worker_done(&a, &p));
                }
            }
        }
    }

    fn sink(&self) -> impl Fn(&events::Envelope) + Send + Sync + 'static {
        let app = self.app.clone();
        move |env| {
            let _ = app.emit("agent://event", env);
        }
    }

    /// The agent is off when a runtime of generation `my` exits; a runtime replaced or stopped since says nothing.
    fn exit_cb(&self, my: u64) -> impl Fn() + Send + Sync + Clone + 'static {
        let (gen, exited, a, p, st) = (self.gen.clone(), self.exited.clone(), self.app.clone(), self.worktree_path.clone(), self.state.clone());
        move || {
            // first: a switch that puts this generation back after it was hidden reads it (`restore_gen`)
            exited.store(my, Ordering::SeqCst);
            if gen.load(Ordering::SeqCst) == my {
                emit_state(&a, &p, &st, AgentState::Terminated);
            }
        }
    }

    /// Starts `target` on the worktree's conversation (callers hold `switching`). The ACP runtime comes back too,
    /// to `begin` once it's registered.
    fn start(self: &Arc<Self>, target: &Target, reason: ResetReason) -> Result<(Arc<dyn Runtime>, Option<Arc<AcpRuntime>>), String> {
        let ctx = self.ctx.as_ref().ok_or("설정 터미널은 다시 시작할 수 없어요")?;
        let mut meta = ctx.files.read_meta().ok_or("이 워크트리에는 에이전트 설정이 없습니다")?;
        let def = catalog::require(&meta.adapter_id)?;
        let path = self.worktree_path.as_str();
        let approvals = self.app.state::<Approvals>();
        let on_exit = self.exit_cb(self.gen.load(Ordering::SeqCst));
        match target {
            Target::Pty => {
                // a missing executable is reported here, before anything starts
                let exe = crate::cli::command(def)?;
                let pa = runtime::pty_agent(&meta.adapter_id);
                let launch = pa.launch(&LaunchCtx { home: &ctx.home, worktree: path, def, approvals: &approvals }, &mut meta, &ctx.files, exe)?;
                let emitter = Emitter::new(path, self.sink(), self.observer());
                // whoever shows this worktree's chat reloads it
                emitter.reset(reason);
                // Primed first so only lines written from now on are emitted live.
                let live = LiveTail::start(pa, &ctx.home, path, ctx.files.meta.clone(), meta.cli_session_id.clone(), launch.transcript, emitter);
                let env = maestro_env(&ctx.home, path, &approvals);
                let rt = PtyRuntime::start(self.app.clone(), path, &launch.argv, Path::new(path), &env, Some(live), def.login_hints, launch.resumed, Box::new(on_exit))?;
                Ok((Arc::new(rt), None))
            }
            Target::Acp(plan) => {
                let emitter = Emitter::with_record(path, self.sink(), self.observer());
                let logs = (std::env::var("MAESTRO_ACP_LOGS").as_deref() == Ok("1")).then(|| acp_setup::app_data().map(|d| d.join("acp").join("logs").join(crate::worktree::path_id(path)))).flatten();
                let env = acp_setup::acp_env(std::env::vars().map(|(k, _)| k), &plan.cli_exe, &maestro_env(&ctx.home, path, &approvals), logs);
                // the guard's hook (`/pretool/<key>`) is keyed like a terminal's session id; the session id isn't known
                // before session/new
                let key = format!("acp-{}", crate::worktree::path_id(path));
                approvals.register(&key, path);
                let settings = def.features.approval_hook.then(|| approvals.acp_settings(&key));
                let (app, cwd) = (self.app.clone(), path.to_string());
                // a guarded command is refused before any card (auto-allowed ones are caught by the hook in `settings`)
                let ask: Ask = Arc::new(move |req, still| {
                    if let Some(why) = crate::guard::live_check(&app, &cwd, None, &req.tool_name, &req.tool_input) {
                        eprintln!("[maestro] guard denied {}: {why}", req.tool_name);
                        return Some(crate::approval::Choice::Deny);
                    }
                    app.state::<Approvals>().request(&app, &cwd, req, still).1
                });
                let weak = Arc::downgrade(self);
                // the composer's `/` menu asks again: the list comes after the session is up, and may change
                let (app, cwd) = (self.app.clone(), path.to_string());
                let on_commands = Box::new(move || {
                    let _ = app.emit("agent://commands", serde_json::json!({ "worktreePath": cwd }));
                });
                let acp = AcpRuntime::new(
                    AcpCtx { home: ctx.home.clone(), cwd: path.into(), files: ctx.files.clone(), plan: plan.clone(), reason, env, brief: system_brief(&ctx.home, &meta, path), settings, agent_id: def.id, ask, on_commands },
                    emitter,
                    Box::new(move |rt, why| weak.upgrade().and_then(|a| a.fallback(rt, why))),
                    Box::new(on_exit),
                );
                Ok((acp.clone(), Some(acp)))
            }
        }
    }

    /// The new runtime owns the conversation (callers hold `switching`).
    fn swap(&self, rt: Arc<dyn Runtime>, kind: catalog::Runtime, note: Option<FallbackReason>) {
        *self.runtime.write().unwrap() = rt;
        *self.owner.lock().unwrap() = Owner { kind, note };
        self.emit_runtime();
    }

    fn emit_runtime(&self) {
        let o = self.owner();
        let _ = self.app.emit("agent://runtime", serde_json::json!({ "worktreePath": self.worktree_path, "runtime": o.kind, "note": o.note.map(|n| n.view()) }));
    }

    /// The ACP runtime didn't come up: the terminal takes over, on the same conversation. Runs on its boot thread.
    /// Some(error) when the terminal didn't start either (the agent is off then).
    fn fallback(self: &Arc<Self>, acp: &Arc<AcpRuntime>, reason: FallbackReason) -> Option<String> {
        let _one = self.switching.lock().unwrap_or_else(|e| e.into_inner());
        if self.retired.load(Ordering::SeqCst) || !same_runtime(&self.rt(), acp) {
            return None;
        }
        self.gen.fetch_add(1, Ordering::SeqCst);
        acp.kill();
        acp_setup::record(acp.agent_id(), &reason);
        match self.start(&Target::Pty, ResetReason::Fallback) {
            Ok((rt, _)) => {
                self.swap(rt, catalog::Runtime::Pty, Some(reason));
                None
            }
            Err(e) => {
                *self.owner.lock().unwrap() = Owner { kind: catalog::Runtime::Pty, note: Some(reason) };
                self.emit_runtime();
                emit_state(&self.app, &self.worktree_path, &self.state, AgentState::Terminated);
                Some(e)
            }
        }
    }

    pub fn owner(&self) -> Owner {
        self.owner.lock().unwrap().clone()
    }

    pub fn alive(&self) -> bool {
        *self.state.lock().unwrap() != AgentState::Terminated
    }

    /// The runtime, kept beyond a borrow of this agent (`runtime::history`, `runtime::snapshot`).
    pub fn shared_runtime(&self) -> Arc<dyn Runtime> {
        self.rt()
    }

    /// The commands its runtime reports, when it does (ACP).
    pub fn commands(&self) -> Option<Vec<(String, String)>> {
        self.rt().commands()
    }

    /// Blocks until the agent takes prompts. Ok: whether it first shows its "trust this folder?" question.
    /// Err: why it didn't come up (exited, wants a login, no prompt in time). A runtime replaced meanwhile (a
    /// fallback, a switch) is waited for in its place.
    pub fn wait_ready(&self) -> Result<bool, LaunchFailure> {
        self.ready_runtime().1
    }

    /// `wait_ready`, with the runtime it's about. A runtime that stops while a fallback or switch replaces it
    /// (both stop it before the new one is in, holding `switching`) isn't the answer: that swap is waited for
    /// first. Never called holding `switching` or `sending`.
    fn ready_runtime(&self) -> (Arc<dyn Runtime>, Result<bool, LaunchFailure>) {
        loop {
            let r = self.rt();
            match r.ready() {
                Err(e) => {
                    drop(self.switching.lock().unwrap_or_else(|e| e.into_inner()));
                    if !Arc::ptr_eq(&r, &self.rt()) {
                        continue;
                    }
                    return (r, Err(e));
                }
                Ok(x) => return (r, Ok(x == Readiness::NeedsTrust)),
            }
        }
    }

    /// Accept the trust question, then wait for the real prompt again.
    pub fn accept_trust(&self) -> Result<(), String> {
        let r = self.rt();
        match r.accept_trust() {
            Err(_) if !Arc::ptr_eq(&r, &self.rt()) => Ok(()),
            x => x,
        }
    }

    /// Raw input for the CLI view's terminal.
    pub fn write(&self, data: &[u8]) -> Result<(), String> {
        self.rt().terminal().ok_or("지금은 채팅이 대화를 쓰고 있어요. CLI로 전환해 주세요.")?.write(data)
    }

    /// A chat message (attachments become @mentions), one at a time.
    pub fn send(&self, text: &str, files: &[String]) -> Result<(), String> {
        self.send_prompt(Prompt { text: text.into(), files: files.to_vec(), kept: vec![] })
    }

    /// Kept worker notes, pasted as one message; a runtime that can't deliver them after all keeps them again.
    pub fn send_note(&self, text: &str, kept: &[(String, String)]) -> Result<(), String> {
        self.send_prompt(Prompt { text: text.into(), files: vec![], kept: kept.to_vec() })
    }

    /// Only into a runtime that takes input: callers wait for that first, but a switch may swap in a new runtime
    /// (still starting) between their wait and `sending` here, and a paste into a TUI that isn't up is lost. So the
    /// runtime waited for has to be the one in place once `sending` is held (a switch swaps only holding it).
    fn send_prompt(&self, p: Prompt) -> Result<(), String> {
        loop {
            // ConPTY still takes writes after the CLI exits, so a paste into a dead agent would look sent
            if !self.alive() {
                return Err("에이전트가 꺼져 있어요".into());
            }
            let (rt, ready) = self.ready_runtime();
            match ready {
                Ok(false) => {}
                Ok(true) => return Err("폴더 신뢰 확인이 먼저 필요해요".into()),
                Err(f) => return Err(f.message().into()),
            }
            let _one = self.sending.lock().unwrap_or_else(|e| e.into_inner());
            if !Arc::ptr_eq(&rt, &self.rt()) {
                continue;
            }
            if !self.alive() {
                return Err("에이전트가 꺼져 있어요".into());
            }
            self.sent_at.store(self.activity.load(Ordering::SeqCst), Ordering::SeqCst);
            if rt.prompt(&p)? == Sent::Turn {
                emit_state(&self.app, &self.worktree_path, &self.state, AgentState::Running);
            }
            return Ok(());
        }
    }

    /// Pastes the worker notes kept while this agent was off or at the trust dialog, once its prompt is up.
    pub fn deliver_kept_notes(self: Arc<Self>) {
        let Some(file) = self.notes.clone().filter(|f| f.exists()) else { return };
        thread::spawn(move || {
            // only once its prompt is up; at the trust dialog or after a failed start the notes stay
            if self.wait_ready() == Ok(false) && self.alive() {
                let _ = crate::control::pass_notes(&file, None, Some(&|t: &str, kept: &[(String, String)]| self.send_note(t, kept)));
            }
        });
    }

    /// Stops the current turn; the agent stays up. A prompt that never reached the transcript (`/help` and other
    /// TUI-only commands open a dialog and write nothing) has no turn whose end would bring the dot back to idle,
    /// so once Esc has closed the dialog and the transcript still shows nothing new, the agent counts as idle.
    /// (ACP: the prompt's response always ends its turn, so no such guess.)
    pub fn interrupt(self: &Arc<Self>) -> Result<(), String> {
        let rt = self.rt();
        rt.cancel()?;
        if rt.kind() == catalog::Runtime::Acp {
            return Ok(());
        }
        let me = self.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(1500));
            me.rt().sync();
            thread::sleep(Duration::from_millis(300));
            let untouched = me.activity.load(Ordering::SeqCst) == me.sent_at.load(Ordering::SeqCst);
            if untouched && *me.state.lock().unwrap() == AgentState::Running {
                emit_state(&me.app, &me.worktree_path, &me.state, AgentState::Idle);
            }
        });
        Ok(())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        self.rt().terminal().map_or(Ok(()), |t| t.resize(cols, rows))
    }

    /// The terminal's scrollback; none (the ACP runtime owns the conversation): `terminal: false`.
    pub fn snapshot(&self) -> PtySnapshot {
        let rt = self.rt();
        let snap = rt.terminal().map(|t| t.snapshot());
        snap.unwrap_or_else(|| PtySnapshot { epoch: crate::stream::pty_epoch(&self.worktree_path), terminal: false, ..Default::default() })
    }

    /// Time since the agent last showed anything (zero while waiting for a redraw).
    pub fn quiet_for(&self) -> Duration {
        self.rt().quiet_for()
    }

    /// A shell is running somewhere under the CLI: a Bash tool call, likely a background one, still going.
    pub fn runs_shell(&self, procs: &[Proc]) -> bool {
        let rt = self.rt();
        rt.pid().is_some_and(|pid| shell_below(procs, pid, rt.started()))
    }

    /// Kills the whole process tree. The agent goes off once it's gone.
    pub fn kill(&self) {
        self.rt().kill()
    }

    /// Discarded or forgotten: stopped for good, and nothing (a switch, a fallback) starts it again.
    pub fn retire(&self) {
        let _one = self.switching.lock().unwrap_or_else(|e| e.into_inner());
        self.retired.store(true, Ordering::SeqCst);
        self.rt().kill();
    }

    /// A switch or fallback is going on (the away stop leaves it alone).
    pub fn switching_now(&self) -> bool {
        self.switching.try_lock().is_err()
    }

    /// Reads what the agent wrote so far now (its Stop hook fired) instead of at the next tick.
    pub fn sync(&self) {
        self.rt().sync()
    }

    /// The agent moved to another conversation (`/clear`): the chat follows it.
    pub fn follow(&self, conversation: &str, transcript: Option<&Path>) {
        self.rt().follow(conversation, transcript)
    }

    /// The tool call a permission prompt is about (`tool_call_id`: the one the prompt names, if any).
    pub fn locate_tool(&self, name: &str, input: &Value, tool_call_id: Option<&str>) -> ToolMatch {
        self.rt().locate_tool(name, input, tool_call_id)
    }

    fn pending_approval(&self) -> bool {
        let same = |w: &str| w.replace('\\', "/").eq_ignore_ascii_case(&self.worktree_path.replace('\\', "/"));
        self.app.state::<Approvals>().pending().iter().any(|a| same(&a.worktree_path))
    }

    /// Nothing under way: idle, nothing queued, no permission question open.
    fn quiet(&self) -> bool {
        *self.state.lock().unwrap() == AgentState::Idle && self.rt().settled() && !self.pending_approval()
    }

    /// Quiet, and silent for `SWITCH_SILENCE`: what a switch needs before it stops the old process.
    fn still(&self) -> bool {
        self.quiet() && self.quiet_for() >= SWITCH_SILENCE
    }

    /// A switch that bumped the generation and then left the runtime in place puts generation `prev` back. An exit
    /// the bump hid counts now, as its callback would have counted it (one that comes after counts by itself; both
    /// at once only say "off" twice).
    fn restore_gen(&self, prev: u64) {
        self.gen.store(prev, Ordering::SeqCst);
        if self.exited.load(Ordering::SeqCst) == prev {
            emit_state(&self.app, &self.worktree_path, &self.state, AgentState::Terminated);
        }
    }

    fn result(&self, status: &'static str, busy: Option<&'static str>) -> SwitchResult {
        let o = self.owner();
        SwitchResult { status, runtime: o.kind, note: o.note.map(|n| n.view()), busy }
    }

    /// The result when the agent already runs as asked.
    pub fn switch_done(&self) -> SwitchResult {
        self.result("done", None)
    }

    fn remember_view(&self, view: View) {
        if let Some(ctx) = &self.ctx {
            remember_view(&ctx.files, view);
        }
    }

    /// The old runtime is gone within 5 s: its process (not its state: a closed ACP session is "not alive" at once),
    /// then its whole tree (an orphaned claude still writes the conversation): the job's count when it has one (ACP),
    /// else the pid it had and whatever ran right below it, among the processes started since it was.
    fn confirm_dead(&self, old: &Arc<dyn Runtime>) -> bool {
        let (pid, since) = (old.pid(), old.started());
        let start = Instant::now();
        let within = || start.elapsed() < Duration::from_secs(5);
        while old.process_alive() && within() {
            thread::sleep(Duration::from_millis(100));
        }
        if old.process_alive() {
            return false;
        }
        // a job that can't be asked: the process list decides
        loop {
            match old.tree_gone() {
                Some(true) => return true,
                Some(false) if within() => thread::sleep(Duration::from_millis(100)),
                Some(false) => return false,
                None => break,
            }
        }
        while let (Some(pid), Some(procs)) = (pid, processes()) {
            if !tree_listed(&procs, pid, since) {
                return true;
            }
            if !within() {
                return false;
            }
            thread::sleep(Duration::from_millis(200));
        }
        true
    }

    /// The chat ↔ CLI switch: the chat view runs ACP (when it can), the CLI view the terminal, on the same
    /// conversation. Only one writer at a time: it waits for the turn to end (`when`), then stops the old process
    /// and makes sure it's gone before the new one starts (under `switching` and `sending`). An old process that
    /// won't go keeps the conversation ("stuck"); `fork` opens a copy instead, if the user agrees.
    pub fn switch(self: &Arc<Self>, view: View, when: When) -> Result<SwitchResult, String> {
        let ctx = self.ctx.as_ref().ok_or("설정 터미널은 전환할 수 없어요")?;
        let my = self.switch_seq.fetch_add(1, Ordering::SeqCst) + 1;
        let meta = ctx.files.read_meta().ok_or("이 워크트리에는 에이전트 설정이 없습니다")?;
        let def = catalog::require(&meta.adapter_id)?;
        let (target, kind, note) = match view {
            View::Cli => (Target::Pty, catalog::Runtime::Pty, None),
            View::Chat => match acp_setup::plan_for(def, &ctx.home, &self.worktree_path, Want::Auto) {
                Ok(plan) => (Target::Acp(plan), catalog::Runtime::Acp, None),
                Err(r) => (Target::Pty, catalog::Runtime::Pty, Some(r)),
            },
        };
        self.remember_view(view);
        let started = Instant::now();
        let mut interrupted = false;
        // "agent://switch" put up a "턴이 끝나면 전환돼요" in every window: however this ends, it comes down again
        // (a swap's "agent://runtime" clears it too)
        let mut announced = Announced { agent: self, on: false };
        loop {
            if self.switch_seq.load(Ordering::SeqCst) != my {
                return Ok(self.result("superseded", None));
            }
            if self.retired.load(Ordering::SeqCst) {
                return Err("에이전트가 꺼졌어요".into());
            }
            // it went off while this waited (a crash mid-turn, the terminal exited): nothing left to stop, the
            // caller starts the view's runtime anew
            if !self.alive() {
                return Ok(self.result("off", None));
            }
            let cur = self.owner().kind;
            let busy = if self.pending_approval() { "approval" } else { "turn" };
            let quiet = self.quiet();
            // idle, but it drew or talked just now (an Enter typed into the TUI, a turn the tail hasn't read yet):
            // look again in a moment rather than stop it
            if quiet && cur != kind && self.quiet_for() < SWITCH_SILENCE {
                if started.elapsed() > wait_limit(when) {
                    return Ok(self.result("busy", Some(busy)));
                }
                thread::sleep(Duration::from_millis(200));
                continue;
            }
            match switch_step(cur, kind, quiet, busy, false, when, interrupted) {
                Step::Done => {
                    // the owner changes only under `switching` (a swap, a fallback): one going on now may still swap
                    // in another runtime, so "done" is decided once it has finished
                    let _one = self.switching.lock().unwrap_or_else(|e| e.into_inner());
                    if self.switch_seq.load(Ordering::SeqCst) != my {
                        return Ok(self.result("superseded", None));
                    }
                    if self.owner().kind != kind {
                        continue;
                    }
                    // the chat on the terminal's transcript: nothing to stop, only the reason may be new
                    if view == View::Chat && kind == catalog::Runtime::Pty {
                        self.owner.lock().unwrap().note = note;
                        self.emit_runtime();
                    }
                    return Ok(self.result("done", None));
                }
                Step::Busy(b) => return Ok(self.result("busy", Some(b))),
                Step::Interrupt => {
                    let _ = self.interrupt();
                    interrupted = true;
                    thread::sleep(Duration::from_millis(200));
                    continue;
                }
                Step::Wait => {
                    if !announced.on {
                        let pending = if view == View::Cli { "cli" } else { "chat" };
                        let _ = self.app.emit("agent://switch", serde_json::json!({ "worktreePath": self.worktree_path, "pending": pending }));
                        announced.on = true;
                    }
                    if started.elapsed() > wait_limit(when) {
                        return Ok(self.result("busy", Some(busy)));
                    }
                    thread::sleep(Duration::from_millis(200));
                    continue;
                }
                Step::Proceed => {}
            }
            let _one = self.switching.lock().unwrap_or_else(|e| e.into_inner());
            let _send = self.sending.lock().unwrap_or_else(|e| e.into_inner());
            // From here an exit of the old runtime is the switch's to read, not its callback's: one in the checks
            // below (the slow process list, /exit typed in the terminal, an adapter crash) would otherwise mark the
            // agent off while the new runtime goes in, and the next mount would start a second writer.
            let old = self.rt();
            let prev = self.gen.fetch_add(1, Ordering::SeqCst);
            // a newer switch came while this waited for the locks: it decides, from what this leaves in place
            if self.switch_seq.load(Ordering::SeqCst) != my {
                self.restore_gen(prev);
                return Ok(self.result("superseded", None));
            }
            // it went off just before: the loop's top answers "off"
            if !self.alive() {
                self.restore_gen(prev);
                continue;
            }
            // a prompt may have slipped in before the locks (`sending` only holds back Maestro's own)
            if !self.still() || self.owner().kind != cur {
                self.restore_gen(prev);
                continue;
            }
            let shell = processes().is_some_and(|p| self.runs_shell(&p));
            if let Step::Busy(b) = switch_step(cur, kind, true, busy, shell, when, interrupted) {
                self.restore_gen(prev);
                return Ok(self.result("busy", Some(b)));
            }
            if self.retired.load(Ordering::SeqCst) {
                self.restore_gen(prev);
                return Err("에이전트가 꺼졌어요".into());
            }
            // the process list took seconds: a turn may have begun meanwhile without Maestro (typed into the TUI,
            // a background task or subagent finishing). Looked at once more right before the stop.
            if !self.still() || self.owner().kind != cur {
                self.restore_gen(prev);
                continue;
            }
            old.shutdown();
            // a slow process list or a late taskkill: once more, the hard way
            let gone = self.confirm_dead(&old) || {
                old.kill();
                self.confirm_dead(&old)
            };
            if !gone {
                // its process, or one it left behind, may still write: never a second writer on the conversation. It
                // stays the owner, never "off" (a later start would be that second writer), even with its root
                // gone and an orphan left: a fork opens a copy, a later switch tries again.
                self.gen.store(prev, Ordering::SeqCst);
                return Ok(self.result("stuck", None));
            }
            return match self.start(&target, ResetReason::Switch) {
                Ok((rt, boot)) => {
                    self.swap(rt, kind, note);
                    if let Some(b) = boot {
                        b.begin();
                    }
                    Ok(self.result("done", None))
                }
                Err(e) => {
                    emit_state(&self.app, &self.worktree_path, &self.state, AgentState::Terminated);
                    Err(e)
                }
            };
        }
    }

    /// After a "stuck" switch the user agreed to: the conversation copied to a new one (`session/fork`), and the
    /// view's runtime opened on the copy. The old process is killed for good measure.
    pub fn fork(self: &Arc<Self>, view: View) -> Result<SwitchResult, String> {
        let ctx = self.ctx.as_ref().ok_or("설정 터미널은 전환할 수 없어요")?;
        let meta = ctx.files.read_meta().ok_or("이 워크트리에는 에이전트 설정이 없습니다")?;
        let def = catalog::require(&meta.adapter_id)?;
        let plan = acp_setup::plan_for(def, &ctx.home, &self.worktree_path, Want::Auto).map_err(|_| "갈래를 만들 수 없어요 (채팅 런타임을 쓸 수 없어요)".to_string())?;
        let id = meta.cli_session_id.clone().ok_or("갈래를 만들 대화가 없어요")?;
        let approvals = self.app.state::<Approvals>();
        let env = acp_setup::acp_env(std::env::vars().map(|(k, _)| k), &plan.cli_exe, &maestro_env(&ctx.home, &self.worktree_path, &approvals), None);
        let new_id = crate::acp_runtime::fork_session(&plan, &env, &self.worktree_path, &id, &system_brief(&ctx.home, &meta, &self.worktree_path), meta.model.as_deref())?;
        let _one = self.switching.lock().unwrap_or_else(|e| e.into_inner());
        // the copy took seconds: the stuck process may have exited meanwhile (it was sent a kill), and the agent is
        // off then. It must stay off: whoever starts it next (`fork_session` on "off") starts the one runtime on the
        // copy; one started here would be a second, never stopped
        let prev = self.gen.fetch_add(1, Ordering::SeqCst);
        if self.retired.load(Ordering::SeqCst) {
            self.restore_gen(prev);
            return Err("에이전트가 꺼졌어요".into());
        }
        let off = !self.alive();
        if off {
            self.restore_gen(prev);
        } else {
            self.rt().kill();
        }
        if let Some(mut m) = ctx.files.read_meta() {
            m.cli_session_id = Some(new_id);
            m.last_view = (view == View::Cli).then(|| "cli".to_string());
            ctx.files.write_meta(&m)?;
        }
        if off {
            return Ok(self.result("off", None));
        }
        let (target, kind) = match view {
            View::Cli => (Target::Pty, catalog::Runtime::Pty),
            View::Chat => (Target::Acp(plan), catalog::Runtime::Acp),
        };
        match self.start(&target, ResetReason::Switch) {
            Ok((rt, boot)) => {
                self.swap(rt, kind, None);
                if let Some(b) = boot {
                    b.begin();
                }
                Ok(self.result("done", None))
            }
            Err(e) => {
                // the old one is killed: off, so the next mount starts the copy
                emit_state(&self.app, &self.worktree_path, &self.state, AgentState::Terminated);
                Err(e)
            }
        }
    }
}

/// A switch that announced a wait: its end takes the announcement down (`pending: null`).
struct Announced<'a> {
    agent: &'a Agent,
    on: bool,
}

impl Drop for Announced<'_> {
    fn drop(&mut self) {
        if self.on {
            let _ = self.agent.app.emit("agent://switch", serde_json::json!({ "worktreePath": self.agent.worktree_path, "pending": null }));
        }
    }
}

/// Which view the user last looked at (`AgentMeta::last_view`): the next start opens its runtime.
pub fn remember_view(files: &Files, view: View) {
    if let Some(mut m) = files.read_meta() {
        let v = (view == View::Cli).then(|| "cli".to_string());
        if m.last_view != v {
            m.last_view = v;
            let _ = files.write_meta(&m);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_switch_waits_for_quiet_and_asks_before_killing_a_shell() {
        use catalog::Runtime::{Acp, Pty};
        // already there
        assert_eq!(switch_step(Pty, Pty, false, "turn", true, When::IfIdle, false), Step::Done);
        // a turn under way
        assert_eq!(switch_step(Acp, Pty, false, "turn", false, When::IfIdle, false), Step::Busy("turn"));
        assert_eq!(switch_step(Acp, Pty, false, "approval", false, When::IfIdle, false), Step::Busy("approval"));
        assert_eq!(switch_step(Acp, Pty, false, "turn", false, When::AfterTurn, false), Step::Wait);
        assert_eq!(switch_step(Pty, Acp, false, "turn", false, When::Interrupt, false), Step::Interrupt);
        assert_eq!(switch_step(Pty, Acp, false, "turn", false, When::Interrupt, true), Step::Wait);
        // quiet, but a background shell would die: only when told to interrupt
        assert_eq!(switch_step(Pty, Acp, true, "turn", true, When::AfterTurn, false), Step::Busy("shell"));
        assert_eq!(switch_step(Pty, Acp, true, "turn", true, When::Interrupt, false), Step::Proceed);
        assert_eq!(switch_step(Pty, Acp, true, "turn", false, When::IfIdle, false), Step::Proceed);
        assert_eq!(View::parse("cli"), Ok(View::Cli));
        assert_eq!(When::parse("afterTurn"), Ok(When::AfterTurn));
        assert!(When::parse("now").is_err());
    }

    #[test]
    fn an_exit_counts_only_in_its_own_generation() {
        // what `exit_cb` and `restore_gen` check: a runtime replaced or stopped on purpose (gen bumped) doesn't mark
        // the agent off; one whose generation is put back does, whether it exited before or after
        let gen = Arc::new(AtomicU64::new(1));
        let exited = Arc::new(AtomicU64::new(0));
        let off = Arc::new(AtomicBool::new(false));
        let cb = |my: u64| {
            let (g, e, o) = (gen.clone(), exited.clone(), off.clone());
            move || {
                e.store(my, Ordering::SeqCst);
                if g.load(Ordering::SeqCst) == my {
                    o.store(true, Ordering::SeqCst);
                }
            }
        };
        let restore = |prev: u64| {
            gen.store(prev, Ordering::SeqCst);
            if exited.load(Ordering::SeqCst) == prev {
                off.store(true, Ordering::SeqCst);
            }
        };
        let old = cb(gen.load(Ordering::SeqCst));
        let prev = gen.fetch_add(1, Ordering::SeqCst);
        // the exit a switch hides while it checks
        old();
        assert!(!off.load(Ordering::SeqCst));
        // the switch gives up (a prompt slipped in, a shell): the hidden exit counts now
        restore(prev);
        assert!(off.load(Ordering::SeqCst));
        // a runtime of the next generation (a swap): put back first, exited after, the callback counts it by itself
        off.store(false, Ordering::SeqCst);
        gen.fetch_add(1, Ordering::SeqCst);
        let old = cb(gen.load(Ordering::SeqCst));
        let prev = gen.fetch_add(1, Ordering::SeqCst);
        restore(prev);
        assert!(!off.load(Ordering::SeqCst));
        old();
        assert!(off.load(Ordering::SeqCst));
    }

    #[test]
    fn drops_host_session_env_only_inside_claude() {
        let vars = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>().into_iter();
        assert!(host_session_env(vars(&["NO_COLOR", "CLAUDE_CODE_SESSION_ID"])).is_empty());
        let dropped = host_session_env(vars(&["CLAUDECODE", "NO_COLOR", "CLAUDE_CODE_SESSION_ID", "CLAUDE_AGENT_SDK_VERSION", "CLAUDE_CODE_GIT_BASH_PATH", "PATH"]));
        assert_eq!(dropped, ["CLAUDECODE", "NO_COLOR", "CLAUDE_CODE_SESSION_ID", "CLAUDE_AGENT_SDK_VERSION"]);
    }

    fn proc(pid: u32, ppid: u32, name: &str, started: u64) -> Proc {
        Proc { pid, ppid, name: name.into(), started }
    }

    #[test]
    fn finds_shells_below_the_cli_only() {
        let p = |pid, ppid, n: &str| proc(pid, ppid, n, 0);
        // cmd (root) → node → { mcp node, bash }; an unrelated bash elsewhere
        let procs = vec![p(10, 1, "cmd.exe"), p(11, 10, "node.exe"), p(12, 11, "node.exe"), p(13, 11, "bash.exe"), p(20, 1, "bash.exe")];
        assert!(shell_below(&procs, 10, 0));
        assert!(!shell_below(&procs[..3], 10, 0));
        assert!(!shell_below(&procs, 12, 0));
        // a cycle in stale pid data must not loop forever
        assert!(!shell_below(&[p(1, 2, "node"), p(2, 1, "node")], 1, 0));
        // a bash older than the CLI whose parent once had its pid isn't below it
        assert!(!shell_below(&[proc(13, 10, "bash.exe", 1_000)], 10, 50_000));
    }

    #[test]
    fn an_orphan_keeps_the_old_tree_listed() {
        let p = |pid, ppid, n: &str| proc(pid, ppid, n, 60_000);
        // node (10) gone, its claude (11) still there: not dead yet
        assert!(tree_listed(&[p(11, 10, "claude.exe"), p(20, 1, "x")], 10, 50_000));
        assert!(tree_listed(&[p(10, 1, "node.exe")], 10, 50_000));
        assert!(!tree_listed(&[p(12, 11, "bash.exe"), p(20, 1, "x")], 10, 50_000));
        // started before node (its parent was another process with node's pid, long gone): not node's
        assert!(!tree_listed(&[proc(11, 10, "svchost.exe", 10_000)], 10, 50_000));
        // a start time the list didn't give, or none known for the root: it counts, to be safe
        assert!(tree_listed(&[proc(11, 10, "claude.exe", 0)], 10, 50_000));
        assert!(tree_listed(&[proc(11, 10, "svchost.exe", 10_000)], 10, 0));
    }

    #[test]
    fn reads_ps_elapsed_times() {
        assert_eq!(elapsed_secs("05:07"), Some(307));
        assert_eq!(elapsed_secs("01:00:00"), Some(3600));
        assert_eq!(elapsed_secs("2-00:00:01"), Some(2 * 86_400 + 1));
        assert_eq!(elapsed_secs("x"), None);
    }

    #[test]
    #[ignore = "reads the live process list"]
    fn lists_processes_live() {
        let procs = processes().expect("process list");
        let me = procs.iter().find(|p| p.pid == std::process::id()).expect("this process");
        // started a moment ago, not in the future
        assert!(me.started > 0 && me.started <= now_ms() && now_ms() - me.started < 3_600_000, "{me:?}");
    }

    #[test]
    fn brief_carries_persona_and_references() {
        let home = std::env::temp_dir().join(format!("maestro-brief-{}", std::process::id()));
        crate::library::seed(&home);
        let persona = crate::library::personas_dir(&home).join("reviewer.md").to_string_lossy().replace('\\', "/");
        let doc = home.join("spec.md");
        std::fs::write(&doc, "# 결제 스펙\n...").unwrap();
        let m = AgentMeta { refs: vec![persona, doc.to_string_lossy().replace('\\', "/")], role: Some("maestro".into()), ..Default::default() };
        let b = system_brief(&home, &m, "C:/p/brief-repo");
        assert!(b.contains(COORDINATOR_HINT));
        // merges go through the app: the conductor asks, nobody pushes
        assert!(b.contains("maestro merge-request --to <worker>") && MAESTRO_HINT.contains("Never push"));
        // the coordination policy comes from the playbooks, with the default one listed
        assert!(b.contains("## Playbooks") && b.contains("- 기본 지휘 ("));
        assert!(b.contains("## Your persona") && b.contains("# 리뷰어"));
        assert!(b.contains("- 결제 스펙 ("));
        assert_eq!(system_brief(&home, &AgentMeta::default(), "C:/p/brief-repo"), MAESTRO_HINT);
        // PR mode: the coordinator hears how merges go now; local mode doesn't mention it
        assert!(!b.contains(PR_MODE_HINT));
        crate::project::write_mode(&home, "C:/p/brief-repo", crate::project::MergeMode::Pr).unwrap();
        let pr = system_brief(&home, &m, "C:/p/brief-repo");
        assert!(pr.contains(PR_MODE_HINT) && pr.contains("maestro merge-request") && PR_MODE_HINT.contains("push, gh pr create/merge/edit"));
        // it replaces the --no-ff part, and a pushed branch is never rewritten (the app never force-pushes)
        assert!(PR_MODE_HINT.contains("--no-ff 병합 설명 대신") && PR_MODE_HINT.contains("rebase") && PR_MODE_HINT.contains("강제 푸시"));
        assert!(!system_brief(&home, &AgentMeta::default(), "C:/p/brief-repo").contains(PR_MODE_HINT));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn worker_brief_in_pr_mode_says_merge_never_rebase() {
        let home = std::env::temp_dir().join(format!("maestro-wbrief-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let repo = home.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let git = |dir: &Path, args: &[&str]| {
            let o = std::process::Command::new("git").current_dir(dir).args(args).output().unwrap();
            assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
        };
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "init"]);
        let wt = home.join("wt");
        git(&repo, &["worktree", "add", "-q", "-b", "feat", &wt.to_string_lossy()]);
        let root = crate::worktree::main_checkout(&wt.to_string_lossy()).unwrap();
        std::fs::create_dir_all(home.join(".maestro")).unwrap();
        // registered with backslashes, as a folder picker may give it
        std::fs::write(home.join(".maestro").join("repos.json"), serde_json::to_string(&vec![root.replace('/', "\\")]).unwrap()).unwrap();
        let wt_path = wt.to_string_lossy().to_string();
        assert!(!system_brief(&home, &AgentMeta::default(), &wt_path).contains(WORKER_PR_HINT));
        crate::project::write_mode(&home, &root.replace('/', "\\"), crate::project::MergeMode::Pr).unwrap();
        let b = system_brief(&home, &AgentMeta::default(), &wt_path);
        assert!(b.contains(WORKER_PR_HINT) && WORKER_PR_HINT.contains("git merge <기준>") && WORKER_PR_HINT.contains("rebase·amend·reset"));
        // the coordinator gets its own PR brief, not the worker's
        let m = AgentMeta { role: Some("maestro".into()), ..Default::default() };
        assert!(!system_brief(&home, &m, &root.replace('/', "\\")).contains(WORKER_PR_HINT));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn previews_launch_commands() {
        assert_eq!(launch_preview("claude-code", "plan", "opus"), "claude --permission-mode plan --model opus");
        assert_eq!(launch_preview("antigravity", "bypassPermissions", ""), "agy --dangerously-skip-permissions");
        assert_eq!(launch_preview("antigravity", "acceptEdits", "m"), "agy --mode accept-edits --model m");
        assert_eq!(launch_preview("codex", "plan", ""), "codex");
    }
}
