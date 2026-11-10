//! The common events every runtime speaks (PRD R3): the PTY runtime makes them from a CLI's transcript
//! (claude.rs, antigravity.rs), the ACP runtime (R4) will make them from `session/update`. The chat view,
//! `maestro read`, worker-done notes and card summaries all read these, never a CLI's own format.
//! - durable events are numbered (`seq`) and come back the same from `runtime::history`
//! - live-only events (`seq` null) can't be rebuilt from history: deltas, approvals, exit, reset

use crate::agent::AgentState;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;

/// One step of an agent conversation, the same for every runtime (PTY transcript now, ACP in R4).
/// Turn-scoped variants always carry turn_id; only session-level ones (Exited, Reset, an approval or error
/// with no open turn, Unknown outside a turn) may have None.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[allow(dead_code)] // ToolUpdated and Delta come with the ACP runtime (R4)
pub enum RuntimeEvent {
    // ---- durable: seq'd, reproducible by history() ----
    TurnStarted { turn_id: String, initiator: Initiator },
    UserMessage { turn_id: String, item_id: String, kind: UserKind, text: String, files: Vec<String> },
    /// A whole text block. ACP (R4) sends it empty when the item opens and again with the full text at the
    /// boundary: an upsert by item_id. message_id = the provider unit the block belongs to (Claude PTY: the
    /// transcript line's uuid, since Claude writes one block per line and legacy read/summary work per line;
    /// agy: "agy-{step}"; ACP: messageId).
    AgentText { turn_id: String, item_id: String, message_id: Option<String>, text: String },
    Thinking { turn_id: String, item_id: String, message_id: Option<String>, text: String },
    ToolStarted { turn_id: String, tool_call_id: String, message_id: Option<String>, name: String, input: Value },
    /// ACP tool_call_update; an absent field means unchanged. The PTY runtime never sends it.
    ToolUpdated { turn_id: String, tool_call_id: String, status: Option<ToolStatus>, title: Option<String>, input: Option<Value>, output: Option<String> },
    ToolFinished { turn_id: String, tool_call_id: String, output: String, is_error: bool },
    /// Shown as a muted note. Text is final ("중단됨" is made here, not in the UI).
    Notice { turn_id: String, item_id: String, kind: NoticeKind, text: String },
    /// Not fatal (Claude system/api_error retry notice). Fatal errors end the turn as TurnOutcome::Failed.
    Error { turn_id: Option<String>, kind: String, message: String },
    TurnEnded { turn_id: String, outcome: TurnOutcome },
    /// Content the normalizer doesn't know, kept (PRD R3-2). raw is capped at 2 KB, else {type, truncated:true}.
    Unknown { turn_id: Option<String>, source: String, raw: Value },
    // ---- live-only: seq None ----
    /// Coalesced ~50 ms in Rust (stream.rs). offset = UTF-16 length of the item's text before this append (idempotent).
    Delta { turn_id: String, item_id: String, channel: DeltaChannel, offset: u64, text: String },
    ApprovalRequested { turn_id: Option<String>, tool_call_id: Option<String>, approval_id: u64, tool_name: String, tool_input: Value, can_always: bool },
    ApprovalResolved { turn_id: Option<String>, tool_call_id: Option<String>, approval_id: u64, resolution: Resolution },
    /// The process ended. turn_id = the turn that was open (it is implicitly over).
    Exited { turn_id: Option<String> },
    /// The epoch changed: drop what you hold and refetch.
    Reset { reason: ResetReason },
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum TurnOutcome {
    /// end_turn, max_tokens, … ; ACP stopReason
    Done { stop_reason: String },
    /// isApiErrorMessage (error: "rate_limit", …); ACP prompt error
    Failed { error: String, message: String },
    /// Esc / "[Request interrupted" / ACP "cancelled"
    Cancelled,
    /// slash or `!` output finished (a local command)
    Command,
    /// a new prompt opened before this turn closed
    Replaced,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Initiator {
    User,
    /// output with no prompt before it (a resumed turn, a straggler after a closed one)
    Agent,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum UserKind {
    Prompt,
    /// a slash command
    Command,
    /// `!` bash mode
    Shell,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum NoticeKind {
    LocalCommand,
    Interrupted,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // ACP tool_call_update (R4)
pub enum ToolStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // the ACP runtime streams these (R4)
pub enum DeltaChannel {
    Text,
    Thinking,
    ToolOutput,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Resolution {
    Allow,
    Always,
    Deny,
    /// handed back to the CLI's own prompt
    Cli,
    Timeout,
    /// the agent stopped asking (answered in the TUI, turn interrupted)
    Withdrawn,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // Switch, Fallback and Replay are the R4 callers' reasons
pub enum ResetReason {
    /// the runtime (re)started: spawn, resume after an away stop, retry
    Restart,
    /// the stream now reads another conversation (agy adoption, `/clear`, a newer agy conversation)
    Source,
    /// the transcript file shrank
    Truncated,
    /// chat ↔ CLI runtime switch
    Switch,
    /// ACP → PTY fallback
    Fallback,
    /// session/load replays the conversation
    Replay,
}

/// "agent://event"
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    pub worktree_path: String,
    pub epoch: u64,
    pub seq: Option<u64>,
    pub event: RuntimeEvent,
}

/// `get_transcript`: the history and the (epoch, seq) it was read at.
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Transcript {
    pub epoch: u64,
    pub seq: u64,
    pub events: Vec<RuntimeEvent>,
}

const UNKNOWN_MAX: usize = 2048;

impl RuntimeEvent {
    /// Numbered and rebuilt by `history()`; the rest is live-only.
    pub fn durable(&self) -> bool {
        use RuntimeEvent::*;
        !matches!(self, Delta { .. } | ApprovalRequested { .. } | ApprovalResolved { .. } | Exited { .. } | Reset { .. })
    }

    #[allow(dead_code)] // the interface-migration reducer's key; used by tests here
    pub fn turn_id(&self) -> Option<&str> {
        use RuntimeEvent::*;
        match self {
            TurnStarted { turn_id, .. }
            | UserMessage { turn_id, .. }
            | AgentText { turn_id, .. }
            | Thinking { turn_id, .. }
            | ToolStarted { turn_id, .. }
            | ToolUpdated { turn_id, .. }
            | ToolFinished { turn_id, .. }
            | Notice { turn_id, .. }
            | TurnEnded { turn_id, .. }
            | Delta { turn_id, .. } => Some(turn_id),
            Error { turn_id, .. } | Unknown { turn_id, .. } | ApprovalRequested { turn_id, .. } | ApprovalResolved { turn_id, .. } | Exited { turn_id } => turn_id.as_deref(),
            Reset { .. } => None,
        }
    }

    /// The provider unit (transcript line, agy step, ACP message) a content block belongs to.
    pub fn message_id(&self) -> Option<&str> {
        match self {
            RuntimeEvent::AgentText { message_id, .. } | RuntimeEvent::Thinking { message_id, .. } | RuntimeEvent::ToolStarted { message_id, .. } => message_id.as_deref(),
            _ => None,
        }
    }

    /// Unknown content, kept whole up to 2 KB.
    pub fn unknown(turn_id: Option<String>, source: &str, raw: &Value) -> RuntimeEvent {
        let raw = if raw.to_string().len() > UNKNOWN_MAX { json!({ "type": raw["type"], "truncated": true }) } else { raw.clone() };
        RuntimeEvent::Unknown { turn_id, source: source.into(), raw }
    }
}

/// How an event moves the busy/idle dot (legacy `state_after`, one for one).
pub fn status_after(ev: &RuntimeEvent) -> Option<AgentState> {
    match ev {
        RuntimeEvent::UserMessage { .. } => Some(AgentState::Running),
        RuntimeEvent::Notice { .. } => Some(AgentState::Idle),
        RuntimeEvent::TurnEnded { outcome, .. } => match outcome {
            TurnOutcome::Replaced => None,
            _ => Some(AgentState::Idle),
        },
        _ => None,
    }
}

/// A finished turn worth telling the agent's lead about (`control::worker_done`): the model answered or failed
/// while the agent was running. Esc and slash commands don't count.
pub fn finishes_turn(prev: &AgentState, ev: &RuntimeEvent) -> bool {
    *prev == AgentState::Running && matches!(ev, RuntimeEvent::TurnEnded { outcome: TurnOutcome::Done { .. } | TurnOutcome::Failed { .. }, .. })
}

/// Text blocks as the chat shows them: an `AgentText` repeated under the same item_id (ACP's empty-then-full
/// upsert) is one block, at its first place, with its last text.
fn upsert_texts<'a>(events: impl Iterator<Item = &'a RuntimeEvent>) -> Vec<&'a str> {
    let mut texts: Vec<(&str, &str)> = vec![];
    for ev in events {
        if let RuntimeEvent::AgentText { item_id, text, .. } = ev {
            match texts.iter_mut().find(|(i, _)| i == item_id) {
                Some(t) => t.1 = text,
                None => texts.push((item_id, text)),
            }
        }
    }
    texts.into_iter().map(|(_, t)| t).collect()
}

/// The agent's latest answer (`maestro read`, worker-done notes): the text blocks of the last unit (Claude: the
/// transcript line) that has any, joined by "\n". Mid-turn text counts; blank text counts. Upserts are one block.
pub fn last_answer(events: &[RuntimeEvent]) -> Option<String> {
    let e = events.iter().rposition(|ev| matches!(ev, RuntimeEvent::AgentText { .. }))?;
    let Some(m) = events[e].message_id() else {
        let RuntimeEvent::AgentText { text, .. } = &events[e] else { unreachable!() };
        return Some(text.clone());
    };
    let start = events[..e].iter().rposition(|ev| matches!(ev, RuntimeEvent::UserMessage { .. } | RuntimeEvent::TurnStarted { .. } | RuntimeEvent::TurnEnded { .. })).map_or(0, |i| i + 1);
    let unit = events[start..=e].iter().filter(|ev| matches!(ev, RuntimeEvent::AgentText { message_id: Some(x), .. } if x == m));
    Some(upsert_texts(unit).join("\n"))
}

/// The most recent answer as plain text for the summarizer: the prompt it answered, the agent's replies
/// up to that answer, and the tools it used. A turn still in progress, or slash commands typed after the
/// answer, don't count, so the summary stays put until a new answer lands. Keeps the tail when long.
/// Byte for byte what it was when it read the CLI's own events (the summary cache is keyed by it).
pub fn recent_turn_text(events: &[RuntimeEvent], max_chars: usize) -> String {
    let Some(e) = events.iter().rposition(|ev| matches!(ev, RuntimeEvent::AgentText { text, .. } if !text.trim().is_empty())) else { return String::new() };
    // the whole unit the answer is in: agy lists its tool calls after the text in the same step
    let mut end = e;
    if let Some(m) = events[e].message_id() {
        while end + 1 < events.len() && (events[end + 1].message_id() == Some(m) || matches!(events[end + 1], RuntimeEvent::Unknown { .. })) {
            end += 1;
        }
    }
    let start = events[..e].iter().rposition(|ev| matches!(ev, RuntimeEvent::UserMessage { .. })).unwrap_or(0);
    // lines in order; an upserted text block (same item_id again) rewrites its own line
    let mut lines: Vec<String> = vec![];
    let mut texts: HashMap<&str, usize> = HashMap::new();
    let mut tools: Vec<String> = vec![];
    for ev in &events[start..=end] {
        match ev {
            RuntimeEvent::UserMessage { text, .. } => lines.push(format!("요청: {text}\n")),
            RuntimeEvent::AgentText { item_id, text, .. } => match texts.get(item_id.as_str()) {
                Some(&i) => lines[i] = format!("결과: {text}\n"),
                None => {
                    texts.insert(item_id, lines.len());
                    lines.push(format!("결과: {text}\n"));
                }
            },
            RuntimeEvent::ToolStarted { name, .. } => tools.push(name.clone()),
            _ => {}
        }
    }
    let mut out = lines.concat();
    if !tools.is_empty() {
        tools.dedup();
        out.push_str(&format!("(사용한 도구: {})\n", tools.join(", ")));
    }
    let chars: Vec<char> = out.trim().chars().collect();
    chars[chars.len().saturating_sub(max_chars)..].iter().collect()
}

/// A tool result's content as text: a string, or the `text` of each part joined by "\n" (what the chat showed).
pub fn result_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts.iter().map(|c| c["text"].as_str().unwrap_or("")).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    }
}

/// Verbatim copies of the code the common events replaced, and the chat view's fold before and after, as
/// Rust. Only for the parity tests: the new path must give what the old one gave.
#[cfg(test)]
pub mod oracle {
    use super::*;
    use std::collections::{HashMap, VecDeque};

    fn between<'a>(s: &'a str, open: &str, close: &str) -> Option<&'a str> {
        let start = s.find(open)? + open.len();
        Some(&s[start..start + s[start..].find(close)?])
    }

