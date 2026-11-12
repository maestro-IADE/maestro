//! The ACP runtime: Claude Code driven through its ACP adapter (`claude-agent-acp`, node) for the chat view.
//! - the transcript stays the durable record: the CLI the adapter starts writes the same
//!   `~/.claude/projects/<slug>/<id>.jsonl`, read by the same tail and normalizer as under the PTY runtime, so the
//!   chat, `maestro read`, worker-done notes and summaries see the same items whichever runtime ran a turn
//! - ACP adds what the file can't: live text (`agent_message_chunk` → `Delta` through the emitter's coalescing,
//!   retired once its block lands in the file), the turn's end (the prompt's response), cancel, permission
//!   prompts (to the approval cards, fail-closed), the permission mode, the commands it takes
//! - prompts queue here, one in flight at a time; the transcript's own turns drive the busy dot as under PTY
//! - anything wrong before the session is up falls back to the PTY runtime (`on_boot_failed`, the facade's)

use crate::acp::{Client, Incoming, RpcError, RpcErrorKind};
use crate::acp_setup::{self, AcpPlan, FallbackReason};
use crate::agent::Files;
use crate::approval::{ApprovalReq, Choice};
use crate::catalog;
use crate::claude;
use crate::events::{DeltaChannel, NoticeKind, ResetReason, RuntimeEvent, TurnOutcome};
use crate::runtime::{self, LaunchFailure, Prompt, Readiness, Runtime, Sent, ToolMatch};
use crate::stream::Emitter;
use crate::transcript::{LiveTail, TailHook};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex, OnceLock, Weak};
use std::thread;
use std::time::Duration;

pub const INIT_LIMIT: Duration = Duration::from_secs(30);
pub const SESSION_LIMIT: Duration = Duration::from_secs(120);
pub const CALL_LIMIT: Duration = Duration::from_secs(30);
/// How often the transcript is read without a wake-up.
const TICK: Duration = Duration::from_millis(250);
/// How long a `/command` waits for the adapter's command list before it's let through.
const COMMANDS_WAIT: Duration = Duration::from_secs(2);
/// The tail is read again at these points after a prompt's response, until the file shows the turn's end.
const SETTLE_TICKS: [u64; 4] = [0, 300, 700, 1000];
/// The only tool Maestro allows without asking: the `maestro` CLI (it can't delete anything, control.rs).
const ALLOWED: &str = "Bash(maestro:*)";

/// A permission prompt shown to the user (`Approvals::request` in the app): the choice, or None when nobody chose.
pub type Ask = Arc<dyn Fn(ApprovalReq, &dyn Fn() -> bool) -> Option<Choice> + Send + Sync>;
/// The facade's fallback to the PTY runtime; Some(error) when that failed too.
pub type OnBootFailed = Box<dyn Fn(&Arc<AcpRuntime>, FallbackReason) -> Option<String> + Send + Sync>;

pub struct AcpCtx {
    pub home: PathBuf,
    pub cwd: String,
    pub files: Files,
    pub plan: AcpPlan,
    /// what the stream's new epoch says: Restart, or Switch from the CLI view
    pub reason: ResetReason,
    /// (vars removed, vars set) for the adapter (`acp_setup::acp_env`)
    pub env: (Vec<String>, Vec<(String, String)>),
    /// appended to Claude Code's system prompt (`agent::system_brief`)
    pub brief: String,
    /// Claude Code settings for the session (`Approvals::acp_settings`: the guard's PreToolUse hook and deny rules);
    /// None for agents without hooks
    pub settings: Option<Value>,
    pub agent_id: &'static str,
    pub ask: Ask,
    /// the adapter's command list came or changed (the `/` menu asks again)
    pub on_commands: Box<dyn Fn() + Send + Sync>,
}

#[derive(Debug, Clone, PartialEq)]
enum Phase {
    Booting,
    /// waiting for Maestro's own "trust this folder?" (the TUI's dialog never shows under ACP)
    NeedsTrust,
    Ready,
    Failed(LaunchFailure),
    Closed,
}

/// A prompt waiting for its turn: the text as the CLI gets it (attachments as `@path`), and the worker notes it carries.
struct Queued {
    n: u64,
    text: String,
    kept: Vec<(String, String)>,
}

#[derive(Default)]
struct SendState {
    queue: VecDeque<Queued>,
    inflight: Option<u64>,
}

pub struct AcpRuntime {
    ctx: AcpCtx,
    me: Weak<AcpRuntime>,
    client: OnceLock<Arc<Client>>,
    phase: Mutex<Phase>,
    phase_cv: Condvar,
    trust_ok: AtomicBool,
    /// stopped on purpose (kill, shutdown): no fallback, no crash
    killed: AtomicBool,
    /// it reached Ready once: its exit is the agent's (before that, the boot thread decides)
    was_ready: AtomicBool,
    exit_done: AtomicBool,
    session: Mutex<Option<String>>,
    /// `session/load` replays the conversation as updates first: the transcript has it already
    replaying: AtomicBool,
    emitter: Arc<Emitter>,
    tail: Arc<Mutex<Option<LiveTail>>>,
    wake: Mutex<Option<Sender<()>>>,
    live: Arc<Mutex<LiveText>>,
    /// toolCallId → (tool name, raw input), from `tool_call` updates
    tools: Mutex<HashMap<String, (String, Value)>>,
    /// (name, description) of the commands the adapter takes
    commands: Mutex<Option<Vec<(String, String)>>>,
    commands_cv: Condvar,
    send: Mutex<SendState>,
    send_cv: Condvar,
    n: AtomicU64,
    /// open permission requests (their JSON-RPC id) → withdrawn (`$/cancel_request`, cancel)
    perms: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// a session/prompt is waiting for its response (a cancel then ends that prompt's turn)
    responding: AtomicBool,
    cancelling: AtomicBool,
    /// the adapter said nobody is logged in (`_auth/status_update` kind none)
    logged_out: AtomicBool,
    on_boot_failed: OnBootFailed,
    on_exit: Box<dyn Fn() + Send + Sync>,
}

impl AcpRuntime {
    /// Ready to `begin`; nothing runs yet (the facade registers it first, so its fallback finds it current).
    pub fn new(ctx: AcpCtx, emitter: Arc<Emitter>, on_boot_failed: OnBootFailed, on_exit: Box<dyn Fn() + Send + Sync>) -> Arc<AcpRuntime> {
        let push_to = emitter.clone();
        Arc::new_cyclic(|me| AcpRuntime {
            ctx,
            me: me.clone(),
            client: OnceLock::new(),
            phase: Mutex::new(Phase::Booting),
            phase_cv: Condvar::new(),
            trust_ok: AtomicBool::new(false),
            killed: AtomicBool::new(false),
            was_ready: AtomicBool::new(false),
            exit_done: AtomicBool::new(false),
            session: Mutex::new(None),
            replaying: AtomicBool::new(false),
            emitter,
            tail: Arc::default(),
            wake: Mutex::new(None),
            live: Arc::new(Mutex::new(LiveText::new(Box::new(move |ev| {
                push_to.push(ev);
            })))),
            tools: Mutex::default(),
            commands: Mutex::new(None),
            commands_cv: Condvar::new(),
            send: Mutex::default(),
            send_cv: Condvar::new(),
            n: AtomicU64::new(0),
            perms: Mutex::default(),
            responding: AtomicBool::new(false),
            cancelling: AtomicBool::new(false),
            logged_out: AtomicBool::new(false),
            on_boot_failed,
            on_exit,
        })
    }

    /// Starts the adapter on its own thread; `ready()` waits for it.
    pub fn begin(&self) {
        let Some(me) = self.me.upgrade() else { return };
        thread::spawn(move || {
            if let Err(reason) = me.boot() {
                me.fail(reason);
            }
        });
    }

