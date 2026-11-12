//! Reading a CLI's transcript file as common events (events.rs), for the PTY runtime.
//! - `Normalize`: one agent's line → events (claude.rs, antigravity.rs), with a `TurnTracker` inside
//! - `Tail`: whole lines from a byte offset, so `history()` and the live tail number events alike
//! - `LiveTail`: the running agent's tail: emits, notices a truncated file, follows a new conversation. The ACP
//!   runtime runs one too (the transcript is its durable record): seeded instead of primed (`start_seeded`), with a
//!   hook around each event (`TailHook`) and the events the transcript never gets (`synth_prompt`, `close_turn`).

use crate::agent::Files;
use crate::events::{Initiator, NoticeKind, ResetReason, RuntimeEvent, TurnOutcome, UserKind};
use crate::runtime::{PtyAgent, ToolMatch};
use crate::stream::Emitter;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Kill switch: follow a conversation the CLI switched to (Claude `/clear` via its SessionStart hook, a newer agy
/// conversation for the worktree). Off = the chat stays on the conversation it started with, as before R3.
/// Off in R3 (no behaviour change): on only once Claude's SessionStart `source: "clear"` is checked on a real
/// install and the user signs off; the `/clear` hint in catalog.rs changes with it.
pub const FOLLOW_SESSIONS: bool = false;
/// How often a running agy agent's latest conversation is checked
const FOLLOW_EVERY: Duration = Duration::from_secs(2);

/// One agent's transcript format → common events. Deterministic: ids come from the file's content and line
/// numbers only, so reading the file again gives the same events.
pub trait Normalize: Send {
    /// `line_no`: 1-based, counting every line from the start of the file
    fn line(&mut self, v: &Value, line_no: u64, out: &mut Vec<RuntimeEvent>);
    fn tracker(&self) -> &TurnTracker;
    /// For events the runtime adds itself (`LiveTail::synth_prompt`, `close_turn`), so the turns stay consistent.
    fn tracker_mut(&mut self) -> &mut TurnTracker;
}

/// Called on the tail's thread around each event it emits (the ACP runtime's live text: `acp_runtime::AcpHook`).
pub trait TailHook: Send {
    fn before(&mut self, ev: &RuntimeEvent);
    fn after(&mut self, ev: &RuntimeEvent);
}

/// What the chat says when the runtime ends a turn as failed. An adapter error (`acp`) is the adapter's own
/// words, so it gets a prefix; the others (`auth`, `exited`) are already sentences. `message` itself stays raw in
/// the outcome (worker-done notes read it).
pub fn failure_note(error: &str, message: &str) -> String {
    if error == "acp" && message != "에이전트 오류" {
        format!("에이전트 오류: {message}")
    } else {
        message.to_string()
    }
}

/// Which turn is open, so every event gets a turn_id, and which turn each tool call belongs to.
#[derive(Default)]
pub struct TurnTracker {
    current: Option<String>,
    /// the id `open_user` was asked for (a repeat of it doesn't open another turn)
    current_key: Option<String>,
    last: Option<String>,
    /// provider message that closed the last turn: its later blocks belong to that turn (stragglers)
    closing_msg: Option<String>,
    /// tool_call_id → turn_id
    owners: HashMap<String, String>,
    /// (tool_call_id, name, input) started in the open turn and not finished yet
    open_tools: Vec<(String, String, Value)>,
    used: HashSet<String>,
}

impl TurnTracker {
    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }

    /// The open turn, else the last one, else "t0": where an event outside a turn is filed.
    pub fn fallback(&self) -> String {
        self.current.clone().or_else(|| self.last.clone()).unwrap_or_else(|| "t0".into())
    }

    fn unique(&mut self, id: String) -> String {
        let mut out = id.clone();
        let mut k = 1;
        while !self.used.insert(out.clone()) {
            out = format!("{id}~{k}");
            k += 1;
        }
        out
    }

    fn open(&mut self, id: String, initiator: Initiator, out: &mut Vec<RuntimeEvent>) -> String {
        let id = self.unique(id);
        out.push(RuntimeEvent::TurnStarted { turn_id: id.clone(), initiator });
        self.current = Some(id.clone());
        self.last = Some(id.clone());
        self.closing_msg = None;
        id
    }

    /// A user prompt (or command) opens a turn; a repeat of the open turn's id (Claude's `promptId`) doesn't.
    /// A turn still open is replaced.
    pub fn open_user(&mut self, key: String, out: &mut Vec<RuntimeEvent>) -> String {
        if let (Some(cur), Some(k)) = (&self.current, &self.current_key) {
            if *k == key {
                return cur.clone();
            }
        }
        if self.current.is_some() {
            self.end(TurnOutcome::Replaced, None, out);
        }
        let id = self.open(key.clone(), Initiator::User, out);
        self.current_key = Some(key);
        id
    }

    /// The turn agent output goes to: the open one; else, for a later block of the message that closed the last
    /// turn, that turn (no reopening, no second end); else a new turn the agent started.
    pub fn ensure(&mut self, msg_id: Option<&str>, synth: impl FnOnce() -> String, out: &mut Vec<RuntimeEvent>) -> String {
        if let Some(cur) = &self.current {
            return cur.clone();
        }
        if msg_id.is_some() && msg_id == self.closing_msg.as_deref() {
            return self.fallback();
        }
        self.current_key = None;
        self.open(synth(), Initiator::Agent, out)
    }

    /// Closes the open turn (if any). `closing_msg`: the provider message whose stop reason closed it.
    pub fn end(&mut self, outcome: TurnOutcome, closing_msg: Option<String>, out: &mut Vec<RuntimeEvent>) {
        let Some(turn) = self.current.take() else { return };
        out.push(RuntimeEvent::TurnEnded { turn_id: turn.clone(), outcome });
        self.last = Some(turn);
        self.current_key = None;
        self.open_tools.clear();
        self.closing_msg = closing_msg;
    }

    /// Ends the open turn for the runtime (`LiveTail::close_turn`), after a note: the one given, else for a failed
    /// outcome a `failed` notice with the reason (`failure_note`). Nothing when no turn is open.
    pub fn close_with_notice(&mut self, outcome: TurnOutcome, notice: Option<(NoticeKind, &str)>, out: &mut Vec<RuntimeEvent>) {
        let Some(turn) = self.current.clone() else { return };
        let note = notice.map(|(k, t)| (k, t.to_string())).or_else(|| match &outcome {
            TurnOutcome::Failed { error, message } => Some((NoticeKind::Failed, failure_note(error, message))),
            _ => None,
        });
        if let Some((kind, text)) = note {
            out.push(RuntimeEvent::Notice { turn_id: turn.clone(), item_id: format!("{turn}:acp"), kind, text });
        }
        self.end(outcome, None, out);
    }

    pub fn tool_started(&mut self, turn: &str, id: &str, name: &str, input: &Value) {
        self.owners.insert(id.into(), turn.into());
        self.open_tools.push((id.into(), name.into(), input.clone()));
    }

    /// The turn a finished tool call belongs to.
    pub fn tool_finished(&mut self, id: &str) -> String {
        self.open_tools.retain(|(t, _, _)| t != id);
        self.owners.get(id).cloned().unwrap_or_else(|| self.fallback())
    }

    /// The open tool call a permission prompt is about: the newest with the same name and input (exact), else the
    /// newest with the same name (a guess). (turn_id, tool_call_id, exact)
    pub fn locate(&self, name: &str, input: &Value) -> Option<(String, String, bool)> {
        let exact = self.open_tools.iter().rev().find(|(_, n, i)| n == name && i == input);
        let (hit, sure) = match exact {
            Some(hit) => (hit, true),
            None => (self.open_tools.iter().rev().find(|(_, n, _)| n == name)?, false),
        };
        Some((self.owners.get(&hit.0).cloned().unwrap_or_else(|| self.fallback()), hit.0.clone(), sure))
    }

    /// The turn a tool call was started in, once its start was read.
    pub fn owner(&self, tool_call_id: &str) -> Option<&str> {
        self.owners.get(tool_call_id).map(String::as_str)
    }
}

