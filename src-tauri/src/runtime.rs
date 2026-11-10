//! What running an agent means to the rest of Maestro, whatever runs it (PRD R3):
//! - `Runtime`: start-up, prompt, cancel, kill, and the event stream behind it. The PTY runtime (pty.rs) and the
//!   ACP runtime (acp_runtime.rs) implement it; the facade (`agent::Agent`) swaps one for the other.
//! - `PtyAgent`: what differs per CLI under the PTY runtime (argv, transcript, normalizer). The only place,
//!   with `pty_agent()`, that knows Claude from Antigravity.
//! - `history()` / `snapshot()`: the common events of a worktree's conversation, running or not. The live runtime's
//!   own record when it keeps one (`Runtime::history`), else the CLI's transcript; `seq` indexes that same list.

use crate::agent::{AgentMeta, Files};
use crate::approval::Approvals;
use crate::catalog::{self, AgentDef};
use crate::events::{RuntimeEvent, Transcript};
use crate::transcript::{Normalize, Tail};
use crate::{antigravity, claude, stream};
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

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

/// The agent takes prompts now; or it first shows its "trust this folder?" dialog.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Readiness {
    Ready,
    NeedsTrust,
}

/// A chat message: text, plus files the user attached (paths relative to the worktree).
pub struct Prompt {
    pub text: String,
    pub files: Vec<String>,
    /// the kept worker notes (worker, note) this text carries (`control::pass_notes`): a runtime that queues the
    /// prompt and then exits before sending it keeps them again, so none is lost
    pub kept: Vec<(String, String)>,
}

/// What `Runtime::prompt` did with a message.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sent {
    /// it goes to the agent as a turn (the dot goes busy)
    Turn,
    /// the runtime handled it itself (ACP `/model` → `session/set_config_option`): no turn
    Handled,
}

/// One running agent, whatever drives it. Sync and thread-based, like the rest of the backend.
pub trait Runtime: Send + Sync {
    fn kind(&self) -> catalog::Runtime;
    /// Blocks until a prompt can be taken. PTY: the TUI's input is up. ACP: initialize + session/new|load answered.
    fn ready(&self) -> Result<Readiness, LaunchFailure>;
    /// PTY: Enter on the trust dialog, then wait for the prompt again. ACP: Maestro's own trust question answered,
    /// then wait for the session.
    fn accept_trust(&self) -> Result<(), String>;
    /// PTY: pasted into the TUI. ACP: queued for session/prompt (or handled: `/model`).
    fn prompt(&self, p: &Prompt) -> Result<Sent, String>;
    /// The current turn stops; the agent stays up. PTY: Esc. ACP: session/cancel.
    fn cancel(&self) -> Result<(), String>;
    /// The whole process tree.
    fn kill(&self);
    /// A clean stop before a switch (ACP: session/close, then stdin closed); then the tree is killed.
    fn shutdown(&self) {
        self.kill()
    }
    fn alive(&self) -> bool;
    /// Its process is still there, whatever the runtime's own state says (ACP: a closed session's node may not have
    /// exited yet). The switch's single-writer check reads this, not `alive`.
    fn process_alive(&self) -> bool {
        self.alive()
    }
    /// Nothing queued or in flight inside the runtime (ACP's client-side queue). PTY: always.
    fn settled(&self) -> bool {
        true
    }
    /// (name, description) of the slash commands the agent itself reports (ACP `available_commands_update`);
    /// None = it doesn't say.
    fn commands(&self) -> Option<Vec<(String, String)>> {
        None
    }
    /// PTY: since the TUI last drew. ACP: since the last stdio traffic.
    fn quiet_for(&self) -> Duration;
    fn pid(&self) -> Option<u32>;
    /// When its process was started (`agent::now_ms` right before the spawn; 0: unknown). A process listed with
    /// `pid` as its parent but older than this isn't in its tree: Windows reuses pids and never updates parent ids.
    fn started(&self) -> u64 {
        0
    }
    /// Its whole tree is gone (ACP: its job has no process left); None: it can't tell, the process list decides.
    fn tree_gone(&self) -> Option<bool> {
        None
    }
    /// The terminal behind it, for the CLI view; None for a runtime without one.
    fn terminal(&self) -> Option<&dyn Terminal> {
        None
    }
    /// Reads what the agent wrote so far now instead of at the next tick (the Stop hook).
    fn sync(&self) {}
    /// The tool call a permission prompt is about. `tool_call_id`: the id the prompt carries, when it has one
    /// (ACP's request_permission does; Claude's PermissionRequest hook doesn't).
    fn locate_tool(&self, _name: &str, _input: &Value, tool_call_id: Option<&str>) -> ToolMatch {
        ToolMatch { tool_call_id: tool_call_id.map(String::from), ..Default::default() }
    }
    /// The agent moved to another conversation: read that one (a new epoch).
    fn follow(&self, _conversation: &str, _transcript: Option<&Path>) {}
    /// (epoch, durable events) when this runtime keeps its own record: the events as it numbered them (`seq` n =
    /// element n-1) in that stream epoch. None = the CLI's transcript is the record (PTY: `transcript_history`, the
    /// tail numbers alike). ACP keeps one in its emitter (`Emitter::with_record`): the transcript as it was when its
    /// epoch started (the seed), what the tail read since, and the events it synthesized (a prompt the transcript
    /// never got, a turn end it never wrote), returned by `Emitter::history()` so record, seq and epoch come from
    /// one lock. A record of another epoch than the stream's (a previous run still registered while the next one
    /// starts) is never used.
    fn history(&self) -> Option<(u64, Vec<RuntimeEvent>)> {
        None
    }
}

