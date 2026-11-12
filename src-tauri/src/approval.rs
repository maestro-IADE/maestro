//! Maestro's local server (127.0.0.1, random port, per-run token):
//! - `/permission/…` — permission prompts answered from the chat view (below)
//! - `/api/…` — the `maestro` CLI agents use to drive Maestro (control.rs), with a token of the agent's own
//!   worktree (`agent_token`), so an agent can't call it as another one
//! - `/hook/…` — Claude's Stop and SessionStart hooks: signals only (read the transcript now, follow `/clear`)
//! - `/pretool/…` — Claude's PreToolUse hook: Bash, PowerShell and Monitor commands checked by the guard (guard.rs) before
//!   they run; no merges into the base branch, no pushes, nothing destructive. Fails closed: a hook that can't
//!   reach the server blocks the command.
//!
//! Permission prompts answered from the chat view.
//! Each agent is started with a `PermissionRequest` hook (via `--settings <file>`) that POSTs the request
//! to this localhost server with curl and prints our reply. The server shows it in the UI and holds the
//! connection until the user answers. No answer (Maestro closed, user picks "answer in CLI") → empty reply
//! → Claude falls back to its own terminal prompt, so nothing is ever approved by silence.

use crate::events::{Resolution, RuntimeEvent};
use crate::AppState;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// Below the hook's own timeout so we always answer before Claude gives up on curl.
const WAIT_MAX: Duration = Duration::from_secs(590);
const HOOK_TIMEOUT_SECS: u64 = 600;
/// Kill switch: Claude's Stop hook makes the chat read the transcript at once instead of at the next tick.
const STOP_HOOK: bool = true;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Approval {
    pub id: u64,
    pub worktree_path: String,
    pub tool_name: String,
    pub tool_input: Value,
    /// Claude offered an "always allow" rule for this call
    pub can_always: bool,
    /// the turn it was asked in, and the tool call it is about (best effort: Claude's hook doesn't say)
    pub turn_id: Option<String>,
    pub tool_call_id: Option<String>,
    /// "CLI에서 답하기" can be offered: the CLI asks again in its own terminal (PTY hook). An ACP prompt has no
    /// terminal to hand it to.
    pub cli_answer: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Done {
    id: u64,
    worktree_path: String,
}

/// The user's answer to a permission prompt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Choice {
    Allow,
    /// allow, and add the rule the agent suggested
    Always,
    Deny,
    /// hand the question back to the CLI's own prompt
    Cli,
}

impl Choice {
    /// "allow" | "always" | "deny"; anything else hands it back to the CLI
    fn parse(s: &str) -> Choice {
        match s {
            "allow" => Choice::Allow,
            "always" => Choice::Always,
            "deny" => Choice::Deny,
            _ => Choice::Cli,
        }
    }

    fn resolution(self) -> Resolution {
        match self {
            Choice::Allow => Resolution::Allow,
            Choice::Always => Resolution::Always,
            Choice::Deny => Resolution::Deny,
            Choice::Cli => Resolution::Cli,
        }
    }
}

struct Pending {
    approval: Approval,
    reply: Sender<Choice>,
}

/// A permission prompt, from whichever runtime asks (the PTY runtime's hook, ACP `session/request_permission`).
pub struct ApprovalReq {
    pub tool_name: String,
    pub tool_input: Value,
    pub can_always: bool,
    pub turn_id: Option<String>,
    pub tool_call_id: Option<String>,
    /// see `Approval::cli_answer`
    pub cli_answer: bool,
    /// give up after this long (the hook's own timeout); None = as long as `still_waiting` says (ACP: the agent
    /// waits, and no answer must never become one)
    pub wait_max: Option<Duration>,
}

#[derive(Default)]
struct Inner {
    next_id: u64,
    pending: HashMap<u64, Pending>,
    /// our session id → worktree path (the URL carries the id; it survives /clear, the hook input's doesn't)
    agents: HashMap<String, String>,
}