    pub fn agent_id(&self) -> &'static str {
        self.ctx.agent_id
    }

    fn set_phase(&self, p: Phase) {
        *self.phase.lock().unwrap() = p;
        self.phase_cv.notify_all();
        self.send_cv.notify_all();
        self.commands_cv.notify_all();
    }

    fn phase(&self) -> Phase {
        self.phase.lock().unwrap().clone()
    }

    fn client(&self) -> Option<Arc<Client>> {
        self.client.get().cloned()
    }

    fn session_id(&self) -> Option<String> {
        self.session.lock().unwrap().clone()
    }

    fn wake(&self) {
        if let Some(w) = &*self.wake.lock().unwrap() {
            let _ = w.send(());
        }
    }

    /// A failed start: None = it was stopped meanwhile (no fallback); else the reason the facade falls back for.
    fn start_error(&self, e: &RpcError) -> Option<FallbackReason> {
        if self.killed.load(Ordering::SeqCst) {
            return None;
        }
        if e.code() == Some(-32000) {
            return Some(FallbackReason::LoginRequired);
        }
        Some(FallbackReason::StartFailed { message: e.message.clone() })
    }

    fn fail(self: &Arc<Self>, reason: Option<FallbackReason>) {
        let Some(reason) = reason.filter(|_| !self.killed.load(Ordering::SeqCst)) else {
            // stopped before it was up (an away stop at the trust question, a kill during session/load): that's
            // the agent's exit, else it would stay "alive" with nothing behind it. A switch or fallback bumped the
            // generation first, so the facade ignores it then.
            self.set_phase(Phase::Closed);
            if !self.exit_done.swap(true, Ordering::SeqCst) {
                (self.on_exit)();
            }
            return;
        };
        let err = (self.on_boot_failed)(self, reason.clone());
        let message = err.unwrap_or_else(|| reason.note().unwrap_or_default());
        self.set_phase(Phase::Failed(LaunchFailure::Exited { message }));
        if let Some(c) = self.client() {
            c.close(Duration::from_millis(500));
        }
    }

    /// initialize → (trust) → session/load or session/new → the permission mode → the tail. Err(None): stopped
    /// meanwhile.
    fn boot(self: &Arc<Self>) -> Result<(), Option<FallbackReason>> {
        let ctx = &self.ctx;
        let meta = ctx.files.read_meta().unwrap_or_default();
        let cmd = acp_setup::adapter_command(&ctx.plan.node, &ctx.plan.entry, Path::new(&ctx.cwd), &ctx.env);
        let (w1, w2) = (self.me.clone(), self.me.clone());
        let client = Client::spawn(
            cmd,
            move |c, inc| {
                if let Some(rt) = w1.upgrade() {
                    rt.incoming(c, inc);
                }
            },
            move |_| {
                if let Some(rt) = w2.upgrade() {
                    rt.exited();
                }
            },
        )
        .map_err(|e| Some(FallbackReason::StartFailed { message: format!("어댑터를 실행하지 못했어요: {e}") }))?;
        let _ = self.client.set(client.clone());
        if self.killed.load(Ordering::SeqCst) {
            client.kill();
            return Err(None);
        }
        let init = client.request("initialize", acp_setup::initialize_params(), Some(INIT_LIMIT)).map_err(|e| self.start_error(&e))?;
        if init["protocolVersion"] != 1 || init["agentCapabilities"]["loadSession"] != true {
            return Err(Some(FallbackReason::StartFailed { message: "프로토콜이 맞지 않아요".into() }));
        }

        // the TUI asks before it works in a new folder; so does Maestro, once per worktree
        if !meta.acp_trusted && !trusted_worktree(&claude_json(&ctx.home), &ctx.cwd) {
            // under the lock `accept_trust` takes: an answer that came first leaves no NeedsTrust behind
            let mut p = self.phase.lock().unwrap();
            if !self.trust_ok.load(Ordering::SeqCst) && *p == Phase::Booting {
                *p = Phase::NeedsTrust;
                self.phase_cv.notify_all();
            }
            while !self.trust_ok.load(Ordering::SeqCst) && !self.killed.load(Ordering::SeqCst) && client.alive() {
                p = self.phase_cv.wait_timeout(p, Duration::from_millis(500)).unwrap().0;
            }
            drop(p);
            if self.killed.load(Ordering::SeqCst) {
                return Err(None);
            }
            if !client.alive() {
                return Err(Some(FallbackReason::StartFailed { message: "어댑터가 꺼졌어요".into() }));
            }
            // `accept_trust` already put the phase back to Booting
            self.update_meta(|m| m.acp_trusted = true);
        }

        let meta = ctx.files.read_meta().unwrap_or(meta);
        let session_meta = session_meta(&ctx.brief, meta.model.as_deref(), ctx.settings.as_ref());
        let pa = runtime::pty_agent(ctx.agent_id);
        let saved = meta.cli_session_id.clone().filter(|id| runtime::transcript_path(pa, &meta, &ctx.home, &ctx.cwd, id).is_file());
        let (sid, resp) = match saved {
            Some(id) => {
                *self.session.lock().unwrap() = Some(id.clone());
                self.replaying.store(true, Ordering::SeqCst);
                let r = client.request("session/load", json!({ "sessionId": id, "cwd": ctx.cwd, "mcpServers": [], "_meta": session_meta }), Some(SESSION_LIMIT));
                self.replaying.store(false, Ordering::SeqCst);
                // never a new session instead: the PTY runtime resumes the file
                let r = r.map_err(|e| match self.start_error(&e) {
                    Some(FallbackReason::StartFailed { message }) => Some(FallbackReason::LoadFailed { message }),
                    other => other,
                })?;
                (id, r)
            }
            None => {
                let r = client.request("session/new", json!({ "cwd": ctx.cwd, "mcpServers": [], "_meta": session_meta }), Some(SESSION_LIMIT)).map_err(|e| self.start_error(&e))?;
                let id = r["sessionId"].as_str().filter(|s| !s.is_empty()).ok_or(Some(FallbackReason::StartFailed { message: "세션 id가 없어요".into() }))?.to_string();
                *self.session.lock().unwrap() = Some(id.clone());
                self.update_meta(|m| m.cli_session_id = Some(id.clone()));
                (id, r)
            }
        };
        if self.logged_out.load(Ordering::SeqCst) {
            return Err(Some(FallbackReason::LoginRequired));
        }

        // always the worktree's own mode: the adapter's default may allow more
        let want = meta.permission_mode.clone();
        if !want.is_empty() && resp["modes"]["currentModeId"].as_str() != Some(want.as_str()) {
            let offered = resp["modes"]["availableModes"].as_array().is_none_or(|a| a.iter().any(|m| m["id"].as_str() == Some(want.as_str())));
            let set = offered && client.request("session/set_mode", json!({ "sessionId": sid, "modeId": want }), Some(CALL_LIMIT)).is_ok();
            if !set {
                let _ = client.request("session/close", json!({ "sessionId": sid }), Some(Duration::from_secs(5)));
                return Err(Some(FallbackReason::ModeRejected { mode: want }).filter(|_| !self.killed.load(Ordering::SeqCst)));
            }
        }
        // the worktree's model too: the adapter ranks ANTHROPIC_MODEL and settings.json's `model` above
        // `options.model` and re-applies that on a load, so the CLI view (`--model`) would run another model
        if let Some(want) = meta.model.clone().filter(|m| !m.is_empty()) {
            if current_model(&resp) != Some(want.as_str()) {
                let r = client.request("session/set_config_option", json!({ "sessionId": sid, "configId": "model", "value": want }), Some(CALL_LIMIT));
                if let Err(e) = r {
                    log(&format!("model {want} rejected: {}", e.message));
                    let _ = client.request("session/close", json!({ "sessionId": sid }), Some(Duration::from_secs(5)));
                    return Err(Some(FallbackReason::ModelRejected { model: want }).filter(|_| !self.killed.load(Ordering::SeqCst)));
                }
            }
        }
        if self.killed.load(Ordering::SeqCst) {
            return Err(None);
        }

        // the transcript: the whole file is the new epoch's record, then each new line live
        let file = runtime::transcript_path(pa, &meta, &ctx.home, &ctx.cwd, &sid);
        let mut tail = LiveTail::start_seeded(pa, &ctx.home, &ctx.cwd, ctx.files.meta.clone(), Some(sid), file, self.emitter.clone(), ctx.reason);
        tail.set_hook(Box::new(AcpHook { live: self.live.clone() }));
        *self.tail.lock().unwrap() = Some(tail);
        let (tx, rx) = mpsc::channel::<()>();
        *self.wake.lock().unwrap() = Some(tx);
        let weak = self.me.clone();
        thread::spawn(move || loop {
            let r = rx.recv_timeout(TICK);
            let Some(rt) = weak.upgrade() else { break };
            if let Some(t) = rt.tail.lock().unwrap().as_mut() {
                t.tick();
            }
            let gone = rt.client().is_none_or(|c| !c.alive());
            if gone || matches!(r, Err(RecvTimeoutError::Disconnected)) {
                break;
            }
        });

        self.was_ready.store(true, Ordering::SeqCst);
        self.set_phase(Phase::Ready);
        if self.killed.load(Ordering::SeqCst) || !client.alive() {
            // stopped or gone while the tail started: its exit was the boot's, now it's the agent's
            self.exited();
            return Ok(());
        }
        let me = self.clone();
        thread::spawn(move || me.sender());
        Ok(())
    }

    fn update_meta(&self, f: impl FnOnce(&mut crate::agent::AgentMeta)) {
        // read-modify-write: others rewrite meta too (summaries, links)
        if let Some(mut m) = self.ctx.files.read_meta() {
            f(&mut m);
            let _ = self.ctx.files.write_meta(&m);
        }
    }

    // ---------- from the adapter (reader thread: never waits on a response) ----------

    fn incoming(&self, c: &Arc<Client>, inc: Incoming) {
        match inc {
            Incoming::Notification { method, params } => match method.as_str() {
                "session/update" => self.update(&params),
                "_auth/status_update" => {
                    if params["authStatus"]["kind"] == "none" {
                        self.logged_out.store(true, Ordering::SeqCst);
                    }
                }
                "$/cancel_request" => {
                    if let Some(f) = self.perms.lock().unwrap().get(&params["requestId"].to_string()) {
                        f.store(true, Ordering::SeqCst);
                    }
                }
                _ => log(&format!("ignored {method}")),
            },
            Incoming::Request { id, method, params } => match method.as_str() {
                "session/request_permission" => self.permission(c, id, params),
                _ => c.respond_error(&id, -32601, "unsupported"),
            },
        }
    }

    fn update(&self, params: &Value) {
        let u = &params["update"];
        let kind = u["sessionUpdate"].as_str().unwrap_or("");
        let ours = self.session_id();
        let mine = ours.as_deref() == params["sessionId"].as_str();
        if kind == "available_commands_update" {
            // may come before the session id is known here (right after session/new's response)
            if ours.is_none() || mine {
                let list = u["availableCommands"].as_array().map(|a| a.iter().filter_map(|c| Some((c["name"].as_str()?.trim_start_matches('/').to_string(), c["description"].as_str().unwrap_or("").to_string()))).collect());
                *self.commands.lock().unwrap() = list;
                self.commands_cv.notify_all();
                (self.ctx.on_commands)();
            }
            return;
        }
        if !mine || self.replaying.load(Ordering::SeqCst) {
            return;
        }
        match kind {
            "agent_message_chunk" | "agent_thought_chunk" if u["content"]["type"] == "text" => {
                let channel = if kind == "agent_message_chunk" { DeltaChannel::Text } else { DeltaChannel::Thinking };
                self.live.lock().unwrap().chunk(channel, u["messageId"].as_str(), u["content"]["text"].as_str().unwrap_or(""));
            }
            "tool_call" => {
                if let Some(id) = u["toolCallId"].as_str() {
                    let name = tool_name(u).unwrap_or("?").to_string();
                    self.tools.lock().unwrap().insert(id.into(), (name, u["rawInput"].clone()));
                }
                self.live.lock().unwrap().boundary();
                self.wake();
            }
            "tool_call_update" => {
                if let Some(id) = u["toolCallId"].as_str() {
                    let mut tools = self.tools.lock().unwrap();
                    let t = tools.entry(id.into()).or_insert_with(|| ("?".into(), Value::Null));
                    if let Some(n) = tool_name(u) {
                        t.0 = n.into();
                    }
                    if !u["rawInput"].is_null() {
                        t.1 = u["rawInput"].clone();
                    }
                }
                if matches!(u["status"].as_str(), Some("completed" | "failed")) {
                    self.wake();
                }
            }
            // kept by the adapter; the PTY doesn't save mode changes either
            _ => log(&format!("ignored update {kind}")),
        }
    }

    /// The tool a permission prompt is about: the name and input its `tool_call` gave, else the prompt's own.
    fn tool_identity(&self, tc: &Value) -> (String, Value) {
        let id = tc["toolCallId"].as_str().unwrap_or("");
        let known = self.tools.lock().unwrap().get(id).cloned();
        let name = known.as_ref().map(|k| k.0.clone()).filter(|n| n != "?").or_else(|| tool_name(tc).map(String::from)).unwrap_or_else(|| "?".into());
        let input = Some(tc["rawInput"].clone()).filter(|v| !v.is_null()).or_else(|| known.map(|k| k.1).filter(|v| !v.is_null())).unwrap_or_else(|| json!({}));
        (name, input)
    }

    /// `session/request_permission` → an approval card. Answered on its own thread; whatever happens there, a
    /// reply goes back (`Reply`'s drop guard rejects), and silence never allows.
    fn permission(&self, c: &Arc<Client>, id: Value, params: Value) {
        let Some(me) = self.me.upgrade() else { return c.respond(&id, choose(None, false, &[])) };
        let key = id.to_string();
        let withdrawn = Arc::new(AtomicBool::new(false));
        self.perms.lock().unwrap().insert(key.clone(), withdrawn.clone());
        let client = c.clone();
        thread::spawn(move || {
            let options = params["options"].as_array().cloned().unwrap_or_default();
            let mut reply = Reply::new(client.clone(), id, options.clone());
            let tc = &params["toolCall"];
            let tool_call_id = tc["toolCallId"].as_str().map(String::from);
            let (tool_name, tool_input) = me.tool_identity(tc);
            let turn_id = me.locate_tool(&tool_name, &tool_input, tool_call_id.as_deref()).turn_id;
            let req = ApprovalReq { tool_name, tool_input, can_always: can_always(&options), turn_id, tool_call_id, cli_answer: false, wait_max: None };
            let still = || client.alive() && !withdrawn.load(Ordering::SeqCst) && !me.cancelling.load(Ordering::SeqCst);
            let choice = (me.ctx.ask)(req, &still);
            let cancelled = withdrawn.load(Ordering::SeqCst) || me.cancelling.load(Ordering::SeqCst);
            reply.send(choose(choice, cancelled, &options));
            me.perms.lock().unwrap().remove(&key);
        });
    }

    // ---------- prompts ----------

    /// The adapter's command names, waiting a little for its first list when a `/command` needs it.
    fn command_names(&self, need: bool) -> Option<Vec<String>> {
        let c = self.commands.lock().unwrap();
        let c = if need { self.commands_cv.wait_timeout_while(c, COMMANDS_WAIT, |c| c.is_none() && matches!(self.phase(), Phase::Ready)).unwrap().0 } else { c };
        c.as_ref().map(|l| l.iter().map(|(n, _)| n.clone()).collect())
    }

    fn set_model(&self, model: &str) -> Result<(), String> {
        let (Some(c), Some(sid)) = (self.client(), self.session_id()) else { return Err("에이전트가 아직 준비 중이에요".into()) };
        c.request("session/set_config_option", json!({ "sessionId": sid, "configId": "model", "value": model }), Some(CALL_LIMIT)).map(|_| ()).map_err(|e| format!("모델을 바꾸지 못했어요: {}", e.message))
    }

    /// One prompt at a time, in order; each waits for the previous turn's end (its response).
    fn sender(self: Arc<Self>) {
        loop {
            let q = {
                let mut s = self.send.lock().unwrap();
                loop {
                    if !matches!(self.phase(), Phase::Ready) {
                        return;
                    }
                    if s.inflight.is_none() {
                        if let Some(q) = s.queue.pop_front() {
                            s.inflight = Some(q.n);
                            break q;
                        }
                    }
                    s = self.send_cv.wait_timeout(s, Duration::from_millis(500)).unwrap().0;
                }
            };
            self.live.lock().unwrap().begin_prompt(q.n);
            let result = match (self.client(), self.session_id()) {
                (Some(c), Some(sid)) => {
                    self.responding.store(true, Ordering::SeqCst);
                    let r = match c.start_request("session/prompt", json!({ "sessionId": sid, "prompt": [{ "type": "text", "text": q.text }] })) {
                        Ok((_, rx)) => {
                            self.wake();
                            rx.recv().unwrap_or_else(|_| Err(RpcError { kind: RpcErrorKind::Exited { code: None }, message: "에이전트 프로세스가 끝났어요".into() }))
                        }
                        Err(e) => Err(e),
                    };
                    self.responding.store(false, Ordering::SeqCst);
                    r
                }
                _ => Err(RpcError { kind: RpcErrorKind::Io, message: "세션이 없어요".into() }),
            };
            let user_seen = self.settle(&q, result);
            // never delivered: the notes it carried are kept for the next start
            if !user_seen && !q.kept.is_empty() && !self.client().is_some_and(|c| c.alive()) {
                crate::control::keep_notes(&self.ctx.files.notes(), &q.kept);
            }
            self.send.lock().unwrap().inflight = None;
            self.send_cv.notify_all();
        }
    }

    /// The prompt's turn is over (its response came): read the file until it shows the end, else end the turn
    /// here (and show the prompt, if the file never got it). Returns whether the file had the prompt.
    fn settle(&self, q: &Queued, result: Result<Value, RpcError>) -> bool {
        let mut open = true;
        let mut user_seen = false;
        for wait in SETTLE_TICKS {
            thread::sleep(Duration::from_millis(wait));
            let mut t = self.tail.lock().unwrap();
            let Some(tail) = t.as_mut() else { break };
            tail.tick();
            open = tail.open_turn().is_some();
            user_seen = self.live.lock().unwrap().user_seen;
            if user_seen && !open {
                break;
            }
        }
        let synth = settle_plan(&result, open, user_seen, &self.client().map(|c| c.stderr_tail()).unwrap_or_default());
        if let Some(tail) = self.tail.lock().unwrap().as_mut() {
            match synth {
                Synth::Nothing => {}
                Synth::End { outcome, notice } => tail.close_turn(outcome, notice.as_ref().map(|(k, t)| (*k, t.as_str()))),
                Synth::PromptAndEnd { outcome, notice } => {
                    tail.synth_prompt(&format!("p:{}", q.n), &q.text);
                    tail.close_turn(outcome, notice.as_ref().map(|(k, t)| (*k, t.as_str())));
                }
            }
        }
        let cancelled = self.cancelling.load(Ordering::SeqCst) || matches!(&result, Ok(v) if v["stopReason"] == "cancelled");
        self.live.lock().unwrap().finish_prompt(!cancelled);
        self.cancelling.store(false, Ordering::SeqCst);
        user_seen
    }

    /// The adapter is gone. A running one going by itself counts as a crash (`acp_setup::record_crash`); prompts
    /// still queued are shown as not delivered, worker notes among them kept for the next start.
    fn exited(&self) {
        if !self.was_ready.load(Ordering::SeqCst) {
            // the boot thread sees the dead client and decides (fallback, or nothing when stopped)
            self.phase_cv.notify_all();
            return;
        }
        // once (the reader's EOF, or a boot that just got Ready)
        if self.exit_done.swap(true, Ordering::SeqCst) {
            return;
        }
        if !matches!(self.phase(), Phase::Failed(_)) {
            self.set_phase(Phase::Closed);
        }
        let queued: Vec<Queued> = self.send.lock().unwrap().queue.drain(..).collect();
        if let Some(tail) = self.tail.lock().unwrap().as_mut() {
            tail.tick();
            tail.exited();
            for q in &queued {
                if q.kept.is_empty() {
                    tail.synth_prompt(&format!("p:{}", q.n), &q.text);
                    tail.close_turn(TurnOutcome::Failed { error: "exited".into(), message: "에이전트가 꺼져 전달되지 않았어요".into() }, None);
                }
            }
        }
        let kept: Vec<(String, String)> = queued.into_iter().flat_map(|q| q.kept).collect();
        if !kept.is_empty() {
            crate::control::keep_notes(&self.ctx.files.notes(), &kept);
        }
        if !self.killed.load(Ordering::SeqCst) {
            acp_setup::record_crash(self.ctx.agent_id);
        }
        (self.on_exit)();
    }
}