    /// agent.rs `to_events` before R3
    pub fn to_events(e: &Value) -> Vec<Value> {
        match e["type"].as_str() {
            Some("assistant") => vec![json!({ "type": "assistant", "message": { "content": e["message"]["content"], "stop_reason": e["message"]["stop_reason"] } })],
            Some("user") if e["isMeta"] != true => {
                let c = &e["message"]["content"];
                if let Some(s) = c.as_str() {
                    if let Some(name) = between(s, "<command-name>", "</command-name>") {
                        let args = between(s, "<command-args>", "</command-args>").unwrap_or("");
                        return vec![json!({ "type": "user_input", "text": format!("{name} {args}").trim() })];
                    }
                    if s.contains("<local-command-stdout>") || s.contains("<local-command-stderr>") {
                        let out = [between(s, "<local-command-stdout>", "</local-command-stdout>"), between(s, "<local-command-stderr>", "</local-command-stderr>")];
                        let text = out.iter().flatten().map(|t| t.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n");
                        return if text.is_empty() { vec![] } else { vec![json!({ "type": "local_command", "text": text })] };
                    }
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

    /// antigravity.rs `to_events` before R3
    pub fn agy_to_events(e: &Value, pending: &mut VecDeque<String>) -> Vec<Value> {
        let step = e["step_index"].as_i64().unwrap_or(0);
        match e["type"].as_str() {
            Some("USER_INPUT") => {
                let c = e["content"].as_str().unwrap_or("");
                let text = between(c, "<USER_REQUEST>", "</USER_REQUEST>").unwrap_or(c).trim();
                pending.clear();
                if text.is_empty() { vec![] } else { vec![json!({ "type": "user_input", "text": text })] }
            }
            Some("PLANNER_RESPONSE") => {
                let mut content = vec![];
                if let Some(t) = e["content"].as_str().filter(|t| !t.trim().is_empty()) {
                    content.push(json!({ "type": "text", "text": t }));
                }
                for (i, call) in e["tool_calls"].as_array().into_iter().flatten().enumerate() {
                    let id = format!("agy-{step}-{i}");
                    pending.push_back(id.clone());
                    content.push(json!({ "type": "tool_use", "id": id, "name": call["name"], "input": call["args"] }));
                }
                if content.is_empty() {
                    return vec![];
                }
                let stop = if e["tool_calls"].as_array().is_some_and(|c| !c.is_empty()) { "tool_use" } else { "end_turn" };
                vec![json!({ "type": "assistant", "message": { "content": content, "stop_reason": stop } })]
            }
            Some("GENERIC") => match pending.pop_front() {
                Some(id) => vec![json!({ "type": "user", "message": { "content": [{ "type": "tool_result", "tool_use_id": id, "content": e["content"], "is_error": e["status"] == "ERROR" }] } })],
                None => vec![],
            },
            _ => vec![],
        }
    }

    /// agent.rs `state_after` before R3
    pub fn state_after(ev: &Value) -> Option<AgentState> {
        match ev["type"].as_str() {
            Some("user_input") => Some(AgentState::Running),
            Some("interrupted") | Some("local_command") => Some(AgentState::Idle),
            Some("assistant") => match ev["message"]["stop_reason"].as_str() {
                Some("tool_use") | None => None,
                Some(_) => Some(AgentState::Idle),
            },
            _ => None,
        }
    }

    /// agent.rs's tail loop: a finished turn is reported to whoever leads the agent
    pub fn done(prev: &AgentState, next: &AgentState, ev: &Value) -> bool {
        *next == AgentState::Idle && ev["type"] == "assistant" && *prev == AgentState::Running
    }

    /// agent.rs `recent_turn_text` before R3
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

    /// control.rs `last_answer` before R3 (after reading the events)
    pub fn last_answer(events: &[Value]) -> Option<String> {
        events.iter().rev().find_map(|e| {
            let texts: Vec<&str> = e["message"]["content"].as_array()?.iter().filter_map(|b| b["text"].as_str()).collect();
            (e["type"] == "assistant" && !texts.is_empty()).then(|| texts.join("\n"))
        })
    }

    /// The chat view's items (AgentNode.tsx `Item`).
    #[derive(Debug, PartialEq, Clone)]
    pub enum Item {
        User { text: String, files: Vec<String> },
        Text { text: String },
        Tool { id: String, name: String, input: Value, result: Option<String>, is_error: Option<bool> },
        Note { text: String },
    }

    /// AgentNode.tsx `fold` before R3, over the old events.
    pub fn legacy_items(events: &[Value]) -> Vec<Item> {
        let mut items: Vec<Item> = vec![];
        let mut tools: HashMap<String, usize> = HashMap::new();
        for ev in events {
            match ev["type"].as_str() {
                Some("user_input") => items.push(Item::User { text: ev["text"].as_str().unwrap_or("").into(), files: vec![] }),
                Some("assistant") => {
                    for b in ev["message"]["content"].as_array().into_iter().flatten() {
                        if b["type"] == "text" && b["text"].as_str().is_some_and(|t| !t.trim().is_empty()) {
                            items.push(Item::Text { text: b["text"].as_str().unwrap().into() });
                        }
                        if b["type"] == "tool_use" {
                            let id = b["id"].as_str().unwrap_or("").to_string();
                            tools.insert(id.clone(), items.len());
                            items.push(Item::Tool { id, name: b["name"].as_str().unwrap_or("?").into(), input: b["input"].clone(), result: None, is_error: None });
                        }
                    }
                }
                Some("user") => {
                    for b in ev["message"]["content"].as_array().into_iter().flatten() {
                        if b["type"] != "tool_result" {
                            continue;
                        }
                        if let Some(&i) = b["tool_use_id"].as_str().and_then(|id| tools.get(id)) {
                            if let Item::Tool { result, is_error, .. } = &mut items[i] {
                                *result = Some(result_text(&b["content"]));
                                *is_error = Some(b["is_error"] == true);
                            }
                        }
                    }
                }
                Some("local_command") => items.push(Item::Note { text: ev["text"].as_str().unwrap_or("").into() }),
                Some("interrupted") => items.push(Item::Note { text: "중단됨".into() }),
                _ => {}
            }
        }
        items
    }

    /// chatFold.ts `fold`, over the common events.
    pub fn items(events: &[RuntimeEvent]) -> Vec<Item> {
        let mut items: Vec<Item> = vec![];
        let mut texts: HashMap<String, usize> = HashMap::new();
        let mut tools: HashMap<String, usize> = HashMap::new();
        for ev in events {
            match ev {
                RuntimeEvent::UserMessage { text, files, .. } => items.push(Item::User { text: text.clone(), files: files.clone() }),
                RuntimeEvent::AgentText { item_id, text, .. } => match texts.get(item_id) {
                    Some(&i) => items[i] = Item::Text { text: text.clone() },
                    None => {
                        texts.insert(item_id.clone(), items.len());
                        items.push(Item::Text { text: text.clone() });
                    }
                },
                RuntimeEvent::ToolStarted { tool_call_id, name, input, .. } => {
                    tools.insert(tool_call_id.clone(), items.len());
                    items.push(Item::Tool { id: tool_call_id.clone(), name: name.clone(), input: input.clone(), result: None, is_error: None });
                }
                RuntimeEvent::ToolFinished { tool_call_id, output, is_error: err, .. } => {
                    if let Some(&i) = tools.get(tool_call_id) {
                        if let Item::Tool { result, is_error, .. } = &mut items[i] {
                            *result = Some(output.clone());
                            *is_error = Some(*err);
                        }
                    }
                }
                RuntimeEvent::Notice { text, .. } => items.push(Item::Note { text: text.clone() }),
                _ => {}
            }
        }
        items.retain(|it| !matches!(it, Item::Text { text } if text.trim().is_empty()));
        items
    }
}

#[cfg(test)]
mod tests {
    use super::oracle::{self, legacy_items, items};
    use super::*;
    use crate::antigravity::AgyNormalizer;
    use crate::claude::ClaudeNormalizer;
    use crate::transcript::Normalize;
    use std::collections::VecDeque;

    #[test]
    fn serializes_to_the_ts_contract() {
        let ev = RuntimeEvent::ToolStarted { turn_id: "p1".into(), tool_call_id: "t1".into(), message_id: Some("u1".into()), name: "Read".into(), input: json!({"file_path": "a"}) };
        assert_eq!(serde_json::to_value(&ev).unwrap(), json!({"type": "toolStarted", "turnId": "p1", "toolCallId": "t1", "messageId": "u1", "name": "Read", "input": {"file_path": "a"}}));
        let end = RuntimeEvent::TurnEnded { turn_id: "p1".into(), outcome: TurnOutcome::Done { stop_reason: "end_turn".into() } };
        assert_eq!(serde_json::to_value(&end).unwrap(), json!({"type": "turnEnded", "turnId": "p1", "outcome": {"kind": "done", "stopReason": "end_turn"}}));
        let cancelled = RuntimeEvent::TurnEnded { turn_id: "p1".into(), outcome: TurnOutcome::Cancelled };
        assert_eq!(serde_json::to_value(&cancelled).unwrap()["outcome"], json!({"kind": "cancelled"}));
        let user = RuntimeEvent::UserMessage { turn_id: "p1".into(), item_id: "u1".into(), kind: UserKind::Shell, text: "!ls".into(), files: vec![] };
        assert_eq!(serde_json::to_value(&user).unwrap(), json!({"type": "userMessage", "turnId": "p1", "itemId": "u1", "kind": "shell", "text": "!ls", "files": []}));
        let env = Envelope { worktree_path: "C:/w".into(), epoch: 3, seq: None, event: RuntimeEvent::Reset { reason: ResetReason::Truncated } };
        assert_eq!(serde_json::to_value(&env).unwrap(), json!({"worktreePath": "C:/w", "epoch": 3, "seq": null, "event": {"type": "reset", "reason": "truncated"}}));
        let ap = RuntimeEvent::ApprovalResolved { turn_id: None, tool_call_id: Some("t".into()), approval_id: 4, resolution: Resolution::Withdrawn };
        assert_eq!(serde_json::to_value(&ap).unwrap(), json!({"type": "approvalResolved", "turnId": null, "toolCallId": "t", "approvalId": 4, "resolution": "withdrawn"}));
        let delta = RuntimeEvent::Delta { turn_id: "p".into(), item_id: "i".into(), channel: DeltaChannel::ToolOutput, offset: 2, text: "x".into() };
        assert_eq!(serde_json::to_value(&delta).unwrap()["channel"], "toolOutput");
        assert!(end.durable() && user.durable() && !delta.durable() && !ap.durable());
        assert!(!RuntimeEvent::Exited { turn_id: None }.durable());
        assert_eq!(delta.turn_id(), Some("p"));
        assert_eq!(RuntimeEvent::Reset { reason: ResetReason::Restart }.turn_id(), None);
    }

    #[test]
    fn caps_unknown_content() {
        let small = RuntimeEvent::unknown(None, "claude", &json!({"type": "x", "a": 1}));
        assert_eq!(small, RuntimeEvent::Unknown { turn_id: None, source: "claude".into(), raw: json!({"type": "x", "a": 1}) });
        let big = RuntimeEvent::unknown(Some("t".into()), "claude", &json!({"type": "image", "data": "a".repeat(3000)}));
        assert_eq!(big, RuntimeEvent::Unknown { turn_id: Some("t".into()), source: "claude".into(), raw: json!({"type": "image", "truncated": true}) });
    }

    #[test]
    fn status_follows_the_legacy_table() {
        let t = || "t".to_string();
        let user = RuntimeEvent::UserMessage { turn_id: t(), item_id: t(), kind: UserKind::Command, text: "/model".into(), files: vec![] };
        let note = RuntimeEvent::Notice { turn_id: t(), item_id: t(), kind: NoticeKind::Interrupted, text: "중단됨".into() };
        let end = |o: TurnOutcome| RuntimeEvent::TurnEnded { turn_id: t(), outcome: o };
        let done = end(TurnOutcome::Done { stop_reason: "end_turn".into() });
        let failed = end(TurnOutcome::Failed { error: "rate_limit".into(), message: "x".into() });
        assert_eq!(status_after(&user), Some(AgentState::Running));
        assert_eq!(status_after(&note), Some(AgentState::Idle));
        for o in [TurnOutcome::Cancelled, TurnOutcome::Command, TurnOutcome::Done { stop_reason: "max_tokens".into() }] {
            assert_eq!(status_after(&end(o)), Some(AgentState::Idle));
        }
        assert_eq!(status_after(&end(TurnOutcome::Replaced)), None);
        assert_eq!(status_after(&RuntimeEvent::TurnStarted { turn_id: t(), initiator: Initiator::User }), None);
        assert_eq!(status_after(&RuntimeEvent::Exited { turn_id: None }), None);

        let run = AgentState::Running;
        assert!(finishes_turn(&run, &done) && finishes_turn(&run, &failed));
        for o in [TurnOutcome::Cancelled, TurnOutcome::Command, TurnOutcome::Replaced] {
            assert!(!finishes_turn(&run, &end(o)));
        }
        assert!(!finishes_turn(&AgentState::Idle, &done));
        assert!(!finishes_turn(&run, &note));
    }

    fn text(turn: &str, item: &str, msg: &str, t: &str) -> RuntimeEvent {
        RuntimeEvent::AgentText { turn_id: turn.into(), item_id: item.into(), message_id: Some(msg.into()), text: t.into() }
    }
    fn prompt(turn: &str, t: &str) -> RuntimeEvent {
        RuntimeEvent::UserMessage { turn_id: turn.into(), item_id: turn.into(), kind: UserKind::Prompt, text: t.into(), files: vec![] }
    }
    fn tool(turn: &str, id: &str, msg: &str, name: &str) -> RuntimeEvent {
        RuntimeEvent::ToolStarted { turn_id: turn.into(), tool_call_id: id.into(), message_id: Some(msg.into()), name: name.into(), input: json!({}) }
    }

    #[test]
    fn recent_turn_text_takes_last_turn_only() {
        let evs = vec![prompt("a", "old"), prompt("b", "로그인 고쳐줘"), tool("b", "t1", "m1", "Edit"), text("b", "m2#0", "m2", "고쳤어요")];
        let t = recent_turn_text(&evs, 1000);
        assert_eq!(t, "요청: 로그인 고쳐줘\n결과: 고쳤어요\n(사용한 도구: Edit)");
        assert_eq!(recent_turn_text(&evs, 4), "dit)");
        assert_eq!(recent_turn_text(&[], 100), "");
        // a slash command or a turn still running after the answer leaves the summary target unchanged
        let mut later = evs.clone();
        later.push(RuntimeEvent::UserMessage { turn_id: "c".into(), item_id: "c".into(), kind: UserKind::Command, text: "/model".into(), files: vec![] });
        later.push(RuntimeEvent::Notice { turn_id: "c".into(), item_id: "n".into(), kind: NoticeKind::LocalCommand, text: "Set model".into() });
        later.push(prompt("d", "다음 작업"));
        assert_eq!(recent_turn_text(&later, 1000), t);
        // agy: the tools named after the text in the same step still count
        let agy = vec![prompt("agy-t0", "봐줘"), text("agy-t0", "agy-1#text", "agy-1", "볼게요"), tool("agy-t0", "agy-1-0", "agy-1", "run_command")];
        assert_eq!(recent_turn_text(&agy, 1000), "요청: 봐줘\n결과: 볼게요\n(사용한 도구: run_command)");
    }

    #[test]
    fn last_answer_is_the_last_unit_with_text() {
        assert_eq!(last_answer(&[]), None);
        assert_eq!(last_answer(&[prompt("a", "hi")]), None);
        // mid-turn text counts; a later tool call doesn't hide it
        let mid = vec![prompt("a", "hi"), text("a", "u1#0", "u1", "읽어 볼게요"), tool("a", "t", "u2", "Read")];
        assert_eq!(last_answer(&mid).as_deref(), Some("읽어 볼게요"));
        // several text blocks of one line are joined, blank ones too; an earlier line's text isn't
        let multi = vec![prompt("a", "hi"), text("a", "u0#0", "u0", "앞"), text("a", "u1#0", "u1", ""), text("a", "u1#1", "u1", "끝")];
        assert_eq!(last_answer(&multi).as_deref(), Some("\n끝"));
    }

    #[test]
    fn an_upserted_text_block_counts_once() {
        // ACP (R4): a block opens empty, then comes whole under the same item_id
        let evs = vec![prompt("p:1", "해줘"), text("p:1", "i1", "M", ""), tool("p:1", "t", "M", "Read"), text("p:1", "i1", "M", "full")];
        assert_eq!(last_answer(&evs).as_deref(), Some("full"));
        assert_eq!(recent_turn_text(&evs, 1000), "요청: 해줘\n결과: full\n(사용한 도구: Read)");
        // two blocks of one message, each upserted, stay in their first order
        let two = vec![prompt("p:1", "x"), text("p:1", "a", "M", ""), text("p:1", "b", "M", ""), text("p:1", "a", "M", "A"), text("p:1", "b", "M", "B")];
        assert_eq!(last_answer(&two).as_deref(), Some("A\nB"));
        assert_eq!(recent_turn_text(&two, 1000), "요청: x\n결과: A\n결과: B");
    }

    // ---------- parity with the code the common events replaced ----------

    fn lines(s: &str) -> Vec<Value> {
        s.lines().map(str::trim).filter(|l| !l.is_empty()).map(|l| serde_json::from_str(l).unwrap()).collect()
    }

    /// (a) the chat items, (b) the state changes from Idle, (c) the lines a worker-done fires on, (d) the
    /// summary text and `maestro read`'s answer: old path vs new, over the same lines.
    pub(crate) fn parity(lines: &[Value], agy: bool) -> Result<(), String> {
        let mut pending = VecDeque::new();
        let mut norm: Box<dyn Normalize> = if agy { Box::new(AgyNormalizer::default()) } else { Box::new(ClaudeNormalizer::default()) };
        let (mut old, mut new) = (vec![], vec![]);
        let (mut old_st, mut new_st) = (AgentState::Idle, AgentState::Idle);
        let (mut old_states, mut new_states, mut old_done, mut new_done) = (vec![], vec![], vec![], vec![]);
        for (n, l) in lines.iter().enumerate() {
            let evs = if agy { oracle::agy_to_events(l, &mut pending) } else { oracle::to_events(l) };
            for ev in &evs {
                if let Some(next) = oracle::state_after(ev) {
                    if oracle::done(&old_st, &next, ev) {
                        old_done.push(n);
                    }
                    if next != old_st {
                        old_st = next.clone();
                        old_states.push((n, next));
                    }
                }
            }
            old.extend(evs);
            let mut out = vec![];
            norm.line(l, n as u64 + 1, &mut out);
            for ev in &out {
                if let Some(next) = status_after(ev) {
                    if finishes_turn(&new_st, ev) {
                        new_done.push(n);
                    }
                    if next != new_st {
                        new_st = next.clone();
                        new_states.push((n, next));
                    }
                }
            }
            new.extend(out);
        }
        let check = |what: &str, a: String, b: String| if a == b { Ok(()) } else { Err(format!("{what}\n old: {a}\n new: {b}")) };
        check("items", format!("{:?}", legacy_items(&old)), format!("{:?}", items(&new)))?;
        check("states", format!("{old_states:?}"), format!("{new_states:?}"))?;
        check("worker_done", format!("{old_done:?}"), format!("{new_done:?}"))?;
        check("recent_turn_text", oracle::recent_turn_text(&old, 4000), recent_turn_text(&new, 4000))?;
        check("last_answer", format!("{:?}", oracle::last_answer(&old)), format!("{:?}", last_answer(&new)))
    }

    /// Every prefix too: `maestro read` and summaries run mid-turn.
    fn parity_all_prefixes(ls: &[Value], agy: bool) {
        for k in 1..=ls.len() {
            if let Err(e) = parity(&ls[..k], agy) {
                panic!("after line {k}: {e}");
            }
        }
    }

    const CLAUDE: &str = r#"
        {"type":"user","uuid":"u1","promptId":"p1","message":{"content":"로그인 고쳐줘"}}
        {"type":"assistant","uuid":"a1","message":{"id":"m1","content":[{"type":"thinking","thinking":"hm"}],"stop_reason":null}}
        {"type":"assistant","uuid":"a2","message":{"id":"m1","content":[{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"a.ts"}}],"stop_reason":"tool_use"}}
        {"type":"user","uuid":"u2","promptId":"p1","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"file body"}]}}
        {"type":"assistant","uuid":"a3","message":{"id":"m2","content":[{"type":"text","text":"고칠게요"},{"type":"tool_use","id":"t2","name":"Bash","input":{"command":"ls"}}],"stop_reason":"tool_use"}}
        {"type":"user","uuid":"u3","promptId":"p1","message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":[{"type":"text","text":"a"},{"type":"image"},{"type":"text","text":"b"}],"is_error":true}]}}
        {"type":"assistant","uuid":"a4","message":{"id":"m3","content":[{"type":"text","text":"고쳤어요"}],"stop_reason":"end_turn"}}
        {"type":"system","subtype":"stop_hook_summary","uuid":"s1"}
        {"type":"system","subtype":"turn_duration","uuid":"s1b"}
        {"type":"user","uuid":"u4","promptId":"p2","message":{"content":"다음"}}
        {"type":"assistant","uuid":"a5","message":{"id":"m4","content":[{"type":"thinking","thinking":"..."}],"stop_reason":"end_turn"}}
        {"type":"assistant","uuid":"a6","message":{"id":"m4","content":[{"type":"text","text":"답"}],"stop_reason":"end_turn"}}
        {"type":"user","uuid":"u5","message":{"content":"<command-name>/model</command-name>\n<command-args>opus</command-args>"}}
        {"type":"user","uuid":"u6","message":{"content":"<local-command-stdout>Set model to opus</local-command-stdout>"}}
        {"type":"user","uuid":"u7","message":{"content":"<command-name>/compact</command-name>"}}
        {"type":"user","uuid":"u8","message":{"content":"<local-command-stdout></local-command-stdout>"}}
        {"type":"user","uuid":"u9","message":{"content":"<bash-input>ls</bash-input>"}}
        {"type":"user","uuid":"u10","message":{"content":"<bash-stdout></bash-stdout><bash-stderr></bash-stderr>"}}
        {"type":"system","subtype":"local_command","uuid":"s2","content":"<local-command-stdout>hi</local-command-stdout>"}
        {"type":"system","subtype":"local_command","uuid":"s2b","content":"<local-command-stdout>  </local-command-stdout>"}
        {"type":"user","uuid":"u11","promptId":"p3","message":{"content":"길게 해줘"}}
        {"type":"assistant","uuid":"a7","message":{"id":"m5","content":[{"type":"text","text":"시작"}],"stop_reason":null}}
        {"type":"user","uuid":"u12","message":{"content":[{"type":"text","text":"[Request interrupted by user]"}]}}
        {"type":"user","uuid":"u13","message":{"content":"[Request interrupted by user for tool use]"}}
        {"type":"user","uuid":"u14","promptId":"p4","message":{"content":"또"}}
        {"type":"assistant","uuid":"a8","isApiErrorMessage":true,"error":"rate_limit","message":{"id":"m6","content":[{"type":"text","text":"API Error: 429"}],"stop_reason":"stop_sequence"}}
        {"type":"system","subtype":"api_error","uuid":"s3","error":{"message":"Connection error."},"retryAttempt":1}
        {"type":"system","subtype":"compact_boundary","uuid":"s4"}
        {"type":"user","isMeta":true,"uuid":"u15","message":{"content":"caveat"}}
        {"type":"file-history-snapshot","messageId":"x"}
        {"type":"user","uuid":"u16","promptId":"p5","message":{"content":[{"type":"text","text":"이 그림 봐"},{"type":"image","source":{}}]}}
        {"type":"user","uuid":"u17","promptId":"p5","message":{"content":"같은 프롬프트"}}
        {"type":"assistant","uuid":"a9","message":{"id":"m7","content":[{"type":"text","text":"보는 중"},{"type":"tool_use","id":"t3","name":"Edit","input":{}},{"type":"server_tool_use","id":"x"}],"stop_reason":"tool_use"}}
        {"type":"user","uuid":"u18","promptId":"p6","message":{"content":"끼어들기"}}
        {"type":"user","uuid":"u19","message":{"content":[{"type":"text","text":"  "}]}}
        {"type":"assistant","uuid":"a10","message":{"id":"m8","content":[{"type":"text","text":""},{"type":"text","text":"끝"}],"stop_reason":"end_turn"}}
        {"type":"assistant","uuid":"a11","message":{"id":"m9","content":[{"type":"text","text":"덧붙임"}],"stop_reason":"end_turn"}}
        {"type":"user","uuid":"u20","message":{"content":"<local-command-stderr>oops</local-command-stderr>"}}
    "#;

    const AGY: &str = r#"
        {"step_index":0,"type":"USER_INPUT","content":"<USER_REQUEST>\n안녕\n</USER_REQUEST>\n<ADDITIONAL_METADATA>x</ADDITIONAL_METADATA>"}
        {"step_index":1,"type":"PLANNER_RESPONSE","thinking":"hm"}
        {"step_index":2,"type":"PLANNER_RESPONSE","content":"볼게요","thinking":"t","tool_calls":[{"name":"run_command","args":{"CommandLine":"ls"}},{"name":"view_file","args":{"AbsolutePath":"a"}}]}
        {"step_index":3,"type":"GENERIC","status":"DONE","content":"a.txt"}
        {"step_index":4,"type":"GENERIC","status":"ERROR","content":"no file"}
        {"step_index":5,"type":"SYSTEM_MESSAGE","content":"x"}
        {"step_index":6,"type":"PLANNER_RESPONSE","content":"끝났어요"}
        {"step_index":7,"type":"USER_INPUT","content":"<USER_REQUEST>다음</USER_REQUEST>"}
        {"step_index":8,"type":"PLANNER_RESPONSE","tool_calls":[{"name":"run_command","args":{}}]}
        {"step_index":9,"type":"GENERIC","status":"DONE","content":"ok"}
        {"step_index":10,"type":"GENERIC","status":"DONE","content":"orphan"}
        {"step_index":11,"type":"PLANNER_RESPONSE","content":"   "}
        {"step_index":12,"type":"CHECKPOINT","content":"?"}
        {"step_index":13,"type":"PLANNER_RESPONSE","content":"다 했어요"}
        {"step_index":14,"type":"USER_INPUT","content":"<USER_REQUEST>  </USER_REQUEST>"}
        {"step_index":15,"type":"PLANNER_RESPONSE","content":"빈 요청에도 답"}
    "#;

    /// The chat items as TS `Item` JSON (chatFold.ts).
    fn item_json(it: &oracle::Item) -> Value {
        match it {
            oracle::Item::User { text, files } => json!({ "kind": "user", "text": text, "files": files }),
            oracle::Item::Text { text } => json!({ "kind": "text", "text": text }),
            oracle::Item::Tool { id, name, input, result, is_error } => {
                let mut t = json!({ "kind": "tool", "id": id, "name": name, "input": input });
                if let Some(r) = result {
                    t["result"] = json!(r);
                }
                if let Some(e) = is_error {
                    t["isError"] = json!(e);
                }
                t
            }
            oracle::Item::Note { text } => json!({ "kind": "note", "text": text }),
        }
    }

    /// The same lines through the old path (its events) and the new one (common events).
    fn both(ls: &[Value], agy: bool) -> (Vec<Value>, Vec<RuntimeEvent>) {
        let mut pending = VecDeque::new();
        let mut norm: Box<dyn Normalize> = if agy { Box::new(AgyNormalizer::default()) } else { Box::new(ClaudeNormalizer::default()) };
        let (mut old, mut new) = (vec![], vec![]);
        for (n, l) in ls.iter().enumerate() {
            old.extend(if agy { oracle::agy_to_events(l, &mut pending) } else { oracle::to_events(l) });
            norm.line(l, n as u64 + 1, &mut new);
        }
        (old, new)
    }

    /// The fixture the TS check (`npm test`, scripts/chat-fold-parity.ts) runs chatFold.ts `fold` over: the common
    /// events of each sample and the items the pre-R3 fold drew from the same lines. The UI's own fold is held to
    /// the old one, not only its Rust mirror (`oracle::items`). `MAESTRO_WRITE_FIXTURES=1 cargo test chat_fold_fixture`
    /// rewrites it after a deliberate change.
    #[test]
    fn chat_fold_fixture_is_current() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/chat-parity.json");
        let cases: Vec<Value> = [("claude", CLAUDE, false), ("antigravity", AGY, true)]
            .iter()
            .map(|(name, src, agy)| {
                let (old, new) = both(&lines(src), *agy);
                json!({ "name": name, "events": new, "items": legacy_items(&old).iter().map(item_json).collect::<Vec<_>>() })
            })
            .collect();
        let want = serde_json::to_string_pretty(&json!({ "cases": cases })).unwrap() + "\n";
        let have = std::fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");
        if std::env::var_os("MAESTRO_WRITE_FIXTURES").is_some() || have.is_empty() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &want).unwrap();
        } else {
            assert!(have == want, "{} is stale: rerun with MAESTRO_WRITE_FIXTURES=1, then `npm test`", path.display());
        }
    }

    #[test]
    fn claude_parity_with_legacy() {
        parity_all_prefixes(&lines(CLAUDE), false);
    }

    #[test]
    fn antigravity_parity_with_legacy() {
        parity_all_prefixes(&lines(AGY), true);
    }

    /// `MAESTRO_PARITY_DIR=~/.claude/projects cargo test -- --ignored parity_over_local --nocapture`: the same checks
    /// over real transcripts (agy's `transcript.jsonl` files are read as agy, the rest as Claude).
    #[test]
    #[ignore]
    fn parity_over_local_transcripts() {
        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x == "jsonl") {
                    out.push(p);
                }
            }
        }
        let dir = std::env::var("MAESTRO_PARITY_DIR").expect("MAESTRO_PARITY_DIR");
        let mut files = vec![];
        walk(std::path::Path::new(&dir), &mut files);
        let mut bad = vec![];
        for f in &files {
            let ls: Vec<Value> = std::fs::read_to_string(f).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
            let agy = f.ends_with("transcript.jsonl") && f.to_string_lossy().contains("antigravity");
            // MAESTRO_PARITY_STEP=n: also every n-th prefix (mid-turn `maestro read`)
            let step: usize = std::env::var("MAESTRO_PARITY_STEP").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
            let ends = (1..=ls.len()).filter(|k| step > 0 && k % step == 0).chain([ls.len()]);
            if let Some(e) = ends.filter_map(|k| parity(&ls[..k], agy).err().map(|e| format!("after line {k}: {e}"))).next() {
                bad.push(format!("{}: {}", f.display(), e.chars().take(600).collect::<String>()));
            }
        }
        println!("{} transcripts, {} mismatched", files.len(), bad.len());
        assert!(bad.is_empty(), "{}", bad.join("\n\n"));
    }
}
