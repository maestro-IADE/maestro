//! One agent per worktree, as the rest of Maestro sees it (`Agent`): a facade over its runtime (runtime.rs).
//! Today every agent runs under the PTY runtime (pty.rs): the real CLI in a terminal, the chat drawn from its
//! transcript as common events (events.rs). Also what every runtime shares: the agent's saved meta, the busy/idle
//! state, the brief Maestro gives it, its environment.

use crate::approval::Approvals;
use crate::events::{self, ResetReason};
use crate::pty::PtyRuntime;
use crate::runtime::{self, LaunchCtx, Prompt, PtySnapshot, Readiness, Runtime, ToolMatch};
use crate::stream::Emitter;
use crate::transcript::LiveTail;
use crate::catalog;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter as _};

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
    /// Maestro only: the branch workers start from and get merged back into (None = the repo's default)
    #[serde(default)]
    pub base_branch: Option<String>,
    /// cached one-line summary of the latest turn, and which conversation text it was made from
    pub summary: Option<String>,
    pub summary_key: Option<String>,
}

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

/// What lets the `maestro` CLI inside an agent's terminal reach this app and say who's calling.
fn maestro_env(home: &Path, worktree_path: &str, approvals: &Approvals) -> Vec<(String, String)> {
    let sep = if cfg!(windows) { ";" } else { ":" };
    let path = format!("{}{sep}{}", cli_dir(home).display(), std::env::var("PATH").unwrap_or_default());
    vec![
        ("MAESTRO_URL".into(), approvals.addr()),
        ("MAESTRO_TOKEN".into(), approvals.token().into()),
        ("MAESTRO_WORKTREE".into(), worktree_path.into()),
        ("PATH".into(), path),
    ]
}

/// Tells Claude the `maestro` command exists (appended to its system prompt).
const MAESTRO_HINT: &str = "You are running inside Maestro, a desktop app that runs several coding agents, each in its own git worktree of this project. \
A `maestro` command is on your PATH: `maestro --help` lists it. With it you can see the other worktrees and their agents (`maestro worktree list`), \
start a new worktree with its own agent for a side task (`maestro worktree create --name … --prompt …`), send another agent instructions (`maestro send --to … --text …`), \
and read its status and latest answer (`maestro read --to …`). Use it when the user asks you to delegate, parallelize, or coordinate with other agents. \
`maestro whoami` also shows your parent, your workers, and the reference documents the user linked to you on the canvas: read those documents before starting work. When the user says \"the selected ones\", \"these\", or the like, run `maestro selection` first: it lists the blocks (agents, documents, prompt blocks, and drawn structure: blocks with their notes, areas with what they hold, arrows) they selected on your canvas.";

/// Extra brief for the project's Maestro agent (role "maestro"): what it can do, not how it should coordinate.
/// The how (delegate or not, how to split, which personas) is the playbooks' (`~/.maestro/playbooks`), so the
/// user can change it by editing them.
const COORDINATOR_HINT: &str = "You are this project's Maestro: the coordinator. You run on the project folder itself (its main branch), not in a worktree. \
Your workers are the worktrees linked to you (`maestro whoami` lists them; `maestro worktree list` marks them). \
`maestro worktree create --name … --prompt …` makes a new linked worker, `maestro send --to …` instructs an existing one, \
`maestro ref add --to … --path …` gives a worker a persona or document, `maestro draw --file …` draws a structure (areas, blocks, arrows) on your canvas for the user to see, `maestro read --to …` reads a worker's conversation \
(when a worker finishes a turn, Maestro also pastes a `[maestro] 워커 … 완료` note with its answer into your prompt). Finished branches are merged here.";

