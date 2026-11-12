//! Antigravity CLI (`agy`, Google's successor to Gemini CLI) as a PTY agent: argv, where its transcript
//! lives, and its transcript steps as common events (events.rs).
//! Everything lives under the CLI's own data dir `~/.gemini/antigravity-cli/` (not `antigravity/`, which
//! is the IDE app's):
//! - transcript: `brain/<conversation id>/.system_generated/logs/transcript.jsonl`
//! - `cache/last_conversations.json`: working folder → its latest conversation id

use crate::agent::{system_brief, AgentMeta, Files};
use crate::catalog::AgentDef;
use crate::events::{result_text, RuntimeEvent, TurnOutcome, UserKind};
use crate::runtime::{Launch, LaunchCtx, PtyAgent};
use crate::transcript::{Normalize, TurnTracker};
use serde_json::Value;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

pub const ID: &str = "antigravity";

fn data_dir(home: &Path) -> PathBuf {
    home.join(".gemini").join("antigravity-cli")
}

pub fn transcript(home: &Path, conversation: &str) -> PathBuf {
    data_dir(home).join("brain").join(conversation).join(".system_generated").join("logs").join("transcript.jsonl")
}

/// The conversation agy is keeping for a working folder. agy can't be told an id up front, but it records
/// this map as soon as a conversation starts, so a new agent's conversation is found by its worktree.
pub fn conversation_for(home: &Path, cwd: &str) -> Option<String> {
    let map: Value = serde_json::from_str(&std::fs::read_to_string(data_dir(home).join("cache").join("last_conversations.json")).ok()?).ok()?;
    lookup(&map, cwd)
}

/// Keys are Windows paths with backslashes; ours use forward slashes. Compared case-insensitively.
fn lookup(map: &Value, cwd: &str) -> Option<String> {
    let want = cwd.replace('\\', "/").to_lowercase();
    map.as_object()?.iter().find(|(k, _)| k.replace('\\', "/").to_lowercase() == want).and_then(|(_, v)| v.as_str().map(String::from))
}

/// Maestro's permission choices → agy's flags, after `exe` (what `cli::command` resolved). `rules`: a folder
/// holding Maestro's brief as `AGENTS.md`; agy has no system-prompt flag but loads the rules of every workspace
/// folder, `--add-dir` ones included.
pub fn argv(exe: Vec<String>, permission_mode: &str, conversation: Option<&str>, model: Option<&str>, rules: &Path) -> Vec<String> {
    let mut a = exe;
    a.extend(["--add-dir".into(), rules.to_string_lossy().into()]);
    match permission_mode {
        "bypassPermissions" => a.push("--dangerously-skip-permissions".into()),
        "plan" => a.extend(["--mode".into(), "plan".into()]),
        "auto" => a.extend(["--mode".into(), "auto".into()]),
        _ => a.extend(["--mode".into(), "accept-edits".into()]),
    }
    if let Some(id) = conversation {
        a.extend(["--conversation".into(), id.into()]);
    }
    if let Some(m) = model {
        a.extend(["--model".into(), m.into()]);
    }
    a
}

/// One-shot `agy -p` for the assistant model (summaries). Prints JSON with `response` and `conversation_id`.
pub fn ask(exe: Vec<String>, prompt: &str, model: &str) -> Result<String, String> {
    let mut cmd = std::process::Command::new(&exe[0]);
    cmd.args(&exe[1..]).args(["-p", prompt, "--output-format", "json", "--disable-slash-commands"]).current_dir(std::env::temp_dir());
    if !model.is_empty() {
        cmd.args(["--model", model]);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().map_err(|e| format!("agy 실행 실패: {e}"))?;
    let v: Value = serde_json::from_slice(&out.stdout).map_err(|_| "agy 응답을 읽지 못했어요".to_string())?;
    // runs in the temp dir, so its conversation never shows up as a worktree's (conversation_for)
    match v["response"].as_str() {
        Some(r) if v["status"] == "SUCCESS" => Ok(r.trim().to_string()),
        _ => Err("agy 요약 실패".into()),
    }
}

fn between<'a>(s: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = s.find(open)? + open.len();
    Some(&s[start..start + s[start..].find(close)?])
}

pub struct AgyAgent;
pub static AGY: AgyAgent = AgyAgent;

