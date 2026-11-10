//! A small JSON-RPC 2.0 client over a child's stdio, for ACP adapters (protocol v1): one JSON object per line,
//! std threads and channels like the rest of the backend (docs/architecture/acp-runtime-research.md: written here
//! instead of the official crate).
//! - requests wait on their own channel, matched by id; a reply to an unknown id is dropped
//! - notifications and the agent's own requests go to `on_incoming`, inline on the reader thread and in order
//!   (`session/load`'s replay before its response). The handler must never wait on a response itself.
//! - the reader's EOF (or the child's exit) is the exit: every waiting request fails with `Exited` and the last
//!   stderr line, the rest of the tree is killed (an orphaned claude would go on writing the conversation), then
//!   `on_exit` runs once

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// What the agent sent that isn't a response.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Notification { method: String, params: Value },
    Request { id: Value, method: String, params: Value },
}

#[derive(Debug, Clone, PartialEq)]
pub enum RpcErrorKind {
    /// the agent answered with an error
    Remote { code: i64, data: Option<Value> },
    Timeout,
    /// the process is gone
    Exited { code: Option<i32> },
    /// writing to it failed
    Io,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RpcError {
    pub kind: RpcErrorKind,
    pub message: String,
}

impl RpcError {
    pub fn code(&self) -> Option<i64> {
        match self.kind {
            RpcErrorKind::Remote { code, .. } => Some(code),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ExitInfo {
    pub code: Option<i32>,
    pub stderr: String,
}

/// One line, classified (`classify`).
#[derive(Debug, PartialEq)]
pub enum Msg {
    Response { id: u64, result: Result<Value, (i64, String, Option<Value>)> },
    Notification { method: String, params: Value },
    Request { id: Value, method: String, params: Value },
    /// not JSON-RPC, or a response to an id we never use (not a number)
    Junk,
}

/// `method` and `id` → the agent's request; `method` only → a notification; `id` only → a response.
pub fn classify(v: &Value) -> Msg {
    let method = v["method"].as_str();
    let id = v.get("id").filter(|i| !i.is_null());
    match (method, id) {
        (Some(m), Some(id)) => Msg::Request { id: id.clone(), method: m.into(), params: v["params"].clone() },
        (Some(m), None) => Msg::Notification { method: m.into(), params: v["params"].clone() },
        (None, Some(id)) => {
            let Some(id) = id.as_u64() else { return Msg::Junk };
            let result = match v.get("error") {
                Some(e) => Err((e["code"].as_i64().unwrap_or(0), e["message"].as_str().unwrap_or("").to_string(), e.get("data").cloned())),
                None => Ok(v.get("result").cloned().unwrap_or(Value::Null)),
            };
            Msg::Response { id, result }
        }
        (None, None) => Msg::Junk,
    }
}

const STDERR_MAX: usize = 4000;

type Reply = Sender<Result<Value, RpcError>>;
type OnIncoming = Box<dyn Fn(&Arc<Client>, Incoming) + Send + Sync>;
type OnExit = Box<dyn FnOnce(ExitInfo) + Send>;

#[derive(Default)]
struct Pending {
    waiting: HashMap<u64, Reply>,
    /// set once the process is gone: nothing waits any more
    exited: Option<ExitInfo>,
}

pub struct Client {
    stdin: Mutex<Option<Box<dyn Write + Send>>>,
    next_id: AtomicU64,
    pending: Mutex<Pending>,
    exit_cv: Condvar,
    last_io: Mutex<Instant>,
    stderr_tail: Arc<Mutex<String>>,
    pid: Option<u32>,
    on_exit: Mutex<Option<OnExit>>,
    finished: AtomicBool,
    /// the child and all it starts (`proc::Tree`): None for a fake, or when it couldn't be set up
    tree: Mutex<Option<crate::proc::Tree>>,
    /// `agent::now_ms` right before the spawn (0: a fake)
    started: AtomicU64,
}

fn push_tail(tail: &Mutex<String>, text: &str) {
    let mut t = tail.lock().unwrap();
    t.push_str(text);
    if t.len() > STDERR_MAX {
        let mut cut = t.len() - STDERR_MAX;
        while !t.is_char_boundary(cut) {
            cut += 1;
        }
        t.drain(..cut);
    }
}

impl Client {
    /// Starts `cmd` with piped stdio (no console window) and reads it.
    pub fn spawn(mut cmd: Command, on_incoming: impl Fn(&Arc<Client>, Incoming) + Send + Sync + 'static, on_exit: impl FnOnce(ExitInfo) + Send + 'static) -> std::io::Result<Arc<Client>> {
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let started = crate::agent::now_ms();
        let mut child = crate::cli::quiet(crate::proc::contain(&mut cmd)).spawn()?;
        // at once: node takes far longer to start claude than this
        let tree = crate::proc::Tree::adopt(&child);
        let (stdin, stdout, stderr) = (child.stdin.take().unwrap(), child.stdout.take().unwrap(), child.stderr.take().unwrap());
        let client = Client::start(BufReader::new(stdout), stdin, Some(child.id()), Box::new(on_incoming), Box::new(on_exit));
        *client.tree.lock().unwrap() = tree;
        client.started.store(started, Ordering::SeqCst);
        // stderr: kept as a tail for error messages
        let tail = client.stderr_tail.clone();
        thread::spawn(move || {
            let mut r = BufReader::new(stderr);
            let mut buf = vec![];
            while r.read_until(b'\n', &mut buf).is_ok_and(|n| n > 0) {
                push_tail(&tail, &String::from_utf8_lossy(&buf));
                buf.clear();
            }
        });
        Client::watch(&client, child);
        Ok(client)
    }

    /// Over any reader and writer (tests: a fake agent).
    #[cfg(test)]
    pub(crate) fn over(r: impl BufRead + Send + 'static, w: impl Write + Send + 'static, on_incoming: impl Fn(&Arc<Client>, Incoming) + Send + Sync + 'static, on_exit: impl FnOnce(ExitInfo) + Send + 'static) -> Arc<Client> {
        Client::start(r, w, None, Box::new(on_incoming), Box::new(on_exit))
    }

    fn start(r: impl BufRead + Send + 'static, w: impl Write + Send + 'static, pid: Option<u32>, on_incoming: OnIncoming, on_exit: OnExit) -> Arc<Client> {
        let client = Arc::new(Client {
            stdin: Mutex::new(Some(Box::new(w))),
            next_id: AtomicU64::new(1),
            pending: Mutex::new(Pending::default()),
            exit_cv: Condvar::new(),
            last_io: Mutex::new(Instant::now()),
            stderr_tail: Arc::default(),
            pid,
            on_exit: Mutex::new(Some(on_exit)),
            finished: AtomicBool::new(false),
            tree: Mutex::new(None),
            started: AtomicU64::new(0),
        });
        let c = client.clone();
        thread::spawn(move || {
            let mut r = r;
            let mut line = vec![];
            loop {
                line.clear();
                match r.read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                *c.last_io.lock().unwrap() = Instant::now();
                if line.iter().all(u8::is_ascii_whitespace) {
                    continue;
                }
                let Ok(v) = serde_json::from_slice::<Value>(&line) else {
                    let head: String = String::from_utf8_lossy(&line[..line.len().min(200)]).into();
                    push_tail(&c.stderr_tail, &format!("[acp] bad line: {}\n", head.trim_end()));
                    continue;
                };
                match classify(&v) {
                    Msg::Response { id, result } => {
                        let reply = c.pending.lock().unwrap().waiting.remove(&id);
                        if let Some(reply) = reply {
                            let _ = reply.send(result.map_err(|(code, message, data)| RpcError { kind: RpcErrorKind::Remote { code, data }, message }));
                        }
                    }
                    Msg::Notification { method, params } => on_incoming(&c, Incoming::Notification { method, params }),
                    Msg::Request { id, method, params } => on_incoming(&c, Incoming::Request { id, method, params }),
                    Msg::Junk => {}
                }
            }
            c.finish(None);
        });
        client
    }

    /// The child's exit: a process can be gone while a grandchild still holds its stdout open, so the reader's EOF
    /// alone isn't enough. A short grace lets the reader drain what came before.
    fn watch(client: &Arc<Client>, mut child: Child) {
        let c = Arc::downgrade(client);
        thread::spawn(move || {
            let code = child.wait().ok().and_then(|s| s.code());
            thread::sleep(Duration::from_millis(1000));
            if let Some(c) = c.upgrade() {
                c.finish(code);
            }
        });
    }

    /// Once: every waiting request fails, `on_exit` runs.
    fn finish(&self, code: Option<i32>) {
        if self.finished.swap(true, Ordering::SeqCst) {
            return;
        }
        let info = ExitInfo { code, stderr: self.stderr_tail() };
        let last = info.stderr.lines().map(str::trim).filter(|l| !l.is_empty()).last().unwrap_or("").to_string();
        let waiting = {
            let mut p = self.pending.lock().unwrap();
            p.exited = Some(info.clone());
            std::mem::take(&mut p.waiting)
        };
        self.exit_cv.notify_all();
        let message = if last.is_empty() { "에이전트 프로세스가 끝났어요".to_string() } else { last };
        for (_, reply) in waiting {
            let _ = reply.send(Err(RpcError { kind: RpcErrorKind::Exited { code }, message: message.clone() }));
        }
        // nothing will be read any more; writes fail from now on
        self.stdin.lock().unwrap().take();
        // whatever the root left behind goes too, before anyone starts another writer on the same conversation
        self.kill_rest();
        if let Some(f) = self.on_exit.lock().unwrap().take() {
            f(info);
        }
    }

    fn write(&self, v: &Value) -> Result<(), RpcError> {
        let mut line = serde_json::to_vec(v).unwrap_or_default();
        line.push(b'\n');
        let mut w = self.stdin.lock().unwrap();
        let io = |m: String| RpcError { kind: RpcErrorKind::Io, message: m };
        let w = w.as_mut().ok_or_else(|| io("에이전트 프로세스가 끝났어요".into()))?;
        w.write_all(&line).and_then(|_| w.flush()).map_err(|e| io(format!("에이전트에 전송 실패: {e}")))?;
        *self.last_io.lock().unwrap() = Instant::now();
        Ok(())
    }

    /// A request whose response is waited for on the returned channel (session/prompt: as long as the turn takes).
    pub fn start_request(&self, method: &str, params: Value) -> Result<(u64, Receiver<Result<Value, RpcError>>), RpcError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        {
            let mut p = self.pending.lock().unwrap();
            if let Some(info) = &p.exited {
                return Err(RpcError { kind: RpcErrorKind::Exited { code: info.code }, message: "에이전트 프로세스가 끝났어요".into() });
            }
            p.waiting.insert(id, tx);
        }
        if let Err(e) = self.write(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })) {
            self.pending.lock().unwrap().waiting.remove(&id);
            return Err(e);
        }
        Ok((id, rx))
    }