/// Everything Maestro tells an agent up front: the `maestro` CLI, the coordinator role, its persona and
/// reference documents. Goes in a file: Claude's `--append-system-prompt-file` (a multi-line argument would be
/// cut at the first newline by `cmd.exe /c`), Antigravity's rules folder (`AGENTS.md`).
pub fn system_brief(home: &Path, meta: &AgentMeta) -> String {
    let mut out = String::from(MAESTRO_HINT);
    if meta.role.as_deref() == Some("maestro") {
        out = format!("{out}\n\n{COORDINATOR_HINT}");
        if let Some(base) = &meta.base_branch {
            out = format!("{out}\n\nBase branch: `{base}`. New workers branch off it (`maestro worktree create` does this by default); merge finished work into `{base}`, not anywhere else.");
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

/// (pid, parent pid, executable name) of every process; None when the list can't be read.
pub fn processes() -> Option<Vec<(u32, u32, String)>> {
    #[cfg(windows)]
    let out = {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", "Get-CimInstance Win32_Process | ForEach-Object { '{0} {1} {2}' -f $_.ProcessId, $_.ParentProcessId, $_.Name }"])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .output()
            .ok()?
    };
    #[cfg(not(windows))]
    let out = std::process::Command::new("ps").args(["-A", "-o", "pid=,ppid=,comm="]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let list: Vec<_> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.collect::<Vec<_>>().join(" ")))
        })
        .collect();
    (!list.is_empty()).then_some(list)
}

/// Shells the agents' tools run in. MCP servers (node, python, cmd wrappers) also live under the CLI, so only these count.
const SHELLS: [&str; 6] = ["bash.exe", "sh.exe", "powershell.exe", "pwsh.exe", "bash", "sh"];