/// Which tool call a permission prompt is about (`Runtime::locate_tool`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolMatch {
    /// the turn the call belongs to; the open turn when no call matched
    pub turn_id: Option<String>,
    /// only when sure (the prompt's own id, or an exact match): screens key tool cards on it, so a guess leaves it None
    pub tool_call_id: Option<String>,
    /// sure: the prompt's own id read in the transcript, or an open call with the same name and input. false = no
    /// call, the prompt's own id not read yet, or only one with the same name (a guess: its turn, no call id)
    pub exact: bool,
}

/// A terminal byte stream ("pty://data").
pub trait Terminal {
    fn write(&self, d: &[u8]) -> Result<(), String>;
    fn resize(&self, cols: u16, rows: u16) -> Result<(), String>;
    fn snapshot(&self) -> PtySnapshot;
}

/// `pty_snapshot`: the scrollback, its byte count and the PTY epoch it belongs to.
#[derive(Serialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PtySnapshot {
    pub data_b64: String,
    pub seq: u64,
    pub epoch: u64,
    /// a terminal owns the conversation (or nothing runs); false = the ACP chat runtime does, the CLI view shows
    /// its "switch to CLI" bar
    pub terminal: bool,
}

/// What a CLI needs to start in a worktree.
pub struct LaunchCtx<'a> {
    pub home: &'a Path,
    pub worktree: &'a str,
    pub def: &'static AgentDef,
    pub approvals: &'a Approvals,
}

pub struct Launch {
    pub argv: Vec<String>,
    /// the transcript to tail; None = an agy conversation not created yet (found once it is)
    pub transcript: Option<PathBuf>,
    /// started on an existing conversation (`--resume`, `--conversation`)
    pub resumed: bool,
}

/// What differs per CLI under the PTY runtime.
pub trait PtyAgent: Sync {
    #[allow(dead_code)] // the registry's key; tests
    fn id(&self) -> &'static str;
    /// argv after `exe` (what `cli::command` resolved), the files it reads, and the conversation it continues.
    /// Writes a new conversation id into `meta` (and its file) when it picks one.
    fn launch(&self, ctx: &LaunchCtx, meta: &mut AgentMeta, files: &Files, exe: Vec<String>) -> Result<Launch, String>;
    /// The conversation a worktree's chat is drawn from: the saved one, else the CLI's latest for the folder.
    fn conversation_id(&self, meta: &AgentMeta, home: &Path, cwd: &str) -> Option<String> {
        meta.cli_session_id.clone().or_else(|| self.current_conversation(home, cwd))
    }
    fn transcript(&self, home: &Path, cwd: &str, id: &str) -> PathBuf;
    /// The conversation the CLI itself records as the folder's latest (agy); None = it doesn't say.
    fn current_conversation(&self, _home: &Path, _cwd: &str) -> Option<String> {
        None
    }
    fn normalizer(&self) -> Box<dyn Normalize>;
    /// The launch command for the settings tile, after the bare executable name.
    fn preview(&self, def: &AgentDef, permission_mode: &str, model: Option<&str>) -> Vec<String>;
    /// One-shot summary of `conversation` with the assistant model.
    fn summarize(&self, exe: Vec<String>, conversation: &str, model: &str) -> Result<String, String>;
}

/// The PTY agent for a catalog id.
pub fn pty_agent(id: &str) -> &'static dyn PtyAgent {
    if id == antigravity::ID {
        &antigravity::AGY
    } else {
        &claude::CLAUDE
    }
}

