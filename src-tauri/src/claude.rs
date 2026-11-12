//! Claude Code as a PTY agent: argv, where its transcript lives, and its transcript lines as common events.
//! The transcript is `<config dir>/projects/<cwd slug>/<session id>.jsonl`, one JSON object per line; the config
//! dir is `CLAUDE_CONFIG_DIR`, else `~/.claude`.

use crate::agent::{host_session_env, system_brief, AgentMeta, Files};
use crate::catalog::AgentDef;
use crate::events::{result_text, NoticeKind, RuntimeEvent, TurnOutcome, UserKind};
use crate::runtime::{Launch, LaunchCtx, PtyAgent};
use crate::transcript::{Normalize, TurnTracker};
use serde_json::Value;
use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct ClaudeAgent;
pub static CLAUDE: ClaudeAgent = ClaudeAgent;

/// Claude shortens longer project folder names with a hash, which isn't computed here (`slug_hashed`).
pub const SLUG_MAX: usize = 200;

/// Claude Code's config folder: `env` (`CLAUDE_CONFIG_DIR`) when set and not empty, else `~/.claude`.
pub fn config_dir_from(home: &Path, env: Option<&OsStr>) -> PathBuf {
    env.filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| home.join(".claude"))
}

/// Claude Code's config folder now. Read on every call: the CLI started now reads the same environment.
pub fn config_dir(home: &Path) -> PathBuf {
    config_dir_from(home, std::env::var_os("CLAUDE_CONFIG_DIR").as_deref())
}

/// The project folder name Claude gives `cwd`: every UTF-16 unit that isn't an ASCII letter or digit becomes '-'
/// (JS `replace(/[^a-zA-Z0-9]/g, '-')`: a character outside the BMP, like an emoji, becomes "--").
pub fn project_slug(cwd: &str) -> String {
    cwd.encode_utf16().map(|u| if u < 0x80 && (u as u8).is_ascii_alphanumeric() { u as u8 as char } else { '-' }).collect()
}

/// Claude hashes this folder's name (too long): where its transcripts go can't be computed, only reported (hooks).
pub fn slug_hashed(cwd: &str) -> bool {
    project_slug(cwd).len() > SLUG_MAX
}

/// The transcript of a session started in `cwd`, under the config folder `config`.
pub fn session_file_in(config: &Path, cwd: &str, session_id: &str) -> PathBuf {
    config.join("projects").join(project_slug(cwd)).join(format!("{session_id}.jsonl"))
}

/// Where Claude Code keeps the transcript of a session started in `cwd` (computed; the path Claude reported through
/// its hooks, `AgentMeta::cli_transcript`, goes first: `runtime::transcript_path`). A relative `CLAUDE_CONFIG_DIR`
/// is the CLI's: it runs in the worktree. A hashed slug (`slug_hashed`) isn't handled: that path never exists.
pub fn session_file(home: &Path, cwd: &str, session_id: &str) -> PathBuf {
    let dir = config_dir(home);
    let dir = if dir.is_relative() { Path::new(cwd).join(dir) } else { dir };
    session_file_in(&dir, cwd, session_id)
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

/// One cheap Claude call: no tools, MCP, slash commands or settings hooks, a tiny system prompt,
/// nothing saved to disk (≈700 tokens with Haiku, well under a cent).
fn summarize_with_claude(exe: Vec<String>, conversation: &str, model: &str) -> Result<String, String> {
    use std::process::{Command, Stdio};
    let system = crate::agent::SUMMARY_PROMPT;
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

impl PtyAgent for ClaudeAgent {
    fn id(&self) -> &'static str {
        "claude-code"
    }

    fn launch(&self, ctx: &LaunchCtx, meta: &mut AgentMeta, files: &Files, exe: Vec<String>) -> Result<Launch, String> {
        let session_id = match &meta.cli_session_id {
            Some(id) => id.clone(),
            None => {
                let id = new_session_id();
                meta.cli_session_id = Some(id.clone());
                files.write_meta(meta)?;
                id
            }
        };
        // the path Claude reported last time (hooks) before the computed one
        let session = crate::runtime::transcript_path(self, meta, ctx.home, ctx.worktree, &session_id);
        let settings = files.meta.with_extension("settings.json");
        if ctx.def.features.approval_hook {
            // permission prompts go to the chat view (approval.rs); rewritten each start since the port changes per run
            ctx.approvals.write_settings(&settings, &session_id)?;
            ctx.approvals.register(&session_id, ctx.worktree);
        } else {
            std::fs::write(&settings, "{}").map_err(|e| e.to_string())?;
        }
        // Claude writes the file on the first message; before that the id is still free to claim
        let brief = files.meta.with_extension("brief.md");
        std::fs::write(&brief, system_brief(ctx.home, meta, ctx.worktree)).map_err(|e| e.to_string())?;
        let resumed = session.is_file();
        Ok(Launch { argv: argv(exe, meta, &session_id, resumed, &settings, &brief), transcript: Some(session), resumed })
    }

    fn transcript(&self, home: &Path, cwd: &str, id: &str) -> PathBuf {
        session_file(home, cwd, id)
    }

    fn normalizer(&self) -> Box<dyn Normalize> {
        Box::new(ClaudeNormalizer::default())
    }

    /// The real argv without the per-worktree parts (session, hook settings, brief).
    fn preview(&self, def: &AgentDef, permission_mode: &str, model: Option<&str>) -> Vec<String> {
        let meta = AgentMeta { permission_mode: permission_mode.into(), model: model.map(String::from), ..Default::default() };
        let mut a = argv(vec![def.bin.into()], &meta, "", false, Path::new(""), Path::new(""));
        a.drain(1..3); // --session-id <id>
        a.retain(|x| !x.is_empty() && x != "--settings" && x != "--append-system-prompt-file");
        a
    }

    fn summarize(&self, exe: Vec<String>, conversation: &str, model: &str) -> Result<String, String> {
        summarize_with_claude(exe, conversation, model)
    }
}

fn between<'a>(s: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = s.find(open)? + open.len();
    Some(&s[start..start + s[start..].find(close)?])
}