impl PtyAgent for AgyAgent {
    fn id(&self) -> &'static str {
        ID
    }

    fn launch(&self, ctx: &LaunchCtx, meta: &mut AgentMeta, files: &Files, exe: Vec<String>) -> Result<Launch, String> {
        // agy may already keep a conversation for this worktree (e.g. one started before we tracked it)
        if meta.cli_session_id.is_none() {
            if let Some(id) = conversation_for(ctx.home, ctx.worktree) {
                meta.cli_session_id = Some(id);
                files.write_meta(meta)?;
            }
        }
        let id = meta.cli_session_id.clone();
        // the same brief Claude gets, as rules agy reads from a folder outside the worktree (nothing to commit by mistake)
        let rules = files.meta.with_extension("brief");
        std::fs::create_dir_all(&rules).map_err(|e| e.to_string())?;
        std::fs::write(rules.join("AGENTS.md"), system_brief(ctx.home, meta, ctx.worktree)).map_err(|e| e.to_string())?;
        let argv = argv(exe, &meta.permission_mode, id.as_deref(), meta.model.as_deref(), &rules);
        Ok(Launch { argv, transcript: id.as_ref().map(|id| transcript(ctx.home, id)), resumed: id.is_some() })
    }

    fn transcript(&self, home: &Path, _cwd: &str, id: &str) -> PathBuf {
        transcript(home, id)
    }

    fn current_conversation(&self, home: &Path, cwd: &str) -> Option<String> {
        conversation_for(home, cwd)
    }

    fn normalizer(&self) -> Box<dyn Normalize> {
        Box::new(AgyNormalizer::default())
    }

    fn preview(&self, def: &AgentDef, permission_mode: &str, model: Option<&str>) -> Vec<String> {
        let mut a = argv(vec![def.bin.into()], permission_mode, None, model, Path::new(""));
        a.drain(1..3); // --add-dir <rules>
        a
    }

    fn summarize(&self, exe: Vec<String>, conversation: &str, model: &str) -> Result<String, String> {
        ask(exe, &format!("{}\n\n---\n{conversation}", crate::agent::SUMMARY_PROMPT), model)
    }
}

/// agy's transcript steps → common events. `pending` carries tool call ids across steps: agy logs a call in the
/// model's step and its output as later GENERIC steps, in order.
/// Ids: turn = `agy-t<step>` of the user's step (`agy-a<step>` when the agent starts one); the text of a step =
/// `agy-<step>#text`; tool calls = `agy-<step>-<i>`.
#[derive(Default)]
pub struct AgyNormalizer {
    t: TurnTracker,
    pending: VecDeque<String>,
}

impl Normalize for AgyNormalizer {
    fn line(&mut self, e: &Value, _line_no: u64, out: &mut Vec<RuntimeEvent>) {
        let step = e["step_index"].as_i64().unwrap_or(0);
        let msg = format!("agy-{step}");
        match e["type"].as_str() {
            Some("USER_INPUT") => {
                let c = e["content"].as_str().unwrap_or("");
                let text = between(c, "<USER_REQUEST>", "</USER_REQUEST>").unwrap_or(c).trim();
                self.pending.clear();
                if !text.is_empty() {
                    let turn_id = self.t.open_user(format!("agy-t{step}"), out);
                    out.push(RuntimeEvent::UserMessage { turn_id, item_id: msg, kind: UserKind::Prompt, text: text.into(), files: vec![] });
                }
            }
            Some("PLANNER_RESPONSE") => {
                let nonblank = |k: &str| e[k].as_str().filter(|t| !t.trim().is_empty());
                let calls: &[Value] = e["tool_calls"].as_array().map_or(&[], |a| a.as_slice());
                let thinking = |turn_id: String| RuntimeEvent::Thinking { turn_id, item_id: format!("{msg}#thinking"), message_id: Some(msg.clone()), text: nonblank("thinking").unwrap_or("").into() };
                if nonblank("content").is_none() && calls.is_empty() {
                    // thinking only: kept, but it neither opens nor ends a turn
                    if nonblank("thinking").is_some() {
                        out.push(thinking(self.t.fallback()));
                    }
                    return;
                }
                let turn = self.t.ensure(None, || format!("agy-a{step}"), out);
                if nonblank("thinking").is_some() {
                    out.push(thinking(turn.clone()));
                }
                if let Some(t) = nonblank("content") {
                    out.push(RuntimeEvent::AgentText { turn_id: turn.clone(), item_id: format!("{msg}#text"), message_id: Some(msg.clone()), text: t.into() });
                }
                for (i, call) in calls.iter().enumerate() {
                    let id = format!("agy-{step}-{i}");
                    let name = call["name"].as_str().unwrap_or("?");
                    self.pending.push_back(id.clone());
                    self.t.tool_started(&turn, &id, name, &call["args"]);
                    out.push(RuntimeEvent::ToolStarted { turn_id: turn.clone(), tool_call_id: id, message_id: Some(msg.clone()), name: name.into(), input: call["args"].clone() });
                }
                // a reply with no tool calls ends the turn
                if calls.is_empty() {
                    self.t.end(TurnOutcome::Done { stop_reason: "end_turn".into() }, None, out);
                }
            }
            Some("GENERIC") => match self.pending.pop_front() {
                Some(tool_call_id) => {
                    let turn_id = self.t.tool_finished(&tool_call_id);
                    out.push(RuntimeEvent::ToolFinished { turn_id, tool_call_id, output: result_text(&e["content"]), is_error: e["status"] == "ERROR" });
                }
                // output of no call we know (the user typed while tools ran, cleared `pending`): kept as it is
                None => out.push(RuntimeEvent::unknown(self.t.current().map(String::from), "antigravity", e)),
            },
            Some("SYSTEM_MESSAGE") => {}
            _ => out.push(RuntimeEvent::unknown(self.t.current().map(String::from), "antigravity", e)),
        }
    }