/// Incremental reader of a transcript. Only whole lines are consumed, so the history read (`read_all`) and the
/// live tail always agree on event numbering.
pub struct Tail {
    path: PathBuf,
    offset: u64,
    /// every line so far, parsed or not (the `l{n}` fallback ids)
    lines: u64,
    /// events so far (all durable)
    pub count: u64,
    pub norm: Box<dyn Normalize>,
}

impl Tail {
    pub fn new(path: PathBuf, norm: Box<dyn Normalize>) -> Tail {
        Tail { path, offset: 0, lines: 0, count: 0, norm }
    }

    pub fn read_new(&mut self) -> Vec<RuntimeEvent> {
        let Ok(mut f) = std::fs::File::open(&self.path) else { return vec![] };
        let mut buf = Vec::new();
        if f.seek(SeekFrom::Start(self.offset)).is_err() || f.read_to_end(&mut buf).is_err() {
            return vec![];
        }
        let Some(end) = buf.iter().rposition(|&b| b == b'\n') else { return vec![] };
        self.offset += end as u64 + 1;
        let mut events = vec![];
        for l in buf[..end].split(|&b| b == b'\n') {
            self.lines += 1;
            if let Ok(v) = serde_json::from_slice::<Value>(l) {
                self.norm.line(&v, self.lines, &mut events);
            }
        }
        self.count += events.len() as u64;
        events
    }

    /// The file is shorter than what was read: it was rewritten.
    pub fn truncated(&self) -> bool {
        std::fs::metadata(&self.path).is_ok_and(|m| m.len() < self.offset)
    }

    /// History: the whole file, for an agent that may not be running.
    pub fn read_all(path: PathBuf, pa: &dyn PtyAgent) -> Vec<RuntimeEvent> {
        Tail::new(path, pa.normalizer()).read_new()
    }
}

/// The running agent's transcript tail: new lines become events on its stream (`Emitter`), numbered after the
/// lines that were there when it started.
pub struct LiveTail {
    pa: &'static dyn PtyAgent,
    home: PathBuf,
    cwd: String,
    meta_file: PathBuf,
    /// the conversation being read; None = an agy conversation not created yet (adopted once agy records it)
    conversation: Option<String>,
    tail: Option<Tail>,
    pub emitter: Arc<Emitter>,
    last_follow_check: Instant,
    hook: Option<Box<dyn TailHook>>,
}