impl Runtime for AcpRuntime {
    fn kind(&self) -> catalog::Runtime {
        catalog::Runtime::Acp
    }

    fn ready(&self) -> Result<Readiness, LaunchFailure> {
        let p = self.phase.lock().unwrap();
        let p = self.phase_cv.wait_while(p, |p| matches!(p, Phase::Booting)).unwrap();
        match &*p {
            Phase::Ready => Ok(Readiness::Ready),
            Phase::NeedsTrust => Ok(Readiness::NeedsTrust),
            Phase::Failed(f) => Err(f.clone()),
            _ => Err(LaunchFailure::Exited { message: "에이전트가 꺼졌어요".into() }),
        }
    }

    /// Doesn't wait: the session opens after it, and `ready()` waits for that. The phase leaves NeedsTrust here, not
    /// on the boot thread's wake-up: a `ready()` right after this (`deliver_kept_notes`, the chat's `agent_ready`)
    /// must wait for the session, not read the trust question again.
    fn accept_trust(&self) -> Result<(), String> {
        // through the lock, so the boot thread can't miss it between its check and its wait
        let mut p = self.phase.lock().unwrap();
        if *p == Phase::NeedsTrust {
            *p = Phase::Booting;
        }
        self.trust_ok.store(true, Ordering::SeqCst);
        drop(p);
        self.phase_cv.notify_all();
        self.send_cv.notify_all();
        self.commands_cv.notify_all();
        Ok(())
    }

    /// Only once the session is up (the facade waits for that): a prompt queued while booting would be lost with
    /// a fallback.
    fn prompt(&self, p: &Prompt) -> Result<Sent, String> {
        match self.phase() {
            Phase::Ready => {}
            Phase::Failed(_) | Phase::Closed => return Err("에이전트가 꺼져 있어요".into()),
            Phase::Booting | Phase::NeedsTrust => return Err("에이전트가 아직 준비 중이에요".into()),
        }
        let commands = self.command_names(p.text.trim_start().starts_with('/'));
        match gate(&p.text, commands.as_deref()) {
            Gate::Reject(m) => return Err(m),
            Gate::SetModel(m) => return self.set_model(&m).map(|_| Sent::Handled),
            Gate::Pass => {}
        }
        // attachments as @mentions, like the TUI paste: the transcript's bubble is the same
        let mut text = p.text.clone();
        for f in &p.files {
            text.push_str(&format!(" @{f}"));
        }
        // the adapter may have died since the check above (`command_names` waits up to 2 s): looked at again under
        // `send`, the lock `exited` drains the queue under once Closed is set. Either the prompt lands before the
        // drain (shown as not delivered, its notes kept) or it sees Closed here and the caller keeps them.
        let mut s = self.send.lock().unwrap();
        if !matches!(self.phase(), Phase::Ready) {
            return Err("에이전트가 꺼져 있어요".into());
        }
        let n = self.n.fetch_add(1, Ordering::SeqCst) + 1;
        s.queue.push_back(Queued { n, text, kept: p.kept.clone() });
        drop(s);
        self.send_cv.notify_all();
        Ok(Sent::Turn)
    }

    /// session/cancel for the open turn: a prompt's, or one Claude Code started by itself (a background task
    /// finished: the adapter's autonomous cycle, no prompt waiting, but the transcript shows it). Nothing open, nothing
    /// sent (the CLI would record an interruption of a turn that already ended). Open permission prompts are answered
    /// `cancelled` only along with a cancel; queued prompts stay (worker notes among them are never lost): they go
    /// after the cancelled turn.
    fn cancel(&self) -> Result<(), String> {
        let responding = self.responding.load(Ordering::SeqCst);
        let open = responding || self.tail.lock().unwrap().as_ref().and_then(|t| t.open_turn()).is_some();
        let (true, Some(c), Some(sid)) = (open, self.client(), self.session_id()) else { return Ok(()) };
        // only a prompt's turn has a response for `settle` to read this from
        if responding {
            self.cancelling.store(true, Ordering::SeqCst);
        }
        c.notify("session/cancel", json!({ "sessionId": sid })).map_err(|e| e.message)?;
        for f in self.perms.lock().unwrap().values() {
            f.store(true, Ordering::SeqCst);
        }
        Ok(())
    }

    fn kill(&self) {
        self.killed.store(true, Ordering::SeqCst);
        if let Some(c) = self.client() {
            c.kill();
        }
        if !matches!(self.phase(), Phase::Failed(_)) {
            self.set_phase(Phase::Closed);
        }
    }

    /// session/close (the session ends cleanly), then the whole tree.
    fn shutdown(&self) {
        self.killed.store(true, Ordering::SeqCst);
        if let (Some(c), Some(sid)) = (self.client(), self.session_id()) {
            if c.alive() {
                let _ = c.request("session/close", json!({ "sessionId": sid }), Some(Duration::from_secs(5)));
            }
            c.close(Duration::from_millis(500));
        } else if let Some(c) = self.client() {
            c.kill();
        }
        if !matches!(self.phase(), Phase::Failed(_)) {
            self.set_phase(Phase::Closed);
        }
    }

    fn alive(&self) -> bool {
        self.client().is_some_and(|c| c.alive()) && !matches!(self.phase(), Phase::Failed(_) | Phase::Closed)
    }

    /// node itself, until its exit is seen (shutdown and kill mark the session closed at once)
    fn process_alive(&self) -> bool {
        self.client().is_some_and(|c| c.alive())
    }

    fn settled(&self) -> bool {
        let s = self.send.lock().unwrap();
        s.queue.is_empty() && s.inflight.is_none()
    }

    fn commands(&self) -> Option<Vec<(String, String)>> {
        self.commands.lock().unwrap().clone()
    }

    fn quiet_for(&self) -> Duration {
        self.client().map_or(Duration::ZERO, |c| c.quiet_for())
    }

    /// node's; claude and its shells run below it
    fn pid(&self) -> Option<u32> {
        self.client().and_then(|c| c.pid())
    }

    fn started(&self) -> u64 {
        self.client().map_or(0, |c| c.started())
    }

    /// its job's count, when it has one
    fn tree_gone(&self) -> Option<bool> {
        self.client().and_then(|c| c.tree_gone())
    }

    fn sync(&self) {
        self.wake();
    }

    fn locate_tool(&self, name: &str, input: &Value, tool_call_id: Option<&str>) -> ToolMatch {
        match self.tail.lock().unwrap().as_mut() {
            Some(t) => {
                t.tick();
                t.locate(name, input, tool_call_id)
            }
            None => ToolMatch { tool_call_id: tool_call_id.map(String::from), ..Default::default() },
        }
    }

    fn history(&self) -> Option<(u64, Vec<RuntimeEvent>)> {
        self.emitter.history()
    }
}

fn log(s: &str) {
    if std::env::var("MAESTRO_ACP_LOGS").as_deref() == Ok("1") {
        eprintln!("[acp] {s}");
    }
}

