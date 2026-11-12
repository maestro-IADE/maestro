//! The bounded runner every git and gh call of PR mode (R6) goes through: no shell, a timeout that kills the whole
//! process tree, stdin null (or a given buffer), capped output, and an environment that can't prompt, page or
//! colour. A credential, passphrase or host-key prompt fails instead of hanging the app.

use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub const OUT_CAP: usize = 4 << 20;
pub const ERR_CAP: usize = 256 << 10;

/// A command to run. Build it with `Cmd::git` / `Cmd::new`, then adjust the fields.
#[derive(Clone, Debug)]
pub struct Cmd {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
    pub env_extra: Vec<(String, OsString)>,
    pub timeout: Duration,
    pub out_cap: usize,
    pub err_cap: usize,
    /// written to stdin, which is then closed; None: stdin is null
    pub input: Option<Vec<u8>>,
    /// PATH for the child (tests put a fake gh first); None: the app's own
    pub path_var: Option<OsString>,
    /// a git call that talks to a remote: ssh gets BatchMode unless the user set an ssh command
    pub network: bool,
    /// stdout past `out_cap` keeps its last bytes, not its first (a log's failure is at its end)
    pub tail: bool,
}

/// What came back. `code` None: killed (timeout) or never started (`spawn_err`).
#[derive(Clone, Debug, Default)]
pub struct Out {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub truncated: bool,
    pub spawn_err: Option<std::io::ErrorKind>,
}

impl Out {
    pub fn ok(&self) -> bool {
        self.code == Some(0) && !self.timed_out
    }

    /// stdout without the trailing newline
    pub fn text(&self) -> &str {
        self.stdout.trim_end()
    }

    /// stderr (or stdout when stderr is empty), trimmed and cut to its last 600 bytes, for a message
    pub fn tail(&self) -> String {
        let s = if self.stderr.trim().is_empty() { self.stdout.trim() } else { self.stderr.trim() };
        crate::util::tail_bytes(s, 600).trim().to_string()
    }
}

/// Environment every call gets: no prompt, no pager, no editor, no colour, no update notices, and git's messages in
/// English (Git for Windows translates them from the Windows language otherwise; the app matches some of them).
const ENV_SET: &[(&str, &str)] = &[
    ("GIT_TERMINAL_PROMPT", "0"),
    // empty, not removed: git then skips `core.askPass` and SSH_ASKPASS too (removed, a GUI askpass would run)
    ("GIT_ASKPASS", ""),
    ("LANGUAGE", "C"),
    ("LC_MESSAGES", "C"),
    ("GCM_INTERACTIVE", "never"),
    ("GIT_PAGER", "cat"),
    ("GIT_EDITOR", "true"),
    ("GIT_MERGE_AUTOEDIT", "no"),
    ("GH_PROMPT_DISABLED", "1"),
    ("GH_PAGER", ""),
    ("PAGER", ""),
    ("GH_NO_UPDATE_NOTIFIER", "1"),
    ("GH_NO_EXTENSION_UPDATE_NOTIFIER", "1"),
    ("GH_SPINNER_DISABLED", "1"),
    ("GH_TELEMETRY", "0"),
    ("NO_COLOR", "1"),
    ("CLICOLOR", "0"),
];