/// A command's output: stdout and stderr parts, trimmed, the empty ones dropped.
fn output(s: &str, out: &str, err: &str) -> String {
    let parts = [between(s, &format!("<{out}>"), &format!("</{out}>")), between(s, &format!("<{err}>"), &format!("</{err}>"))];
    parts.iter().flatten().map(|t| t.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n")
}

/// Claude Code's own session bookkeeping line types (seen in real transcripts): no conversation, nothing to keep.
/// Any other line type, one a newer Claude adds included, is kept as `unknown` (PRD R3-2).
const BOOKKEEPING: &[&str] = &[
    "attachment",
    "queue-operation",
    "file-history-snapshot",
    "file-history-delta",
    "summary",
    "custom-title",
    "ai-title",
    "agent-name",
    "last-prompt",
    "progress",
    "mode",
    "permission-mode",
    "cost-state",
    "tools-used",
    "session-owner",
    "frame-link",
    "bridge-session",
    "api-request",
    "api-request-blob",
    "api-request-shape",
    "atis-latch",
    "isolation-latch",
    "artifact-autoreact-ledger",
    "artifact-comment-monitor",
];

/// Claude's transcript lines → common events. `user`, `assistant` and `system` lines are conversation; the
/// `BOOKKEEPING` types and meta user lines give nothing; any other line is kept as `unknown`.
/// Ids: turn = the opening line's `promptId` (else its uuid); items = line uuid (`#block` for content blocks);
/// tool calls = `tool_use.id`; a turn the agent opened itself = `a:<uuid>`.
#[derive(Default)]
pub struct ClaudeNormalizer {
    t: TurnTracker,
}

impl Normalize for ClaudeNormalizer {
    fn line(&mut self, e: &Value, line_no: u64, out: &mut Vec<RuntimeEvent>) {
        let uid = e["uuid"].as_str().map(String::from).unwrap_or_else(|| format!("l{line_no}"));
        match e["type"].as_str() {
            Some("assistant") => self.assistant(e, &uid, out),
            Some("user") if e["isMeta"] != true => self.user(e, uid, out),
            // meta user lines: caveats and reminders Claude adds itself
            Some("user") => {}
            Some("system") => self.system(e, uid, out),
            Some(t) if BOOKKEEPING.contains(&t) => {}
            _ => out.push(RuntimeEvent::unknown(self.t.current().map(String::from), "claude", e)),
        }
    }

    fn tracker(&self) -> &TurnTracker {
        &self.t
    }

    fn tracker_mut(&mut self) -> &mut TurnTracker {
        &mut self.t
    }
}

impl ClaudeNormalizer {
    fn prompt(&mut self, e: &Value, uid: String, kind: UserKind, text: String, out: &mut Vec<RuntimeEvent>) {
        let key = e["promptId"].as_str().map(String::from).unwrap_or_else(|| uid.clone());
        let turn_id = self.t.open_user(key, out);
        out.push(RuntimeEvent::UserMessage { turn_id, item_id: uid, kind, text, files: vec![] });
    }

    /// A muted note that ends its turn; one with no turn open gets a turn of its own.
    fn notice(&mut self, uid: String, kind: NoticeKind, text: String, outcome: TurnOutcome, out: &mut Vec<RuntimeEvent>) {
        let turn_id = self.t.ensure(None, || format!("a:{uid}"), out);
        out.push(RuntimeEvent::Notice { turn_id, item_id: uid, kind, text });
        self.t.end(outcome, None, out);
    }

    fn user(&mut self, e: &Value, uid: String, out: &mut Vec<RuntimeEvent>) {
        let c = &e["message"]["content"];
        if let Some(s) = c.as_str() {
            // a slash command typed in the TUI is stored as tagged text
            if let Some(name) = between(s, "<command-name>", "</command-name>") {
                let args = between(s, "<command-args>", "</command-args>").unwrap_or("");
                return self.prompt(e, uid, UserKind::Command, format!("{name} {args}").trim().into(), out);
            }
            // a slash command's output arrives as a user entry, not a system one
            if s.contains("<local-command-stdout>") || s.contains("<local-command-stderr>") {
                let text = output(s, "local-command-stdout", "local-command-stderr");
                if !text.is_empty() {
                    self.notice(uid, NoticeKind::LocalCommand, text, TurnOutcome::Command, out);
                }
                return;
            }
            // `!` bash mode: the command and its output are stored as tagged text too
            if let Some(cmd) = between(s, "<bash-input>", "</bash-input>") {
                return self.prompt(e, uid, UserKind::Shell, format!("!{cmd}"), out);
            }
            if s.contains("<bash-stdout>") || s.contains("<bash-stderr>") {
                let text = output(s, "bash-stdout", "bash-stderr");
                let text = if text.is_empty() { "(출력 없음)".into() } else { text };
                return self.notice(uid, NoticeKind::LocalCommand, text, TurnOutcome::Command, out);
            }
            // "[Request interrupted…" as plain text is what the user typed: still a prompt
            return self.prompt(e, uid, UserKind::Prompt, s.into(), out);
        }
        let blocks: &[Value] = c.as_array().map_or(&[], |a| a.as_slice());
        if blocks.iter().any(|b| b["type"] == "tool_result") {
            for b in blocks {
                if b["type"] != "tool_result" {
                    // a part next to the results (text, image): kept, not shown
                    out.push(RuntimeEvent::unknown(self.t.current().map(String::from), "claude", b));
                    continue;
                }
                let tool_call_id = b["tool_use_id"].as_str().unwrap_or("").to_string();
                let turn_id = self.t.tool_finished(&tool_call_id);
                out.push(RuntimeEvent::ToolFinished { turn_id, tool_call_id, output: result_text(&b["content"]), is_error: b["is_error"] == true });
            }
            return;
        }
        let text = blocks.iter().filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n");
        if text.starts_with("[Request interrupted") {
            return self.notice(uid, NoticeKind::Interrupted, "중단됨".into(), TurnOutcome::Cancelled, out);
        }
        if !text.trim().is_empty() {
            self.prompt(e, uid, UserKind::Prompt, text, out);
        }
        // an image or other part, next to the text or alone (an image-only prompt): kept, not shown
        for b in blocks.iter().filter(|b| b["text"].as_str().is_none()) {
            out.push(RuntimeEvent::unknown(self.t.current().map(String::from), "claude", b));
        }
    }

    fn assistant(&mut self, e: &Value, uid: &str, out: &mut Vec<RuntimeEvent>) {
        let msg = &e["message"];
        let mid = msg["id"].as_str();
        let turn = self.t.ensure(mid, || format!("a:{uid}"), out);
        let blocks: &[Value] = msg["content"].as_array().map_or(&[], |a| a.as_slice());
        let message_id = || Some(uid.to_string());
        for (i, b) in blocks.iter().enumerate() {
            let item_id = format!("{uid}#{i}");
            let turn_id = turn.clone();
            match b["type"].as_str() {
                Some("text") => out.push(RuntimeEvent::AgentText { turn_id, item_id, message_id: message_id(), text: b["text"].as_str().unwrap_or("").into() }),
                Some("thinking") => out.push(RuntimeEvent::Thinking { turn_id, item_id, message_id: message_id(), text: b["thinking"].as_str().unwrap_or("").into() }),
                Some("tool_use") => {
                    let (id, name) = (b["id"].as_str().unwrap_or(""), b["name"].as_str().unwrap_or("?"));
                    self.t.tool_started(&turn, id, name, &b["input"]);
                    out.push(RuntimeEvent::ToolStarted { turn_id, tool_call_id: id.into(), message_id: message_id(), name: name.into(), input: b["input"].clone() });
                }
                _ => out.push(RuntimeEvent::unknown(Some(turn_id), "claude", b)),
            }
        }
        // a final stop reason ends the turn (end_turn, max_tokens, an API error's stop_sequence, …)
        if let Some(stop) = msg["stop_reason"].as_str().filter(|s| *s != "tool_use") {
            if self.t.current().is_some() {
                let outcome = if e["isApiErrorMessage"] == true {
                    let message = blocks.iter().filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n");
                    TurnOutcome::Failed { error: e["error"].as_str().unwrap_or("api").into(), message }
                } else {
                    TurnOutcome::Done { stop_reason: stop.into() }
                };
                self.t.end(outcome, mid.map(String::from), out);
            }
        }
    }

    fn system(&mut self, e: &Value, uid: String, out: &mut Vec<RuntimeEvent>) {
        match e["subtype"].as_str() {
            Some("local_command") => {
                let s = e["content"].as_str().unwrap_or("");
                let text = between(s, "<local-command-stdout>", "</local-command-stdout>").unwrap_or(s).trim();
                if !text.is_empty() {
                    self.notice(uid, NoticeKind::LocalCommand, text.into(), TurnOutcome::Command, out);
                }
            }
            // the API call failed and Claude retries: not the end of the turn
            Some("api_error") => {
                let err = &e["error"];
                let message = e["content"].as_str().or(err["message"].as_str()).map(String::from).unwrap_or_else(|| err.to_string());
                out.push(RuntimeEvent::Error { turn_id: self.t.current().map(String::from), kind: "apiRetry".into(), message });
            }
            // hook bookkeeping and timing
            Some("stop_hook_summary") | Some("turn_duration") => {}
            _ => out.push(RuntimeEvent::unknown(self.t.current().map(String::from), "claude", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Initiator;
    use serde_json::json;

    fn run(lines: &[Value]) -> Vec<RuntimeEvent> {
        let mut n = ClaudeNormalizer::default();
        let mut out = vec![];
        for (i, l) in lines.iter().enumerate() {
            n.line(l, i as u64 + 1, &mut out);
        }
        out
    }

    #[test]
    fn session_file_matches_claude_slug() {
        let cwd = "C:/Users/u/.maestro/projects/demo_1a/worktrees/agent-1";
        let p = session_file_in(&config_dir_from(Path::new("/h"), None), cwd, "abc");
        assert_eq!(p, Path::new("/h/.claude/projects/C--Users-u--maestro-projects-demo-1a-worktrees-agent-1/abc.jsonl"));
        // CLAUDE_CONFIG_DIR moves the whole folder
        let p = session_file_in(&config_dir_from(Path::new("/h"), Some(OsStr::new("D:/cfg"))), cwd, "abc");
        assert_eq!(p, Path::new("D:/cfg/projects/C--Users-u--maestro-projects-demo-1a-worktrees-agent-1/abc.jsonl"));
    }

    #[test]
    fn config_dir_follows_claude_config_dir() {
        let h = Path::new("/h");
        assert_eq!(config_dir_from(h, None), Path::new("/h/.claude"));
        assert_eq!(config_dir_from(h, Some(OsStr::new(""))), Path::new("/h/.claude"));
        assert_eq!(config_dir_from(h, Some(OsStr::new("D:/cfg"))), Path::new("D:/cfg"));
    }

    #[test]
    fn slug_matches_claude_rules() {
        // one '-' per UTF-16 unit: a Korean letter is one, an emoji two
        assert_eq!(project_slug("C:/작업/a_b"), "C-----a-b");
        assert_eq!(project_slug("C:/😀x"), "C----x");
        let at = |n: usize| format!("C:/{}", "a".repeat(n - 3));
        assert!(!slug_hashed(&at(SLUG_MAX)));
        assert!(slug_hashed(&at(SLUG_MAX + 1)));
        assert!(slug_hashed(&format!("C:/{}", "작".repeat(SLUG_MAX - 2))));
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

    /// Real Haiku call (network, ~$0.0002): `cargo test -- --ignored haiku`
    #[test]
    #[ignore]
    fn haiku_summary_live() {
        let exe = crate::cli::command(crate::catalog::get("claude-code").unwrap()).unwrap();
        let s = summarize_with_claude(exe, "요청: html 보일러플레이트 알려줘\n결과: ```html\n<!doctype html>\n<html lang=\"ko\">\n<head>\n  <meta charset=\"utf-8\">\n  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n  <title>제목</title>\n</head>\n<body>\n</body>\n</html>\n```", "haiku").unwrap();
        assert!(!s.is_empty(), "{s}");
        println!("summary: {s}");
    }

    #[test]
    fn maps_lines_with_ids() {
        let evs = run(&[
            json!({"type": "user", "uuid": "u1", "promptId": "p1", "message": {"content": [{"type": "text", "text": "안녕"}]}}),
            json!({"type": "assistant", "uuid": "a1", "message": {"id": "m1", "content": [{"type": "thinking", "thinking": "음"}, {"type": "tool_use", "id": "toolu_1", "name": "Read", "input": {"f": 1}}], "stop_reason": "tool_use"}}),
            json!({"type": "user", "uuid": "u2", "promptId": "p1", "message": {"content": [{"type": "tool_result", "tool_use_id": "toolu_1", "content": [{"type": "text", "text": "x"}], "is_error": true}]}}),
            json!({"type": "assistant", "uuid": "a2", "message": {"id": "m2", "content": [{"type": "text", "text": "끝"}], "stop_reason": "end_turn"}}),
        ]);
        assert_eq!(evs, [
            RuntimeEvent::TurnStarted { turn_id: "p1".into(), initiator: Initiator::User },
            RuntimeEvent::UserMessage { turn_id: "p1".into(), item_id: "u1".into(), kind: UserKind::Prompt, text: "안녕".into(), files: vec![] },
            RuntimeEvent::Thinking { turn_id: "p1".into(), item_id: "a1#0".into(), message_id: Some("a1".into()), text: "음".into() },
            RuntimeEvent::ToolStarted { turn_id: "p1".into(), tool_call_id: "toolu_1".into(), message_id: Some("a1".into()), name: "Read".into(), input: json!({"f": 1}) },
            RuntimeEvent::ToolFinished { turn_id: "p1".into(), tool_call_id: "toolu_1".into(), output: "x".into(), is_error: true },
            RuntimeEvent::AgentText { turn_id: "p1".into(), item_id: "a2#0".into(), message_id: Some("a2".into()), text: "끝".into() },
            RuntimeEvent::TurnEnded { turn_id: "p1".into(), outcome: TurnOutcome::Done { stop_reason: "end_turn".into() } },
        ]);
        // every turn-scoped event carries its turn
        assert!(evs.iter().all(|e| e.turn_id() == Some("p1")));
    }

    #[test]
    fn commands_notices_and_errors() {
        assert!(run(&[json!({"type": "user", "isMeta": true, "message": {"content": "x"}})]).is_empty());
        assert!(run(&[json!({"type": "queue-operation"})]).is_empty());
        let cmd = run(&[json!({"type": "user", "uuid": "c", "message": {"content": "<command-name>/model</command-name>\n<command-args>opus</command-args>"}})]);
        assert!(matches!(&cmd[1], RuntimeEvent::UserMessage { kind: UserKind::Command, text, turn_id, .. } if text == "/model opus" && turn_id == "c"));
        let bash = run(&[json!({"type": "user", "uuid": "b", "message": {"content": "<bash-input>ls</bash-input>"}}), json!({"type": "user", "uuid": "o", "message": {"content": "<bash-stdout>a.txt\n</bash-stdout><bash-stderr></bash-stderr>"}})]);
        assert!(matches!(&bash[1], RuntimeEvent::UserMessage { kind: UserKind::Shell, text, .. } if text == "!ls"));
        assert!(matches!(&bash[2], RuntimeEvent::Notice { kind: NoticeKind::LocalCommand, text, turn_id, .. } if text == "a.txt" && turn_id == "b"));
        assert!(matches!(&bash[3], RuntimeEvent::TurnEnded { outcome: TurnOutcome::Command, .. }));
        // output with no turn open gets a turn of its own
        let out = run(&[json!({"type": "system", "subtype": "local_command", "uuid": "s", "content": "<local-command-stdout>hi</local-command-stdout>"})]);
        assert!(matches!(&out[..], [RuntimeEvent::TurnStarted { initiator: Initiator::Agent, turn_id, .. }, RuntimeEvent::Notice { .. }, RuntimeEvent::TurnEnded { .. }] if turn_id == "a:s"));
        // Esc
        let esc = run(&[json!({"type": "user", "uuid": "u", "message": {"content": "go"}}), json!({"type": "user", "uuid": "i", "message": {"content": [{"type": "text", "text": "[Request interrupted by user]"}]}})]);
        assert!(matches!(&esc[2], RuntimeEvent::Notice { kind: NoticeKind::Interrupted, text, .. } if text == "중단됨"));
        assert!(matches!(&esc[3], RuntimeEvent::TurnEnded { outcome: TurnOutcome::Cancelled, .. }));
        // an API error fails the turn; a retry notice doesn't end it
        let api = run(&[
            json!({"type": "user", "uuid": "u", "message": {"content": "go"}}),
            json!({"type": "system", "subtype": "api_error", "uuid": "r", "error": {"message": "Connection error."}}),
            json!({"type": "assistant", "uuid": "a", "isApiErrorMessage": true, "error": "rate_limit", "message": {"id": "m", "content": [{"type": "text", "text": "API Error: 429"}], "stop_reason": "stop_sequence"}}),
        ]);
        assert!(matches!(&api[2], RuntimeEvent::Error { kind, message, turn_id: Some(t) } if kind == "apiRetry" && message == "Connection error." && t == "u"));
        assert_eq!(api[4], RuntimeEvent::TurnEnded { turn_id: "u".into(), outcome: TurnOutcome::Failed { error: "rate_limit".into(), message: "API Error: 429".into() } });
        // unknown lines are kept
        let unk = run(&[json!({"type": "system", "subtype": "compact_boundary", "uuid": "x"})]);
        assert!(matches!(&unk[0], RuntimeEvent::Unknown { source, .. } if source == "claude"));
    }

    #[test]
    fn unknown_content_is_kept_not_dropped() {
        // bookkeeping lines give nothing; a line type Claude adds later is kept whole, in the open turn
        assert!(run(&[json!({"type": "file-history-snapshot"}), json!({"type": "custom-title"}), json!({"type": "attachment"})]).is_empty());
        let later = run(&[json!({"type": "user", "uuid": "u", "message": {"content": "hi"}}), json!({"type": "brand-new-line", "x": 1})]);
        assert_eq!(later[2], RuntimeEvent::Unknown { turn_id: Some("u".into()), source: "claude".into(), raw: json!({"type": "brand-new-line", "x": 1}) });
        // an image-only prompt: no prompt, the image kept
        let img = json!({"type": "image", "source": {"type": "base64", "data": "AA"}});
        let only = run(&[json!({"type": "user", "uuid": "i", "message": {"content": [img.clone()]}})]);
        assert_eq!(only, [RuntimeEvent::Unknown { turn_id: None, source: "claude".into(), raw: img.clone() }]);
        // a part next to tool results, in its place
        let res = run(&[
            json!({"type": "user", "uuid": "u", "promptId": "p", "message": {"content": "go"}}),
            json!({"type": "assistant", "uuid": "a", "message": {"id": "m", "content": [{"type": "tool_use", "id": "t", "name": "Read", "input": {}}], "stop_reason": "tool_use"}}),
            json!({"type": "user", "uuid": "r", "promptId": "p", "message": {"content": [{"type": "tool_result", "tool_use_id": "t", "content": "x"}, img.clone()]}}),
        ]);
        assert!(matches!(&res[3..], [RuntimeEvent::ToolFinished { .. }, RuntimeEvent::Unknown { turn_id: Some(t), raw, .. }] if t == "p" && *raw == img));
    }

    #[test]
    fn a_straggler_block_stays_in_its_closed_turn() {
        let evs = run(&[
            json!({"type": "user", "uuid": "u", "promptId": "p", "message": {"content": "hi"}}),
            json!({"type": "assistant", "uuid": "a1", "message": {"id": "m", "content": [{"type": "thinking", "thinking": "."}], "stop_reason": "end_turn"}}),
            json!({"type": "assistant", "uuid": "a2", "message": {"id": "m", "content": [{"type": "text", "text": "답"}], "stop_reason": "end_turn"}}),
        ]);
        assert_eq!(evs.iter().filter(|e| matches!(e, RuntimeEvent::TurnEnded { .. })).count(), 1);
        assert_eq!(evs.iter().filter(|e| matches!(e, RuntimeEvent::TurnStarted { .. })).count(), 1);
        assert!(matches!(evs.last(), Some(RuntimeEvent::AgentText { turn_id, .. }) if turn_id == "p"));
    }
}