    /// A request, waited for up to `timeout` (None: until it's answered or the process ends).
    pub fn request(&self, method: &str, params: Value, timeout: Option<Duration>) -> Result<Value, RpcError> {
        let (id, rx) = self.start_request(method, params)?;
        let got = match timeout {
            Some(t) => rx.recv_timeout(t),
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match got {
            Ok(r) => r,
            Err(_) => {
                self.pending.lock().unwrap().waiting.remove(&id);
                Err(RpcError { kind: RpcErrorKind::Timeout, message: format!("{method} 응답이 없어요") })
            }
        }
    }

    pub fn notify(&self, method: &str, params: Value) -> Result<(), RpcError> {
        self.write(&json!({ "jsonrpc": "2.0", "method": method, "params": params }))
    }

    /// Answers one of the agent's requests.
    pub fn respond(&self, id: &Value, result: Value) {
        let _ = self.write(&json!({ "jsonrpc": "2.0", "id": id, "result": result }));
    }

    pub fn respond_error(&self, id: &Value, code: i64, message: &str) {
        let _ = self.write(&json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }));
    }

    /// The tree is killed while its root is still there to find it by (node exiting by itself on stdin's EOF would
    /// leave a shell still running under claude behind), stdin closed, then up to `grace` for the exit to be seen.
    /// A clean end of the session is the caller's (`session/close` first).
    pub fn close(&self, grace: Duration) {
        self.kill();
        self.stdin.lock().unwrap().take();
        self.wait_exit(grace);
    }