impl LiveTail {
    /// Primed: what the file already holds is counted, not emitted.
    pub fn start(pa: &'static dyn PtyAgent, home: &Path, cwd: &str, meta_file: PathBuf, conversation: Option<String>, transcript: Option<PathBuf>, emitter: Arc<Emitter>) -> LiveTail {
        let tail = transcript.map(|p| {
            let mut t = Tail::new(p, pa.normalizer());
            t.read_new();
            emitter.prime(t.count);
            t
        });
        LiveTail { pa, home: home.into(), cwd: cwd.into(), meta_file, conversation, tail, emitter, last_follow_check: Instant::now(), hook: None }
    }

    /// For a record-keeping emitter (the ACP runtime): what the file already holds becomes the record of a new epoch
    /// (`Emitter::reset_seeded`, sent as one `Reset`), so `history()` is the whole conversation. No file yet: an
    /// empty seed, read from its start once it appears.
    #[allow(clippy::too_many_arguments)]
    pub fn start_seeded(pa: &'static dyn PtyAgent, home: &Path, cwd: &str, meta_file: PathBuf, conversation: Option<String>, transcript: PathBuf, emitter: Arc<Emitter>, reason: ResetReason) -> LiveTail {
        let mut t = Tail::new(transcript, pa.normalizer());
        let seed = t.read_new();
        emitter.reset_seeded(reason, seed);
        LiveTail { pa, home: home.into(), cwd: cwd.into(), meta_file, conversation, tail: Some(t), emitter, last_follow_check: Instant::now(), hook: None }
    }

    pub fn set_hook(&mut self, hook: Box<dyn TailHook>) {
        self.hook = Some(hook);
    }

    /// Emits one event, with the hook around it.
    fn emit(&mut self, ev: RuntimeEvent) {
        match &mut self.hook {
            Some(h) => {
                h.before(&ev);
                self.emitter.push(ev.clone());
                h.after(&ev);
            }
            None => {
                self.emitter.push(ev);
            }
        }
    }

    /// A prompt the transcript never recorded (it failed before the CLI took it, or never reached it): shown
    /// anyway, as its own turn, so a sent message is never lost from view. `key`: the turn id it opens.
    pub fn synth_prompt(&mut self, key: &str, text: &str) {
        let Some(t) = self.tail.as_mut() else { return };
        let mut out = vec![];
        let turn_id = t.norm.tracker_mut().open_user(key.into(), &mut out);
        let kind = if text.starts_with('/') { UserKind::Command } else { UserKind::Prompt };
        out.push(RuntimeEvent::UserMessage { turn_id, item_id: format!("{key}:u"), kind, text: text.into(), files: vec![] });
        for ev in out {
            self.emit(ev);
        }
    }

    /// Ends the open turn the transcript didn't end (the runtime knows it ended: the prompt's response, an exit),
    /// after an optional muted note; a Failed outcome with no note gets a `failed` notice carrying the reason, so
    /// the chat shows why. Nothing when no turn is open.
    pub fn close_turn(&mut self, outcome: TurnOutcome, notice: Option<(NoticeKind, &str)>) {
        let Some(t) = self.tail.as_mut() else { return };
        let mut out = vec![];
        t.norm.tracker_mut().close_with_notice(outcome, notice, &mut out);
        for ev in out {
            self.emit(ev);
        }
    }

    fn remember(&self, id: &str) {
        // read-modify-write under the meta's lock: others rewrite meta too (summaries, links, settings)
        let f = Files { meta: self.meta_file.clone() };
        let _ = f.update_meta(|m| {
            m.cli_session_id = Some(id.into());
            Ok(())
        });
    }

    /// Reads what's new and emits it.
    pub fn tick(&mut self) {
        let Some(tail) = &self.tail else {
            // a new agy conversation: adopt the one agy records for this worktree, and remember it for --conversation.
            // Read from its start, live (as before R3): it was just created, so what it holds is the turn under way.
            if let Some(id) = self.pa.current_conversation(&self.home, &self.cwd) {
                self.switch(&id, None, false);
            }
            return;
        };
        if tail.truncated() {
            let path = tail.path.clone();
            self.restart(path, ResetReason::Truncated);
        }
        let events = self.tail.as_mut().map(Tail::read_new).unwrap_or_default();
        for ev in events {
            self.emit(ev);
        }
        if FOLLOW_SESSIONS && self.last_follow_check.elapsed() >= FOLLOW_EVERY {
            self.last_follow_check = Instant::now();
            // only one written more recently than ours: agy may not have recorded the one it was started on yet
            let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
            if let Some(id) = self.pa.current_conversation(&self.home, &self.cwd).filter(|id| self.conversation.as_deref() != Some(id.as_str())) {
                let current = self.tail.as_ref().and_then(|t| modified(&t.path));
                if modified(&self.pa.transcript(&self.home, &self.cwd, &id)) > current {
                    self.follow(&id, None);
                }
            }
        }
    }

    /// The CLI moved to another conversation (`/clear`, a newer agy conversation): read that one under a new
    /// epoch, primed (its history comes from the UI's refetch). Nothing when it's the one already read.
    pub fn follow(&mut self, id: &str, transcript: Option<&Path>) {
        if self.conversation.as_deref() == Some(id) {
            return;
        }
        self.switch(id, transcript, true);
    }