/// A worktree's conversation as common events: `live`'s own record when it keeps one, else the CLI's transcript.
/// `live` = the worktree's running runtime, if any; nothing else assumes one runs.
pub fn history(live: Option<&dyn Runtime>, home: &Path, meta: &AgentMeta, cwd: &str) -> Vec<RuntimeEvent> {
    own_record(live, stream::epoch(cwd)).unwrap_or_else(|| transcript_history(home, meta, cwd))
}

/// `live`'s own record, when it keeps one numbered in `epoch`.
fn own_record(live: Option<&dyn Runtime>, epoch: u64) -> Option<Vec<RuntimeEvent>> {
    live.and_then(|r| r.history()).filter(|(e, _)| *e == epoch).map(|(_, events)| events)
}

/// The conversation read from the CLI's transcript (the PTY runtime's record).
pub fn transcript_history(home: &Path, meta: &AgentMeta, cwd: &str) -> Vec<RuntimeEvent> {
    let pa = pty_agent(&meta.adapter_id);
    match pa.conversation_id(meta, home, cwd) {
        Some(id) => Tail::read_all(pa.transcript(home, cwd, &id), pa),
        None => vec![],
    }
}

/// `get_transcript`: the history with the epoch it belongs to. Read between two epoch reads (once more if they
/// differ), so a stream that started over meanwhile isn't mixed with the old one. Live events still to come for
/// lines read here carry seq ≤ the returned seq and are dropped by the UI.
/// `live` is asked for the worktree's runtime on every try: a restart resets the stream before the new runtime is
/// registered, so the one found first may be the previous run, whose record (another epoch) is then skipped.
pub fn snapshot(live: impl Fn() -> Option<Arc<dyn Runtime>>, home: &Path, files: &Files, cwd: &str) -> Transcript {
    let mut tries = 0;
    loop {
        let e1 = stream::epoch(cwd);
        let meta = files.read_meta().unwrap_or_default();
        let rt = live();
        // (the runtime's own record was used, a record of another epoch was skipped)
        let (own, stale, events) = match rt.as_deref().and_then(|r| r.history()) {
            Some((e, events)) if e == e1 => (true, false, events),
            // the previous run, the next one not registered yet
            Some(_) => (false, true, transcript_history(home, &meta, cwd)),
            None => (false, false, transcript_history(home, &meta, cwd)),
        };
        let e2 = stream::epoch(cwd);
        tries += 1;
        if (e1 == e2 && !stale) || tries == 2 {
            // never stamp a runtime's record with an epoch it wasn't numbered in
            let events = if own && e1 != e2 { transcript_history(home, &meta, cwd) } else { events };
            return Transcript { epoch: e2, seq: events.len() as u64, events };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_picks_the_cli() {
        assert_eq!(pty_agent("antigravity").id(), antigravity::ID);
        assert_eq!(pty_agent("claude-code").id(), "claude-code");
        // today's fallback for anything else
        assert_eq!(pty_agent("").id(), "claude-code");
    }

    #[test]
    fn history_without_a_conversation_is_empty() {
        let home = std::env::temp_dir().join(format!("maestro-history-{}", std::process::id()));
        assert!(history(None, &home, &AgentMeta::default(), "C:/nowhere").is_empty());
        let meta = AgentMeta { adapter_id: "claude-code".into(), cli_session_id: Some("s".into()), ..Default::default() };
        let f = claude::session_file(&home, "C:/w", "s");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, "{\"type\":\"user\",\"message\":{\"content\":\"hi\"}}\n").unwrap();
        assert_eq!(history(None, &home, &meta, "C:/w").len(), 2);
        let files = Files { meta: home.join("m.json") };
        files.write_meta(&meta).unwrap();
        let t = snapshot(|| None, &home, &files, "C:/w");
        assert_eq!((t.seq, t.events.len(), t.epoch), (2, 2, stream::epoch("c:/W")));
        // a runtime keeping its own record is read instead of the transcript; one that doesn't falls back to it
        let now = stream::epoch("C:/w");
        let own = fake(Some((now, vec![RuntimeEvent::Exited { turn_id: None }; 3])));
        assert_eq!(snapshot(|| Some(own.clone()), &home, &files, "C:/w").seq, 3);
        assert_eq!(history(Some(&*own), &home, &meta, "C:/w").len(), 3);
        assert_eq!(history(Some(&*fake(None)), &home, &meta, "C:/w").len(), 2);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_record_of_another_epoch_is_never_used() {
        let home = std::env::temp_dir().join(format!("maestro-stale-{}", std::process::id()));
        let cwd = "C:/stale";
        let meta = AgentMeta { adapter_id: "claude-code".into(), cli_session_id: Some("s".into()), ..Default::default() };
        let f = claude::session_file(&home, cwd, "s");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, "{\"type\":\"user\",\"message\":{\"content\":\"hi\"}}\n").unwrap();
        let files = Files { meta: home.join("m.json") };
        files.write_meta(&meta).unwrap();
        // the stream was reset (a restart) while the previous run, with its record of the old epoch, is still the
        // one registered: its 5 events must not go out under the new epoch
        let e = stream::Emitter::new(cwd, |_| {}, |_| {});
        let old = stream::epoch(cwd);
        e.reset(crate::events::ResetReason::Restart);
        let previous = fake(Some((old, vec![RuntimeEvent::Exited { turn_id: None }; 5])));
        let t = snapshot(|| Some(previous.clone()), &home, &files, cwd);
        assert_eq!((t.epoch, t.seq), (old + 1, 2));
        assert_eq!(history(Some(&*previous), &home, &meta, cwd).len(), 2);
        // asked again on the next try: the new run, registered meanwhile, is read
        let tries = std::cell::Cell::new(0);
        let next = fake(Some((old + 1, vec![RuntimeEvent::Exited { turn_id: None }; 1])));
        let t = snapshot(
            || {
                tries.set(tries.get() + 1);
                Some(if tries.get() == 1 { previous.clone() } else { next.clone() })
            },
            &home,
            &files,
            cwd,
        );
        assert_eq!((t.epoch, t.seq, tries.get()), (old + 1, 1, 2));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_record_kept_by_the_emitter_snapshots_at_the_live_seq() {
        let home = std::env::temp_dir().join(format!("maestro-recorded-{}", std::process::id()));
        let cwd = "C:/recorded";
        let files = Files { meta: home.join("m.json") };
        std::fs::create_dir_all(&home).unwrap();
        files.write_meta(&AgentMeta { adapter_id: "claude-code".into(), ..Default::default() }).unwrap();
        // an ACP-like runtime: its record is the emitter's, so it can't drift from what went out live
        let e = stream::Emitter::with_record(cwd, |_| {}, |_| {});
        let rt: Arc<dyn Runtime> = Arc::new(Recorded(e.clone()));
        let note = |t: &str| RuntimeEvent::Notice { turn_id: "t".into(), item_id: t.into(), kind: crate::events::NoticeKind::LocalCommand, text: t.into() };
        e.push(note("old"));
        e.reset(crate::events::ResetReason::Restart);
        let mut last = 0;
        for t in ["a", "b", "c"] {
            last = e.push(note(t)).unwrap();
        }
        let t = snapshot(|| Some(rt.clone()), &home, &files, cwd);
        assert_eq!((t.epoch, t.seq), (e.epoch(), last));
        assert_eq!(t.events, [note("a"), note("b"), note("c")]);
        let _ = std::fs::remove_dir_all(home);
    }

    struct Recorded(Arc<stream::Emitter>);
    impl Runtime for Recorded {
        fn kind(&self) -> catalog::Runtime {
            catalog::Runtime::Acp
        }
        fn ready(&self) -> Result<Readiness, LaunchFailure> {
            Ok(Readiness::Ready)
        }
        fn accept_trust(&self) -> Result<(), String> {
            Ok(())
        }
        fn prompt(&self, _: &Prompt) -> Result<Sent, String> {
            Ok(Sent::Turn)
        }
        fn cancel(&self) -> Result<(), String> {
            Ok(())
        }
        fn kill(&self) {}
        fn alive(&self) -> bool {
            true
        }
        fn quiet_for(&self) -> Duration {
            Duration::ZERO
        }
        fn pid(&self) -> Option<u32> {
            None
        }
        fn history(&self) -> Option<(u64, Vec<RuntimeEvent>)> {
            self.0.history()
        }
    }

    fn fake(record: Option<(u64, Vec<RuntimeEvent>)>) -> Arc<dyn Runtime> {
        Arc::new(Fake(record))
    }

    struct Fake(Option<(u64, Vec<RuntimeEvent>)>);
    impl Runtime for Fake {
        fn kind(&self) -> catalog::Runtime {
            catalog::Runtime::Acp
        }
        fn ready(&self) -> Result<Readiness, LaunchFailure> {
            Ok(Readiness::Ready)
        }
        fn accept_trust(&self) -> Result<(), String> {
            Ok(())
        }
        fn prompt(&self, _: &Prompt) -> Result<Sent, String> {
            Ok(Sent::Turn)
        }
        fn cancel(&self) -> Result<(), String> {
            Ok(())
        }
        fn kill(&self) {}
        fn alive(&self) -> bool {
            true
        }
        fn quiet_for(&self) -> Duration {
            Duration::ZERO
        }
        fn pid(&self) -> Option<u32> {
            None
        }
        fn history(&self) -> Option<(u64, Vec<RuntimeEvent>)> {
            self.0.clone()
        }
    }
}