    /// The whole tree (node → claude → its shells and MCP servers): while the root is still there, by walking down
    /// from it (first: once node and claude are gone, their orphans can't be found by parent id any more), then
    /// through its job (`proc::Tree`).
    pub fn kill(&self) {
        if self.alive() {
            if let Some(pid) = self.pid {
                crate::proc::kill_tree(pid);
            }
        }
        if let Some(t) = &*self.tree.lock().unwrap() {
            t.kill();
        }
    }

    /// After the root's exit: the tree's job, or (none could be set up) the dead root's children found by their
    /// parent id (the root's own pid may already be someone else's). Only those started since the root: an older
    /// process whose parent once had the same pid isn't ours. Called once, from `finish`.
    fn kill_rest(&self) {
        if let Some(t) = &*self.tree.lock().unwrap() {
            return t.kill();
        }
        let Some(pid) = self.pid else { return };
        let since = self.started();
        for p in crate::agent::processes().unwrap_or_default().into_iter().filter(|p| p.ppid == pid && crate::agent::born_since(p, since)) {
            crate::proc::kill_tree(p.pid);
        }
    }

    /// No process of its tree is left (its job's count); None without a job.
    pub fn tree_gone(&self) -> Option<bool> {
        self.tree.lock().unwrap().as_ref().and_then(|t| t.empty())
    }