/// Environment no call inherits: debug output, a forced TTY, another repo or host, ssh's askpass program.
const ENV_REMOVE: &[&str] = &["GH_DEBUG", "DEBUG", "GH_FORCE_TTY", "GH_REPO", "GH_HOST", "GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "SSH_ASKPASS"];

impl Cmd {
    pub fn new(program: impl AsRef<OsStr>, args: &[&str]) -> Cmd {
        Cmd {
            program: program.as_ref().to_os_string(),
            args: args.iter().map(OsString::from).collect(),
            cwd: None,
            env_extra: vec![],
            timeout: Duration::from_secs(30),
            out_cap: OUT_CAP,
            err_cap: ERR_CAP,
            input: None,
            path_var: None,
            network: false,
            tail: false,
        }
    }

    /// `git --no-pager -c color.ui=false -C <dir> <args>`
    pub fn git(dir: &Path, args: &[&str]) -> Cmd {
        let mut c = Cmd::new("git", &["--no-pager", "-c", "color.ui=false", "-C"]);
        c.args.push(dir.as_os_str().to_os_string());
        c.args.extend(args.iter().map(OsString::from));
        c.cwd = Some(dir.to_path_buf());
        c
    }

    pub fn timeout(mut self, secs: f64) -> Cmd {
        self.timeout = Duration::from_secs_f64(secs);
        self
    }

    pub fn path(mut self, path_var: Option<&OsStr>) -> Cmd {
        self.path_var = path_var.map(OsStr::to_os_string);
        self
    }

    pub fn input(mut self, bytes: Vec<u8>) -> Cmd {
        self.input = Some(bytes);
        self
    }

    pub fn network(mut self) -> Cmd {
        self.network = true;
        self
    }

    /// stdout capped at `out` bytes, keeping the end: the cut starts at a line, `truncated` says it was cut
    pub fn tail_cap(mut self, out: usize) -> Cmd {
        self.out_cap = out;
        self.tail = true;
        self
    }
}

/// Whether ssh may prompt: no ssh command set by the user (environment or `core.sshCommand`).
fn ssh_unset(cmd: &Cmd) -> bool {
    if std::env::var_os("GIT_SSH_COMMAND").is_some() || std::env::var_os("GIT_SSH").is_some() {
        return false;
    }
    let Some(dir) = &cmd.cwd else { return true };
    // a process start costs up to a second on Windows: the answer is kept a minute per folder
    static SEEN: std::sync::Mutex<Vec<(PathBuf, Instant, bool)>> = std::sync::Mutex::new(Vec::new());
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    seen.retain(|(_, t, _)| t.elapsed() < Duration::from_secs(60));
    if let Some((_, _, unset)) = seen.iter().find(|(d, _, _)| d == dir) {
        return *unset;
    }
    drop(seen);
    let mut probe = Cmd::git(dir, &["config", "--get", "core.sshCommand"]).timeout(10.0);
    probe.path_var = cmd.path_var.clone();
    let out = run_plain(probe);
    let unset = !(out.ok() && !out.text().is_empty());
    SEEN.lock().unwrap_or_else(|e| e.into_inner()).push((dir.clone(), Instant::now(), unset));
    unset
}

/// Runs `cmd` to its end or its timeout (then the whole tree is killed). Never panics; a program that can't start
/// comes back with `spawn_err`.
pub fn run(cmd: Cmd) -> Out {
    let batch = cmd.network && ssh_unset(&cmd);
    let mut cmd = cmd;
    if batch {
        cmd.env_extra.push(("GIT_SSH_COMMAND".into(), "ssh -o BatchMode=yes".into()));
    }
    let trace = std::env::var_os("MAESTRO_EXEC_TRACE").is_some();
    let (t, line) = (Instant::now(), if trace { format!("{:?} {:?}", cmd.program, cmd.args) } else { String::new() });
    let out = run_plain(cmd);
    if trace {
        eprintln!("[exec] {:>6} ms {line}", t.elapsed().as_millis());
    }
    out
}

fn run_plain(cmd: Cmd) -> Out {
    let mut c = Command::new(&cmd.program);
    c.args(&cmd.args);
    if let Some(d) = &cmd.cwd {
        c.current_dir(d);
    }
    for k in ENV_REMOVE {
        c.env_remove(k);
    }
    for (k, v) in ENV_SET {
        c.env(k, v);
    }
    if let Some(p) = &cmd.path_var {
        c.env("PATH", p);
    }
    for (k, v) in &cmd.env_extra {
        c.env(k, v);
    }
    c.stdin(if cmd.input.is_some() { Stdio::piped() } else { Stdio::null() });
    c.stdout(Stdio::piped()).stderr(Stdio::piped());
    crate::cli::quiet(&mut c);
    let mut child = match c.spawn() {
        Ok(ch) => ch,
        Err(e) => return Out { spawn_err: Some(e.kind()), stderr: e.to_string(), ..Default::default() },
    };
    if let (Some(bytes), Some(mut stdin)) = (cmd.input.clone(), child.stdin.take()) {
        std::thread::spawn(move || {
            let _ = stdin.write_all(&bytes);
            // dropped: closed
        });
    }
    let (tx, rx) = mpsc::channel::<(bool, Vec<u8>, bool)>();
    for (is_out, pipe, cap, tail) in [(true, child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>), cmd.out_cap, cmd.tail), (false, child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>), cmd.err_cap, false)] {
        let Some(mut pipe) = pipe else {
            let _ = tx.send((is_out, vec![], false));
            continue;
        };
        let tx = tx.clone();
        std::thread::spawn(move || {
            // bytes past the cap are read and dropped, so the child never blocks on a full pipe
            let mut keep = vec![];
            let mut cut = false;
            let mut buf = [0u8; 16 << 10];
            loop {
                match pipe.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) if tail => {
                        keep.extend_from_slice(&buf[..n]);
                        // dropped from the front in batches: at most twice the cap is held
                        if keep.len() > cap.saturating_mul(2).max(64 << 10) {
                            keep.drain(..keep.len() - cap);
                            cut = true;
                        }
                    }
                    Ok(n) => {
                        let room = cap.saturating_sub(keep.len());
                        if n > room {
                            cut = true;
                        }
                        keep.extend_from_slice(&buf[..n.min(room)]);
                    }
                }
            }
            if tail && keep.len() > cap {
                keep.drain(..keep.len() - cap);
                cut = true;
            }
            // a tail starts at a whole line (and so at a whole character)
            if tail && cut {
                let start = keep.iter().position(|b| *b == b'\n').map_or(keep.len(), |i| i + 1);
                keep.drain(..start);
            }
            let _ = tx.send((is_out, keep, cut));
        });
    }
    drop(tx);
    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if start.elapsed() >= cmd.timeout => {
                timed_out = true;
                crate::proc::kill_tree(child.id());
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(_) => break None,
        }
    };
    let mut out = Out { code: status.and_then(|s| s.code()), timed_out, ..Default::default() };
    // the readers end when the pipes close; a grandchild left holding one gets 2 s
    let until = Instant::now() + Duration::from_secs(2);
    for _ in 0..2 {
        let Some(left) = until.checked_duration_since(Instant::now()) else { break };
        let Ok((is_out, bytes, cut)) = rx.recv_timeout(left) else { break };
        out.truncated |= cut;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        if is_out {
            out.stdout = text;
        } else {
            out.stderr = text;
        }
    }
    out
}

