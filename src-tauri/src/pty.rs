//! The PTY runtime: a CLI's real TUI in a pseudo-terminal.
//! - CLI view: that terminal, as is ("pty://data", `Terminal`).
//! - Chat view: the CLI's transcript on disk read as common events (`LiveTail`); prompts are pasted into the TUI.
//! So both views share one process and one conversation. Setup terminals (settings → install / login) are the
//! same runtime without a transcript.

use crate::agent::host_session_env;
use crate::catalog;
use crate::runtime::{LaunchFailure, Prompt, PtySnapshot, Readiness, Runtime, Terminal, ToolMatch};
use crate::stream;
use crate::transcript::LiveTail;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use serde_json::Value;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

const BATCH_WINDOW: Duration = Duration::from_millis(16);
const BATCH_MAX: usize = 64 * 1024;
const SCROLLBACK_MAX: usize = 1024 * 1024;
const TAIL_EVERY: Duration = Duration::from_millis(250);
const STARTUP_LIMIT: Duration = Duration::from_secs(45);

/// Event "pty://data". `seq` = total bytes so far, so a freshly mounted terminal can drop live batches its
/// scrollback snapshot already holds. `epoch` = which PTY under this key: a new process starts over.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PtyData {
    worktree_path: String,
    data_b64: String,
    seq: u64,
    epoch: u64,
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

pub struct PtyRuntime {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    scrollback: Arc<Mutex<Scrollback>>,
    exited: Arc<AtomicBool>,
    /// this PTY's epoch under its key (`stream::bump_pty`)
    epoch: u64,
    /// the catalog's startup texts that mean "log in first"; none for setup terminals
    login_hints: &'static [&'static str],
    /// the transcript tail behind the chat view; None for setup terminals
    live: Option<Arc<Mutex<LiveTail>>>,
}

impl PtyRuntime {
    /// A CLI in a PTY: output batched to `pty://data` under `key`, cursor queries answered, exit noticed (then
    /// `on_exit`). With `live`, its transcript is tailed into the chat stream until it exits.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        app: AppHandle,
        key: &str,
        argv: &[String],
        cwd: &Path,
        env: &[(String, String)],
        live: Option<LiveTail>,
        login_hints: &'static [&'static str],
        resumed: bool,
        on_exit: Box<dyn FnOnce() + Send>,
    ) -> Result<PtyRuntime, String> {
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

        let scrollback = Arc::new(Mutex::new(Scrollback { resumed, ..Default::default() }));
        let child = Arc::new(Mutex::new(child));
        let exited = Arc::new(AtomicBool::new(false));
        let path = key.to_string();
        // a terminal already showing this key (the previous process) starts over
        let epoch = stream::bump_pty(key);
        let _ = app.emit("pty://data", PtyData { worktree_path: path.clone(), data_b64: String::new(), seq: 0, epoch });

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
                let _ = a.emit("pty://data", PtyData { worktree_path: p.clone(), data_b64: B64.encode(&batch), seq, epoch });
            }
        });

        // Transcript tail → chat events, until the process is gone.
        let live = live.map(|l| Arc::new(Mutex::new(l)));
        if let Some(l) = live.clone() {
            let ex = exited.clone();
            thread::spawn(move || {
                while !ex.load(Ordering::SeqCst) {
                    thread::sleep(TAIL_EVERY);
                    l.lock().unwrap().tick();
                }
            });
        }

        // ConPTY may keep the read pipe open after the child exits, so poll the child instead of waiting for EOF.
        let (ch, ex, l) = (child.clone(), exited.clone(), live.clone());
        thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(500));
            let status = ch.lock().unwrap().try_wait();
            match status {
                Ok(None) => continue,
                Ok(Some(_)) => {
                    ex.store(true, Ordering::SeqCst);
                    on_exit();
                    if let Some(l) = l {
                        l.lock().unwrap().exited();
                    }
                    break;
                }
                Err(_) => break,
            }
        });

        Ok(PtyRuntime { writer, master: Mutex::new(pair.master), child, scrollback, exited, epoch, login_hints, live })
    }
}