#[derive(Clone)]
pub struct Approvals {
    port: u16,
    token: String,
    /// keys of the per-agent `/api` tokens (`agent_token`), new each run
    agent_key: (RandomState, RandomState),
    inner: Arc<Mutex<Inner>>,
}

/// Which runtime the settings are for: the terminal (`--settings` file) or the chat (ACP `_meta`).
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Pty,
    Acp,
}

/// The second layer of the guard: Claude's own deny rules, which hold in bypass mode and when hooks are off.
const STATIC_DENY: [&str; 6] = ["Bash(git push:*)", "Bash(git reset --hard:*)", "Bash(git branch -D:*)", "Bash(git clean -f:*)", "Bash(git clean -fd:*)", "Bash(git clean -fdx:*)"];

/// The guard's PreToolUse hook (`/pretool/…`). Fails closed: `-f` turns a 4xx/5xx into a curl error, and a curl
/// error (none installed, a timeout, Maestro gone) exits 2, which blocks the call. ASCII message: no code page issue.
fn pretool_hook(port: u16, token: &str, key: &str) -> Value {
    let url = format!("http://127.0.0.1:{port}/pretool/{key}?t={token}");
    let command = format!("curl -sfS --max-time 10 -X POST -H \"Content-Type: application/json\" --data-binary @- \"{url}\" || {{ echo 'maestro guard unreachable: command blocked' >&2; exit 2; }}");
    json!([{ "matcher": "Bash|PowerShell|Monitor", "hooks": [{ "type": "command", "timeout": 15, "command": command }] }])
}

/// The hook's answer when the guard denies a command.
fn pretool_reply(reason: &str) -> Value {
    json!({ "hookSpecificOutput": { "hookEventName": "PreToolUse", "permissionDecision": "deny", "permissionDecisionReason": reason } })
}

/// Claude's settings from Maestro. The terminal (`--settings` file): the permission hook, Stop and SessionStart as
/// signals (`/hook/…`), the guard. The signal hooks never block or fail a turn (`|| exit 0`: a Stop hook's exit code
/// 2 would make Claude go on) and get an empty answer (SessionStart's output would land in Claude's context). The
/// chat (ACP): only the guard; its permission prompts come as `session/request_permission`, and it ticks itself.
fn settings_json(port: u16, token: &str, key: &str, kind: Kind) -> Value {
    let guard = pretool_hook(port, token, key);
    if kind == Kind::Acp {
        return json!({ "permissions": { "deny": STATIC_DENY }, "hooks": { "PreToolUse": guard } });
    }
    let url = format!("http://127.0.0.1:{port}/permission/{key}?t={token}");
    let command = format!("curl -s --max-time {HOOK_TIMEOUT_SECS} -X POST -H \"Content-Type: application/json\" --data-binary @- \"{url}\"");
    let signal_url = format!("http://127.0.0.1:{port}/hook/{key}?t={token}");
    let signal = format!("curl -s --max-time 5 -X POST -H \"Content-Type: application/json\" --data-binary @- \"{signal_url}\" || exit 0");
    let signal = json!([{ "matcher": "*", "hooks": [{ "type": "command", "command": signal, "timeout": 10 }] }]);
    // the maestro CLI can't delete anything (control.rs; a merge-request only asks the user), and asking each time
    // would stall orchestration
    json!({
        "permissions": { "allow": ["Bash(maestro:*)"], "deny": STATIC_DENY },
        "hooks": {
            "PreToolUse": guard,
            "PermissionRequest": [{ "matcher": "*", "hooks": [{ "type": "command", "command": command, "timeout": HOOK_TIMEOUT_SECS }] }],
            "Stop": signal.clone(),
            "SessionStart": signal,
        },
    })
}

/// The chat runtime's settings for a server at `port` (also what the ACP live test hands the adapter).
pub(crate) fn settings_for_acp(port: u16, token: &str, key: &str) -> Value {
    settings_json(port, token, key, Kind::Acp)
}