/// Text that says a read failed for a reason a retry may fix (server error, dropped connection, timeout).
pub fn transient(out: &Out) -> bool {
    if out.timed_out {
        return true;
    }
    if out.ok() || out.spawn_err.is_some() {
        return false;
    }
    let t = format!("{}\n{}", out.stderr, out.stdout).to_lowercase();
    ["http 500", "http 502", "http 503", "http 504", "econnreset", "etimedout", "socket hang up", "connection reset", "i/o timeout", "tls handshake timeout", "server error"]
        .iter()
        .any(|k| t.contains(k))
}

/// A read-only call, tried again after 250 ms and 1 s when it failed for a transient reason. Never for a write.
pub fn run_read(cmd: Cmd, pauses: &[Duration]) -> Out {
    let mut out = run(cmd.clone());
    for p in pauses {
        if !transient(&out) {
            break;
        }
        std::thread::sleep(*p);
        out = run(cmd.clone());
    }
    out
}

pub const READ_RETRIES: [Duration; 2] = [Duration::from_millis(250), Duration::from_millis(1000)];

#[cfg(test)]
mod tests {
    use super::*;

    fn shell(script: &str) -> Cmd {
        if cfg!(windows) {
            Cmd::new("cmd", &["/d", "/c", script])
        } else {
            Cmd::new("sh", &["-c", script])
        }
    }