    /// `Runtime::started`
    pub fn started(&self) -> u64 {
        self.started.load(Ordering::SeqCst)
    }

    pub fn alive(&self) -> bool {
        self.pending.lock().unwrap().exited.is_none()
    }

    /// Waits up to `t` for the process to be gone.
    pub fn wait_exit(&self, t: Duration) -> bool {
        let p = self.pending.lock().unwrap();
        let (p, _) = self.exit_cv.wait_timeout_while(p, t, |p| p.exited.is_none()).unwrap();
        p.exited.is_some()
    }

    /// Since the last line either way.
    pub fn quiet_for(&self) -> Duration {
        self.last_io.lock().unwrap().elapsed()
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    pub fn stderr_tail(&self) -> String {
        self.stderr_tail.lock().unwrap().clone()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::{Cursor, Read};

    /// The agent side of a fake connection: what the client wrote, and lines to feed it.
    pub struct Fake {
        pub to_client: Sender<Vec<u8>>,
        pub written: Arc<Mutex<Vec<u8>>>,
    }

    impl Fake {
        pub fn send(&self, v: Value) {
            let _ = self.to_client.send(format!("{v}\n").into_bytes());
        }
        pub fn raw(&self, s: &str) {
            let _ = self.to_client.send(s.as_bytes().to_vec());
        }
        /// Lines the client wrote, parsed.
        pub fn frames(&self) -> Vec<Value> {
            let w = self.written.lock().unwrap();
            String::from_utf8_lossy(&w).lines().map(|l| serde_json::from_str(l).unwrap()).collect()
        }
        /// Waits until the client wrote `n` frames.
        pub fn wait_frames(&self, n: usize) -> Vec<Value> {
            let start = Instant::now();
            while self.frames().len() < n && start.elapsed() < Duration::from_secs(5) {
                thread::sleep(Duration::from_millis(5));
            }
            self.frames()
        }
    }

    /// A reader fed through a channel; dropping the sender is EOF.
    pub struct ChanReader {
        rx: Receiver<Vec<u8>>,
        buf: Cursor<Vec<u8>>,
    }

    impl Read for ChanReader {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            loop {
                let n = self.buf.read(out)?;
                if n > 0 {
                    return Ok(n);
                }
                match self.rx.recv() {
                    Ok(b) => self.buf = Cursor::new(b),
                    Err(_) => return Ok(0),
                }
            }
        }
    }

    pub struct SharedWriter(pub Arc<Mutex<Vec<u8>>>);
    impl Write for SharedWriter {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    pub fn pair(on_incoming: impl Fn(&Arc<Client>, Incoming) + Send + Sync + 'static, on_exit: impl FnOnce(ExitInfo) + Send + 'static) -> (Arc<Client>, Fake) {
        let (tx, rx) = mpsc::channel();
        let written = Arc::new(Mutex::new(vec![]));
        let c = Client::over(BufReader::new(ChanReader { rx, buf: Cursor::new(vec![]) }), SharedWriter(written.clone()), on_incoming, on_exit);
        (c, Fake { to_client: tx, written })
    }

    #[test]
    fn classifies_lines() {
        assert_eq!(classify(&json!({"jsonrpc": "2.0", "id": 3, "result": {"a": 1}})), Msg::Response { id: 3, result: Ok(json!({"a": 1})) });
        assert_eq!(classify(&json!({"id": 4, "result": null})), Msg::Response { id: 4, result: Ok(Value::Null) });
        assert_eq!(classify(&json!({"id": 5, "error": {"code": -32000, "message": "auth", "data": {"x": 1}}})), Msg::Response { id: 5, result: Err((-32000, "auth".into(), Some(json!({"x": 1})))) });
        assert_eq!(classify(&json!({"method": "session/update", "params": {"a": 1}})), Msg::Notification { method: "session/update".into(), params: json!({"a": 1}) });
        assert_eq!(classify(&json!({"id": "r1", "method": "session/request_permission", "params": {}})), Msg::Request { id: json!("r1"), method: "session/request_permission".into(), params: json!({}) });
        assert_eq!(classify(&json!({"id": "x", "result": 1})), Msg::Junk);
        assert_eq!(classify(&json!({"hello": 1})), Msg::Junk);
        // a null id is no id
        assert!(matches!(classify(&json!({"id": null, "method": "m"})), Msg::Notification { .. }));
    }

    #[test]
    fn responses_find_their_request_in_any_order() {
        let (c, fake) = pair(|_, _| {}, |_| {});
        let (c1, c2) = (c.clone(), c.clone());
        let a = thread::spawn(move || c1.request("a", json!({}), Some(Duration::from_secs(5))));
        let b = thread::spawn(move || c2.request("b", json!({}), Some(Duration::from_secs(5))));
        let frames = fake.wait_frames(2);
        let id = |m: &str| frames.iter().find(|f| f["method"] == m).unwrap()["id"].clone();
        // every frame is one JSON-RPC 2.0 line
        assert!(frames.iter().all(|f| f["jsonrpc"] == "2.0"));
        fake.send(json!({"jsonrpc": "2.0", "id": id("b"), "error": {"code": -32603, "message": "boom"}}));
        fake.send(json!({"jsonrpc": "2.0", "id": 999, "result": "nobody"}));
        fake.send(json!({"jsonrpc": "2.0", "id": id("a"), "result": "A"}));
        assert_eq!(a.join().unwrap(), Ok(json!("A")));
        let e = b.join().unwrap().unwrap_err();
        assert_eq!((e.kind.clone(), e.message.as_str()), (RpcErrorKind::Remote { code: -32603, data: None }, "boom"));
        assert_eq!(e.code(), Some(-32603));
    }

    #[test]
    fn a_request_times_out() {
        let (c, _fake) = pair(|_, _| {}, |_| {});
        let e = c.request("slow", json!({}), Some(Duration::from_millis(50))).unwrap_err();
        assert_eq!(e.kind, RpcErrorKind::Timeout);
        assert!(c.pending.lock().unwrap().waiting.is_empty());
    }

    #[test]
    fn incoming_messages_arrive_in_order_and_requests_get_answers() {
        let seen = Arc::new(Mutex::new(vec![]));
        let s = seen.clone();
        let (c, fake) = pair(
            move |c, inc| {
                s.lock().unwrap().push(inc.clone());
                if let Incoming::Request { id, method, .. } = inc {
                    if method == "ping" {
                        c.respond(&id, json!("pong"));
                    } else {
                        c.respond_error(&id, -32601, "unsupported");
                    }
                }
            },
            |_| {},
        );
        // a load: its replay comes before its response, so it's seen first
        let c2 = c.clone();
        let load = thread::spawn(move || c2.request("session/load", json!({}), Some(Duration::from_secs(5))));
        let id = fake.wait_frames(1)[0]["id"].clone();
        fake.raw("\n   \n");
        fake.raw("not json at all\n");
        for i in 0..3 {
            fake.send(json!({"jsonrpc": "2.0", "method": "session/update", "params": {"n": i}}));
        }
        fake.send(json!({"jsonrpc": "2.0", "id": id, "result": {}}));
        assert!(load.join().unwrap().is_ok());
        let n: Vec<i64> = seen.lock().unwrap().iter().filter_map(|i| match i { Incoming::Notification { params, .. } => params["n"].as_i64(), _ => None }).collect();
        assert_eq!(n, [0, 1, 2]);
        assert!(c.stderr_tail().contains("[acp] bad line: not json at all"));
        fake.send(json!({"jsonrpc": "2.0", "id": "q1", "method": "ping", "params": {}}));
        fake.send(json!({"jsonrpc": "2.0", "id": 7, "method": "fs/read_text_file", "params": {}}));
        let frames = fake.wait_frames(3);
        assert_eq!(frames[1], json!({"jsonrpc": "2.0", "id": "q1", "result": "pong"}));
        assert_eq!(frames[2]["error"]["code"], -32601);
        assert_eq!(frames[2]["id"], 7);
    }

    #[test]
    fn eof_fails_waiting_requests_and_exits_once() {
        let exits = Arc::new(AtomicU64::new(0));
        let e = exits.clone();
        let (c, fake) = pair(|_, _| {}, move |_| {
            e.fetch_add(1, Ordering::SeqCst);
        });
        let c2 = c.clone();
        let waiting = thread::spawn(move || c2.request("session/prompt", json!({}), None));
        fake.wait_frames(1);
        push_tail(&c.stderr_tail, "warming up\nError: the CLI crashed\n");
        drop(fake);
        let err = waiting.join().unwrap().unwrap_err();
        assert_eq!(err.kind, RpcErrorKind::Exited { code: None });
        assert_eq!(err.message, "Error: the CLI crashed");
        assert!(c.wait_exit(Duration::from_secs(1)));
        c.finish(Some(1));
        assert_eq!(exits.load(Ordering::SeqCst), 1);
        assert!(!c.alive());
        // writing after the exit fails, requesting too
        assert_eq!(c.notify("session/cancel", json!({})).unwrap_err().kind, RpcErrorKind::Io);
        assert!(matches!(c.request("x", json!({}), None).unwrap_err().kind, RpcErrorKind::Exited { .. }));
    }

    #[test]
    fn stderr_tail_keeps_the_end() {
        let t = Mutex::new(String::new());
        push_tail(&t, &"가".repeat(3000));
        push_tail(&t, "end");
        let t = t.into_inner().unwrap();
        assert!(t.len() <= STDERR_MAX && t.ends_with("end"));
    }
}