    /// The CLI said where conversation `id`'s transcript is (Claude's hooks: `CLAUDE_CONFIG_DIR`, a hashed folder
    /// name): read that file when it's another than the one being read. Nothing read yet and nothing there yet (a
    /// new conversation): the path just changes; otherwise the stream starts over on it, primed (`restart`).
    pub fn retarget(&mut self, id: &str, path: &Path) {
        if self.conversation.as_deref() != Some(id) {
            return;
        }
        let key = |p: &Path| crate::util::path_key(&p.to_string_lossy());
        match &self.tail {
            Some(t) if key(&t.path) == key(path) => {}
            Some(t) if t.count == 0 && t.offset == 0 && std::fs::metadata(path).map_or(0, |m| m.len()) == 0 => {
                self.tail = Some(Tail::new(path.to_path_buf(), self.pa.normalizer()));
            }
            _ => self.restart(path.to_path_buf(), ResetReason::Source),
        }
    }

    fn switch(&mut self, id: &str, transcript: Option<&Path>, primed: bool) {
        // meta first, then the epoch: a refetch after the reset reads the new conversation
        self.remember(id);
        let path = transcript.map(Path::to_path_buf).unwrap_or_else(|| self.pa.transcript(&self.home, &self.cwd, id));
        self.conversation = Some(id.into());
        if primed {
            self.restart(path, ResetReason::Source);
        } else {
            self.tail = Some(Tail::new(path, self.pa.normalizer()));
            self.emitter.reset(ResetReason::Source);
        }
    }

    /// The stream starts over on `path`, primed like `start`: what the file holds now reaches the UI through its
    /// refetch after the `Reset`, not as live events, so the observer (state dot, worker-done) never sees old turns
    /// again. Only lines written later go out live.
    /// A record-keeping emitter (ACP) gets what the file holds as its new record instead (`reset_seeded`).
    fn restart(&mut self, path: PathBuf, reason: ResetReason) {
        let mut t = Tail::new(path, self.pa.normalizer());
        if self.emitter.keeps_record() {
            let seed = t.read_new();
            self.emitter.reset_seeded(reason, seed);
        } else {
            self.emitter.reset(reason);
            t.read_new();
            self.emitter.prime(t.count);
        }
        self.tail = Some(t);
    }

    /// The process ended: the open turn is over.
    pub fn exited(&self) {
        self.emitter.push(RuntimeEvent::Exited { turn_id: self.open_turn() });
    }

    pub fn open_turn(&self) -> Option<String> {
        self.tail.as_ref().and_then(|t| t.norm.tracker().current().map(String::from))
    }