fn read_json(p: &Path) -> Value {
    std::fs::read_to_string(p).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

/// Claude's `.claude.json`: in its config folder (`CLAUDE_CONFIG_DIR`) when it's there, else `~/.claude.json`.
fn claude_json(home: &Path) -> Value {
    let v = read_json(&claude::config_dir(home).join(".claude.json"));
    if v.is_null() { read_json(&home.join(".claude.json")) } else { v }
}

/// A tool's name as the adapter reports it: Claude's own name when it says, else the title.
fn tool_name(u: &Value) -> Option<&str> {
    u["_meta"]["claudeCode"]["toolName"].as_str().or_else(|| u["title"].as_str()).filter(|s| !s.is_empty())
}

/// The folder (or one above it) was trusted in the TUI (`~/.claude.json` `projects[<path>].hasTrustDialogAccepted`).
/// Read only: Maestro never writes Claude's config. Unreadable = not trusted.
pub fn trusted(claude_json: &Value, cwd: &str) -> bool {
    let norm = |p: &str| {
        let p = p.replace('\\', "/");
        let p = p.trim_end_matches('/').to_string();
        if cfg!(windows) { p.to_lowercase() } else { p }
    };
    let Some(projects) = claude_json["projects"].as_object() else { return false };
    let trusted: Vec<String> = projects.iter().filter(|(_, v)| v["hasTrustDialogAccepted"] == true).map(|(k, _)| norm(k)).collect();
    let mut p = norm(cwd);
    loop {
        if trusted.contains(&p) {
            return true;
        }
        match p.rfind('/') {
            Some(i) if i > 0 => p.truncate(i),
            _ => return false,
        }
    }
}

/// A linked worktree's main repository, from its `.git` file (`gitdir: <main>/.git/worktrees/<name>`): Claude Code
/// keeps a worktree's trust under the main repository's path, so the TUI never asks again there.
pub fn main_repo(cwd: &str) -> Option<String> {
    let text = std::fs::read_to_string(Path::new(cwd).join(".git")).ok()?;
    main_repo_of(&text)
}

fn main_repo_of(git_file: &str) -> Option<String> {
    let dir = git_file.lines().find_map(|l| l.strip_prefix("gitdir:"))?.trim().replace('\\', "/");
    let (dotgit, _) = dir.trim_end_matches('/').rsplit_once("/worktrees/")?;
    let main = dotgit.strip_suffix("/.git")?;
    (!main.is_empty()).then(|| main.to_string())
}

/// Trusted in the TUI: the folder itself, or (a worktree) its main repository.
pub fn trusted_worktree(claude_json: &Value, cwd: &str) -> bool {
    trusted(claude_json, cwd) || main_repo(cwd).is_some_and(|m| trusted(claude_json, &m))
}

/// `_meta` of session/new, load and fork: Maestro's brief appended to Claude Code's own system prompt (a plain
/// string would replace it), the `maestro` CLI allowed, the worktree's model, and Maestro's settings (the adapter
/// hands `options.settings` to the CLI as its `--settings` layer, so the guard's hook runs in every permission mode).
pub fn session_meta(brief: &str, model: Option<&str>, settings: Option<&Value>) -> Value {
    let mut options = json!({ "allowedTools": [ALLOWED] });
    if let Some(m) = model {
        options["model"] = json!(m);
    }
    if let Some(s) = settings {
        options["settings"] = s.clone();
    }
    json!({ "systemPrompt": { "append": brief }, "claudeCode": { "options": options } })
}

/// The model a session/new or session/load response says the session runs: `models.currentModelId`, else the
/// `model` config option's value.
pub fn current_model(resp: &Value) -> Option<&str> {
    resp["models"]["currentModelId"].as_str().or_else(|| resp["configOptions"].as_array()?.iter().find(|o| o["id"] == "model")?["currentValue"].as_str())
}

#[derive(Debug, PartialEq)]
pub enum Gate {
    Pass,
    /// `/model <name>` when the adapter has no such command: `session/set_config_option`
    SetModel(String),
    /// what the chat can't do; the message says to use the CLI view
    Reject(String),
}

/// What a chat message does under ACP. `commands`: the adapter's list, None = not known (let it through).
pub fn gate(text: &str, commands: Option<&[String]>) -> Gate {
    let t = text.trim_start();
    if t.starts_with('!') {
        return Gate::Reject("셸 명령(!)은 CLI 보기에서 실행해요".into());
    }
    let Some(rest) = t.strip_prefix('/') else { return Gate::Pass };
    let name = rest.split_whitespace().next().unwrap_or("");
    // a path or anything else that isn't a command name is just text
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':')) {
        return Gate::Pass;
    }
    let Some(list) = commands else { return Gate::Pass };
    if list.iter().any(|c| c == name) {
        return Gate::Pass;
    }
    let arg = rest[name.len()..].trim();
    if name == "model" && !arg.is_empty() {
        return Gate::SetModel(arg.into());
    }
    Gate::Reject(format!("/{name} 명령은 채팅에서 쓸 수 없어요. CLI 보기에서 실행해 주세요."))
}

/// Option ids the "always" answer may pick, best first: the rule Claude suggested, a skill's exact rule, leaving
/// plan mode to accept edits. Never one that clears the context or raises the mode beyond accepting edits.
const ALWAYS: [&str; 3] = ["allow-with-updates", "allow-skill-exact", "exit-plan-accept-edits"];

fn option_id<'a>(options: &'a [Value], pred: impl Fn(&Value) -> bool) -> Option<&'a str> {
    options.iter().find(|o| pred(o)).and_then(|o| o["optionId"].as_str())
}

fn always_option(options: &[Value]) -> Option<&str> {
    ALWAYS.iter().find_map(|id| option_id(options, |o| o["optionId"] == *id)).or_else(|| option_id(options, |o| o["kind"] == "allow_always" && !o["optionId"].as_str().unwrap_or("").starts_with("exit-plan-")))
}

/// An "always" answer means more than "allow" here.
pub fn can_always(options: &[Value]) -> bool {
    always_option(options).is_some()
}

/// The reply to `session/request_permission`. Only an offered option is ever named; "allow" never grants more
/// than once; no answer is a rejection, `cancelled` only after a cancel (in a foreground turn the adapter takes
/// `cancelled` as "stop the tool", acp-agent.js 8280-8290).
pub fn choose(choice: Option<Choice>, cancelled: bool, options: &[Value]) -> Value {
    let pick = |id: &str| json!({ "outcome": { "outcome": "selected", "optionId": id } });
    let gone = json!({ "outcome": { "outcome": "cancelled" } });
    let reject = || option_id(options, |o| o["kind"] == "reject_once").or_else(|| option_id(options, |o| o["kind"] == "reject_always")).map(pick).unwrap_or_else(|| gone.clone());
    let once = option_id(options, |o| o["kind"] == "allow_once");
    match choice {
        Some(Choice::Allow) => once.map(pick).unwrap_or_else(reject),
        Some(Choice::Always) => always_option(options).or(once).map(pick).unwrap_or_else(reject),
        Some(Choice::Deny) | Some(Choice::Cli) => reject(),
        None if cancelled => gone,
        None => reject(),
    }
}

/// Answers a permission request exactly once: what `send` says, else (a panic, an early return) a rejection.
pub struct Reply {
    client: Arc<Client>,
    id: Value,
    options: Vec<Value>,
    sent: bool,
}

impl Reply {
    pub fn new(client: Arc<Client>, id: Value, options: Vec<Value>) -> Reply {
        Reply { client, id, options, sent: false }
    }

    pub fn send(&mut self, outcome: Value) {
        self.client.respond(&self.id, outcome);
        self.sent = true;
    }
}

impl Drop for Reply {
    fn drop(&mut self) {
        if !self.sent {
            self.client.respond(&self.id, choose(None, false, &self.options));
        }
    }
}

/// What the runtime adds once a prompt's response came and the file didn't end the turn.
#[derive(Debug, PartialEq)]
pub enum Synth {
    Nothing,
    End { outcome: TurnOutcome, notice: Option<(NoticeKind, String)> },
    /// the file never got the prompt: show it, then its end
    PromptAndEnd { outcome: TurnOutcome, notice: Option<(NoticeKind, String)> },
}

/// `turn_open`: the file's turn is still open; `user_seen`: the file has the prompt. A failure carries no notice:
/// `close_turn` shows its message as a `failed` notice.
pub fn settle_plan(result: &Result<Value, RpcError>, turn_open: bool, user_seen: bool, stderr: &str) -> Synth {
    if user_seen && !turn_open {
        return Synth::Nothing;
    }
    let (outcome, notice) = match result {
        Ok(v) => match v["stopReason"].as_str().unwrap_or("end_turn") {
            // like Esc in the TUI
            "cancelled" => (TurnOutcome::Cancelled, Some((NoticeKind::Interrupted, "중단됨".to_string()))),
            stop => (TurnOutcome::Done { stop_reason: stop.into() }, None),
        },
        Err(e) if e.code() == Some(-32000) => (TurnOutcome::Failed { error: "auth".into(), message: "로그인이 필요해요. CLI 보기에서 /login 해 주세요".into() }, None),
        Err(e) => {
            let data = match &e.kind {
                RpcErrorKind::Remote { data: Some(d), .. } => d["message"].as_str().map(String::from),
                _ => None,
            };
            let last = stderr.lines().map(str::trim).filter(|l| !l.is_empty()).last().map(String::from);
            let message = data.or_else(|| Some(e.message.clone()).filter(|m| !m.is_empty())).or(last).unwrap_or_else(|| "에이전트 오류".into());
            (TurnOutcome::Failed { error: "acp".into(), message }, None)
        }
    };
    if user_seen {
        Synth::End { outcome, notice }
    } else {
        Synth::PromptAndEnd { outcome, notice }
    }
}

// ---------- live text ----------

/// One streamed text (or thinking) block, until its block lands in the transcript.
struct Seg {
    item_id: String,
    channel: DeltaChannel,
    message_id: Option<String>,
    text: String,
    /// bytes and UTF-16 units of `text` already sent
    sent: usize,
    sent16: u64,
    retired: bool,
}

/// The adapter's streamed text as live items (`live:<n>:<k>`), next to the transcript's durable ones: deltas while
/// it streams, then an empty delta at offset 0 (the chat drops blank text) when its durable block arrives, so the
/// chat never shows both. Held back until the transcript has the prompt, so it never draws above the bubble.
pub struct LiveText {
    segs: Vec<Seg>,
    open: Option<usize>,
    prompt: Option<u64>,
    pub user_seen: bool,
    k: u64,
    turn: Option<String>,
    /// the turn that ended last (a leftover note goes there)
    last_turn: Option<String>,
    /// a durable text block came during this prompt
    durable_text: bool,
    push: Box<dyn Fn(RuntimeEvent) + Send>,
}

fn utf16(s: &str) -> u64 {
    s.encode_utf16().count() as u64
}

impl LiveText {
    pub fn new(push: Box<dyn Fn(RuntimeEvent) + Send>) -> LiveText {
        LiveText { segs: vec![], open: None, prompt: None, user_seen: false, k: 0, turn: None, last_turn: None, durable_text: false, push }
    }

    pub fn begin_prompt(&mut self, n: u64) {
        self.prompt = Some(n);
        self.user_seen = false;
        self.open = None;
        self.durable_text = false;
    }

    fn holding(&self) -> bool {
        self.prompt.is_some() && !self.user_seen
    }

    fn emit(&mut self, i: usize) {
        let turn_id = self.turn.clone().unwrap_or_else(|| format!("p:{}", self.prompt.unwrap_or(0)));
        let s = &mut self.segs[i];
        if s.retired || s.sent >= s.text.len() {
            return;
        }
        let text = s.text[s.sent..].to_string();
        (self.push)(RuntimeEvent::Delta { turn_id, item_id: s.item_id.clone(), channel: s.channel, offset: s.sent16, text: text.clone() });
        s.sent = s.text.len();
        s.sent16 += utf16(&text);
    }

    pub fn chunk(&mut self, channel: DeltaChannel, message_id: Option<&str>, text: &str) {
        if text.is_empty() {
            return;
        }
        let same = self.open.filter(|&i| self.segs[i].channel == channel && self.segs[i].message_id.as_deref() == message_id);
        let i = match same {
            Some(i) => i,
            None => {
                self.k += 1;
                self.segs.push(Seg { item_id: format!("live:{}:{}", self.prompt.unwrap_or(0), self.k), channel, message_id: message_id.map(String::from), text: String::new(), sent: 0, sent16: 0, retired: false });
                self.open = Some(self.segs.len() - 1);
                self.segs.len() - 1
            }
        };
        // its block is in the transcript already: a late piece of it is dropped
        if self.segs[i].retired {
            return;
        }
        self.segs[i].text.push_str(text);
        if !self.holding() {
            self.emit(i);
        }
    }

