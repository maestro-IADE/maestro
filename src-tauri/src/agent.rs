//! One real Claude Code TUI per agent block, running in a PTY.
//! - CLI view: that terminal, as is.
//! - Chat view: drawn from the session file Claude writes as it goes
//!   (`~/.claude/projects/<cwd slug>/<session id>.jsonl`); chat input is pasted into the same terminal.
//! So both views share one process and one conversation.

use crate::antigravity;
use crate::approval::Approvals;
use crate::catalog;
use std::collections::VecDeque;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

const BATCH_WINDOW: Duration = Duration::from_millis(16);
const BATCH_MAX: usize = 64 * 1024;
const SCROLLBACK_MAX: usize = 1024 * 1024;
const TAIL_EVERY: Duration = Duration::from_millis(250);

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

/// Where Claude Code keeps the transcript of a session started in `cwd`.
/// shortcut: Claude shortens very long slugs (>200 chars) with a hash; not handled, worktree paths stay short.
pub fn session_file(home: &Path, cwd: &str, session_id: &str) -> PathBuf {
    let slug: String = cwd.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    home.join(".claude").join("projects").join(slug).join(format!("{session_id}.jsonl"))
}

/// Random v4 UUID without a crate: std's RandomState is seeded randomly per instance.
fn new_session_id() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::BuildHasher;
    let a = RandomState::new().hash_one(std::time::SystemTime::now());
    let b = RandomState::new().hash_one(std::process::id());
    let mut x = ((a as u128) << 64) | b as u128;
    x = (x & !(0xF << 76)) | (0x4 << 76); // version 4
    x = (x & !(0x3 << 62)) | (0x2 << 62); // RFC 4122 variant
    let h = format!("{x:032x}");
    format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
}

fn between<'a>(s: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = s.find(open)? + open.len();
    Some(&s[start..start + s[start..].find(close)?])
}

