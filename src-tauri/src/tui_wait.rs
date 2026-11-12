//! "The TUI is waiting for an answer": an agent's own terminal shows a question (a permission picker, a choice) that
//! only the CLI view can answer. Anything pasted into the TUI then would land in that picker, and its Enter would
//! pick the highlighted choice ("Yes"), so while it lasts `Agent::send_prompt` refuses every prompt (chat, `maestro
//! send`, worker notes) and the UI locks the composer and points to the CLI view.
//!
//! Set by: Claude's permission hook getting no decision from Maestro (the card's "CLI에서 답하기", its time limit),
//! after which Claude asks in its own terminal; a picker seen in the PTY output (`pty::picker_shown`: agents
//! without the hook, Claude's other pickers). Cleared by: keys that answer it in the CLI view (Enter, a digit, Esc),
//! the interrupt, transcript progress that only comes after an answer, the turn's end, the process going away.
//! One value per agent (`Watch`), pushed to every window as "agent://tui-wait" (`list_tui_waits` for late mounts).

use crate::agent::AgentState;
use crate::events::{NoticeKind, RuntimeEvent};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

/// Why the TUI is waiting.
#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    /// the permission card was handed to the CLI ("CLI에서 답하기")
    Cli,
    /// the permission card ran out of time
    Timeout,
    /// a picker seen in the terminal output
    Screen,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TuiWait {
    pub worktree_path: String,
    pub source: Source,
    pub tool_name: Option<String>,
    pub turn_id: Option<String>,
    pub tool_call_id: Option<String>,
}