    /// A tool call: the next text is a new block.
    pub fn boundary(&mut self) {
        self.open = None;
    }

    /// The transcript has the prompt: what was held goes out now, after its bubble.
    pub fn saw_user(&mut self) {
        self.user_seen = true;
        for i in 0..self.segs.len() {
            self.emit(i);
        }
    }

    fn retire(&mut self, i: usize) {
        let turn_id = self.turn.clone().unwrap_or_else(|| format!("p:{}", self.prompt.unwrap_or(0)));
        let s = &mut self.segs[i];
        if s.retired {
            return;
        }
        s.retired = true;
        if s.sent16 > 0 {
            (self.push)(RuntimeEvent::Delta { turn_id, item_id: s.item_id.clone(), channel: s.channel, offset: 0, text: String::new() });
        }
    }

    /// A durable block arrived: the oldest live block of its channel that matches it (one's text starts the
    /// other's: streaming may lag or lead the file) goes.
    pub fn retire_match(&mut self, channel: DeltaChannel, durable: &str) {
        let d = durable.trim();
        if d.is_empty() {
            return;
        }
        if channel == DeltaChannel::Text {
            self.durable_text = true;
        }
        let hit = self.segs.iter().position(|s| {
            let t = s.text.trim();
            !s.retired && s.channel == channel && !t.is_empty() && (d.starts_with(t) || t.starts_with(d))
        });
        if let Some(i) = hit {
            self.retire(i);
        }
    }

    /// The prompt's turn is over (its response came and the file was read after it; a turn ended by the file
    /// alone when no prompt is in flight): every live block goes. Not at the file's own turn end while a prompt is
    /// in flight: Claude may write the stop reason on an earlier block's line, before the last text block.
    /// Returns the text of the ones no durable text came for in a prompt whose file got no text at all (output
    /// only the adapter shows, e.g. a command's), to keep as a note.
    pub fn retire_all(&mut self) -> Option<String> {
        let leftover: Vec<String> = self.segs.iter().filter(|s| !s.retired && s.channel == DeltaChannel::Text && !s.text.trim().is_empty()).map(|s| s.text.trim().to_string()).collect();
        for i in 0..self.segs.len() {
            self.retire(i);
        }
        self.segs.clear();
        self.open = None;
        let keep = self.prompt.is_some() && !self.durable_text && !leftover.is_empty();
        keep.then(|| leftover.join("\n\n"))
    }
}

/// The tail's hook under ACP: keeps the live text in step with the transcript.
struct AcpHook {
    live: Arc<Mutex<LiveText>>,
}

impl TailHook for AcpHook {
    fn before(&mut self, ev: &RuntimeEvent) {
        let mut l = self.live.lock().unwrap();
        match ev {
            RuntimeEvent::AgentText { text, .. } => l.retire_match(DeltaChannel::Text, text),
            RuntimeEvent::Thinking { text, .. } => l.retire_match(DeltaChannel::Thinking, text),
            _ => {}
        }
    }

    fn after(&mut self, ev: &RuntimeEvent) {
        let mut l = self.live.lock().unwrap();
        match ev {
            RuntimeEvent::TurnStarted { turn_id, .. } => l.turn = Some(turn_id.clone()),
            RuntimeEvent::UserMessage { .. } if l.holding() => l.saw_user(),
            RuntimeEvent::TurnEnded { turn_id, .. } => {
                l.turn = None;
                l.last_turn = Some(turn_id.clone());
                // a turn of the agent's own (no prompt of ours in flight): nothing settles it later
                if l.prompt.is_none() {
                    l.retire_all();
                }
            }
            _ => {}
        }
    }
}

impl LiveText {
    /// After a prompt settled: the live blocks left go; output only the adapter showed stays as a note, unless
    /// the turn was cancelled (its partial text is dropped, as in the TUI).
    pub fn finish_prompt(&mut self, keep: bool) {
        if let Some(text) = self.retire_all().filter(|_| keep) {
            let turn_id = self.last_turn.clone().unwrap_or_else(|| format!("p:{}", self.prompt.unwrap_or(0)));
            (self.push)(RuntimeEvent::Notice { turn_id: turn_id.clone(), item_id: format!("{turn_id}:acp-out"), kind: NoticeKind::LocalCommand, text });
        }
        self.prompt = None;
    }
}