impl Runtime for PtyRuntime {
    fn kind(&self) -> catalog::Runtime {
        catalog::Runtime::Pty
    }

    /// Blocks until the TUI's input is up and its first draw has settled, so pasted input lands in its prompt.
    fn ready(&self) -> Result<Readiness, LaunchFailure> {
        let start = Instant::now();
        while !self.exited.load(Ordering::SeqCst) {
            if let Some(r) = self.scrollback.lock().unwrap().startup(start.elapsed() > STARTUP_LIMIT, self.login_hints) {
                return r.map(|trust| if trust { Readiness::NeedsTrust } else { Readiness::Ready });
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
    fn accept_trust(&self) -> Result<(), String> {
        {
            let mut s = self.scrollback.lock().unwrap();
            s.ready = None;
            s.mark = s.total;
            s.last = None; // wait for the redraw the Enter causes
        }
        self.write(b"\r")
    }

    /// Bracketed paste (keeps newlines as text), then Enter. Attachments become @mentions.
    /// A leading `!` is typed as a key first, since only a typed `!` switches the TUI into bash mode.
    fn prompt(&self, p: &Prompt) -> Result<(), String> {
        let mut body = p.text.clone();
        // A bare `/cmd` leaves the TUI's autocomplete open, and Enter then completes it again (`/model/model`).
        // A trailing space puts the TUI in argument mode, so Enter runs the command.
        if p.text.starts_with('/') && !p.text.contains(char::is_whitespace) {
            body.push(' ');
        }
        if let Some(rest) = p.text.strip_prefix('!') {
            self.write(b"!")?;
            thread::sleep(Duration::from_millis(80));
            body = rest.to_string();
        }
        for f in &p.files {
            body.push_str(&format!(" @{f}"));
        }
        self.write(format!("\x1b[200~{body}\x1b[201~").as_bytes())?;
        thread::sleep(Duration::from_millis(60)); // let the TUI finish taking the paste before Enter
        self.write(b"\r")
    }

    /// Esc: interrupts the current turn in the TUI; the process stays up.
    fn cancel(&self) -> Result<(), String> {
        self.write(b"\x1b")
    }

    /// Kills the whole process tree (cmd.exe → node → tools on Windows).
    fn kill(&self) {
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

    fn alive(&self) -> bool {
        !self.exited.load(Ordering::SeqCst)
    }

    /// Time since the TUI last drew anything (zero while waiting for a redraw).
    fn quiet_for(&self) -> Duration {
        self.scrollback.lock().unwrap().last.map_or(Duration::ZERO, |t| t.elapsed())
    }

    fn pid(&self) -> Option<u32> {
        self.child.lock().unwrap().process_id()
    }

    fn terminal(&self) -> Option<&dyn Terminal> {
        Some(self)
    }

    fn sync(&self) {
        if let Some(l) = &self.live {
            l.lock().unwrap().tick();
        }
    }

    fn locate_tool(&self, name: &str, input: &Value, tool_call_id: Option<&str>) -> ToolMatch {
        match &self.live {
            Some(l) => {
                let mut l = l.lock().unwrap();
                l.tick();
                l.locate(name, input, tool_call_id)
            }
            None => ToolMatch { tool_call_id: tool_call_id.map(String::from), ..Default::default() },
        }
    }

    fn follow(&self, conversation: &str, transcript: Option<&Path>) {
        if let Some(l) = &self.live {
            l.lock().unwrap().follow(conversation, transcript);
        }
    }
}

impl Terminal for PtyRuntime {
    fn write(&self, data: &[u8]) -> Result<(), String> {
        let mut w = self.writer.lock().unwrap();
        w.write_all(data).and_then(|_| w.flush()).map_err(|e| format!("에이전트에 전송 실패: {e}"))
    }

    fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        self.master.lock().unwrap()
            .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| e.to_string())
    }

    fn snapshot(&self) -> PtySnapshot {
        let s = self.scrollback.lock().unwrap();
        PtySnapshot { data_b64: B64.encode(&s.buf), seq: s.total, epoch: self.epoch }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