    /// The tool call a permission prompt is about; the open turn when no call matches. `tool_call_id`: the prompt's
    /// own id, whose turn is its owner's (the open turn until its start is read), never a same-named call's.
    pub fn locate(&self, name: &str, input: &Value, tool_call_id: Option<&str>) -> ToolMatch {
        let tracker = self.tail.as_ref().map(|t| t.norm.tracker());
        if let Some(id) = tool_call_id {
            let owner = tracker.and_then(|t| t.owner(id)).map(String::from);
            return ToolMatch { exact: owner.is_some(), turn_id: owner.or_else(|| self.open_turn()), tool_call_id: Some(id.into()) };
        }
        match tracker.and_then(|t| t.locate(name, input)) {
            // a guess (same name, other input: likely another call still open) keeps its turn but never names a
            // call, so nothing keys a tool card on it
            Some((turn, tool, exact)) => ToolMatch { turn_id: Some(turn), tool_call_id: exact.then_some(tool), exact },
            None => ToolMatch { turn_id: self.open_turn(), ..Default::default() },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentMeta;
    use crate::events::Envelope;
    use crate::{antigravity, claude};
    use serde_json::json;
    use std::sync::Mutex;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("maestro-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn line(v: Value) -> String {
        format!("{v}\n")
    }

    fn sink() -> (Arc<Mutex<Vec<Envelope>>>, impl Fn(&Envelope) + Send + Sync + 'static) {
        let got = Arc::new(Mutex::new(vec![]));
        let g = got.clone();
        (got, move |e: &Envelope| g.lock().unwrap().push(e.clone()))
    }

    #[test]
    fn tail_reads_whole_lines_only() {
        let dir = temp("tail");
        let f = dir.join("s.jsonl");
        let user = |t: &str| line(json!({"type": "user", "message": {"content": t}}));
        std::fs::write(&f, format!("{}{{\"type\":\"user\",\"mess", user("a"))).unwrap();
        let mut tail = Tail::new(f.clone(), claude::CLAUDE.normalizer());
        // the prompt opens a turn: TurnStarted + UserMessage
        assert_eq!(tail.read_new().len(), 2);
        std::fs::write(&f, format!("{}{}", user("a"), user("b"))).unwrap();
        let evs = tail.read_new();
        assert!(matches!(&evs[..], [RuntimeEvent::TurnEnded { .. }, RuntimeEvent::TurnStarted { .. }, RuntimeEvent::UserMessage { text, .. }] if text == "b"));
        assert_eq!(tail.count, 5);
        assert_eq!(Tail::read_all(f, &claude::CLAUDE).len(), 5);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn tracker_ids_and_stragglers() {
        let mut t = TurnTracker::default();
        let mut out = vec![];
        assert_eq!(t.open_user("p1".into(), &mut out), "p1");
        // the same prompt id again: same turn, nothing new
        assert_eq!(t.open_user("p1".into(), &mut out), "p1");
        assert_eq!(out.len(), 1);
        t.tool_started("p1", "t1", "Read", &json!({"f": 1}));
        t.end(TurnOutcome::Done { stop_reason: "end_turn".into() }, Some("m1".into()), &mut out);
        // a later block of the closing message stays in that turn without reopening it
        assert_eq!(t.ensure(Some("m1"), || "a:x".into(), &mut out), "p1");
        assert_eq!(t.current(), None);
        // other output opens a turn of the agent's own
        assert_eq!(t.ensure(Some("m2"), || "a:y".into(), &mut out), "a:y");
        assert!(matches!(out.last(), Some(RuntimeEvent::TurnStarted { initiator: Initiator::Agent, .. })));
        // the tool call keeps its owner turn after that turn ended
        assert_eq!(t.tool_finished("t1"), "p1");
        assert_eq!(t.tool_finished("nope"), "a:y");
        // a new prompt replaces the open turn; a reused id gets a suffix
        let before = out.len();
        assert_eq!(t.open_user("p1".into(), &mut out), "p1~1");
        assert!(matches!(&out[before..], [RuntimeEvent::TurnEnded { outcome: TurnOutcome::Replaced, .. }, RuntimeEvent::TurnStarted { .. }]));
    }

    #[test]
    fn locate_matches_name_and_input() {
        let mut t = TurnTracker::default();
        let mut out = vec![];
        t.open_user("p".into(), &mut out);
        t.tool_started("p", "t1", "Bash", &json!({"command": "ls"}));
        t.tool_started("p", "t2", "Bash", &json!({"command": "rm x"}));
        t.tool_started("p", "t3", "Edit", &json!({}));
        assert_eq!(t.locate("Bash", &json!({"command": "ls"})), Some(("p".into(), "t1".into(), true)));
        // no exact input: the newest call of that name, as a guess
        assert_eq!(t.locate("Bash", &json!({"command": "pwd"})), Some(("p".into(), "t2".into(), false)));
        assert_eq!(t.locate("Write", &json!({})), None);
        t.tool_finished("t2");
        assert_eq!(t.locate("Bash", &json!({})), Some(("p".into(), "t1".into(), false)));
        assert_eq!(t.owner("t2"), Some("p"));
        assert_eq!(t.owner("t9"), None);
    }

    #[test]
    fn a_prompt_with_its_own_id_takes_that_calls_turn() {
        let dir = temp("locate-id");
        let f = dir.join("s.jsonl");
        let all = [
            json!({"type": "user", "uuid": "u1", "promptId": "p1", "message": {"content": "hi"}}),
            json!({"type": "assistant", "uuid": "a1", "message": {"id": "m1", "content": [{"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "ls"}}], "stop_reason": "tool_use"}}),
        ];
        std::fs::write(&f, all.iter().map(|v| line(v.clone())).collect::<String>()).unwrap();
        let (_, e) = emitter("test://locate-id");
        let mut live = LiveTail::start(&claude::CLAUDE, &dir, "C:/w", dir.join("m.json"), Some("s".into()), Some(f), e);
        live.tick();
        let open = || Some("p1".to_string());
        // the call named by id was read: its own turn, sure
        assert_eq!(live.locate("Bash", &json!({}), Some("t1")), ToolMatch { turn_id: open(), tool_call_id: Some("t1".into()), exact: true });
        // not read yet: the open turn, never the turn of another Bash call
        assert_eq!(live.locate("Bash", &json!({"command": "rm"}), Some("t2")), ToolMatch { turn_id: open(), tool_call_id: Some("t2".into()), exact: false });
        // no id and only a same-named call open: a guess, its turn only (never another call's id)
        assert_eq!(live.locate("Bash", &json!({"command": "rm"}), None), ToolMatch { turn_id: open(), tool_call_id: None, exact: false });
        assert_eq!(live.locate("Bash", &json!({"command": "ls"}), None), ToolMatch { turn_id: open(), tool_call_id: Some("t1".into()), exact: true });
        let _ = std::fs::remove_dir_all(dir);
    }

    fn emitter(key: &str) -> (Arc<Mutex<Vec<Envelope>>>, Arc<Emitter>) {
        let (got, s) = sink();
        (got, Emitter::new(key, s, |_| {}))
    }

    #[test]
    fn live_and_history_number_alike() {
        let dir = temp("live");
        let f = dir.join("s.jsonl");
        let all: Vec<String> = [
            json!({"type": "user", "uuid": "u1", "promptId": "p1", "message": {"content": "hi"}}),
            json!({"type": "assistant", "uuid": "a1", "message": {"id": "m1", "content": [{"type": "tool_use", "id": "t1", "name": "Read", "input": {}}], "stop_reason": "tool_use"}}),
            json!({"type": "user", "uuid": "u2", "promptId": "p1", "message": {"content": [{"type": "tool_result", "tool_use_id": "t1", "content": "x"}]}}),
            json!({"type": "assistant", "uuid": "a2", "message": {"id": "m2", "content": [{"type": "text", "text": "done"}], "stop_reason": "end_turn"}}),
            json!({"type": "user", "uuid": "u3", "promptId": "p2", "message": {"content": "more"}}),
        ]
        .into_iter()
        .map(line)
        .collect();
        for primed in [0, 2] {
            let key = format!("{}/live-{primed}", dir.display());
            std::fs::write(&f, all[..primed].concat()).unwrap();
            let (got, em) = emitter(&key);
            let mut live = LiveTail::start(&claude::CLAUDE, &dir, &key, dir.join("m.json"), Some("s".into()), Some(f.clone()), em);
            // appended in chunks, one cut mid-line
            let rest = all[primed..].concat();
            let cut = rest.len() / 2;
            std::fs::write(&f, format!("{}{}", all[..primed].concat(), &rest[..cut])).unwrap();
            live.tick();
            std::fs::write(&f, all.concat()).unwrap();
            live.tick();
            let history = Tail::read_all(f.clone(), &claude::CLAUDE);
            let got = got.lock().unwrap();
            let seqs: Vec<u64> = got.iter().map(|e| e.seq.unwrap()).collect();
            let first = history.len() - got.len();
            assert_eq!(seqs, ((first + 1) as u64..=history.len() as u64).collect::<Vec<_>>(), "primed {primed}");
            for e in got.iter() {
                assert_eq!(e.event, history[e.seq.unwrap() as usize - 1]);
            }
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn truncation_and_source_switch_reset_the_stream() {
        let dir = temp("switch");
        let f = dir.join("s.jsonl");
        let meta = dir.join("m.json");
        Files { meta: meta.clone() }.write_meta(&AgentMeta { adapter_id: "claude-code".into(), cli_session_id: Some("s".into()), ..Default::default() }).unwrap();
        let user = |t: &str| line(json!({"type": "user", "message": {"content": t}}));
        std::fs::write(&f, format!("{}{}", user(&"a".repeat(500)), user("b"))).unwrap();
        let key = format!("{}/switch", dir.display());
        let (got, s) = sink();
        let seen = Arc::new(Mutex::new(0));
        let o = seen.clone();
        let em = Emitter::new(&key, s, move |_| *o.lock().unwrap() += 1);
        let mut live = LiveTail::start(&claude::CLAUDE, &dir, &key, meta.clone(), Some("s".into()), Some(f.clone()), em.clone());
        let epoch = crate::stream::epoch(&key);

        // the file was rewritten shorter: a new epoch, primed on what it holds (the UI refetches it), so old
        // turns never reach the observer again; only later lines go out live
        let assistant = line(json!({"type": "assistant", "uuid": "a1", "message": {"id": "m1", "content": [{"type": "text", "text": "ok"}], "stop_reason": "end_turn"}}));
        std::fs::write(&f, format!("{}{assistant}", user("c"))).unwrap();
        live.tick();
        {
            let got = got.lock().unwrap();
            assert_eq!(got.len(), 1);
            assert!(matches!(got[0].event, RuntimeEvent::Reset { reason: ResetReason::Truncated }));
            assert_eq!(got[0].epoch, epoch + 1);
            assert_eq!(*seen.lock().unwrap(), 0);
        }
        std::fs::write(&f, format!("{}{assistant}{}", user("c"), user("d"))).unwrap();
        live.tick();
        {
            let got = got.lock().unwrap();
            let n = Tail::read_all(f.clone(), &claude::CLAUDE).len() as u64;
            assert_eq!(got.iter().filter_map(|e| e.seq).last(), Some(n));
            assert!(matches!(&got.last().unwrap().event, RuntimeEvent::UserMessage { text, .. } if text == "d"));
        }

        // /clear: the new conversation's file, meta first, then the reset, primed the same way
        let f2 = dir.join("s2.jsonl");
        std::fs::write(&f2, user("new")).unwrap();
        got.lock().unwrap().clear();
        live.follow("s2", Some(&f2));
        live.follow("s2", Some(&f2)); // the same one again: nothing
        std::fs::write(&f2, format!("{}{}", user("new"), user("next"))).unwrap();
        live.tick();
        assert_eq!(Files { meta }.read_meta().unwrap().cli_session_id.as_deref(), Some("s2"));
        let got = got.lock().unwrap();
        assert!(matches!(got[0].event, RuntimeEvent::Reset { reason: ResetReason::Source }));
        assert_eq!(got.iter().filter(|e| matches!(e.event, RuntimeEvent::Reset { .. })).count(), 1);
        // "new" (2 events) was primed; "next" ends that turn and opens its own
        assert!(matches!(&got.last().unwrap().event, RuntimeEvent::UserMessage { text, .. } if text == "next"));
        assert_eq!(got.iter().filter_map(|e| e.seq).collect::<Vec<_>>(), [3, 4, 5]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn retarget_moves_to_the_reported_path() {
        let dir = temp("retarget");
        let user = |t: &str| line(json!({"type": "user", "message": {"content": t}}));
        let key = format!("{}/retarget", dir.display());
        let (got, s) = sink();
        let em = Emitter::new(&key, s, |_| {});
        // a new conversation: Claude reported another path before writing anything; the path just changes
        let mut live = LiveTail::start(&claude::CLAUDE, &dir, &key, dir.join("m.json"), Some("s".into()), Some(dir.join("guess.jsonl")), em.clone());
        let g = dir.join("cfg.jsonl");
        live.retarget("s", &g);
        std::fs::write(&g, user("hi")).unwrap();
        live.tick();
        assert!(matches!(&got.lock().unwrap().last().unwrap().event, RuntimeEvent::UserMessage { text, .. } if text == "hi"));
        assert!(!got.lock().unwrap().iter().any(|e| matches!(e.event, RuntimeEvent::Reset { .. })));

        // a guess that held something else: the stream starts over on the reported file, primed
        let h = dir.join("other.jsonl");
        std::fs::write(&h, user("old")).unwrap();
        got.lock().unwrap().clear();
        live.retarget("s", &h);
        live.retarget("s", &h); // the same one again: nothing
        let spelled = h.to_string_lossy().replace('/', "\\").to_uppercase();
        live.retarget("s", Path::new(&spelled)); // another spelling of it: nothing
        live.retarget("other", &g); // another conversation: nothing
        std::fs::write(&h, format!("{}{}", user("old"), user("next"))).unwrap();
        live.tick();
        let got = got.lock().unwrap();
        assert_eq!(got.iter().filter(|e| matches!(e.event, RuntimeEvent::Reset { .. })).count(), 1);
        assert!(matches!(got[0].event, RuntimeEvent::Reset { reason: ResetReason::Source }));
        assert!(matches!(&got.last().unwrap().event, RuntimeEvent::UserMessage { text, .. } if text == "next"));
        let _ = std::fs::remove_dir_all(dir);
    }

    fn record_emitter(key: &str) -> (Arc<Mutex<Vec<Envelope>>>, Arc<Emitter>) {
        let (got, s) = sink();
        (got, Emitter::with_record(key, s, |_| {}))
    }

    #[test]
    fn a_seeded_tail_keeps_the_whole_transcript_as_its_record() {
        let dir = temp("seeded");
        let f = dir.join("s.jsonl");
        let user = |id: &str, t: &str| line(json!({"type": "user", "uuid": id, "promptId": id, "message": {"content": t}}));
        let answer = |id: &str, t: &str| line(json!({"type": "assistant", "uuid": id, "message": {"id": id, "content": [{"type": "text", "text": t}], "stop_reason": "end_turn"}}));
        std::fs::write(&f, format!("{}{}", user("u1", "hi"), answer("a1", "yo"))).unwrap();
        let key = format!("{}/seeded", dir.display());
        let (got, em) = record_emitter(&key);
        let mut live = LiveTail::start_seeded(&claude::CLAUDE, &dir, &key, dir.join("m.json"), Some("s".into()), f.clone(), em.clone(), ResetReason::Switch);
        assert_eq!(em.history().unwrap().1, Tail::read_all(f.clone(), &claude::CLAUDE));
        std::fs::write(&f, format!("{}{}{}{}", user("u1", "hi"), answer("a1", "yo"), user("u2", "more"), answer("a2", "ok"))).unwrap();
        live.tick();
        assert_eq!(em.history().unwrap().1, Tail::read_all(f.clone(), &claude::CLAUDE));
        let got = got.lock().unwrap();
        assert!(matches!(got[0].event, RuntimeEvent::Reset { reason: ResetReason::Switch }));
        // the seed went out as the Reset only; live events continue its numbering
        assert_eq!(got.iter().filter_map(|e| e.seq).collect::<Vec<_>>(), [5, 6, 7, 8]);
        let _ = std::fs::remove_dir_all(dir);
    }

    struct Order(Arc<Mutex<Vec<String>>>);
    impl TailHook for Order {
        fn before(&mut self, ev: &RuntimeEvent) {
            self.0.lock().unwrap().push(format!("before {}", serde_json::to_value(ev).unwrap()["type"]));
        }
        fn after(&mut self, ev: &RuntimeEvent) {
            self.0.lock().unwrap().push(format!("after {}", serde_json::to_value(ev).unwrap()["type"]));
        }
    }

    #[test]
    fn the_hook_runs_around_each_event() {
        let dir = temp("hook");
        let f = dir.join("s.jsonl");
        let key = format!("{}/hook", dir.display());
        let (got, em) = record_emitter(&key);
        let mut live = LiveTail::start_seeded(&claude::CLAUDE, &dir, &key, dir.join("m.json"), Some("s".into()), f.clone(), em, ResetReason::Restart);
        let log = Arc::new(Mutex::new(vec![]));
        live.set_hook(Box::new(Order(log.clone())));
        std::fs::write(&f, line(json!({"type": "user", "uuid": "u", "message": {"content": "hi"}}))).unwrap();
        live.tick();
        assert_eq!(*log.lock().unwrap(), ["before \"turnStarted\"", "after \"turnStarted\"", "before \"userMessage\"", "after \"userMessage\""]);
        assert_eq!(got.lock().unwrap().iter().filter(|e| e.seq.is_some()).count(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn failure_note_says_why() {
        let login = "로그인이 필요해요. CLI 보기에서 /login 해 주세요";
        assert_eq!(failure_note("auth", login), login);
        assert_eq!(failure_note("exited", "에이전트가 꺼져 전달되지 않았어요"), "에이전트가 꺼져 전달되지 않았어요");
        assert_eq!(failure_note("acp", "overloaded"), "에이전트 오류: overloaded");
        assert_eq!(failure_note("acp", "에이전트 오류"), "에이전트 오류");
        // a given note wins; a cancel gets none of its own; nothing open, nothing shown
        let mut t = TurnTracker::default();
        let mut out = vec![];
        t.close_with_notice(TurnOutcome::Failed { error: "auth".into(), message: login.into() }, None, &mut out);
        assert!(out.is_empty());
        t.open_user("p:1".into(), &mut out);
        t.close_with_notice(TurnOutcome::Failed { error: "acp".into(), message: "x".into() }, Some((NoticeKind::Interrupted, "중단됨")), &mut out);
        t.open_user("p:2".into(), &mut out);
        t.close_with_notice(TurnOutcome::Cancelled, None, &mut out);
        let notes: Vec<_> = out.iter().filter(|e| matches!(e, RuntimeEvent::Notice { .. })).collect();
        assert!(matches!(notes[..], [RuntimeEvent::Notice { kind: NoticeKind::Interrupted, .. }]));
    }

    #[test]
    fn synthesized_turns_keep_the_tracker_consistent() {
        let dir = temp("synth");
        let f = dir.join("s.jsonl");
        let key = format!("{}/synth", dir.display());
        let (_, em) = record_emitter(&key);
        let mut live = LiveTail::start_seeded(&claude::CLAUDE, &dir, &key, dir.join("m.json"), Some("s".into()), f.clone(), em.clone(), ResetReason::Restart);
        // nothing open: closing does nothing
        live.close_turn(TurnOutcome::Cancelled, None);
        assert!(em.history().unwrap().1.is_empty());
        live.synth_prompt("p:1", "lost");
        assert_eq!(live.open_turn().as_deref(), Some("p:1"));
        live.close_turn(TurnOutcome::Failed { error: "acp".into(), message: "x".into() }, None);
        live.close_turn(TurnOutcome::Cancelled, Some((NoticeKind::Interrupted, "중단됨"))); // no double end
        assert_eq!(live.open_turn(), None);
        // a later prompt in the file opens its own turn, no ghost
        std::fs::write(&f, line(json!({"type": "user", "uuid": "u", "promptId": "p2", "message": {"content": "next"}}))).unwrap();
        live.tick();
        live.synth_prompt("p:2", "/compact");
        live.close_turn(TurnOutcome::Cancelled, Some((NoticeKind::Interrupted, "중단됨")));
        let h = em.history().unwrap().1;
        let kinds: Vec<String> = h.iter().map(|e| serde_json::to_value(e).unwrap()["type"].as_str().unwrap().to_string()).collect();
        assert_eq!(kinds, ["turnStarted", "userMessage", "notice", "turnEnded", "turnStarted", "userMessage", "turnEnded", "turnStarted", "userMessage", "notice", "turnEnded"]);
        // the failed close says why
        assert!(matches!(&h[2], RuntimeEvent::Notice { kind: NoticeKind::Failed, item_id, text, .. } if item_id == "p:1:acp" && text == "에이전트 오류: x"));
        assert!(matches!(&h[6], RuntimeEvent::TurnEnded { turn_id, outcome: TurnOutcome::Replaced } if turn_id == "p2"));
        assert!(matches!(&h[8], RuntimeEvent::UserMessage { kind: UserKind::Command, item_id, .. } if item_id == "p:2:u"));
        assert!(matches!(&h[9], RuntimeEvent::Notice { kind: NoticeKind::Interrupted, item_id, .. } if item_id == "p:2:acp"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn agy_conversation_is_adopted_once_recorded() {
        let home = temp("adopt");
        let cwd = "C:/w/agent-9";
        let key = format!("{}/adopt", home.display());
        let meta = home.join("m.json");
        Files { meta: meta.clone() }.write_meta(&AgentMeta { adapter_id: "antigravity".into(), ..Default::default() }).unwrap();
        let (got, em) = emitter(&key);
        let mut live = LiveTail::start(&antigravity::AGY, &home, cwd, meta.clone(), None, None, em);
        live.tick();
        assert!(got.lock().unwrap().is_empty());
        let cache = home.join(".gemini/antigravity-cli/cache");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join("last_conversations.json"), json!({ "C:\\w\\agent-9": "c1" }).to_string()).unwrap();
        let t = antigravity::transcript(&home, "c1");
        std::fs::create_dir_all(t.parent().unwrap()).unwrap();
        std::fs::write(&t, line(json!({"step_index": 0, "type": "USER_INPUT", "content": "<USER_REQUEST>hi</USER_REQUEST>"}))).unwrap();
        live.tick();
        live.tick();
        assert_eq!(Files { meta }.read_meta().unwrap().cli_session_id.as_deref(), Some("c1"));
        let got = got.lock().unwrap();
        assert!(matches!(got[0].event, RuntimeEvent::Reset { reason: ResetReason::Source }));
        assert_eq!(got.iter().filter_map(|e| e.seq).collect::<Vec<_>>(), [1, 2]);
        let _ = std::fs::remove_dir_all(home);
    }
}