    fn tracker(&self) -> &TurnTracker {
        &self.t
    }

    fn tracker_mut(&mut self) -> &mut TurnTracker {
        &mut self.t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Initiator;
    use serde_json::json;

    fn run(lines: &[Value]) -> Vec<RuntimeEvent> {
        let mut n = AgyNormalizer::default();
        let mut out = vec![];
        for (i, l) in lines.iter().enumerate() {
            n.line(l, i as u64 + 1, &mut out);
        }
        out
    }

    #[test]
    fn converts_transcript_steps() {
        let user = json!({"step_index": 0, "type": "USER_INPUT", "content": "<USER_REQUEST>\n안녕\n</USER_REQUEST>\n<ADDITIONAL_METADATA>x</ADDITIONAL_METADATA>"});
        let call = json!({"step_index": 3, "type": "PLANNER_RESPONSE", "tool_calls": [{"name": "run_command", "args": {"CommandLine": "ls"}}]});
        let out = json!({"step_index": 4, "type": "GENERIC", "status": "DONE", "content": "a.txt"});
        let done = json!({"step_index": 5, "type": "PLANNER_RESPONSE", "content": "끝났어요"});
        let evs = run(&[user, call, out, done]);
        assert_eq!(evs[0], RuntimeEvent::TurnStarted { turn_id: "agy-t0".into(), initiator: Initiator::User });
        assert!(matches!(&evs[1], RuntimeEvent::UserMessage { text, .. } if text == "안녕"));
        assert!(matches!(&evs[2], RuntimeEvent::ToolStarted { tool_call_id, turn_id, .. } if tool_call_id == "agy-3-0" && turn_id == "agy-t0"));
        assert!(matches!(&evs[3], RuntimeEvent::ToolFinished { tool_call_id, output, is_error: false, .. } if tool_call_id == "agy-3-0" && output == "a.txt"));
        assert!(matches!(&evs[4], RuntimeEvent::AgentText { item_id, message_id: Some(m), .. } if item_id == "agy-5#text" && m == "agy-5"));
        assert_eq!(evs[5], RuntimeEvent::TurnEnded { turn_id: "agy-t0".into(), outcome: TurnOutcome::Done { stop_reason: "end_turn".into() } });
        // thinking only: kept as thinking, no turn change; system messages give nothing; unknown steps are kept
        let more = run(&[json!({"step_index": 6, "type": "PLANNER_RESPONSE", "thinking": "hm"}), json!({"type": "SYSTEM_MESSAGE", "content": "x"}), json!({"step_index": 8, "type": "CHECKPOINT"})]);
        assert!(matches!(&more[..], [RuntimeEvent::Thinking { turn_id, .. }, RuntimeEvent::Unknown { source, .. }] if turn_id == "t0" && source == "antigravity"));
        // the agent answering with no prompt before it opens a turn of its own
        let own = run(&[json!({"step_index": 9, "type": "PLANNER_RESPONSE", "content": "계속"})]);
        assert_eq!(own[0], RuntimeEvent::TurnStarted { turn_id: "agy-a9".into(), initiator: Initiator::Agent });
        // tool output with no call waiting for it is kept raw, not dropped
        let orphan = run(&[json!({"step_index": 10, "type": "GENERIC", "content": "late"})]);
        assert!(matches!(&orphan[..], [RuntimeEvent::Unknown { source, .. }] if source == "antigravity"));
    }

    #[test]
    fn maps_permission_modes() {
        let r = Path::new("R");
        let agy = || vec!["agy".to_string()];
        assert_eq!(argv(agy(), "acceptEdits", None, None, r), ["agy", "--add-dir", "R", "--mode", "accept-edits"]);
        assert_eq!(argv(agy(), "bypassPermissions", Some("c1"), Some("m"), r), ["agy", "--add-dir", "R", "--dangerously-skip-permissions", "--conversation", "c1", "--model", "m"]);
    }

    #[test]
    fn finds_conversation_by_worktree() {
        let map = json!({ "C:\\Users\\u\\.maestro\\projects\\demo\\worktrees\\agent-2": "be10", "C:\\Users\\u\\AppData\\Local\\Temp": "d136" });
        assert_eq!(lookup(&map, "C:/Users/u/.maestro/projects/demo/worktrees/agent-2").as_deref(), Some("be10"));
        assert_eq!(lookup(&map, "c:/users/u/.maestro/projects/demo/worktrees/AGENT-2").as_deref(), Some("be10"));
        assert_eq!(lookup(&map, "C:/Users/u/.maestro/projects/demo/worktrees/agent-3"), None);
    }
}