/// One spelling of a worktree path for its token.
fn token_path(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

impl Approvals {
    pub fn start(app: AppHandle) -> Result<Approvals, String> {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let token = format!("{:016x}{:016x}", RandomState::new().hash_one(1), RandomState::new().hash_one(2));
        let me = Approvals { port, token, agent_key: (RandomState::new(), RandomState::new()), inner: Arc::default() };
        let srv = me.clone();
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (srv, app) = (srv.clone(), app.clone());
                thread::spawn(move || srv.handle(stream, &app));
            }
        });
        Ok(me)
    }

    pub fn register(&self, session_id: &str, worktree_path: &str) {
        self.inner.lock().unwrap().agents.insert(session_id.into(), worktree_path.into());
    }

    /// Per-agent settings file for `--settings`: just the hooks (and the guard's deny rules), user settings stay untouched.
    pub fn write_settings(&self, file: &Path, session_id: &str) -> Result<(), String> {
        std::fs::create_dir_all(file.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(file, serde_json::to_string_pretty(&settings_json(self.port, &self.token, session_id, Kind::Pty)).unwrap()).map_err(|e| e.to_string())
    }

    /// The chat runtime's settings (`_meta.claudeCode.options.settings`): the guard only. `key` must be registered.
    pub fn acp_settings(&self, key: &str) -> Value {
        settings_for_acp(self.port, &self.token, key)
    }

    /// The `/api` token of the agent in `path` (its MAESTRO_TOKEN): the CLI's caller header must match it.
    pub fn agent_token(&self, path: &str) -> String {
        let p = token_path(path);
        format!("{:016x}{:016x}", self.agent_key.0.hash_one(&p), self.agent_key.1.hash_one(&p))
    }

    pub fn pending(&self) -> Vec<Approval> {
        self.inner.lock().unwrap().pending.values().map(|p| p.approval.clone()).collect()
    }

    /// The prompt was answered somewhere else, in the TUI: the tool call it asked about finished, or (when the
    /// prompt couldn't be matched to a call) its turn ended. Claude doesn't always kill the hook's curl then, so
    /// the card would otherwise stay until WAIT_MAX. Dropping the entry ends `request` as Withdrawn.
    pub fn settle(&self, worktree_path: &str, turn_id: &str, tool_call_id: Option<&str>) {
        let same = |w: &str| w.replace('\\', "/").eq_ignore_ascii_case(&worktree_path.replace('\\', "/"));
        self.inner.lock().unwrap().pending.retain(|_, p| {
            let a = &p.approval;
            let answered = match tool_call_id {
                Some(id) => a.tool_call_id.as_deref() == Some(id),
                None => a.turn_id.as_deref() == Some(turn_id),
            };
            !(answered && same(&a.worktree_path))
        });
    }

    /// "allow" | "always" | "deny" | "cli" (hand the question back to the terminal prompt)
    pub fn answer(&self, id: u64, choice: &str) {
        let Some(p) = self.inner.lock().unwrap().pending.remove(&id) else { return };
        let _ = p.reply.send(Choice::parse(choice));
    }

    /// `127.0.0.1:<port>`, for the maestro CLI (MAESTRO_URL)
    pub fn addr(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    /// Shows a permission prompt in the UI and waits for the answer (`req.wait_max`, or until `still_waiting` says
    /// the agent stopped asking). Every runtime's prompts go through here. The choice is None when nobody chose.
    pub fn request(&self, app: &AppHandle, worktree_path: &str, req: ApprovalReq, still_waiting: impl Fn() -> bool) -> (Resolution, Option<Choice>) {
        let (tx, rx) = mpsc::channel();
        let wait_max = req.wait_max;
        let approval = {
            let mut inner = self.inner.lock().unwrap();
            inner.next_id += 1;
            let approval = Approval {
                id: inner.next_id,
                worktree_path: worktree_path.into(),
                tool_name: req.tool_name,
                tool_input: req.tool_input,
                can_always: req.can_always,
                turn_id: req.turn_id,
                tool_call_id: req.tool_call_id,
                cli_answer: req.cli_answer,
            };
            inner.pending.insert(approval.id, Pending { approval: approval.clone(), reply: tx });
            approval
        };
        let _ = app.emit("agent://approval", &approval);
        let live = crate::stream::emitter(worktree_path);
        if let Some(e) = &live {
            e.push(RuntimeEvent::ApprovalRequested {
                turn_id: approval.turn_id.clone(),
                tool_call_id: approval.tool_call_id.clone(),
                approval_id: approval.id,
                tool_name: approval.tool_name.clone(),
                tool_input: approval.tool_input.clone(),
                can_always: approval.can_always,
            });
        }

        let start = Instant::now();
        let timed_out = || wait_max.is_some_and(|m| start.elapsed() > m);
        let (resolution, choice) = loop {
            match rx.recv_timeout(Duration::from_secs(1)) {
                Ok(c) => break (c.resolution(), Some(c)),
                Err(RecvTimeoutError::Disconnected) => break (Resolution::Withdrawn, None),
                Err(RecvTimeoutError::Timeout) if timed_out() || !still_waiting() => {
                    self.inner.lock().unwrap().pending.remove(&approval.id);
                    break (if timed_out() { Resolution::Timeout } else { Resolution::Withdrawn }, None);
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
        };
        let _ = app.emit("agent://approval-done", Done { id: approval.id, worktree_path: worktree_path.into() });
        if let Some(e) = &live {
            e.push(RuntimeEvent::ApprovalResolved { turn_id: approval.turn_id, tool_call_id: approval.tool_call_id, approval_id: approval.id, resolution });
        }
        (resolution, choice)
    }

    fn handle(&self, mut stream: TcpStream, app: &AppHandle) {
        let Some((path, headers, body)) = read_request(&stream) else { return };
        // the maestro CLI: caller's worktree in a header, and that worktree's own token (control.rs)
        if let Some(route) = path.strip_prefix("/api/") {
            let caller = headers.get("x-maestro-worktree").cloned().unwrap_or_default();
            if headers.get("x-maestro-token") != Some(&self.agent_token(&caller)) {
                return respond(&mut stream, 403, &json!({ "error": "토큰이 맞지 않아요" }).to_string());
            }
            let input: Value = serde_json::from_slice(&body).unwrap_or_default();
            return match crate::control::handle(app, route, &caller, &input) {
                Ok(v) => respond(&mut stream, 200, &v.to_string()),
                Err(e) => respond(&mut stream, 400, &json!({ "error": e }).to_string()),
            };
        }
        let (route, rest) = match (path.strip_prefix("/permission/"), path.strip_prefix("/hook/"), path.strip_prefix("/pretool/")) {
            (Some(rest), ..) => (Route::Permission, rest),
            (_, Some(rest), _) => (Route::Signal, rest),
            (.., Some(rest)) => (Route::Guard, rest),
            _ => return respond(&mut stream, 404, ""),
        };
        let (sid, query) = rest.split_once('?').unwrap_or((rest, ""));
        if query != format!("t={}", self.token) {
            return respond(&mut stream, 403, "");
        }
        let worktree_path = self.inner.lock().unwrap().agents.get(sid).cloned();
        let input: Value = serde_json::from_slice(&body).unwrap_or_default();
        if route == Route::Guard {
            let cwd = input["cwd"].as_str();
            let wt = worktree_path.as_deref().or(cwd).unwrap_or("");
            let tool = input["tool_name"].as_str().unwrap_or("");
            // a panic answers 500, which the hook's curl turns into a block
            let verdict = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::guard::live_check(app, wt, cwd, tool, &input["tool_input"])));
            return match verdict {
                Ok(Some(why)) => respond(&mut stream, 200, &pretool_reply(&why).to_string()),
                Ok(None) => respond(&mut stream, 200, ""),
                Err(_) => respond(&mut stream, 500, ""),
            };
        }
        let hook = route == Route::Signal;
        if hook {
            // answered at once and empty: a signal hook never holds Claude up or adds to its context
            respond(&mut stream, 200, "");
            return signal(app, worktree_path.as_deref(), &input);
        }
        let Some(worktree_path) = worktree_path else { return respond(&mut stream, 200, "") };
        let (turn_id, tool_call_id) = correlate(app, &worktree_path, &input);
        let tool_name: String = input["tool_name"].as_str().unwrap_or("?").into();
        let req = ApprovalReq {
            tool_name: tool_name.clone(),
            tool_input: input["tool_input"].clone(),
            can_always: input["permission_suggestions"].as_array().is_some_and(|s| !s.is_empty()),
            turn_id: turn_id.clone(),
            tool_call_id: tool_call_id.clone(),
            cli_answer: true,
            wait_max: Some(WAIT_MAX),
        };
        // Wait for the UI, but notice when Claude gives up (answered in the TUI, turn interrupted → curl killed).
        let (resolution, choice) = self.request(app, &worktree_path, req, || !peer_closed(&stream));
        // no decision: Claude asks in its own terminal next, so nothing may be pasted there until that's answered
        // (marked before the reply, so before Claude draws its picker)
        if let (Some(source), Some(agent)) = (tui_source(resolution), app.state::<AppState>().get(&worktree_path)) {
            agent.tui.apply(crate::tui_wait::Signal::Asked { source, tool_name: Some(tool_name), turn_id, tool_call_id });
        }
        let reply = choice.and_then(|c| decision(c, &input["permission_suggestions"]));
        respond(&mut stream, 200, &reply.map(|d| d.to_string()).unwrap_or_default());
    }
}

#[derive(PartialEq)]
enum Route {
    Permission,
    Signal,
    Guard,
}

/// The turn and tool call a hook's permission prompt is about. Claude's PermissionRequest input has no tool call id
/// (checked on 2.1.302), so it's matched by tool name and input against the transcript's open calls, which may
/// still be on their way to disk: a few short retries until the match is sure. After that, a call of the same name
/// with other input (another Bash still running) only lends its turn: the tool call id stays None, never a guess.
fn correlate(app: &AppHandle, worktree_path: &str, input: &Value) -> (Option<String>, Option<String>) {
    let Some(agent) = app.state::<AppState>().get(worktree_path) else { return (None, None) };
    let (name, tool_input) = (input["tool_name"].as_str().unwrap_or("?"), &input["tool_input"]);
    let id = input["tool_use_id"].as_str().filter(|id| !id.is_empty());
    let mut found = agent.locate_tool(name, tool_input, id);
    for _ in 0..3 {
        if found.exact {
            break;
        }
        thread::sleep(Duration::from_millis(100));
        found = agent.locate_tool(name, tool_input, id);
    }
    (found.turn_id, found.tool_call_id)
}

/// Claude's signal hooks: never events themselves, only a nudge to read the transcript (Stop) or to follow the
/// conversation `/clear` started (SessionStart). State and worker-done still come from the transcript.
fn signal(app: &AppHandle, worktree_path: Option<&str>, input: &Value) {
    let Some(agent) = worktree_path.and_then(|w| app.state::<AppState>().get(w)) else { return };
    match input["hook_event_name"].as_str() {
        Some("Stop") if STOP_HOOK => agent.sync(),
        Some("SessionStart") if crate::transcript::FOLLOW_SESSIONS && input["source"] == "clear" => {
            if let Some(id) = input["session_id"].as_str().filter(|id| !id.is_empty()) {
                agent.follow(id, input["transcript_path"].as_str().map(Path::new));
            }
        }
        _ => {}
    }
    // every hook says where the conversation's transcript is (CLAUDE_CONFIG_DIR, a hashed folder name): after a
    // follow above, so the conversation it names is already the one in meta
    let id = input["session_id"].as_str().filter(|s| !s.is_empty());
    if let (Some(id), Some(p)) = (id, input["transcript_path"].as_str().filter(|s| !s.is_empty())) {
        agent.note_transcript(id, Path::new(p));
    }
}

/// A hook answered with no decision leaves the question to Claude's own terminal prompt: why, for the TUI wait.
/// None when it was decided, or the agent stopped asking (answered in the TUI already, the turn interrupted).
fn tui_source(r: Resolution) -> Option<crate::tui_wait::Source> {
    match r {
        Resolution::Cli => Some(crate::tui_wait::Source::Cli),
        Resolution::Timeout => Some(crate::tui_wait::Source::Timeout),
        Resolution::Allow | Resolution::Always | Resolution::Deny | Resolution::Withdrawn => None,
    }
}

/// The hook's stdout for a choice; None = no decision (Claude shows its own prompt).
fn decision(choice: Choice, suggestions: &Value) -> Option<Value> {
    let d = match choice {
        Choice::Allow => json!({ "behavior": "allow" }),
        Choice::Always => json!({ "behavior": "allow", "updatedPermissions": suggestions }),
        Choice::Deny => json!({ "behavior": "deny", "message": "사용자가 Maestro에서 거부했어요." }),
        Choice::Cli => return None,
    };
    Some(json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": d } }))
}

/// (path, `x-maestro-*` headers lowercased, body)
fn read_request(stream: &TcpStream) -> Option<(String, HashMap<String, String>, Vec<u8>)> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let mut r = BufReader::new(stream);
    let mut line = String::new();
    r.read_line(&mut line).ok()?;
    let path = line.split_whitespace().nth(1)?.to_string();
    let mut len = 0usize;
    let mut headers = HashMap::new();
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).ok()? == 0 || h.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            let k = k.trim().to_ascii_lowercase();
            if k == "content-length" {
                len = v.trim().parse().ok()?;
            } else if k.starts_with("x-maestro-") {
                headers.insert(k, v.trim().to_string());
            }
        }
    }
    let mut body = vec![0; len.min(4 << 20)];
    r.read_exact(&mut body).ok()?;
    Some((path, headers, body))
}