/// Any shell among the descendants of `root` (not `root` itself).
fn shell_below(procs: &[(u32, u32, String)], root: u32) -> bool {
    let mut stack = vec![root];
    let mut seen = std::collections::HashSet::from([root]);
    while let Some(p) = stack.pop() {
        for (pid, ppid, name) in procs {
            if *ppid == p && seen.insert(*pid) {
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
    runtime: Arc<dyn Runtime>,
    app: AppHandle,
    /// `Files::notes` for an agent; None for setup terminals
    pub notes: Option<PathBuf>,
    /// one prompt at a time, so a worker note and a chat message can't interleave
    sending: Mutex<()>,
}

impl Agent {
    pub fn spawn(app: AppHandle, home: &Path, worktree_path: &str, files: &Files, mut meta: AgentMeta, approvals: &Approvals) -> Result<Agent, String> {
        let def = catalog::require(&meta.adapter_id)?;
        if !def.launchable() {
            return Err(format!("{}은(는) 아직 워크트리 에이전트로 실행할 수 없어요", def.name));
        }
        // a missing executable is reported here, before anything starts
        let exe = crate::cli::command(def)?;
        let pa = runtime::pty_agent(&meta.adapter_id);
        let launch = pa.launch(&LaunchCtx { home, worktree: worktree_path, def, approvals }, &mut meta, files, exe)?;
        let state = Arc::new(Mutex::new(AgentState::Idle));

        // Every durable event moves the state dot first, then goes out; a finished turn is reported to whoever
        // leads this agent (delivering blocks until the parent's prompt is up, so on its own thread).
        let (a, p, st) = (app.clone(), worktree_path.to_string(), state.clone());
        let sink_app = app.clone();
        let emitter = Emitter::new(
            worktree_path,
            move |env| {
                let _ = sink_app.emit("agent://event", env);
            },
            move |ev| {
                if let Some(next) = events::status_after(ev) {
                    let done = events::finishes_turn(&st.lock().unwrap(), ev);
                    emit_state(&a, &p, &st, next);
                    if done {
                        let (a, p) = (a.clone(), p.clone());
                        thread::spawn(move || crate::control::worker_done(&a, &p));
                    }
                }
            },
        );
        // a (re)start: whoever shows this worktree's chat reloads it
        emitter.reset(ResetReason::Restart);
        // Primed first so only lines written from now on are emitted live.
        let live = LiveTail::start(pa, home, worktree_path, files.meta.clone(), meta.cli_session_id.clone(), launch.transcript, emitter);
        let (a, p, st) = (app.clone(), worktree_path.to_string(), state.clone());
        let on_exit = Box::new(move || emit_state(&a, &p, &st, AgentState::Terminated));
        let rt = PtyRuntime::start(app.clone(), worktree_path, &launch.argv, Path::new(worktree_path), &maestro_env(home, worktree_path, approvals), Some(live), def.login_hints, launch.resumed, on_exit)?;
        // a restart (after an exit or an away stop) would otherwise still read "off" until its first turn
        let _ = app.emit("agent://state", AgentStateEvent { worktree_path: worktree_path.into(), state: AgentState::Idle });
        Ok(Agent { worktree_path: worktree_path.into(), state, runtime: Arc::new(rt), app, notes: Some(files.notes()), sending: Mutex::new(()) })
    }

    /// A CLI in a terminal with no transcript behind it: settings' setup terminals (install / login).
    pub fn run(app: AppHandle, key: &str, argv: &[String], cwd: &Path, env: &[(String, String)]) -> Result<Agent, String> {
        let state = Arc::new(Mutex::new(AgentState::Idle));
        let (a, p, st) = (app.clone(), key.to_string(), state.clone());
        let on_exit = Box::new(move || emit_state(&a, &p, &st, AgentState::Terminated));
        let rt = PtyRuntime::start(app.clone(), key, argv, cwd, env, None, &[], false, on_exit)?;
        Ok(Agent { worktree_path: key.into(), state, runtime: Arc::new(rt), app, notes: None, sending: Mutex::new(()) })
    }

    pub fn alive(&self) -> bool {
        *self.state.lock().unwrap() != AgentState::Terminated
    }

    /// The runtime driving it, for `runtime::history` (its own record when it keeps one).
    pub fn runtime(&self) -> &dyn Runtime {
        &*self.runtime
    }

    /// The runtime, kept beyond a borrow of this agent (`runtime::snapshot` asks for it on every try).
    pub fn shared_runtime(&self) -> Arc<dyn Runtime> {
        self.runtime.clone()
    }

    /// Blocks until the agent takes prompts. Ok: whether it first shows its "trust this folder?" dialog.
    /// Err: why it didn't come up (exited, wants a login, no prompt in time).
    pub fn wait_ready(&self) -> Result<bool, LaunchFailure> {
        self.runtime.ready().map(|r| r == Readiness::NeedsTrust)
    }

    /// Accept the trust dialog, then wait for the real prompt again.
    pub fn accept_trust(&self) -> Result<(), String> {
        self.runtime.accept_trust()
    }

    /// Raw input for the CLI view's terminal.
    pub fn write(&self, data: &[u8]) -> Result<(), String> {
        self.runtime.terminal().ok_or("터미널이 없는 에이전트예요")?.write(data)
    }

    /// A chat message (attachments become @mentions), one at a time.
    pub fn send(&self, text: &str, files: &[String]) -> Result<(), String> {
        let _one = self.sending.lock().unwrap_or_else(|e| e.into_inner());
        // ConPTY still takes writes after the CLI exits, so a paste into a dead agent would look sent
        if !self.alive() {
            return Err("에이전트가 꺼져 있어요".into());
        }
        self.runtime.prompt(&Prompt { text: text.into(), files: files.to_vec() })?;
        emit_state(&self.app, &self.worktree_path, &self.state, AgentState::Running);
        Ok(())
    }

    /// Pastes the worker notes kept while this agent was off or at the trust dialog, once its prompt is up.
    pub fn deliver_kept_notes(self: Arc<Self>) {
        let Some(file) = self.notes.clone().filter(|f| f.exists()) else { return };
        thread::spawn(move || {
            // only once its prompt is up; at the trust dialog or after a failed start the notes stay
            if self.wait_ready() == Ok(false) && self.alive() {
                let _ = crate::control::pass_notes(&file, None, Some(&|t: &str| self.send(t, &[])));
            }
        });
    }

    /// Stops the current turn; the agent stays up.
    pub fn interrupt(&self) -> Result<(), String> {
        self.runtime.cancel()
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        self.runtime.terminal().map_or(Ok(()), |t| t.resize(cols, rows))
    }

    pub fn snapshot(&self) -> PtySnapshot {
        self.runtime.terminal().map(|t| t.snapshot()).unwrap_or_default()
    }

    /// Time since the agent last showed anything (zero while waiting for a redraw).
    pub fn quiet_for(&self) -> Duration {
        self.runtime.quiet_for()
    }

    /// A shell is running somewhere under the CLI: a Bash tool call, likely a background one, still going.
    pub fn runs_shell(&self, procs: &[(u32, u32, String)]) -> bool {
        self.runtime.pid().is_some_and(|pid| shell_below(procs, pid))
    }

    /// Kills the whole process tree.
    pub fn kill(&self) {
        self.runtime.kill()
    }

    /// Reads what the agent wrote so far now (its Stop hook fired) instead of at the next tick.
    pub fn sync(&self) {
        self.runtime.sync()
    }

    /// The agent moved to another conversation (`/clear`): the chat follows it.
    pub fn follow(&self, conversation: &str, transcript: Option<&Path>) {
        self.runtime.follow(conversation, transcript)
    }

    /// The tool call a permission prompt is about (`tool_call_id`: the one the prompt names, if any).
    pub fn locate_tool(&self, name: &str, input: &Value, tool_call_id: Option<&str>) -> ToolMatch {
        self.runtime.locate_tool(name, input, tool_call_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_host_session_env_only_inside_claude() {
        let vars = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>().into_iter();
        assert!(host_session_env(vars(&["NO_COLOR", "CLAUDE_CODE_SESSION_ID"])).is_empty());
        let dropped = host_session_env(vars(&["CLAUDECODE", "NO_COLOR", "CLAUDE_CODE_SESSION_ID", "CLAUDE_AGENT_SDK_VERSION", "CLAUDE_CODE_GIT_BASH_PATH", "PATH"]));
        assert_eq!(dropped, ["CLAUDECODE", "NO_COLOR", "CLAUDE_CODE_SESSION_ID", "CLAUDE_AGENT_SDK_VERSION"]);
    }

    #[test]
    fn finds_shells_below_the_cli_only() {
        let p = |pid, ppid, n: &str| (pid, ppid, n.to_string());
        // cmd (root) → node → { mcp node, bash }; an unrelated bash elsewhere
        let procs = vec![p(10, 1, "cmd.exe"), p(11, 10, "node.exe"), p(12, 11, "node.exe"), p(13, 11, "bash.exe"), p(20, 1, "bash.exe")];
        assert!(shell_below(&procs, 10));
        assert!(!shell_below(&procs[..3], 10));
        assert!(!shell_below(&procs, 12));
        // a cycle in stale pid data must not loop forever
        assert!(!shell_below(&[p(1, 2, "node"), p(2, 1, "node")], 1));
    }

    #[test]
    #[ignore = "reads the live process list"]
    fn lists_processes_live() {
        let procs = processes().expect("process list");
        assert!(procs.iter().any(|(pid, _, _)| *pid == std::process::id()));
    }

    #[test]
    fn brief_carries_persona_and_references() {
        let home = std::env::temp_dir().join(format!("maestro-brief-{}", std::process::id()));
        crate::library::seed(&home);
        let persona = crate::library::personas_dir(&home).join("reviewer.md").to_string_lossy().replace('\\', "/");
        let doc = home.join("spec.md");
        std::fs::write(&doc, "# 결제 스펙\n...").unwrap();
        let m = AgentMeta { refs: vec![persona, doc.to_string_lossy().replace('\\', "/")], role: Some("maestro".into()), ..Default::default() };
        let b = system_brief(&home, &m);
        assert!(b.contains(COORDINATOR_HINT));
        // the coordination policy comes from the playbooks, with the default one listed
        assert!(b.contains("## Playbooks") && b.contains("- 기본 지휘 ("));
        assert!(b.contains("## Your persona") && b.contains("# 리뷰어"));
        assert!(b.contains("- 결제 스펙 ("));
        assert_eq!(system_brief(&home, &AgentMeta::default()), MAESTRO_HINT);
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