/// What may change the state.
pub enum Signal<'a> {
    /// the permission hook handed the question to the TUI
    Asked { source: Source, tool_name: Option<String>, turn_id: Option<String>, tool_call_id: Option<String> },
    /// a picker is on screen (there is no "gone": raw output can't tell a picker gone from one partly redrawn)
    Screen,
    /// a durable event from the transcript
    Event(&'a RuntimeEvent),
    /// input typed into the terminal (the CLI view, the interrupt's Esc)
    Keys(&'a [u8]),
    /// the process exited or was replaced
    Gone,
}

/// Keys that answer a picker: Enter (also inside a longer write), a lone Esc ("no"), Ctrl+C, a digit choice.
/// Escape sequences (arrows, focus in/out, the cursor-position reply) only move around.
fn answers(keys: &[u8]) -> bool {
    keys.contains(&b'\r') || keys == b"\x1b" || keys == b"\x03" || matches!(keys, [b'1'..=b'9'])
}

/// New output in the transcript only comes once the TUI isn't blocked on its picker any more, or (a screen match)
/// shows the match was model text after all.
fn clears(w: &TuiWait, ev: &RuntimeEvent) -> bool {
    match ev {
        RuntimeEvent::TurnEnded { .. } | RuntimeEvent::UserMessage { .. } | RuntimeEvent::Exited { .. } => true,
        RuntimeEvent::Notice { kind: NoticeKind::Interrupted, .. } => true,
        RuntimeEvent::ToolFinished { tool_call_id, .. } => w.tool_call_id.as_deref() == Some(tool_call_id.as_str()),
        RuntimeEvent::AgentText { .. } | RuntimeEvent::Thinking { .. } | RuntimeEvent::ToolStarted { .. } => w.source == Source::Screen || w.tool_call_id.is_none(),
        _ => false,
    }
}

/// The state after `sig`: None = unchanged, Some(next) = changed. `running`: the agent is in a turn (a screen
/// match outside one, the startup trust dialog, never counts).
pub fn step(path: &str, cur: &Option<TuiWait>, running: bool, sig: Signal) -> Option<Option<TuiWait>> {
    let next = match sig {
        Signal::Asked { source, tool_name, turn_id, tool_call_id } => Some(TuiWait { worktree_path: path.into(), source, tool_name, turn_id, tool_call_id }),
        // a hook-set wait keeps its source and ids
        Signal::Screen => match cur {
            None if running => Some(TuiWait { worktree_path: path.into(), source: Source::Screen, tool_name: None, turn_id: None, tool_call_id: None }),
            _ => cur.clone(),
        },
        Signal::Event(ev) => cur.clone().filter(|w| !clears(w, ev)),
        Signal::Keys(k) => cur.clone().filter(|_| !answers(k)),
        Signal::Gone => None,
    };
    (next != *cur).then_some(next)
}

/// One agent's wait, as the backend enforces it and the UI shows it.
pub struct Watch {
    /// None in tests
    app: Option<AppHandle>,
    path: String,
    state: Arc<Mutex<AgentState>>,
    cur: Mutex<Option<TuiWait>>,
    /// cleared since the screen check last looked: it skips the output it already matched
    rearm: AtomicBool,
}

impl Watch {
    pub fn new(app: Option<AppHandle>, path: &str, state: Arc<Mutex<AgentState>>) -> Arc<Watch> {
        Arc::new(Watch { app, path: path.into(), state, cur: Mutex::new(None), rearm: AtomicBool::new(false) })
    }

    /// Applies `sig`; a change goes out as "agent://tui-wait". True when this cleared a wait. Called from the event
    /// observer (under the stream lock): only emits, never pushes events.
    pub fn apply(&self, sig: Signal) -> bool {
        let running = *self.state.lock().unwrap() == AgentState::Running;
        let mut cur = self.cur.lock().unwrap();
        let Some(next) = step(&self.path, &cur, running, sig) else { return false };
        let cleared = next.is_none();
        *cur = next;
        if cleared {
            self.rearm.store(true, Ordering::SeqCst);
        }
        if let Some(app) = &self.app {
            let _ = app.emit("agent://tui-wait", serde_json::json!({ "worktreePath": self.path, "wait": *cur }));
        }
        cleared
    }

    pub fn get(&self) -> Option<TuiWait> {
        self.cur.lock().unwrap().clone()
    }

    /// Whether a wait was cleared since the last call (the screen check moves its mark then).
    pub fn take_rearm(&self) -> bool {
        self.rearm.swap(false, Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::TurnOutcome;
    use serde_json::json;

    fn asked(source: Source, id: Option<&str>) -> Signal<'static> {
        Signal::Asked { source, tool_name: Some("Bash".into()), turn_id: Some("u1".into()), tool_call_id: id.map(String::from) }
    }

    fn watch(state: AgentState) -> Arc<Watch> {
        Watch::new(None, "C:/wt", Arc::new(Mutex::new(state)))
    }

    fn text() -> RuntimeEvent {
        RuntimeEvent::AgentText { turn_id: "u1".into(), item_id: "i".into(), message_id: None, text: "ok".into() }
    }

    fn finished(id: &str) -> RuntimeEvent {
        RuntimeEvent::ToolFinished { turn_id: "u1".into(), tool_call_id: id.into(), output: String::new(), is_error: false }
    }

    #[test]
    fn a_hook_wait_with_its_tool_call_clears_when_that_call_finishes() {
        let w = watch(AgentState::Running);
        w.apply(asked(Source::Cli, Some("t1")));
        assert_eq!(w.get().unwrap().source, Source::Cli);
        assert!(!w.apply(Signal::Event(&finished("t2"))));
        // its own call still waits: model text written before the answer doesn't lift it
        assert!(!w.apply(Signal::Event(&text())));
        assert!(w.get().is_some());
        assert!(w.apply(Signal::Event(&finished("t1"))));
        assert!(w.get().is_none());
        assert!(w.take_rearm());
        assert!(!w.take_rearm());
    }

    #[test]
    fn a_hook_wait_without_a_tool_call_clears_on_progress_or_the_turn_end() {
        let ended = RuntimeEvent::TurnEnded { turn_id: "u1".into(), outcome: TurnOutcome::Done { stop_reason: "end_turn".into() } };
        let started = RuntimeEvent::ToolStarted { turn_id: "u1".into(), tool_call_id: "t9".into(), message_id: None, name: "Read".into(), input: json!({}) };
        for ev in [text(), started, ended] {
            let w = watch(AgentState::Running);
            w.apply(asked(Source::Timeout, None));
            assert!(!w.apply(Signal::Event(&finished("t1"))));
            assert!(w.apply(Signal::Event(&ev)), "{ev:?}");
        }
    }

    #[test]
    fn only_answering_keys_clear() {
        let w = watch(AgentState::Running);
        w.apply(asked(Source::Cli, None));
        for k in [&b"\x1b[A"[..], b"\x1b[I", b"\x1b[O", b"\x1b[1;1R", b"a", b"0"] {
            assert!(!w.apply(Signal::Keys(k)), "{k:?}");
        }
        for k in [&b"\r"[..], b"2", b"\x1b", b"\x03", b"yes\r"] {
            w.apply(asked(Source::Cli, None));
            assert!(w.apply(Signal::Keys(k)), "{k:?}");
        }
    }

    #[test]
    fn a_screen_match_counts_only_in_a_turn_and_keeps_a_hook_source() {
        let idle = watch(AgentState::Idle);
        idle.apply(Signal::Screen);
        assert!(idle.get().is_none());
        let w = watch(AgentState::Running);
        w.apply(Signal::Screen);
        assert_eq!(w.get().unwrap().source, Source::Screen);
        // a screen match clears on any progress
        assert!(w.apply(Signal::Event(&text())));
        w.apply(asked(Source::Cli, Some("t1")));
        w.apply(Signal::Screen);
        assert_eq!(w.get().unwrap().source, Source::Cli);
    }

    #[test]
    fn an_interrupt_a_new_prompt_or_an_exit_clears() {
        let interrupted = RuntimeEvent::Notice { turn_id: "u1".into(), item_id: "n".into(), kind: NoticeKind::Interrupted, text: "중단됨".into() };
        let prompt = RuntimeEvent::UserMessage { turn_id: "u2".into(), item_id: "m".into(), kind: crate::events::UserKind::Prompt, text: "hi".into(), files: vec![] };
        for sig in [Signal::Event(&interrupted), Signal::Event(&prompt), Signal::Gone] {
            let w = watch(AgentState::Running);
            w.apply(asked(Source::Cli, Some("t1")));
            assert!(w.apply(sig));
        }
    }

    #[test]
    fn nothing_changed_is_no_change() {
        let cur = None;
        assert_eq!(step("p", &cur, true, Signal::Gone), None);
        assert_eq!(step("p", &cur, true, Signal::Keys(b"\r")), None);
        let set = step("p", &cur, true, Signal::Screen).unwrap();
        assert_eq!(step("p", &set, true, Signal::Screen), None);
        let a = || Signal::Asked { source: Source::Cli, tool_name: None, turn_id: None, tool_call_id: None };
        let hook = step("p", &None, false, a()).unwrap();
        assert_eq!(step("p", &hook, false, a()), None);
    }

    #[test]
    fn serializes_for_the_ui() {
        let w = TuiWait { worktree_path: "C:/wt".into(), source: Source::Timeout, tool_name: Some("Bash".into()), turn_id: None, tool_call_id: Some("t1".into()) };
        assert_eq!(serde_json::to_value(&w).unwrap(), json!({ "worktreePath": "C:/wt", "source": "timeout", "toolName": "Bash", "turnId": null, "toolCallId": "t1" }));
        assert_eq!(serde_json::to_value(Source::Screen).unwrap(), json!("screen"));
        assert_eq!(serde_json::to_value(Source::Cli).unwrap(), json!("cli"));
    }
}