/// One session-file entry → the chat events the UI renders. Bookkeeping lines (hooks, snapshots, queue ops) give nothing.
pub fn to_events(e: &Value) -> Vec<Value> {
    match e["type"].as_str() {
        Some("assistant") => vec![json!({ "type": "assistant", "message": { "content": e["message"]["content"], "stop_reason": e["message"]["stop_reason"] } })],
        Some("user") if e["isMeta"] != true => {
            let c = &e["message"]["content"];
            if let Some(s) = c.as_str() {
                // a slash command typed in the TUI is stored as tagged text
                if let Some(name) = between(s, "<command-name>", "</command-name>") {
                    let args = between(s, "<command-args>", "</command-args>").unwrap_or("");
                    return vec![json!({ "type": "user_input", "text": format!("{name} {args}").trim() })];
                }
                // a slash command's output arrives as a user entry, not a system one
                if s.contains("<local-command-stdout>") || s.contains("<local-command-stderr>") {
                    let out = [between(s, "<local-command-stdout>", "</local-command-stdout>"), between(s, "<local-command-stderr>", "</local-command-stderr>")];
                    let text = out.iter().flatten().map(|t| t.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n");
                    return if text.is_empty() { vec![] } else { vec![json!({ "type": "local_command", "text": text })] };
                }
                // `!` bash mode: the command and its output are stored as tagged text too
                if let Some(cmd) = between(s, "<bash-input>", "</bash-input>") {
                    return vec![json!({ "type": "user_input", "text": format!("!{cmd}") })];
                }
                if s.contains("<bash-stdout>") || s.contains("<bash-stderr>") {
                    let out = [between(s, "<bash-stdout>", "</bash-stdout>"), between(s, "<bash-stderr>", "</bash-stderr>")];
                    let text = out.iter().flatten().map(|t| t.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n");
                    return vec![json!({ "type": "local_command", "text": if text.is_empty() { "(출력 없음)".into() } else { text } })];
                }
                return vec![json!({ "type": "user_input", "text": s })];
            }
            let blocks = c.as_array().cloned().unwrap_or_default();
            if blocks.iter().any(|b| b["type"] == "tool_result") {
                return vec![json!({ "type": "user", "message": { "content": c } })];
            }
            let text: Vec<&str> = blocks.iter().filter_map(|b| b["text"].as_str()).collect();
            let text = text.join("\n");
            if text.starts_with("[Request interrupted") {
                vec![json!({ "type": "interrupted" })]
            } else if text.trim().is_empty() {
                vec![]
            } else {
                vec![json!({ "type": "user_input", "text": text })]
            }
        }
        Some("system") if e["subtype"] == "local_command" => {
            let s = e["content"].as_str().unwrap_or("");
            let out = between(s, "<local-command-stdout>", "</local-command-stdout>").unwrap_or(s).trim();
            if out.is_empty() { vec![] } else { vec![json!({ "type": "local_command", "text": out })] }
        }
        _ => vec![],
    }
}

/// How an event moves the busy/idle dot.
fn state_after(ev: &Value) -> Option<AgentState> {
    match ev["type"].as_str() {
        Some("user_input") => Some(AgentState::Running),
        Some("interrupted") | Some("local_command") => Some(AgentState::Idle),
        Some("assistant") => match ev["message"]["stop_reason"].as_str() {
            Some("tool_use") | None => None,
            Some(_) => Some(AgentState::Idle), // end_turn, max_tokens, …
        },
        _ => None,
    }
}

/// Incremental reader of a session file. Only whole lines are consumed, so the history
/// read (`read_all`) and the live tail always agree on event numbering.
pub struct Tail {
    path: PathBuf,
    offset: u64,
    pub count: u64,
    adapter: String,
    /// antigravity: tool calls still waiting for their output step
    pending: VecDeque<String>,
}

impl Tail {
    pub fn new(path: PathBuf, adapter: &str) -> Tail {
        Tail { path, offset: 0, count: 0, adapter: adapter.into(), pending: VecDeque::new() }
    }

    pub fn read_new(&mut self) -> Vec<Value> {
        let Ok(mut f) = std::fs::File::open(&self.path) else { return vec![] };
        let mut buf = Vec::new();
        if f.seek(SeekFrom::Start(self.offset)).is_err() || f.read_to_end(&mut buf).is_err() {
            return vec![];
        }
        let Some(end) = buf.iter().rposition(|&b| b == b'\n') else { return vec![] };
        self.offset += end as u64 + 1;
        let mut events = vec![];
        for e in buf[..end].split(|&b| b == b'\n').filter_map(|l| serde_json::from_slice::<Value>(l).ok()) {
            events.extend(if self.adapter == antigravity::ID { antigravity::to_events(&e, &mut self.pending) } else { to_events(&e) });
        }
        self.count += events.len() as u64;
        events
    }

    pub fn read_all(path: PathBuf, adapter: &str) -> Vec<Value> {
        Tail::new(path, adapter).read_new()
    }
}

/// The conversation id for a worktree: the saved one, or (Antigravity) the one agy has recorded for the folder.
pub fn conversation_id(meta: &AgentMeta, home: &Path, cwd: &str) -> Option<String> {
    meta.cli_session_id.clone().or_else(|| (meta.adapter_id == antigravity::ID).then(|| antigravity::conversation_for(home, cwd)).flatten())
}

/// Where an agent's CLI keeps the conversation the chat view is drawn from.
pub fn transcript_file(adapter: &str, home: &Path, cwd: &str, session_id: &str) -> PathBuf {
    if adapter == antigravity::ID { antigravity::transcript(home, session_id) } else { session_file(home, cwd, session_id) }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AgentEvent {
    worktree_path: String,
    seq: u64,
    event: Value,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AgentStateEvent {
    worktree_path: String,
    state: AgentState,
}

/// Event "pty://data". `seq` = total bytes so far, so a freshly mounted terminal can drop
/// live batches its scrollback snapshot already holds.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PtyData {
    worktree_path: String,
    data_b64: String,
    seq: u64,
}

#[derive(Default)]
pub struct Scrollback {
    pub buf: Vec<u8>,
    pub total: u64,
    last: Option<Instant>,
    /// Some(asks_trust) once the TUI has drawn and gone quiet; decided once, so later chat text
    /// that happens to say "trust" can't block sending. Failures aren't kept: a retry looks again.
    ready: Option<bool>,
    /// absolute byte offset; the trust-prompt and login checks only look at output after it
    mark: u64,
    /// a login screen was reported and `mark` moved past it: until new output comes, it's still showing
    login_shown: bool,
    /// the TUI turned on bracketed paste (`ESC[?2004h`), i.e. its input is up. Startup has multi-second
    /// silent gaps before that, so silence alone doesn't mean ready.
    input_on: bool,
    /// started on an existing session (`--resume`, `--conversation`): the TUI redraws the old conversation,
    /// which may itself mention logging in
    pub resumed: bool,
}

/// ConPTY asks for the cursor position (`ESC[6n`) at startup and the TUI doesn't start until someone
/// answers. xterm does when the CLI view is open; with only the chat view open nobody would.
const CURSOR_QUERY: &[u8] = b"\x1b[6n";
const CURSOR_REPLY: &[u8] = b"\x1b[1;1R";
const PASTE_ON: &[u8] = b"\x1b[?2004h";

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// Why an agent's TUI didn't come up, for the UI to say. A missing executable is caught before the start
/// (`cli::command`), so it arrives as a plain start error instead.
#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LaunchFailure {
    /// its startup output asks for a login or its first-run setup
    LoginRequired { message: String },
    /// it exited while starting
    Exited { message: String },
    /// no input prompt (`ESC[?2004h`) within `STARTUP_LIMIT`
    Timeout { message: String },
}

impl LaunchFailure {
    pub fn message(&self) -> &str {
        match self {
            LaunchFailure::LoginRequired { message } | LaunchFailure::Exited { message } | LaunchFailure::Timeout { message } => message,
        }
    }
}

const STARTUP_LIMIT: Duration = Duration::from_secs(45);

fn login_required() -> LaunchFailure {
    LaunchFailure::LoginRequired { message: "로그인(또는 첫 실행 설정)이 필요해요. CLI 보기에서 마치고 다시 시도하거나, 설정 → 에이전트의 터미널에서 로그인해 주세요.".into() }
}

/// TUI output as text: escape sequences (CSI, OSC) and control characters dropped, lines kept.
fn plain(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes);
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\x1b' => match it.next() {
                Some('[') => while it.next().is_some_and(|c| !('@'..='~').contains(&c)) {},
                Some(']') => {
                    while let Some(c) = it.next() {
                        if c == '\x07' || (c == '\x1b' && it.next_if_eq(&'\\').is_some()) {
                            break;
                        }
                    }
                }
                _ => {}
            },
            '\n' => out.push('\n'),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// Classifies a start that went wrong from what the TUI printed. `exited`: the process is gone; otherwise it's
/// still up and showing `text` (a login screen, or nothing usable before the time limit).
fn classify(text: &str, exited: bool, login_hints: &[&str]) -> Option<LaunchFailure> {
    let lower = text.to_lowercase();
    let last: String = text.lines().map(str::trim).filter(|l| !l.is_empty()).last().unwrap_or("").chars().take(200).collect();
    if login_hints.iter().any(|h| lower.contains(h)) {
        return Some(login_required());
    }
    if exited {
        let why = if last.is_empty() { String::new() } else { format!(" 마지막 출력: {last}") };
        return Some(LaunchFailure::Exited { message: format!("에이전트가 시작하다 종료됐어요.{why}") });
    }
    None
}

impl Scrollback {
    /// Output since `mark`, at most the last 8 KB.
    fn since_mark(&self) -> &[u8] {
        let since = (self.total - self.mark).min(self.buf.len() as u64).min(8192) as usize;
        &self.buf[self.buf.len() - since..]
    }

    /// One look at a starting TUI that's still running: Some once decided, None to keep waiting. Ok(asks_trust)
    /// is kept (`ready`); a failure isn't, so a retry looks again.
    fn startup(&mut self, timed_out: bool, login_hints: &[&str]) -> Option<Result<bool, LaunchFailure>> {
        if let Some(trust) = self.ready {
            return Some(Ok(trust));
        }
        let quiet = self.last.is_some_and(|t| t.elapsed() > Duration::from_millis(800));
        if !quiet && !timed_out {
            return None;
        }
        // the login screen reported last time, nothing new since: still showing
        if self.login_shown && self.total == self.mark {
            return Some(Err(login_required()));
        }
        // a login screen may never turn input on, so it's looked for whenever output pauses. Not once a resumed
        // session's input is up: its redrawn history can't be told from a login screen, and a lapsed login
        // shows up on the first turn anyway.
        let history = self.resumed && self.input_on;
        if let Some(f) = classify(&plain(self.since_mark()), false, login_hints).filter(|_| !history) {
            // later looks only read what comes after it: the user may log in right there in the CLI view
            self.mark = self.total;
            self.login_shown = true;
            return Some(Err(f));
        }
        // input up but never quiet (something animating) still counts once the time is up
        if self.input_on {
            let trust = self.asks_trust();
            self.ready = Some(trust);
            return Some(Ok(trust));
        }
        timed_out.then(|| {
            Err(LaunchFailure::Timeout { message: format!("{}초 안에 입력창이 뜨지 않았어요. CLI 보기에서 화면을 확인해 주세요.", STARTUP_LIMIT.as_secs()) })
        })
    }

    /// Output since `mark` mentions trust → Claude's first-run "trust this folder?" dialog.
    /// shortcut: plain text match on raw TUI output; switch to a screen parser if the wording changes.
    fn asks_trust(&self) -> bool {
        String::from_utf8_lossy(self.since_mark()).to_lowercase().contains("trust")
    }

    fn push(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
        self.total += data.len() as u64;
        self.last = Some(Instant::now());
        self.input_on |= contains(data, PASTE_ON);
        if self.buf.len() > SCROLLBACK_MAX {
            // shortcut: may cut an escape sequence at the front; the TUI redraws on the next resize anyway
            let cut = self.buf.len() - SCROLLBACK_MAX;
            self.buf.drain(..cut);
        }
    }
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
fn host_session_env(vars: impl Iterator<Item = String>) -> Vec<String> {
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

/// The most recent answer as plain text for the summarizer: the prompt it answered, the agent's replies
/// up to that answer, and the tools it used. A turn still in progress, or slash commands typed after the
/// answer, don't count, so the summary stays put until a new answer lands. Keeps the tail when long.
pub fn recent_turn_text(events: &[Value], max_chars: usize) -> String {
    let has_text = |e: &Value| {
        e["type"] == "assistant" && e["message"]["content"].as_array().is_some_and(|c| c.iter().any(|b| b["type"] == "text" && b["text"].as_str().is_some_and(|t| !t.trim().is_empty())))
    };
    let Some(end) = events.iter().rposition(has_text) else { return String::new() };
    let start = events[..end].iter().rposition(|e| e["type"] == "user_input").unwrap_or(0);
    let mut out = String::new();
    let mut tools: Vec<String> = vec![];
    for e in &events[start..=end] {
        match e["type"].as_str() {
            Some("user_input") => out.push_str(&format!("요청: {}\n", e["text"].as_str().unwrap_or(""))),
            Some("assistant") => {
                for b in e["message"]["content"].as_array().into_iter().flatten() {
                    match b["type"].as_str() {
                        Some("text") => out.push_str(&format!("결과: {}\n", b["text"].as_str().unwrap_or(""))),
                        Some("tool_use") => tools.push(b["name"].as_str().unwrap_or("?").into()),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    if !tools.is_empty() {
        tools.dedup();
        out.push_str(&format!("(사용한 도구: {})\n", tools.join(", ")));
    }
    let chars: Vec<char> = out.trim().chars().collect();
    chars[chars.len().saturating_sub(max_chars)..].iter().collect()
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
    if agent == antigravity::ID {
        antigravity::ask(exe, &format!("{SUMMARY_PROMPT}\n\n---\n{conversation}"), model)
    } else {
        summarize_with_claude(exe, conversation, model)
    }
}

/// One cheap Claude call: no tools, MCP, slash commands or settings hooks, a tiny system prompt,
/// nothing saved to disk (≈700 tokens with Haiku, well under a cent).
fn summarize_with_claude(exe: Vec<String>, conversation: &str, model: &str) -> Result<String, String> {
    use std::process::{Command, Stdio};
    let system = SUMMARY_PROMPT;
    let mut a = exe;
    a.extend(["-p", "--model", model, "--no-session-persistence", "--setting-sources", "local", "--strict-mcp-config", "--disable-slash-commands", "--tools", "", "--output-format", "json", "--system-prompt", system].map(String::from));
    let mut cmd = Command::new(&a[0]);
    cmd.args(&a[1..]).current_dir(std::env::temp_dir()).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    for k in host_session_env(std::env::vars().map(|(k, _)| k)) {
        cmd.env_remove(k);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().map_err(|e| format!("요약 실행 실패: {e}"))?;
    child.stdin.take().unwrap().write_all(conversation.as_bytes()).map_err(|e| e.to_string())?; // dropped → EOF
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_slice(&out.stdout).map_err(|_| "요약 응답을 읽지 못했어요".to_string())?;
    match v["result"].as_str() {
        Some(s) if v["is_error"] != true => Ok(s.trim().to_string()),
        _ => Err("요약 실패".into()),
    }
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

/// Claude Code's command line (Antigravity's is in antigravity.rs).
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

/// Claude Code's arguments after `exe` (what `cli::command` resolved to start it).
fn argv(exe: Vec<String>, meta: &AgentMeta, session_id: &str, resume: bool, settings: &Path, brief_file: &Path) -> Vec<String> {
    let mut a = exe;
    a.extend([if resume { "--resume" } else { "--session-id" }.into(), session_id.into()]);
    a.extend(["--permission-mode".into(), meta.permission_mode.clone()]);
    a.extend(["--settings".into(), settings.to_string_lossy().into()]);
    a.extend(["--append-system-prompt-file".into(), brief_file.to_string_lossy().into()]);
    if let Some(m) = &meta.model {
        a.extend(["--model".into(), m.clone()]);
    }
    a
}

/// The command a new worktree with this agent would run, for the settings tile: the real argv without the
/// per-worktree parts (session, hook settings, brief) and with the bare executable name.
pub fn launch_preview(adapter: &str, permission_mode: &str, model: &str) -> String {
    let Some(def) = catalog::get(adapter) else { return adapter.into() };
    let model = Some(model).filter(|m| !m.is_empty());
    let a = if adapter == antigravity::ID {
        let mut a = antigravity::argv(vec![def.bin.into()], permission_mode, None, model, Path::new(""));
        a.drain(1..3); // --add-dir <rules>
        a
    } else if def.launchable() {
        let meta = AgentMeta { permission_mode: permission_mode.into(), model: model.map(String::from), ..Default::default() };
        let mut a = argv(vec![def.bin.into()], &meta, "", false, Path::new(""), Path::new(""));
        a.drain(1..3); // --session-id <id>
        a.retain(|x| !x.is_empty() && x != "--settings" && x != "--append-system-prompt-file");
        a
    } else {
        vec![def.bin.into()]
    };
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

pub struct Agent {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    scrollback: Arc<Mutex<Scrollback>>,
    pub state: Arc<Mutex<AgentState>>,
    app: AppHandle,
    worktree_path: String,
    /// `Files::notes` for an agent; None for setup terminals
    notes: Option<PathBuf>,
    /// one paste-and-Enter at a time, so a worker note and a chat message can't interleave
    sending: Mutex<()>,
    /// the catalog's startup texts that mean "log in first"; none for setup terminals
    login_hints: &'static [&'static str],
}

impl Agent {
    pub fn spawn(app: AppHandle, home: &Path, worktree_path: &str, files: &Files, mut meta: AgentMeta, approvals: &Approvals) -> Result<Agent, String> {
        let adapter = meta.adapter_id.clone();
        let def = catalog::require(&adapter)?;
        if !def.launchable() {
            return Err(format!("{}은(는) 아직 워크트리 에이전트로 실행할 수 없어요", def.name));
        }
        // a missing executable is reported here, before anything starts
        let exe = crate::cli::command(def)?;
        // the transcript to tail; None = an Antigravity conversation not created yet (found once it is)
        let (argv, session, resumed) = if adapter == antigravity::ID {
            // agy may already keep a conversation for this worktree (e.g. one started before we tracked it)
            if meta.cli_session_id.is_none() {
                if let Some(id) = antigravity::conversation_for(home, worktree_path) {
                    meta.cli_session_id = Some(id);
                    files.write_meta(&meta)?;
                }
            }
            let id = meta.cli_session_id.clone();
            // the same brief Claude gets, as rules agy reads from a folder outside the worktree (nothing to commit by mistake)
            let rules = files.meta.with_extension("brief");
            std::fs::create_dir_all(&rules).map_err(|e| e.to_string())?;
            std::fs::write(rules.join("AGENTS.md"), system_brief(home, &meta)).map_err(|e| e.to_string())?;
            let argv = antigravity::argv(exe, &meta.permission_mode, id.as_deref(), meta.model.as_deref(), &rules);
            (argv, id.as_ref().map(|id| antigravity::transcript(home, id)), id.is_some())
        } else {
            let session_id = match &meta.cli_session_id {
                Some(id) => id.clone(),
                None => {
                    let id = new_session_id();
                    meta.cli_session_id = Some(id.clone());
                    files.write_meta(&meta)?;
                    id
                }
            };
            let session = session_file(home, worktree_path, &session_id);
            // permission prompts go to the chat view (approval.rs); rewritten each start since the port changes per run
            let settings = files.meta.with_extension("settings.json");
            approvals.write_settings(&settings, &session_id)?;
            approvals.register(&session_id, worktree_path);
            // Claude writes the file on the first message; before that the id is still free to claim
            let brief = files.meta.with_extension("brief.md");
            std::fs::write(&brief, system_brief(home, &meta)).map_err(|e| e.to_string())?;
            let resumed = session.exists();
            (argv(exe, &meta, &session_id, resumed, &settings, &brief), Some(session), resumed)
        };
        let mut agent = Agent::run(app.clone(), worktree_path, &argv, Path::new(worktree_path), &maestro_env(home, worktree_path, approvals))?;
        agent.notes = Some(files.notes());
        agent.login_hints = def.login_hints;
        agent.scrollback.lock().unwrap().resumed = resumed;
        // a restart (after an exit or an away stop) would otherwise still read "off" until its first turn
        let _ = app.emit("agent://state", AgentStateEvent { worktree_path: worktree_path.into(), state: AgentState::Idle });

        // Session-file tail → chat events + busy/idle. Primed first so only new lines are emitted live.
        let (a, p, st, home, meta_file) = (app.clone(), agent.worktree_path.clone(), agent.state.clone(), home.to_path_buf(), files.meta.clone());
        thread::spawn(move || {
            let mut tail = session.map(|s| {
                let mut t = Tail::new(s, &adapter);
                t.read_new();
                t
            });
            while *st.lock().unwrap() != AgentState::Terminated {
                thread::sleep(TAIL_EVERY);
                let tail = match &mut tail {
                    Some(t) => t,
                    None => {
                        // new Antigravity conversation: adopt the one agy records for this worktree, and remember it for --conversation
                        let Some(id) = antigravity::conversation_for(&home, &p) else { continue };
                        let f = Files { meta: meta_file.clone() };
                        if let Some(mut m) = f.read_meta() {
                            m.cli_session_id = Some(id.clone());
                            let _ = f.write_meta(&m);
                        }
                        tail.insert(Tail::new(antigravity::transcript(&home, &id), &adapter))
                    }
                };
                let base = tail.count;
                for (i, ev) in tail.read_new().into_iter().enumerate() {
                    if let Some(next) = state_after(&ev) {
                        // a finished turn (not an Esc or a slash command) is reported to whoever leads this agent
                        let done = next == AgentState::Idle && ev["type"] == "assistant" && *st.lock().unwrap() == AgentState::Running;
                        emit_state(&a, &p, &st, next);
                        if done {
                            let (a, p) = (a.clone(), p.clone());
                            thread::spawn(move || crate::control::worker_done(&a, &p)); // delivering blocks until the parent's prompt is up
                        }
                    }
                    let _ = a.emit("agent://event", AgentEvent { worktree_path: p.clone(), seq: base + i as u64 + 1, event: ev });
                }
            }
        });

        Ok(agent)
    }

    /// A CLI in a PTY: output batched to `pty://data` under `key`, cursor queries answered, exit noticed.
    /// Agents add their transcript tail on top; setup terminals (settings → install / login) use it as is.
    pub fn run(app: AppHandle, key: &str, argv: &[String], cwd: &Path, env: &[(String, String)]) -> Result<Agent, String> {
        let pair = native_pty_system()
            .openpty(PtySize { rows: 32, cols: 110, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| e.to_string())?;
        let mut cmd = CommandBuilder::new(&argv[0]);
        cmd.args(&argv[1..]);
        cmd.cwd(cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        for (k, v) in env {
            cmd.env(k, v);
        }
        for k in host_session_env(std::env::vars().map(|(k, _)| k)) {
            cmd.env_remove(k);
        }
        let child = pair.slave.spawn_command(cmd).map_err(|e| format!("에이전트 실행 실패: {e}"))?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pair.master.take_writer().map_err(|e| e.to_string())?;

        let scrollback = Arc::new(Mutex::new(Scrollback::default()));
        let child = Arc::new(Mutex::new(child));
        let state = Arc::new(Mutex::new(AgentState::Idle));
        let path = key.to_string();

        // Blocking PTY reads on their own thread; a batcher turns them into one IPC event per 16 ms.
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        thread::spawn(move || {
            let mut buf = [0u8; 8192];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
        });
        let writer = Arc::new(Mutex::new(writer));
        let (sb, a, p, w) = (scrollback.clone(), app.clone(), path.clone(), writer.clone());
        thread::spawn(move || {
            while let Ok(mut batch) = rx.recv() {
                let deadline = Instant::now() + BATCH_WINDOW;
                while batch.len() < BATCH_MAX {
                    match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                        Ok(c) => batch.extend(c),
                        Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
                // the CLI view's xterm answers too; it drops its own reply (XTerm.tsx) so the TUI gets one
                if contains(&batch, CURSOR_QUERY) {
                    let mut w = w.lock().unwrap();
                    let _ = w.write_all(CURSOR_REPLY).and_then(|_| w.flush());
                }
                let seq = {
                    let mut s = sb.lock().unwrap();
                    s.push(&batch);
                    s.total
                };
                let _ = a.emit("pty://data", PtyData { worktree_path: p.clone(), data_b64: B64.encode(&batch), seq });
            }
        });

        // ConPTY may keep the read pipe open after the child exits, so poll the child instead of waiting for EOF.
        let (a, p, st, ch) = (app.clone(), path.clone(), state.clone(), child.clone());
        thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(500));
            let status = ch.lock().unwrap().try_wait();
            match status {
                Ok(None) => continue,
                Ok(Some(_)) => {
                    emit_state(&a, &p, &st, AgentState::Terminated);
                    break;
                }
                Err(_) => break,
            }
        });

        Ok(Agent { writer, master: Mutex::new(pair.master), child, scrollback, state, app, worktree_path: path, notes: None, sending: Mutex::new(()), login_hints: &[] })
    }

    pub fn alive(&self) -> bool {
        *self.state.lock().unwrap() != AgentState::Terminated
    }

    /// Blocks until the TUI's input is up and its first draw has settled, so pasted input lands in its prompt.
    /// Ok: whether the TUI is now showing its "trust this folder?" dialog instead of the prompt. Err: why it
    /// didn't come up (exited, wants a login, no prompt in time).
    pub fn wait_ready(&self) -> Result<bool, LaunchFailure> {
        let start = Instant::now();
        while self.alive() {
            if let Some(r) = self.scrollback.lock().unwrap().startup(start.elapsed() > STARTUP_LIMIT, self.login_hints) {
                return r;
            }
            thread::sleep(Duration::from_millis(100));
        }
        let s = self.scrollback.lock().unwrap();
        if s.login_shown && s.total == s.mark {
            return Err(login_required());
        }
        Err(classify(&plain(s.since_mark()), true, self.login_hints).expect("an exit is always classified"))
    }

    /// Accept the trust dialog (its default choice is "yes"), then wait for the real prompt again.
    pub fn accept_trust(&self) -> Result<(), String> {
        {
            let mut s = self.scrollback.lock().unwrap();
            s.ready = None;
            s.mark = s.total;
            s.last = None; // wait for the redraw the Enter causes
        }
        self.write(b"\r")
    }

    pub fn write(&self, data: &[u8]) -> Result<(), String> {
        let mut w = self.writer.lock().unwrap();
        w.write_all(data).and_then(|_| w.flush()).map_err(|e| format!("에이전트에 전송 실패: {e}"))
    }

    /// A chat message: bracketed paste (keeps newlines as text), then Enter. Attachments become @mentions.
    /// A leading `!` is typed as a key first, since only a typed `!` switches the TUI into bash mode.
    pub fn send(&self, text: &str, files: &[String]) -> Result<(), String> {
        let _one = self.sending.lock().unwrap_or_else(|e| e.into_inner());
        // ConPTY still takes writes after the CLI exits, so a paste into a dead agent would look sent
        if !self.alive() {
            return Err("에이전트가 꺼져 있어요".into());
        }
        let mut body = text.to_string();
        // A bare `/cmd` leaves the TUI's autocomplete open, and Enter then completes it again (`/model/model`).
        // A trailing space puts the TUI in argument mode, so Enter runs the command.
        if text.starts_with('/') && !text.contains(char::is_whitespace) {
            body.push(' ');
        }
        if let Some(rest) = text.strip_prefix('!') {
            self.write(b"!")?;
            thread::sleep(Duration::from_millis(80));
            body = rest.to_string();
        }
        for f in files {
            body.push_str(&format!(" @{f}"));
        }
        self.write(format!("\x1b[200~{body}\x1b[201~").as_bytes())?;
        thread::sleep(Duration::from_millis(60)); // let the TUI finish taking the paste before Enter
        self.write(b"\r")?;
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

    /// Esc: interrupts the current turn in the TUI; the process stays up.
    pub fn interrupt(&self) -> Result<(), String> {
        self.write(b"\x1b")
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        self.master.lock().unwrap()
            .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| e.to_string())
    }

    pub fn snapshot(&self) -> (String, u64) {
        let s = self.scrollback.lock().unwrap();
        (B64.encode(&s.buf), s.total)
    }

    /// Time since the TUI last drew anything (zero while waiting for a redraw).
    pub fn quiet_for(&self) -> Duration {
        self.scrollback.lock().unwrap().last.map_or(Duration::ZERO, |t| t.elapsed())
    }

    /// A shell is running somewhere under the CLI: a Bash tool call, likely a background one, still going.
    pub fn runs_shell(&self, procs: &[(u32, u32, String)]) -> bool {
        self.child.lock().unwrap().process_id().is_some_and(|pid| shell_below(procs, pid))
    }

    /// Kills the whole process tree (cmd.exe → node → tools on Windows).
    pub fn kill(&self) {
        let mut child = self.child.lock().unwrap();
        #[cfg(windows)]
        if let Some(pid) = child.process_id() {
            use std::os::windows::process::CommandExt;
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
                .output();
        }
        let _ = child.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_file_matches_claude_slug() {
        let p = session_file(Path::new("/h"), "C:/Users/u/.maestro/projects/demo_1a/worktrees/agent-1", "abc");
        assert!(p.ends_with(".claude/projects/C--Users-u--maestro-projects-demo-1a-worktrees-agent-1/abc.jsonl"));
    }

    #[test]
    fn session_ids_are_v4_uuids() {
        let (a, b) = (new_session_id(), new_session_id());
        assert_ne!(a, b);
        let parts: Vec<_> = a.split('-').map(str::len).collect();
        assert_eq!(parts, [8, 4, 4, 4, 12]);
        assert_eq!(&a[14..15], "4");
        assert!("89ab".contains(&a[19..20]));
    }

    #[test]
    fn converts_session_entries() {
        let user = json!({"type": "user", "message": {"content": [{"type": "text", "text": "안녕"}]}});
        assert_eq!(to_events(&user), [json!({"type": "user_input", "text": "안녕"})]);
        assert!(to_events(&json!({"type": "user", "isMeta": true, "message": {"content": "x"}})).is_empty());
        let cmd = json!({"type": "user", "message": {"content": "<command-name>/model</command-name>\n<command-args>opus</command-args>"}});
        assert_eq!(to_events(&cmd)[0]["text"], "/model opus");
        let tr = json!({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": "t"}]}});
        assert_eq!(to_events(&tr)[0]["type"], "user");
        let out = json!({"type": "system", "subtype": "local_command", "content": "<local-command-stdout>hi</local-command-stdout>"});
        assert_eq!(to_events(&out), [json!({"type": "local_command", "text": "hi"})]);
        assert!(to_events(&json!({"type": "queue-operation"})).is_empty());
        let cmd_out = json!({"type": "user", "message": {"content": "<local-command-stdout>Set model to `Opus`</local-command-stdout>"}});
        assert_eq!(to_events(&cmd_out), [json!({"type": "local_command", "text": "Set model to `Opus`"})]);
        let bash_in = json!({"type": "user", "message": {"content": "<bash-input>ls</bash-input>"}});
        assert_eq!(to_events(&bash_in)[0]["text"], "!ls");
        let bash_out = json!({"type": "user", "message": {"content": "<bash-stdout>a.txt\n</bash-stdout><bash-stderr></bash-stderr>"}});
        assert_eq!(to_events(&bash_out), [json!({"type": "local_command", "text": "a.txt"})]);
        let done = to_events(&json!({"type": "assistant", "message": {"content": [], "stop_reason": "end_turn"}}));
        assert_eq!(state_after(&done[0]), Some(AgentState::Idle));
        let mid = to_events(&json!({"type": "assistant", "message": {"content": [], "stop_reason": "tool_use"}}));
        assert_eq!(state_after(&mid[0]), None);
    }

    #[test]
    fn tail_reads_whole_lines_only() {
        let dir = std::env::temp_dir().join(format!("maestro-tail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("s.jsonl");
        let line = |t: &str| format!("{}\n", json!({"type": "user", "message": {"content": t}}));
        std::fs::write(&f, format!("{}{{\"type\":\"user\",\"mess", line("a"))).unwrap();
        let mut tail = Tail::new(f.clone(), "claude-code");
        assert_eq!(tail.read_new().len(), 1);
        std::fs::write(&f, format!("{}{}", line("a"), line("b"))).unwrap();
        assert_eq!(tail.read_new()[0]["text"], "b");
        assert_eq!(tail.count, 2);
        assert_eq!(Tail::read_all(f, "claude-code").len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn drops_host_session_env_only_inside_claude() {
        let vars = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>().into_iter();
        assert!(host_session_env(vars(&["NO_COLOR", "CLAUDE_CODE_SESSION_ID"])).is_empty());
        let dropped = host_session_env(vars(&["CLAUDECODE", "NO_COLOR", "CLAUDE_CODE_SESSION_ID", "CLAUDE_AGENT_SDK_VERSION", "CLAUDE_CODE_GIT_BASH_PATH", "PATH"]));
        assert_eq!(dropped, ["CLAUDECODE", "NO_COLOR", "CLAUDE_CODE_SESSION_ID", "CLAUDE_AGENT_SDK_VERSION"]);
    }

    /// Dumps what the Claude TUI prints while starting (no message sent): `cargo test -- --ignored tui_startup --nocapture`
    #[test]
    #[ignore]
    fn tui_startup_dump() {
        let pair = native_pty_system().openpty(PtySize { rows: 32, cols: 110, pixel_width: 0, pixel_height: 0 }).unwrap();
        let mut cmd = CommandBuilder::new("cmd.exe");
        cmd.args(["/c", &std::env::var("TUI_BIN").unwrap_or("claude".into())]);
        cmd.cwd(std::env::var("TUI_CWD").unwrap_or(".".into()));
        cmd.env("TERM", "xterm-256color");
        for k in host_session_env(std::env::vars().map(|(k, _)| k)) {
            cmd.env_remove(k);
        }
        let mut child = pair.slave.spawn_command(cmd).unwrap();
        let mut reader = pair.master.try_clone_reader().unwrap();
        let mut writer = pair.master.take_writer().unwrap();
        let start = Instant::now();
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        thread::spawn(move || {
            let mut buf = [0u8; 8192];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
        });
        while start.elapsed() < Duration::from_secs(15) {
            if let Ok(c) = rx.recv_timeout(Duration::from_millis(200)) {
                if c.windows(4).any(|w| w == b"\x1b[6n") {
                    writer.write_all(b"\x1b[1;1R").unwrap();
                }
                let text: String = String::from_utf8_lossy(&c).chars().filter(|ch| !ch.is_control() || *ch == '\n').collect();
                println!("[{:>5}ms] {}", start.elapsed().as_millis(), text.replace('\n', " "));
            }
        }
        let _ = child.kill();
    }

    /// Real Haiku call (network, ~$0.0002): `cargo test -- --ignored haiku`
    #[test]
    #[ignore]
    fn haiku_summary_live() {
        let exe = crate::cli::command(catalog::get("claude-code").unwrap()).unwrap();
        let s = summarize_with_claude(exe, "요청: html 보일러플레이트 알려줘\n결과: ```html\n<!doctype html>\n<html lang=\"ko\">\n<head>\n  <meta charset=\"utf-8\">\n  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n  <title>제목</title>\n</head>\n<body>\n</body>\n</html>\n```", "haiku").unwrap();
        assert!(!s.is_empty(), "{s}");
        println!("summary: {s}");
    }

    #[test]
    fn recent_turn_text_takes_last_turn_only() {
        let evs = vec![
            json!({"type": "user_input", "text": "old"}),
            json!({"type": "user_input", "text": "로그인 고쳐줘"}),
            json!({"type": "assistant", "message": {"content": [{"type": "tool_use", "name": "Edit"}]}}),
            json!({"type": "assistant", "message": {"content": [{"type": "text", "text": "고쳤어요"}]}}),
        ];
        let t = recent_turn_text(&evs, 1000);
        assert_eq!(t, "요청: 로그인 고쳐줘\n결과: 고쳤어요\n(사용한 도구: Edit)");
        assert_eq!(recent_turn_text(&evs, 4), "dit)");
        assert_eq!(recent_turn_text(&[], 100), "");
        // a slash command or a turn still running after the answer leaves the summary target unchanged
        let mut later = evs.clone();
        later.push(json!({"type": "user_input", "text": "/model"}));
        later.push(json!({"type": "local_command", "text": "Set model"}));
        later.push(json!({"type": "user_input", "text": "다음 작업"}));
        assert_eq!(recent_turn_text(&later, 1000), t);
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
    fn input_marker_seen_in_output() {
        let mut s = Scrollback::default();
        s.push(b"\x1b[6n\x1b]0;claude\x07");
        assert!(!s.input_on);
        s.push(b"\x1b[?2004h\x1b[?1004h");
        assert!(s.input_on);
        assert!(contains(b"ab\x1b[6ncd", CURSOR_QUERY));
    }

    #[test]
    fn trust_check_only_reads_output_after_mark() {
        let mut s = Scrollback::default();
        s.push(b"\x1b[1mDo you trust the files in this folder?\x1b[0m");
        assert!(s.asks_trust());
        s.mark = s.total;
        s.push(b"> Try \"fix lint errors\"");
        assert!(!s.asks_trust());
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
    fn argv_resumes_or_claims_session() {
        let m = AgentMeta { adapter_id: "claude-code".into(), permission_mode: "acceptEdits".into(), ..Default::default() };
        let s = Path::new("s.json");
        let exe = || vec!["C:/npm/claude.exe".to_string()];
        let a = argv(exe(), &m, "abc", true, s, s);
        assert_eq!(a[0], "C:/npm/claude.exe");
        assert!(a.windows(2).any(|w| w == ["--resume", "abc"]));
        assert!(a.windows(2).any(|w| w == ["--permission-mode", "acceptEdits"]));
        assert!(a.windows(2).any(|w| w == ["--settings", "s.json"]));
        assert!(argv(exe(), &m, "abc", false, s, s).windows(2).any(|w| w == ["--session-id", "abc"]));
    }

    #[test]
    fn previews_launch_commands() {
        assert_eq!(launch_preview("claude-code", "plan", "opus"), "claude --permission-mode plan --model opus");
        assert_eq!(launch_preview("antigravity", "bypassPermissions", ""), "agy --dangerously-skip-permissions");
        assert_eq!(launch_preview("antigravity", "acceptEdits", "m"), "agy --mode accept-edits --model m");
        assert_eq!(launch_preview("codex", "plan", ""), "codex");
    }

    #[test]
    fn strips_escapes_from_tui_output() {
        assert_eq!(plain(b"\x1b[1mDo you\x1b[0m trust\x1b]0;claude\x07 it?\r\nok"), "Do you trust it? \nok");
        assert_eq!(plain(b"\x1b]8;;http://x\x1b\\link\x1b]8;;\x1b\\"), "link");
    }

    #[test]
    fn classifies_failed_starts() {
        let hints = catalog::get("claude-code").unwrap().login_hints;
        let login = classify("Welcome to Claude Code\n Select login method:\n 1. Claude account", false, hints);
        assert!(matches!(login, Some(LaunchFailure::LoginRequired { .. })));
        // a login message the CLI exited with is still a login problem
        assert!(matches!(classify("Invalid API key · Please run /login", true, hints), Some(LaunchFailure::LoginRequired { .. })));
        let Some(LaunchFailure::Exited { message }) = classify("error: unknown option '--foo'\n\n", true, hints) else { panic!() };
        assert!(message.ends_with("마지막 출력: error: unknown option '--foo'"));
        assert!(matches!(classify("", true, hints), Some(LaunchFailure::Exited { .. })));
        // still up with an ordinary screen: not a failure (the caller decides ready or timeout)
        assert_eq!(classify("> Try \"fix lint errors\"", false, hints), None);
    }

    #[test]
    fn startup_decides_ready_login_or_timeout() {
        let hints = catalog::get("claude-code").unwrap().login_hints;
        let settle = |s: &mut Scrollback| s.last = Instant::now().checked_sub(Duration::from_secs(1));
        // still drawing: keep waiting; quiet without the input marker: keep waiting until the time is up
        let mut s = Scrollback::default();
        s.push(b"Welcome to Claude Code");
        assert_eq!(s.startup(false, hints), None);
        settle(&mut s);
        assert_eq!(s.startup(false, hints), None);
        assert!(matches!(s.startup(true, hints), Some(Err(LaunchFailure::Timeout { .. }))));

        // a login screen, even with input on; asked again with nothing new, it's still the login screen
        let mut s = Scrollback::default();
        s.push(b"\x1b[?2004h Select login method:");
        settle(&mut s);
        assert!(matches!(s.startup(false, hints), Some(Err(LaunchFailure::LoginRequired { .. }))));
        assert!(matches!(s.startup(false, hints), Some(Err(LaunchFailure::LoginRequired { .. }))));
        // logged in from the CLI view: the prompt that follows is ready, and that is kept
        s.push(b"Login successful\r\n> ");
        settle(&mut s);
        assert_eq!(s.startup(false, hints), Some(Ok(false)));
        assert_eq!(s.startup(true, hints), Some(Ok(false)));

        // input up and quiet on the trust dialog
        let mut s = Scrollback::default();
        s.push(b"\x1b[?2004hDo you trust the files in this folder?");
        settle(&mut s);
        assert_eq!(s.startup(false, hints), Some(Ok(true)));

        // a resumed session redraws an old turn that failed on a lapsed login: once input is up, that's history
        let history = "> fix the build\r\nInvalid API key · Please run /login\r\n\x1b[?2004h> ".as_bytes();
        let mut s = Scrollback { resumed: true, ..Default::default() };
        s.push(history);
        settle(&mut s);
        assert_eq!(s.startup(false, hints), Some(Ok(false)));
        // a fresh start showing the same text is still a login problem
        let mut s = Scrollback::default();
        s.push(history);
        settle(&mut s);
        assert!(matches!(s.startup(false, hints), Some(Err(LaunchFailure::LoginRequired { .. }))));
    }
}