fn peer_closed(stream: &TcpStream) -> bool {
    let _ = stream.set_nonblocking(true);
    let closed = matches!(stream.peek(&mut [0u8; 1]), Ok(0));
    let _ = stream.set_nonblocking(false);
    closed
}

fn respond(stream: &mut TcpStream, status: u16, body: &str) {
    let _ = write!(stream, "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_decision_leaves_the_tui_waiting() {
        use crate::tui_wait::Source;
        assert_eq!(tui_source(Resolution::Cli), Some(Source::Cli));
        assert_eq!(tui_source(Resolution::Timeout), Some(Source::Timeout));
        for r in [Resolution::Allow, Resolution::Always, Resolution::Deny, Resolution::Withdrawn] {
            assert_eq!(tui_source(r), None);
        }
    }

    #[test]
    fn decisions_match_hook_schema() {
        let sug = json!([{ "type": "addRules", "behavior": "allow" }]);
        let allow = decision(Choice::parse("allow"), &sug).unwrap();
        assert_eq!(allow["hookSpecificOutput"]["hookEventName"], "PermissionRequest");
        assert_eq!(allow["hookSpecificOutput"]["decision"]["behavior"], "allow");
        assert_eq!(decision(Choice::parse("always"), &sug).unwrap()["hookSpecificOutput"]["decision"]["updatedPermissions"], sug);
        assert_eq!(decision(Choice::parse("deny"), &sug).unwrap()["hookSpecificOutput"]["decision"]["behavior"], "deny");
        assert!(decision(Choice::parse("cli"), &sug).is_none());
        assert_eq!(Choice::parse("whatever"), Choice::Cli);
        assert_eq!(Choice::Always.resolution(), Resolution::Always);
    }

    #[test]
    fn approvals_carry_turn_and_tool_ids() {
        let a = Approval { id: 1, worktree_path: "C:/w".into(), tool_name: "Bash".into(), tool_input: json!({}), can_always: true, turn_id: Some("p1".into()), tool_call_id: None, cli_answer: false };
        let v = serde_json::to_value(&a).unwrap();
        assert_eq!(v["turnId"], "p1");
        assert_eq!(v["cliAnswer"], false);
        assert!(v["toolCallId"].is_null() && v["canAlways"] == true);
    }

    #[test]
    fn settings_add_signal_hooks() {
        let s = settings_json(4321, "tk", "sid", Kind::Pty);
        let cmd = |k: &str| s["hooks"][k][0]["hooks"][0]["command"].as_str().unwrap().to_string();
        assert!(cmd("PermissionRequest").contains("http://127.0.0.1:4321/permission/sid?t=tk"));
        for k in ["Stop", "SessionStart"] {
            assert!(cmd(k).contains("http://127.0.0.1:4321/hook/sid?t=tk"), "{k}");
            // a failed curl must not turn into a blocking exit code
            assert!(cmd(k).ends_with("|| exit 0"), "{k}");
            assert_eq!(s["hooks"][k][0]["hooks"][0]["timeout"], 10);
        }
        assert_eq!(s["permissions"]["allow"][0], "Bash(maestro:*)");
        // the guard: shell tools only, fails closed (any curl failure exits 2, which blocks the call)
        let pre = &s["hooks"]["PreToolUse"][0];
        assert_eq!(pre["matcher"], "Bash|PowerShell|Monitor");
        assert_eq!(pre["hooks"][0]["timeout"], 15);
        assert!(cmd("PreToolUse").starts_with("curl -sfS ") && cmd("PreToolUse").contains("\"http://127.0.0.1:4321/pretool/sid?t=tk\""));
        assert!(cmd("PreToolUse").ends_with("exit 2; }"));
        assert!(cmd("PreToolUse").is_ascii());
        assert_eq!(s["permissions"]["deny"], json!(STATIC_DENY));
        assert!(STATIC_DENY.contains(&"Bash(git push:*)"));
    }

    #[test]
    fn acp_settings_only_guard() {
        let s = settings_json(4321, "tk", "acp-1234", Kind::Acp);
        let hooks = s["hooks"].as_object().unwrap();
        assert_eq!(hooks.keys().collect::<Vec<_>>(), ["PreToolUse"]);
        assert!(s["hooks"]["PreToolUse"][0]["hooks"][0]["command"].as_str().unwrap().contains("/pretool/acp-1234?t=tk"));
        // the adapter's own allow list stays (session_meta); permission prompts come over ACP
        assert!(s["permissions"]["allow"].is_null());
        assert_eq!(s["permissions"]["deny"], json!(STATIC_DENY));
    }

    #[test]
    fn pretool_reply_matches_schema() {
        let r = pretool_reply("no");
        assert_eq!(r["hookSpecificOutput"]["hookEventName"], "PreToolUse");
        assert_eq!(r["hookSpecificOutput"]["permissionDecision"], "deny");
        assert_eq!(r["hookSpecificOutput"]["permissionDecisionReason"], "no");
    }

    #[test]
    fn agent_tokens_differ_per_worktree_ignore_case_and_slashes() {
        let a = Approvals { port: 1, token: "t".into(), agent_key: (RandomState::new(), RandomState::new()), inner: Arc::default() };
        let t = a.agent_token("C:\\p\\wt\\");
        assert_eq!(t, a.agent_token("c:/P/WT"));
        assert_eq!(t.len(), 32);
        assert_ne!(t, a.agent_token("C:/p/repo"));
        assert_ne!(t, a.token);
        // another run: other tokens
        let b = Approvals { agent_key: (RandomState::new(), RandomState::new()), ..a.clone() };
        assert_ne!(t, b.agent_token("C:/p/wt"));
    }

    #[test]
    fn parses_hook_post() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let t = thread::spawn(move || {
            let mut c = TcpStream::connect(addr).unwrap();
            c.write_all(b"POST /permission/abc?t=x HTTP/1.1\r\nHost: h\r\nX-Maestro-Token: tk\r\nContent-Length: 13\r\n\r\n{\"a\":\"hello\"}").unwrap();
        });
        let (s, _) = l.accept().unwrap();
        let (path, headers, body) = read_request(&s).unwrap();
        t.join().unwrap();
        assert_eq!(path, "/permission/abc?t=x");
        assert_eq!(headers.get("x-maestro-token").map(String::as_str), Some("tk"));
        assert!(!headers.contains_key("host"));
        assert_eq!(body, br#"{"a":"hello"}"#);
    }
}