/// A new conversation with the history of `session_id` (`session/fork` in a fresh adapter): for a switch whose old
/// side wouldn't stop, after the user agreed. Returns the new id.
pub fn fork_session(plan: &AcpPlan, env: &(Vec<String>, Vec<(String, String)>), cwd: &str, session_id: &str, brief: &str, model: Option<&str>) -> Result<String, String> {
    let cmd = acp_setup::adapter_command(&plan.node, &plan.entry, Path::new(cwd), env);
    let (client, _) = acp_setup::handshake(cmd, INIT_LIMIT)?;
    // runs no tools: the session's next load carries the settings
    let r = client.request("session/fork", json!({ "sessionId": session_id, "cwd": cwd, "mcpServers": [], "_meta": session_meta(brief, model, None) }), Some(SESSION_LIMIT));
    let id = r.as_ref().ok().and_then(|v| v["sessionId"].as_str()).map(String::from);
    if let Some(id) = &id {
        let _ = client.request("session/close", json!({ "sessionId": id }), Some(Duration::from_secs(5)));
    }
    client.close(Duration::from_millis(500));
    match (id, r) {
        (Some(id), _) => Ok(id),
        (None, Err(e)) => Err(format!("갈래를 만들 수 없어요: {}", e.message)),
        (None, Ok(_)) => Err("갈래를 만들 수 없어요".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::tests::pair;
    use crate::events::oracle;

    fn opt(id: &str, kind: &str) -> Value {
        json!({ "optionId": id, "name": id, "kind": kind })
    }

    fn selected(v: &Value) -> Option<&str> {
        v["outcome"]["optionId"].as_str()
    }

    #[test]
    fn gates_what_the_chat_cant_do() {
        let list: Vec<String> = ["compact", "review", "git:pr"].iter().map(|s| s.to_string()).collect();
        assert!(matches!(gate("!ls", Some(&list)), Gate::Reject(m) if m.contains("CLI 보기")));
        assert!(matches!(gate("  !ls", None), Gate::Reject(_)));
        assert_eq!(gate("/compact", Some(&list)), Gate::Pass);
        assert_eq!(gate("/git:pr now", Some(&list)), Gate::Pass);
        assert_eq!(gate("/model opus", Some(&list)), Gate::SetModel("opus".into()));
        assert!(matches!(gate("/model", Some(&list)), Gate::Reject(_)));
        assert!(matches!(gate("/config", Some(&list)), Gate::Reject(m) if m.starts_with("/config 명령은")));
        // the list not known (yet): let it through
        assert_eq!(gate("/config", None), Gate::Pass);
        // text, not a command
        assert_eq!(gate("hello", Some(&list)), Gate::Pass);
        assert_eq!(gate("/usr/bin/env 를 봐줘", Some(&list)), Gate::Pass);
        assert_eq!(gate("/", Some(&list)), Gate::Pass);
    }

    #[test]
    fn permission_answers_never_grant_more_than_asked() {
        let bash = [opt("allow-once", "allow_once"), opt("allow-with-updates", "allow_always"), opt("reject", "reject_once")];
        assert_eq!(selected(&choose(Some(Choice::Allow), false, &bash)), Some("allow-once"));
        assert_eq!(selected(&choose(Some(Choice::Always), false, &bash)), Some("allow-with-updates"));
        assert_eq!(selected(&choose(Some(Choice::Deny), false, &bash)), Some("reject"));
        assert_eq!(selected(&choose(Some(Choice::Cli), false, &bash)), Some("reject"));
        assert!(can_always(&bash));
        // no answer: rejected; after a cancel: cancelled
        assert_eq!(selected(&choose(None, false, &bash)), Some("reject"));
        assert_eq!(choose(None, true, &bash), json!({"outcome": {"outcome": "cancelled"}}));
        // only "always" and "once" offered, no reject: allow never becomes always; no answer is cancelled
        let no_reject = [opt("allow-with-updates", "allow_always")];
        assert_eq!(choose(Some(Choice::Allow), false, &no_reject), json!({"outcome": {"outcome": "cancelled"}}));
        assert_eq!(selected(&choose(Some(Choice::Always), false, &no_reject)), Some("allow-with-updates"));
        assert_eq!(selected(&choose(None, false, &[opt("r", "reject_always")])), Some("r"));
        // leaving plan mode: allow = manual approval; always never clears the context or raises to auto/bypass
        let plan = [opt("exit-plan-clear-auto", "allow_always"), opt("exit-plan-auto", "allow_always"), opt("exit-plan-bypass", "allow_always"), opt("exit-plan-default", "allow_once"), opt("reject", "reject_once")];
        assert_eq!(selected(&choose(Some(Choice::Allow), false, &plan)), Some("exit-plan-default"));
        assert_eq!(selected(&choose(Some(Choice::Always), false, &plan)), Some("exit-plan-default"));
        assert!(!can_always(&plan));
        let plan2 = [opt("exit-plan-accept-edits", "allow_always"), opt("exit-plan-default", "allow_once"), opt("reject", "reject_once")];
        assert_eq!(selected(&choose(Some(Choice::Always), false, &plan2)), Some("exit-plan-accept-edits"));
        // an option nobody offered is never named
        for c in [Some(Choice::Allow), Some(Choice::Always), Some(Choice::Deny), None] {
            let v = choose(c, false, &bash);
            assert!(selected(&v).is_none_or(|id| bash.iter().any(|o| o["optionId"] == id)));
        }
    }

    #[test]
    fn a_dropped_reply_rejects() {
        let (c, fake) = pair(|_, _| {}, |_| {});
        {
            let _r = Reply::new(c.clone(), json!(5), vec![opt("allow-once", "allow_once"), opt("reject", "reject_once")]);
            // dropped without an answer (a panic, an early return)
        }
        let mut r = Reply::new(c.clone(), json!("x"), vec![opt("allow-once", "allow_once")]);
        r.send(choose(Some(Choice::Allow), false, &[opt("allow-once", "allow_once")]));
        drop(r);
        let f = fake.wait_frames(2);
        assert_eq!(f.len(), 2);
        assert_eq!((f[0]["id"].clone(), selected(&f[0]["result"])), (json!(5), Some("reject")));
        assert_eq!(selected(&f[1]["result"]), Some("allow-once"));
    }

    #[test]
    fn settles_what_the_file_didnt_say() {
        let ok = |s: &str| Ok(json!({ "stopReason": s }));
        assert_eq!(settle_plan(&ok("end_turn"), false, true, ""), Synth::Nothing);
        assert_eq!(settle_plan(&ok("end_turn"), true, true, ""), Synth::End { outcome: TurnOutcome::Done { stop_reason: "end_turn".into() }, notice: None });
        assert_eq!(settle_plan(&ok("max_tokens"), true, true, ""), Synth::End { outcome: TurnOutcome::Done { stop_reason: "max_tokens".into() }, notice: None });
        assert_eq!(settle_plan(&ok("cancelled"), true, true, ""), Synth::End { outcome: TurnOutcome::Cancelled, notice: Some((NoticeKind::Interrupted, "중단됨".into())) });
        // the file never got the prompt: shown anyway
        assert_eq!(settle_plan(&ok("cancelled"), false, false, ""), Synth::PromptAndEnd { outcome: TurnOutcome::Cancelled, notice: Some((NoticeKind::Interrupted, "중단됨".into())) });
        let auth = Err(RpcError { kind: RpcErrorKind::Remote { code: -32000, data: None }, message: "Authentication required".into() });
        assert!(matches!(settle_plan(&auth, true, true, ""), Synth::End { outcome: TurnOutcome::Failed { error, .. }, .. } if error == "auth"));
        let internal = Err(RpcError { kind: RpcErrorKind::Remote { code: -32603, data: Some(json!({"message": "overloaded"})) }, message: "Internal error".into() });
        assert_eq!(settle_plan(&internal, true, true, ""), Synth::End { outcome: TurnOutcome::Failed { error: "acp".into(), message: "overloaded".into() }, notice: None });
        let exited = Err(RpcError { kind: RpcErrorKind::Exited { code: Some(1) }, message: String::new() });
        assert_eq!(settle_plan(&exited, true, false, "x\nError: boom\n"), Synth::PromptAndEnd { outcome: TurnOutcome::Failed { error: "acp".into(), message: "Error: boom".into() }, notice: None });
        // (a failure's notice is added by `close_turn`: a `failed` one with the message, transcript.rs `failure_note`)
    }

    #[test]
    fn session_meta_appends_the_brief() {
        let m = session_meta("BRIEF", None, None);
        assert_eq!(m["systemPrompt"], json!({ "append": "BRIEF" }));
        assert_eq!(m["claudeCode"]["options"], json!({ "allowedTools": ["Bash(maestro:*)"] }));
        assert_eq!(session_meta("b", Some("opus"), None)["claudeCode"]["options"]["model"], "opus");
        // Maestro's settings (the guard) go inline, next to the allow list
        let s = json!({ "hooks": { "PreToolUse": [] }, "permissions": { "deny": ["Bash(git push:*)"] } });
        let with = session_meta("b", None, Some(&s));
        assert_eq!(with["claudeCode"]["options"]["settings"], s);
        assert_eq!(with["claudeCode"]["options"]["allowedTools"], json!(["Bash(maestro:*)"]));
        assert_eq!(current_model(&json!({ "models": { "currentModelId": "opus" } })), Some("opus"));
        assert_eq!(current_model(&json!({ "configOptions": [{ "id": "mode", "currentValue": "x" }, { "id": "model", "currentValue": "haiku" }] })), Some("haiku"));
        assert_eq!(current_model(&json!({})), None);
    }

    #[test]
    fn worktree_trust_follows_its_main_repo() {
        assert_eq!(main_repo_of("gitdir: C:/Users/u/repo/.git/worktrees/agent-1\n").as_deref(), Some("C:/Users/u/repo"));
        assert_eq!(main_repo_of("gitdir: C:\\Users\\u\\repo\\.git\\worktrees\\agent-1\r\n").as_deref(), Some("C:/Users/u/repo"));
        assert_eq!(main_repo_of("gitdir: /x/.git/modules/sub"), None);
        assert_eq!(main_repo_of("ref: refs/heads/main"), None);
        let dir = std::env::temp_dir().join(format!("maestro-trust-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".git"), "gitdir: C:/Users/u/repo/.git/worktrees/w\n").unwrap();
        let cfg = json!({ "projects": { "C:/Users/u/repo": { "hasTrustDialogAccepted": true } } });
        let cwd = dir.to_string_lossy().to_string();
        assert!(!trusted(&cfg, &cwd));
        assert!(trusted_worktree(&cfg, &cwd));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn trust_is_read_from_claudes_config() {
        let cfg = json!({ "projects": { "C:/Users/u/repo": { "hasTrustDialogAccepted": true }, "C:/Users/u/other": { "hasTrustDialogAccepted": false } } });
        assert!(trusted(&cfg, "C:/Users/u/repo"));
        assert!(trusted(&cfg, "C:\\Users\\u\\repo\\"));
        // under a trusted folder
        assert!(trusted(&cfg, "C:/Users/u/repo/sub/deeper"));
        assert!(!trusted(&cfg, "C:/Users/u/other"));
        assert!(!trusted(&cfg, "C:/Users/u/repo2"));
        assert!(!trusted(&Value::Null, "C:/Users/u/repo"));
        if cfg!(windows) {
            assert!(trusted(&cfg, "c:/users/U/Repo"));
        }
    }

    fn collect() -> (Arc<Mutex<Vec<RuntimeEvent>>>, LiveText) {
        let got = Arc::new(Mutex::new(vec![]));
        let g = got.clone();
        (got, LiveText::new(Box::new(move |ev| g.lock().unwrap().push(ev))))
    }

    /// What chatFold.ts (`reduceChat`) draws with live deltas (oracle::items has none): text items by item id, grown at
    /// their offset
    pub(super) fn fold(events: &[RuntimeEvent]) -> Vec<oracle::Item> {
        let mut texts: Vec<(String, Vec<u16>)> = vec![];
        let mut rest = vec![];
        let mut order: Vec<(bool, usize)> = vec![];
        for ev in events {
            match ev {
                RuntimeEvent::Delta { item_id, channel: DeltaChannel::Text, offset, text, .. } => {
                    let i = match texts.iter().position(|(id, _)| id == item_id) {
                        Some(i) => i,
                        None => {
                            texts.push((item_id.clone(), vec![]));
                            order.push((true, texts.len() - 1));
                            texts.len() - 1
                        }
                    };
                    texts[i].1.truncate(*offset as usize);
                    texts[i].1.extend(text.encode_utf16());
                }
                RuntimeEvent::Delta { .. } => {}
                ev => {
                    rest.push(ev.clone());
                    order.push((false, rest.len() - 1));
                }
            }
        }
        // live texts, then the durable items; blank text draws nothing
        let mut out: Vec<oracle::Item> = texts.iter().map(|(_, u)| String::from_utf16(u).unwrap()).filter(|t| !t.trim().is_empty()).map(|text| oracle::Item::Text { text }).collect();
        out.extend(oracle::items(&rest));
        out
    }

    fn text(t: &str, item: &str) -> RuntimeEvent {
        RuntimeEvent::AgentText { turn_id: "p".into(), item_id: item.into(), message_id: None, text: t.into() }
    }

    #[test]
    fn live_text_streams_with_utf16_offsets() {
        let (got, mut l) = collect();
        l.chunk(DeltaChannel::Text, Some("m1"), "안녕");
        l.chunk(DeltaChannel::Text, Some("m1"), "😀!");
        let got = got.lock().unwrap();
        assert!(matches!(&got[0], RuntimeEvent::Delta { offset: 0, text, item_id, .. } if text == "안녕" && item_id == "live:0:1"));
        assert!(matches!(&got[1], RuntimeEvent::Delta { offset: 2, text, .. } if text == "😀!"));
    }

    #[test]
    fn live_text_waits_for_the_prompt_in_the_file() {
        let (got, mut l) = collect();
        l.begin_prompt(3);
        l.chunk(DeltaChannel::Text, Some("m1"), "he");
        l.chunk(DeltaChannel::Text, Some("m1"), "llo");
        assert!(got.lock().unwrap().is_empty());
        l.saw_user();
        assert!(matches!(&got.lock().unwrap()[..], [RuntimeEvent::Delta { offset: 0, text, item_id, .. }] if text == "hello" && item_id == "live:3:1"));
        l.chunk(DeltaChannel::Text, Some("m1"), "!");
        assert!(matches!(got.lock().unwrap().last(), Some(RuntimeEvent::Delta { offset: 5, text, .. }) if text == "!"));
    }

    #[test]
    fn live_blocks_end_at_tools_channels_and_messages() {
        let (got, mut l) = collect();
        l.chunk(DeltaChannel::Thinking, Some("m1"), "hmm");
        l.chunk(DeltaChannel::Text, Some("m1"), "a");
        l.boundary();
        l.chunk(DeltaChannel::Text, Some("m1"), "b");
        l.chunk(DeltaChannel::Text, Some("m2"), "c");
        let ids: Vec<String> = got.lock().unwrap().iter().map(|e| match e { RuntimeEvent::Delta { item_id, .. } => item_id.clone(), _ => String::new() }).collect();
        assert_eq!(ids, ["live:0:1", "live:0:2", "live:0:3", "live:0:4"]);
    }

    #[test]
    fn durable_blocks_retire_their_live_text() {
        let (got, mut l) = collect();
        l.begin_prompt(1);
        l.saw_user();
        l.chunk(DeltaChannel::Text, Some("m1"), "Hello wor");
        l.boundary();
        l.chunk(DeltaChannel::Text, Some("m2"), "Second block, longer than the file says");
        // streaming lagged: the file has more; then led: the file has less
        l.retire_match(DeltaChannel::Text, "Hello world");
        l.retire_match(DeltaChannel::Text, "Second block");
        // a late piece of a retired block is dropped
        l.boundary();
        l.chunk(DeltaChannel::Text, Some("m1"), "x");
        let all = got.lock().unwrap().clone();
        let retired: Vec<&RuntimeEvent> = all.iter().filter(|e| matches!(e, RuntimeEvent::Delta { offset: 0, text, .. } if text.is_empty())).collect();
        assert_eq!(retired.len(), 2);
        // a block nothing matches stays until the turn ends
        l.chunk(DeltaChannel::Text, Some("m3"), "unrelated");
        l.retire_match(DeltaChannel::Text, "something else");
        assert_eq!(l.retire_all(), None); // durable text came this prompt: no leftover note
        let mut events = got.lock().unwrap().clone();
        events.extend([text("Hello world", "a1#0"), text("Second block", "a2#0")]);
        // live text then the durable blocks: the chat shows only the durable ones
        assert_eq!(fold(&events), oracle::items(&[text("Hello world", "a1#0"), text("Second block", "a2#0")]));
    }

    #[test]
    fn output_only_the_adapter_shows_is_kept_as_a_note() {
        let (got, mut l) = collect();
        l.begin_prompt(2);
        l.saw_user();
        l.chunk(DeltaChannel::Text, Some("m1"), "Context: 12% used");
        assert_eq!(l.retire_all().as_deref(), Some("Context: 12% used"));
        assert!(matches!(got.lock().unwrap().last(), Some(RuntimeEvent::Delta { offset: 0, text, .. }) if text.is_empty()));
        // as a note, in the turn that ended last
        l.chunk(DeltaChannel::Text, Some("m1b"), "Context: 13% used");
        l.last_turn = Some("t9".into());
        l.finish_prompt(true);
        assert!(matches!(got.lock().unwrap().last(), Some(RuntimeEvent::Notice { turn_id, text, .. }) if turn_id == "t9" && text == "Context: 13% used"));
        // a cancelled turn's partial text goes without a note
        let before = got.lock().unwrap().len();
        l.begin_prompt(3);
        l.saw_user();
        l.chunk(DeltaChannel::Text, Some("m3"), "Tea was first");
        l.finish_prompt(false);
        assert!(!got.lock().unwrap()[before..].iter().any(|e| matches!(e, RuntimeEvent::Notice { .. })));
        // not within a prompt (a background turn): nothing kept
        l.chunk(DeltaChannel::Text, Some("m2"), "late");
        assert_eq!(l.retire_all(), None);
    }

    #[test]
    fn the_hook_swaps_live_for_durable_in_one_order() {
        let (got, l) = collect();
        let live = Arc::new(Mutex::new(l));
        let mut hook = AcpHook { live: live.clone() };
        live.lock().unwrap().begin_prompt(1);
        live.lock().unwrap().chunk(DeltaChannel::Text, Some("m"), "pong");
        let user = RuntimeEvent::UserMessage { turn_id: "p".into(), item_id: "u".into(), kind: crate::events::UserKind::Prompt, text: "ping".into(), files: vec![] };
        let started = RuntimeEvent::TurnStarted { turn_id: "p".into(), initiator: crate::events::Initiator::User };
        let thinking = RuntimeEvent::Thinking { turn_id: "p".into(), item_id: "a0#0".into(), message_id: None, text: String::new() };
        let mut sent = vec![];
        // as Claude writes it: the stop reason on the thinking line, the text block after the turn's end
        let ended = RuntimeEvent::TurnEnded { turn_id: "p".into(), outcome: TurnOutcome::Done { stop_reason: "end_turn".into() } };
        for ev in [started, user, thinking, ended, text("pong", "a#0")] {
            hook.before(&ev);
            got.lock().unwrap().push(ev.clone());
            sent.push(ev.clone());
            hook.after(&ev);
        }
        let all = got.lock().unwrap().clone();
        // held until the bubble, then retired right before its durable block
        let pos = |f: &dyn Fn(&RuntimeEvent) -> bool| all.iter().position(|e| f(e)).unwrap();
        let bubble = pos(&|e| matches!(e, RuntimeEvent::UserMessage { .. }));
        let first = pos(&|e| matches!(e, RuntimeEvent::Delta { text, .. } if text == "pong"));
        let retire = pos(&|e| matches!(e, RuntimeEvent::Delta { text, .. } if text.is_empty()));
        let durable = pos(&|e| matches!(e, RuntimeEvent::AgentText { .. }));
        assert!(bubble < first && first < retire && retire < durable);
        assert!(matches!(&all[first], RuntimeEvent::Delta { turn_id, .. } if turn_id == "p"));
        // the prompt settles: nothing left, no note
        live.lock().unwrap().finish_prompt(true);
        assert_eq!(got.lock().unwrap().len(), all.len());
        assert_eq!(fold(&all), oracle::items(&sent));
    }
}

/// Against the real adapter and the user's logged-in `claude` (network, Haiku, tiny prompts):
/// `MAESTRO_ACP_CLAUDE_DIR=<folder with node_modules/@agentclientprotocol/claude-agent-acp> cargo test -- --ignored acp_live --nocapture --test-threads=1`
#[cfg(test)]
mod live {
    use super::*;
    use crate::acp::Incoming;
    use crate::agent::AgentMeta;
    use crate::events::{oracle, Envelope};
    use crate::transcript::Tail;
    use std::sync::mpsc::Receiver;

    struct Live {
        work: PathBuf,
        home: PathBuf,
        cwd: String,
        files: Files,
        plan: AcpPlan,
    }

    impl Live {
        /// None (and a message) when the adapter folder isn't given.
        fn new(name: &str) -> Option<Live> {
            let Some(dir) = std::env::var_os("MAESTRO_ACP_CLAUDE_DIR").map(PathBuf::from) else {
                println!("skipped: set MAESTRO_ACP_CLAUDE_DIR");
                return None;
            };
            let def = catalog::get("claude-code").unwrap();
            let acp = def.acp_adapter().unwrap();
            let (node, node_version) = acp_setup::find_node().expect("node.exe");
            assert!(!crate::cli::older(&node_version, acp.node_min), "node {node_version}");
            let acp_setup::Adapter::Found { entry, version, .. } = acp_setup::locate_adapter(acp, None, Some(dir)) else { panic!("no adapter there") };
            let cli_exe = acp_setup::cli_executable(def).expect("claude executable");
            let work = std::env::temp_dir().join(format!("maestro-acp-r4-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&work);
            std::fs::create_dir_all(&work).unwrap();
            let home = PathBuf::from(std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).unwrap());
            let files = Files { meta: work.join("agents").join("m.json") };
            files.write_meta(&AgentMeta { adapter_id: "claude-code".into(), permission_mode: "default".into(), model: Some("haiku".into()), acp_trusted: true, ..Default::default() }).unwrap();
            let cwd = crate::worktree::norm(&work);
            println!("adapter {version}, node {node_version}, cli {}", cli_exe.display());
            Some(Live { work, home, cwd, files, plan: AcpPlan { node, entry, cli_exe, adapter_version: version, dev: true } })
        }

        /// As the app sets it, except `maestro` on PATH is a shim that only prints: cargo puts `target/debug` (the
        /// app's own binary, also named maestro) on a test's PATH, and running that would start the app.
        fn env(&self) -> (Vec<String>, Vec<(String, String)>) {
            let bin = self.work.join("bin");
            let _ = std::fs::create_dir_all(&bin);
            let _ = std::fs::write(bin.join("maestro"), "#!/bin/sh\necho maestro-shim 0.0.0\n");
            let _ = std::fs::write(bin.join("maestro.cmd"), "@echo maestro-shim 0.0.0\r\n");
            let rest: Vec<PathBuf> = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).filter(|p| !p.to_string_lossy().replace('\\', "/").contains("target/debug")).collect();
            let path = std::env::join_paths(std::iter::once(bin).chain(rest)).unwrap().to_string_lossy().to_string();
            acp_setup::acp_env(std::env::vars().map(|(k, _)| k), &self.plan.cli_exe, &[("PATH".into(), path)], None)
        }

        /// A runtime as the facade starts one, without the app: envelopes collected, permission answers from `ask`.
        fn start(&self, brief: &str, ask: Ask) -> (Arc<AcpRuntime>, Arc<Mutex<Vec<Envelope>>>, Receiver<FallbackReason>) {
            self.start_with(brief, ask, None)
        }

        fn start_with(&self, brief: &str, ask: Ask, settings: Option<Value>) -> (Arc<AcpRuntime>, Arc<Mutex<Vec<Envelope>>>, Receiver<FallbackReason>) {
            let got = Arc::new(Mutex::new(vec![]));
            let g = got.clone();
            let emitter = Emitter::with_record(&self.cwd, move |e: &Envelope| g.lock().unwrap().push(e.clone()), |_| {});
            let (tx, rx) = mpsc::channel();
            let tx = Mutex::new(tx);
            let ctx = AcpCtx { home: self.home.clone(), cwd: self.cwd.clone(), files: self.files.clone(), plan: self.plan.clone(), reason: ResetReason::Restart, env: self.env(), brief: brief.into(), settings, agent_id: "claude-code", ask, on_commands: Box::new(|| {}) };
            let rt = AcpRuntime::new(ctx, emitter, Box::new(move |_, why| {
                let _ = tx.lock().unwrap().send(why);
                Some("fallback".into())
            }), Box::new(|| {}));
            rt.begin();
            (rt, got, rx)
        }

        fn session(&self) -> String {
            self.files.read_meta().unwrap().cli_session_id.unwrap()
        }

        fn transcript(&self) -> Vec<RuntimeEvent> {
            Tail::read_all(claude::session_file(&self.home, &self.cwd, &self.session()), runtime::pty_agent("claude-code"))
        }

        fn cleanup(&self) {
            if let Some(id) = self.files.read_meta().and_then(|m| m.cli_session_id) {
                let f = claude::session_file(&self.home, &self.cwd, &id);
                if let Some(dir) = f.parent() {
                    let _ = std::fs::remove_dir_all(dir);
                }
            }
            let _ = std::fs::remove_dir_all(&self.work);
        }
    }

    fn ready(rt: &AcpRuntime, fell: &Receiver<FallbackReason>) {
        let r = rt.ready();
        if let Ok(why) = fell.try_recv() {
            panic!("fell back: {why:?} / {}", rt.client().map(|c| c.stderr_tail()).unwrap_or_default());
        }
        assert_eq!(r, Ok(Readiness::Ready), "stderr: {}", rt.client().map(|c| c.stderr_tail()).unwrap_or_default());
    }

    fn say(rt: &AcpRuntime, text: &str) {
        assert_eq!(rt.prompt(&Prompt { text: text.into(), files: vec![], kept: vec![] }), Ok(Sent::Turn));
        let start = std::time::Instant::now();
        thread::sleep(Duration::from_millis(300));
        while !rt.settled() {
            assert!(start.elapsed() < Duration::from_secs(180), "turn never ended");
            thread::sleep(Duration::from_millis(200));
        }
    }

    fn no_ask() -> Ask {
        Arc::new(|req, _| panic!("unexpected permission prompt: {} {}", req.tool_name, req.tool_input))
    }

    /// The node process and everything under it.
    fn tree(pid: u32) -> Vec<u32> {
        let procs = crate::agent::processes().unwrap_or_default();
        let mut out = vec![pid];
        let mut i = 0;
        while i < out.len() {
            let p = out[i];
            out.extend(procs.iter().filter(|c| c.ppid == p && !out.contains(&c.pid)).map(|c| c.pid).collect::<Vec<_>>());
            i += 1;
        }
        out
    }

    /// What the chat shows from a runtime's stream: its seed, then every live event after the reset.
    fn chat_items(rt: &AcpRuntime, got: &[Envelope]) -> Vec<oracle::Item> {
        let (epoch, record) = rt.history().expect("a record");
        let live: Vec<RuntimeEvent> = got.iter().filter(|e| e.epoch == epoch && !matches!(e.event, RuntimeEvent::Reset { .. })).map(|e| e.event.clone()).collect();
        let durable_live = live.iter().filter(|e| e.durable()).count();
        let mut all = record[..record.len() - durable_live].to_vec();
        all.extend(live);
        tests::fold(&all)
    }

    #[test]
    #[ignore]
    fn acp_live_roundtrip() {
        let Some(l) = Live::new("roundtrip") else { return };
        let (rt, got, fell) = l.start("For this test session: whatever the user says, reply with exactly the word PONG and nothing else.", no_ask());
        ready(&rt, &fell);
        let pid = rt.pid().unwrap();
        say(&rt, "ping");
        let t = l.transcript();
        assert!(t.iter().any(|e| matches!(e, RuntimeEvent::UserMessage { text, .. } if text == "ping")), "{t:?}");
        let answer = t.iter().rev().find_map(|e| match e { RuntimeEvent::AgentText { text, .. } if !text.trim().is_empty() => Some(text.clone()), _ => None }).expect("an answer");
        assert!(answer.contains("PONG"), "{answer}");
        // the streamed text: a prefix of the durable block (or all of it)
        let env = got.lock().unwrap().clone();
        let mut streamed: Vec<(String, String)> = vec![];
        for e in &env {
            if let RuntimeEvent::Delta { item_id, channel: DeltaChannel::Text, offset, text, .. } = &e.event {
                match streamed.iter_mut().find(|(i, _)| i == item_id) {
                    Some((_, t)) if !text.is_empty() => {
                        let keep: String = String::from_utf16(&t.encode_utf16().take(*offset as usize).collect::<Vec<_>>()).unwrap();
                        *t = keep + text;
                    }
                    Some(_) => {}
                    None => streamed.push((item_id.clone(), text.clone())),
                }
            }
        }
        println!("streamed: {streamed:?}");
        assert!(!streamed.is_empty(), "no deltas");
        assert!(streamed.iter().any(|(_, s)| answer.trim().starts_with(s.trim()) || s.trim().starts_with(answer.trim())));
        // every live item was retired: the chat shows what a reload shows
        if chat_items(&rt, &env) != oracle::items(&l.transcript()) {
            for e in &env {
                println!("ENV {:?} {:?}", e.seq, e.event);
            }
            println!("RAW {}", std::fs::read_to_string(claude::session_file(&l.home, &l.cwd, &l.session())).unwrap());
        }
        assert_eq!(chat_items(&rt, &env), oracle::items(&l.transcript()));
        assert_eq!(oracle::items(&rt.history().unwrap().1), oracle::items(&l.transcript()));
        // the host session's entrypoint didn't leak in: the SDK filled its own
        let raw = std::fs::read_to_string(claude::session_file(&l.home, &l.cwd, &l.session())).unwrap();
        let entry: Vec<String> = raw.lines().filter_map(|x| serde_json::from_str::<Value>(x).ok()).filter_map(|v| v["entrypoint"].as_str().map(String::from)).collect();
        println!("entrypoints: {:?} (host: {:?})", entry.first(), std::env::var("CLAUDE_CODE_ENTRYPOINT").ok());
        if let Ok(host) = std::env::var("CLAUDE_CODE_ENTRYPOINT") {
            assert!(entry.iter().all(|e| *e != host), "{entry:?}");
        }
        // close: node, claude and everything below are gone
        let procs = tree(pid);
        println!("process tree: {procs:?}");
        rt.shutdown();
        thread::sleep(Duration::from_millis(500));
        let left: Vec<u32> = crate::agent::processes().unwrap_or_default().iter().map(|p| p.pid).filter(|p| procs.contains(p)).collect();
        assert!(left.is_empty(), "left running: {left:?}");
        assert_eq!(rt.tree_gone(), Some(true), "the job still counts a process");

        // a fresh adapter loads it: the replay comes before the response
        let seen = Arc::new(Mutex::new(vec![]));
        let s = seen.clone();
        let client = Client::spawn(acp_setup::adapter_command(&l.plan.node, &l.plan.entry, &l.work, &l.env()), move |_, inc| {
            if let Incoming::Notification { params, .. } = inc {
                s.lock().unwrap().push(params["update"]["sessionUpdate"].as_str().unwrap_or("").to_string());
            }
        }, |_| {}).unwrap();
        client.request("initialize", acp_setup::initialize_params(), Some(INIT_LIMIT)).unwrap();
        let load = client.request("session/load", json!({ "sessionId": l.session(), "cwd": l.cwd, "mcpServers": [], "_meta": session_meta("x", Some("haiku"), None) }), Some(SESSION_LIMIT));
        let before = seen.lock().unwrap().clone();
        println!("load: {:?}\nbefore the response: {before:?}", load.as_ref().map(|v| v["modes"]["currentModeId"].clone()));
        assert!(load.is_ok());
        assert!(before.iter().any(|k| k == "user_message_chunk") && before.iter().any(|k| k == "agent_message_chunk"));
        client.close(Duration::from_millis(500));

        // the runtime itself on the saved conversation: loaded, seeded with the file, and on
        let (rt2, got2, fell2) = l.start("", no_ask());
        ready(&rt2, &fell2);
        let start = std::time::Instant::now();
        while rt2.commands().is_none() && start.elapsed() < Duration::from_secs(10) {
            thread::sleep(Duration::from_millis(100));
        }
        let commands = rt2.commands().unwrap_or_default();
        println!("commands: {:?}", commands.iter().map(|c| c.0.as_str()).collect::<Vec<_>>());
        assert!(!commands.is_empty());
        assert_eq!(oracle::items(&rt2.history().unwrap().1), oracle::items(&l.transcript()));
        say(&rt2, "ping again");
        let env2 = got2.lock().unwrap().clone();
        assert_eq!(chat_items(&rt2, &env2), oracle::items(&l.transcript()));
        assert!(matches!(l.transcript().iter().rev().find(|e| matches!(e, RuntimeEvent::TurnEnded { .. })), Some(RuntimeEvent::TurnEnded { outcome: TurnOutcome::Done { .. }, .. })));
        // the worktree's model on both turns, the loaded one too (not settings.json's or ANTHROPIC_MODEL's)
        let raw = std::fs::read_to_string(claude::session_file(&l.home, &l.cwd, &l.session())).unwrap();
        let models: Vec<String> = raw.lines().filter_map(|x| serde_json::from_str::<Value>(x).ok()).filter(|v| v["type"] == "assistant").filter_map(|v| v["message"]["model"].as_str().map(String::from)).filter(|m| m != "<synthetic>").collect();
        println!("models: {models:?}");
        assert!(!models.is_empty() && models.iter().all(|m| m.contains("haiku")), "{models:?}");
        if std::env::var("MAESTRO_ACP_LIVE_CLI").as_deref() == Ok("1") {
            rt2.shutdown();
            // the same conversation goes on in the CLI (`claude --resume`)
            let mut cmd = std::process::Command::new(&l.plan.cli_exe);
            cmd.args(["-p", "--model", "haiku", "--resume", &l.session(), "say PONG once more"]).current_dir(&l.work);
            for k in crate::agent::host_session_env(std::env::vars().map(|(k, _)| k)).into_iter().chain(["CLAUDE_CODE_ENTRYPOINT".to_string()]) {
                cmd.env_remove(k);
            }
            let out = crate::cli::quiet(&mut cmd).output().unwrap();
            println!("cli: {}", String::from_utf8_lossy(&out.stdout));
            assert!(out.status.success());
            assert!(l.transcript().iter().filter(|e| matches!(e, RuntimeEvent::UserMessage { .. })).count() >= 4);
        } else {
            rt2.shutdown();
        }
        l.cleanup();
    }

    #[test]
    #[ignore]
    fn acp_live_permission() {
        let Some(l) = Live::new("perm") else { return };
        let asked: Arc<Mutex<Vec<ApprovalReq>>> = Arc::default();
        let a = asked.clone();
        let ask: Ask = Arc::new(move |req, _| {
            a.lock().unwrap().push(req);
            None // nobody answers: rejected
        });
        let (rt, got, fell) = l.start("", ask);
        ready(&rt, &fell);
        say(&rt, "Use the Bash tool to run exactly this command: mkdir maestro-r4-dir");
        println!("transcript: {:?}", l.transcript().iter().filter(|e| matches!(e, RuntimeEvent::ToolStarted { .. } | RuntimeEvent::ToolFinished { .. })).collect::<Vec<_>>());
        let asked = asked.lock().unwrap();
        assert_eq!(asked.len(), 1, "one prompt");
        let req = &asked[0];
        println!("asked: {} {} can_always={} turn={:?} call={:?}", req.tool_name, req.tool_input, req.can_always, req.turn_id, req.tool_call_id);
        assert_eq!(req.tool_name, "Bash");
        assert!(req.tool_input["command"].as_str().unwrap_or("").contains("maestro-r4"));
        assert!(!l.work.join("maestro-r4-dir").exists(), "refused: not run");
        // can_always depends on the rule the CLI suggests for the command (none for mkdir here)
        assert!(!req.cli_answer && req.wait_max.is_none());
        let id = req.tool_call_id.clone().unwrap();
        assert!(rt.tools.lock().unwrap().contains_key(&id), "a tool_call came first");
        let t = l.transcript();
        // the card's id is the transcript's tool call; the call was refused
        assert!(t.iter().any(|e| matches!(e, RuntimeEvent::ToolStarted { tool_call_id, .. } if *tool_call_id == id)));
        assert!(t.iter().any(|e| matches!(e, RuntimeEvent::ToolFinished { tool_call_id, is_error: true, .. } if *tool_call_id == id)), "{t:?}");
        // ended (the stop reason may come on a line before the last text block)
        assert!(t.iter().any(|e| matches!(e, RuntimeEvent::TurnEnded { .. })));
        drop(asked);
        // a cancel with nothing in flight does nothing; one in flight ends the turn cancelled
        rt.cancel().unwrap();
        let long = "Write a detailed essay of about 2000 words on the history of tea. No tools.";
        assert_eq!(rt.prompt(&Prompt { text: long.into(), files: vec![], kept: vec![] }), Ok(Sent::Turn));
        let start = std::time::Instant::now();
        let in_file = |l: &Live| l.transcript().iter().any(|e| matches!(e, RuntimeEvent::UserMessage { text, .. } if text == long));
        while !in_file(&l) && start.elapsed() < Duration::from_secs(60) {
            thread::sleep(Duration::from_millis(100));
        }
        thread::sleep(Duration::from_millis(2500));
        rt.cancel().unwrap();
        while !rt.settled() && start.elapsed() < Duration::from_secs(120) {
            thread::sleep(Duration::from_millis(200));
        }
        // a late "[Request interrupted" line must not double anything
        thread::sleep(Duration::from_secs(3));
        rt.sync();
        thread::sleep(Duration::from_millis(500));
        let h = rt.history().unwrap().1;
        println!("after cancel: {:?}", h.iter().rev().take(4).collect::<Vec<_>>());
        let items = chat_items(&rt, &got.lock().unwrap().clone());
        let bubbles = items.iter().filter(|i| matches!(i, oracle::Item::User { text, .. } if text == long)).count();
        let stops = items.iter().filter(|i| matches!(i, oracle::Item::Note { text } if text == "중단됨")).count();
        println!("bubbles {bubbles}, stop notes {stops}");
        assert_eq!((bubbles, stops), (1, 1));
        assert_eq!(items, oracle::items(&l.transcript()));
        assert!(h.iter().rev().find_map(|e| match e { RuntimeEvent::TurnEnded { outcome, .. } => Some(outcome.clone()), _ => None }) == Some(TurnOutcome::Cancelled));

        rt.shutdown();
        l.cleanup();
    }

    /// The guard's hook reaches Claude under ACP through `_meta.claudeCode.options.settings`, also in bypass mode: a
    /// fake `/pretool` server denies, and `git push` to a local bare remote must not happen.
    #[test]
    #[ignore]
    fn acp_live_pretool_blocks_push() {
        use std::io::{BufRead, BufReader, Read as _, Write as _};
        let Some(l) = Live::new("pretool") else { return };
        let git = |dir: &Path, args: &[&str]| {
            let out = std::process::Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
            assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        let remote = std::env::temp_dir().join(format!("maestro-acp-remote-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&remote);
        std::fs::create_dir_all(&remote).unwrap();
        git(&remote, &["init", "--bare", "-b", "main"]);
        git(&l.work, &["init", "-b", "main"]);
        std::fs::write(l.work.join("a.txt"), "a\n").unwrap();
        git(&l.work, &["add", "a.txt"]);
        git(&l.work, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-m", "init"]);
        git(&l.work, &["remote", "add", "origin", &format!("file:///{}", crate::worktree::norm(&remote).trim_start_matches('/'))]);
        // a stand-in for Maestro's server: every /pretool call is denied
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let hits = Arc::new(Mutex::new(Vec::<String>::new()));
        let h = hits.clone();
        thread::spawn(move || {
            for s in server.incoming().flatten() {
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut line = String::new();
                let _ = r.read_line(&mut line);
                let mut len = 0usize;
                loop {
                    let mut hd = String::new();
                    if r.read_line(&mut hd).unwrap_or(0) == 0 || hd.trim().is_empty() {
                        break;
                    }
                    if let Some(v) = hd.to_ascii_lowercase().strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0; len];
                let _ = r.read_exact(&mut body);
                h.lock().unwrap().push(format!("{} {}", line.trim(), String::from_utf8_lossy(&body)));
                let reply = json!({ "hookSpecificOutput": { "hookEventName": "PreToolUse", "permissionDecision": "deny", "permissionDecisionReason": "test guard: no pushes" } }).to_string();
                let _ = write!(&s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}", reply.len());
            }
        });
        let mut meta = l.files.read_meta().unwrap();
        meta.permission_mode = "bypassPermissions".into();
        l.files.write_meta(&meta).unwrap();
        let settings = crate::approval::settings_for_acp(port, "tk", "acp-test");
        let ask: Ask = Arc::new(|req, _| {
            println!("asked: {} {}", req.tool_name, req.tool_input);
            Some(Choice::Deny)
        });
        let (rt, _, fell) = l.start_with("", ask, Some(settings));
        ready(&rt, &fell);
        say(&rt, "Use the Bash tool to run exactly this command, once, and nothing else: git push origin HEAD");
        let hits = hits.lock().unwrap().clone();
        println!("pretool hits: {hits:?}");
        assert!(hits.iter().any(|h| h.starts_with("POST /pretool/acp-test?t=tk") && h.contains("git push")), "the hook ran");
        assert_eq!(git(&remote, &["for-each-ref"]), "", "nothing was pushed");
        rt.shutdown();
        l.cleanup();
        let _ = std::fs::remove_dir_all(&remote);
    }

    #[test]
    #[ignore]
    fn acp_live_maestro_allowed() {
        let Some(l) = Live::new("allowed") else { return };
        let (rt, _, fell) = l.start("Maestro test marker: the secret word is kiwi-42.", no_ask());
        ready(&rt, &fell);
        // allowedTools: a `maestro …` Bash call never asks (it may fail: no Maestro running)
        say(&rt, "Run exactly this shell command with the Bash tool: maestro --version");
        let t = l.transcript();
        assert!(t.iter().any(|e| matches!(e, RuntimeEvent::ToolStarted { name, input, .. } if name == "Bash" && input["command"].as_str().unwrap_or("").starts_with("maestro"))), "{t:?}");
        // the shim ran, not the app
        assert!(t.iter().any(|e| matches!(e, RuntimeEvent::ToolFinished { output, .. } if output.contains("maestro-shim"))), "{t:?}");
        // the brief reached the system prompt
        say(&rt, "What is the secret word from the Maestro test marker? Reply with the word only.");
        let answer = l.transcript().iter().rev().find_map(|e| match e { RuntimeEvent::AgentText { text, .. } if !text.trim().is_empty() => Some(text.clone()), _ => None }).unwrap();
        println!("answer: {answer}");
        assert!(answer.contains("kiwi-42"));
        rt.shutdown();
        l.cleanup();
    }
}