    #[test]
    fn timeout_kills_the_tree() {
        let script = if cfg!(windows) { "ping -n 30 127.0.0.1 >nul" } else { "sleep 30" };
        let t = Instant::now();
        let out = run(shell(script).timeout(1.0));
        assert!(out.timed_out && out.code.is_none(), "{out:?}");
        assert!(t.elapsed() < Duration::from_secs(30), "{:?}", t.elapsed());
    }

    #[test]
    fn output_is_capped_and_drained() {
        let script = if cfg!(windows) { "for /L %i in (1,1,3000) do @echo 0123456789012345678901234567890123456789" } else { "for i in $(seq 1 3000); do echo 0123456789012345678901234567890123456789; done" };
        let mut c = shell(script).timeout(60.0);
        c.out_cap = 1000;
        let out = run(c);
        assert!(out.ok(), "{out:?}");
        assert!(out.truncated && out.stdout.len() == 1000);
    }

    #[test]
    fn a_tail_cap_keeps_the_end() {
        let script = if cfg!(windows) { "for /L %i in (1,1,3000) do @echo line %i" } else { "for i in $(seq 1 3000); do echo line $i; done" };
        let out = run(shell(script).timeout(60.0).tail_cap(1000));
        assert!(out.ok() && out.truncated, "{out:?}");
        assert!(out.stdout.len() <= 1000 && out.stdout.starts_with("line ") && out.stdout.trim_end().ends_with("line 3000"), "{:?}", &out.stdout[..40.min(out.stdout.len())]);
        let small = run(shell(script).timeout(60.0).tail_cap(1 << 20));
        assert!(!small.truncated && small.stdout.starts_with("line 1"));
    }

    #[test]
    fn stdin_is_null_or_given() {
        // a program that reads stdin ends at once: nothing is there to wait for
        let script = if cfg!(windows) { "more" } else { "cat" };
        let out = run(shell(script).timeout(10.0));
        assert!(!out.timed_out && out.stdout.trim().is_empty(), "{out:?}");
        let out = run(shell(script).timeout(10.0).input(b"hello\n".to_vec()));
        assert_eq!(out.stdout.trim(), "hello");
    }

    #[test]
    fn env_is_non_interactive() {
        let script = if cfg!(windows) { "echo %GIT_TERMINAL_PROMPT%,%GH_PROMPT_DISABLED%,%NO_COLOR%,%GCM_INTERACTIVE%" } else { "echo $GIT_TERMINAL_PROMPT,$GH_PROMPT_DISABLED,$NO_COLOR,$GCM_INTERACTIVE" };
        let out = run(shell(script).timeout(10.0));
        assert_eq!(out.stdout.trim(), "0,1,1,never");
        // an empty GIT_ASKPASS reaches git: a `core.askPass` program isn't run, the credential read fails at once
        // (and git's messages come in English)
        let mut fill = Cmd::new("git", &["-c", "core.askPass=echo", "-c", "credential.helper=", "credential", "fill"]).timeout(30.0).input(b"protocol=https\nhost=example.invalid\n\n".to_vec());
        fill.cwd = Some(std::env::temp_dir());
        // the control: that askpass does answer when git is let use it
        let mut asked = fill.clone();
        asked.env_extra.push(("GIT_ASKPASS".into(), "echo".into()));
        let out = run(asked);
        assert!(out.stdout.contains("username="), "{out:?}");
        let out = run(fill);
        assert!(!out.ok() && !out.stdout.contains("username=") && out.stderr.contains("terminal prompts disabled"), "{out:?}");
        let missing = run(Cmd::new("maestro-no-such-program-xyz", &[]));
        assert!(missing.spawn_err.is_some() && missing.code.is_none());
    }

    #[test]
    fn transient_reads_only() {
        let o = |code: i32, err: &str| Out { code: Some(code), stderr: err.into(), ..Default::default() };
        assert!(transient(&o(1, "HTTP 502: Bad Gateway")));
        assert!(transient(&Out { timed_out: true, ..Default::default() }));
        assert!(!transient(&o(1, "HTTP 422: Validation Failed")));
        assert!(!transient(&o(0, "")));
    }
}
