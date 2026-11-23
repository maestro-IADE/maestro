//! 워크트리 준비 (worker folder prep). A new worktree holds only what git tracks, so a worker would start without the
//! project's `.env` and without its dependencies installed. Right after `worktree::create` (main.rs `create_worktree`):
//! - `copy_local`: the project folder's files matching the project's `copyFiles` patterns that git doesn't track
//!   (ignored or untracked) are copied to the same place in the new worktree, never over a file already there;
//! - `start_setup`: the project's `setupCommand` runs there in the background through the platform shell, its output
//!   in `<project_dir>/setup-logs/<worktree folder>.log`. While it runs the agent's brief says so (`brief`), so it
//!   doesn't install the same things meanwhile; when it ends a `[maestro]` note tells it how it went (control.rs
//!   `tell`).
//!
//! Both settings live in `project.json` (project.rs); a copy or setup failure never fails the creation.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::AppHandle;

/// `copyFiles` when `project.json` has none: the env files a checkout never has.
pub const DEFAULT_COPY: &str = ".env*";
/// A setup still running after this is stopped, its whole process tree with it.
const SETUP_TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// Lines of the log a failure note carries.
const TAIL_LINES: usize = 15;
/// Files one creation copies at most (a `*` in a big folder).
const MAX_FILES: usize = 500;

// ---------- patterns ----------

/// A `copyFiles` pattern as its path segments: relative to the project folder, `/` or `\` between folders, `*`
/// within one segment. Err (Korean, for the settings row and the brief): empty, absolute, `..`, or into `.git`.
pub fn segments(pattern: &str) -> Result<Vec<String>, String> {
    let p = pattern.trim();
    if p.is_empty() {
        return Err("빈 패턴이에요".into());
    }
    let unified = p.replace('\\', "/");
    // a root, a UNC path (`\\server`), a drive (`C:`) or an NTFS stream (`a:b`)
    if unified.starts_with('/') || unified.contains(':') {
        return Err(format!("프로젝트 폴더 안의 상대 경로만 쓸 수 있어요: {p}"));
    }
    let segs: Vec<String> = unified.split('/').filter(|s| !s.is_empty() && *s != ".").map(String::from).collect();
    if segs.iter().any(|s| s == "..") {
        return Err(format!("..은 쓸 수 없어요: {p}"));
    }
    if segs.iter().any(|s| s.eq_ignore_ascii_case(".git")) {
        return Err(format!(".git 안은 복사하지 않아요: {p}"));
    }
    if segs.is_empty() {
        return Err(format!("파일을 가리키지 않아요: {p}"));
    }
    Ok(segs)
}

/// The patterns as saved: trimmed, `/` between folders, empty ones dropped, each once. Err: the first unsafe one.
pub fn clean_patterns(list: &[String]) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = vec![];
    for p in list.iter().filter(|p| !p.trim().is_empty()) {
        let p = segments(p)?.join("/");
        if !out.contains(&p) {
            out.push(p);
        }
    }
    Ok(out)
}

/// Whether `name` (one path segment) matches `pat`, where `*` stands for any run of characters, none included.
/// `fold`: ignoring case, as the file systems of Windows and macOS do.
pub fn matches(pat: &str, name: &str, fold: bool) -> bool {
    let chars = |s: &str| -> Vec<char> { if fold { s.to_lowercase().chars().collect() } else { s.chars().collect() } };
    let (p, n) = (chars(pat), chars(name));
    // the last `*` seen and where in `name` it started: on a mismatch it takes one more character
    let (mut pi, mut ni, mut star, mut from) = (0, 0, None, 0);
    while ni < n.len() {
        if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            from = ni;
            pi += 1;
        } else if pi < p.len() && p[pi] == n[ni] {
            pi += 1;
            ni += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            from += 1;
            ni = from;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|c| *c == '*')
}

/// Whether two relative paths are the same file (`fold`: case ignored).
fn same(a: &str, b: &str, fold: bool) -> bool {
    if fold { a.to_lowercase() == b.to_lowercase() } else { a == b }
}

/// The files under `root` matching pattern `segs`, as relative paths with `/`: the folders on the way taken as named
/// or matched, never `.git`, never a folder or file that resolves outside `root` (a link out of the project).
fn find(root: &Path, segs: &[String], fold: bool) -> Vec<String> {
    let Ok(base) = root.canonicalize() else { return vec![] };
    let mut level: Vec<(PathBuf, String)> = vec![(root.to_path_buf(), String::new())];
    for (i, seg) in segs.iter().enumerate() {
        let last = i + 1 == segs.len();
        let mut next = vec![];
        for (dir, rel) in &level {
            let names: Vec<String> = if seg.contains('*') {
                let mut v: Vec<String> = std::fs::read_dir(dir)
                    .map(|rd| rd.flatten().filter_map(|e| e.file_name().into_string().ok()).filter(|n| matches(seg, n, fold)).collect())
                    .unwrap_or_default();
                v.sort();
                v
            } else {
                vec![seg.clone()]
            };
            for name in names.into_iter().filter(|n| !n.eq_ignore_ascii_case(".git")) {
                let p = dir.join(&name);
                let Ok(real) = p.canonicalize() else { continue };
                if !real.starts_with(&base) || (if last { !real.is_file() } else { !real.is_dir() }) {
                    continue;
                }
                let r = if rel.is_empty() { name } else { format!("{rel}/{name}") };
                next.push((p, r));
            }
        }
        level = next;
    }
    level.into_iter().map(|(_, r)| r).collect()
}

// ---------- copying ----------

/// What `copy_local` did: the files copied (relative paths), those of them the worktree's git doesn't ignore (a
/// `git add -A` there would commit them), and what couldn't be copied, with why.
#[derive(Default, Debug, PartialEq)]
pub struct Copied {
    pub copied: Vec<String>,
    pub unignored: Vec<String>,
    pub failed: Vec<String>,
}

/// Copies the files of the project folder `root` matching `patterns` that git doesn't track into `worktree`, at the
/// same relative path. A file already there is left as it is (checked out, or put there by someone else).
pub fn copy_local(root: &str, worktree: &str, patterns: &[String]) -> Copied {
    let fold = cfg!(any(windows, target_os = "macos"));
    let (src, dst) = (Path::new(root), Path::new(worktree));
    let mut out = Copied::default();
    let mut found: Vec<String> = vec![];
    for p in patterns {
        match segments(p) {
            Ok(segs) => {
                for r in find(src, &segs, fold) {
                    if !found.iter().any(|f| same(f, &r, fold)) {
                        found.push(r);
                    }
                }
            }
            Err(e) => out.failed.push(e),
        }
    }
    if found.len() > MAX_FILES {
        out.failed.push(format!("맞는 파일이 {}개라 앞의 {MAX_FILES}개만 복사했어요", found.len()));
        found.truncate(MAX_FILES);
    }
    if found.is_empty() {
        return out;
    }
    // tracked files are in the checkout already (at the worktree's own commit, which may differ: never replaced)
    let tracked = match crate::worktree::tracked_among(src, &found) {
        Ok(t) => t,
        Err(e) => {
            out.failed.push(format!("git이 추적하는 파일인지 확인하지 못해 복사하지 않았어요: {e}"));
            return out;
        }
    };
    for rel in found.into_iter().filter(|r| !tracked.iter().any(|t| same(t, r, fold))) {
        match copy_new(&src.join(&rel), &dst.join(&rel)) {
            Ok(true) => out.copied.push(rel),
            Ok(false) => {}
            Err(e) => out.failed.push(format!("{rel}: {e}")),
        }
    }
    // an untracked file the worktree's .gitignore doesn't cover (a forgotten `.env`): the agent is warned off committing
    // it; when git can't say, every copied file is taken for one
    if !out.copied.is_empty() {
        out.unignored = match crate::worktree::unignored_among(dst, &out.copied) {
            Ok(open) => out.copied.iter().filter(|c| open.iter().any(|o| same(o, c, fold))).cloned().collect(),
            Err(_) => out.copied.clone(),
        };
    }
    out
}

/// Copies `from` to `to` only when nothing is at `to` (`create_new`: no race with a writer). Ok(false): something
/// was there.
fn copy_new(from: &Path, to: &Path) -> std::io::Result<bool> {
    if to.symlink_metadata().is_ok() {
        return Ok(false);
    }
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut src = std::fs::File::open(from)?;
    let mut dst = match std::fs::OpenOptions::new().write(true).create_new(true).open(to) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Ok(false),
        Err(e) => return Err(e),
    };
    if let Err(e) = std::io::copy(&mut src, &mut dst) {
        drop(dst);
        let _ = std::fs::remove_file(to);
        return Err(e);
    }
    // an executable script stays executable
    if let Ok(m) = src.metadata() {
        let _ = dst.set_permissions(m.permissions());
    }
    Ok(true)
}

// ---------- what the agent is told ----------

/// A worktree's prep the brief tells its agent about, while the app runs.
#[derive(Default)]
struct Prep {
    /// what `copy_local` couldn't copy
    failed: Vec<String>,
    /// copied files git doesn't ignore there
    unignored: Vec<String>,
    /// the setup command while it runs
    setup: Option<Running>,
}

impl Prep {
    /// Nothing left to say once the setup is over.
    fn quiet(&self) -> bool {
        self.failed.is_empty() && self.unignored.is_empty() && self.setup.is_none()
    }
}

struct Running {
    /// which run (a worktree made again under the same path starts another)
    id: u64,
    command: String,
    log: String,
    /// its process tree, once started (None until then, or when it couldn't be set up: then `pid`)
    tree: Option<Arc<crate::proc::Tree>>,
    pid: Option<u32>,
}

static PREPS: Mutex<Option<HashMap<String, Prep>>> = Mutex::new(None);
static NEXT: AtomicU64 = AtomicU64::new(1);

fn with<T>(f: impl FnOnce(&mut HashMap<String, Prep>) -> T) -> T {
    let mut g = PREPS.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(HashMap::new))
}

/// Records a new worktree's prep before its agent starts, so its first brief has it: what `copy_local` reported,
/// and the setup command about to run (`setup`: command and log). Returns the run's id for `start_setup`.
pub fn begin(worktree: &str, copied: Copied, setup: Option<(String, String)>) -> u64 {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let setup = setup.map(|(command, log)| Running { id, command, log, tree: None, pid: None });
    let p = Prep { failed: copied.failed, unignored: copied.unignored, setup };
    let key = crate::util::path_key(worktree);
    with(|m| if p.quiet() { m.remove(&key); } else { m.insert(key, p); });
    id
}

/// The worktree is going away (its creation failed, or it's discarded): its setup is stopped, and nothing more is
/// said about it. Whether a setup was running.
pub fn forget(worktree: &str) -> bool {
    let gone = with(|m| m.remove(&crate::util::path_key(worktree))).and_then(|p| p.setup);
    let Some(run) = gone else { return false };
    match (run.tree, run.pid) {
        (Some(t), _) => t.kill(),
        (None, Some(pid)) => crate::proc::kill_tree(pid),
        _ => {}
    }
    true
}

/// What the agent in `worktree` is told up front about its folder's prep (agent.rs `system_brief`): the setup
/// command while it runs, the copied files it must not commit, and the files that couldn't be copied. None: nothing
/// to say.
pub fn brief(worktree: &str) -> Option<String> {
    with(|m| {
        let p = m.get(&crate::util::path_key(worktree))?;
        let mut out = String::from("## 워크트리 준비");
        if let Some(r) = &p.setup {
            out.push_str(&format!(
                "\n\n이 워크트리를 만들 때 Maestro가 준비 명령 `{}`을(를) 백그라운드에서 돌리기 시작했어요(로그: {}). 끝나면 [maestro] 준비 명령 메모가 와요. 그 전에는 같은 설치를 직접 하지 말고, 설치가 있어야 하는 일(빌드, 테스트)은 메모를 받은 뒤에 하세요. 메모가 이미 왔으면 그 결과를 따르세요.",
                r.command, r.log
            ));
        }
        let list = |v: &[String]| v.iter().map(|f| format!("- {f}")).collect::<Vec<_>>().join("\n");
        if !p.unignored.is_empty() {
            out.push_str(&format!(
                "\n\nMaestro가 프로젝트 폴더에서 복사해 온 로컬 파일 중 이 워크트리의 .gitignore에 없는 것이 있어요. 비밀 값이 들었을 수 있으니 절대 커밋하지 마세요(git add -A 대신 파일을 골라서 add):\n{}",
                list(&p.unignored)
            ));
        }
        if !p.failed.is_empty() {
            out.push_str(&format!("\n\n프로젝트 폴더에서 이 워크트리로 복사하지 못한 로컬 파일이 있어요(설정 → 프로젝트 → 워크트리 준비):\n{}", list(&p.failed)));
        }
        Some(out)
    })
}

/// How a setup run ended.
#[derive(Debug, PartialEq)]
pub enum End {
    Code(i32),
    /// ended without a code (a signal)
    NoCode,
    TimedOut,
    NotStarted(String),
}

/// The `[maestro]` note for a setup that ended: how, the log, and on failure its last lines.
pub fn done_note(command: &str, end: &End, log: &str, tail: &str) -> String {
    let tail = if tail.trim().is_empty() { String::new() } else { format!("\n마지막 줄:\n{}", tail.trim_end()) };
    match end {
        End::Code(0) => format!("[maestro] 준비 명령이 끝났어요: {command} (종료 코드 0). 로그: {log}"),
        End::Code(c) => format!("[maestro] 준비 명령이 실패했어요: {command} (종료 코드 {c}). 로그: {log}{tail}\n필요한 설치는 이제 직접 하세요."),
        End::NoCode => format!("[maestro] 준비 명령이 종료 코드 없이 멈췄어요: {command}. 로그: {log}{tail}\n필요한 설치는 이제 직접 하세요."),
        End::TimedOut => format!(
            "[maestro] 준비 명령이 {}분 안에 끝나지 않아 멈췄어요: {command}. 로그: {log}{tail}\n필요한 설치는 이제 직접 하세요.",
            SETUP_TIMEOUT.as_secs() / 60
        ),
        End::NotStarted(e) => format!("[maestro] 준비 명령을 실행하지 못했어요: {command} ({e}). 필요한 설치는 직접 하세요."),
    }
}

/// The last `n` non-empty lines of `text`, each cut to 300 characters.
pub fn tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().map(str::trim_end).filter(|l| !l.is_empty()).collect();
    let cut = |l: &str| l.char_indices().nth(300).map_or(l.to_string(), |(i, _)| format!("{}…", &l[..i]));
    lines[lines.len().saturating_sub(n)..].iter().map(|l| cut(l)).collect::<Vec<_>>().join("\n")
}

/// The kept-note key of a worktree's setup result (control.rs `pass_notes`), apart from worker names.
fn note_key(worktree: &str) -> String {
    format!("setup:{}", crate::util::path_key(worktree))
}

// ---------- the setup command ----------

/// `<project_dir>/setup-logs/<worktree folder>.log`
pub fn log_path(project_dir: &Path, worktree: &str) -> PathBuf {
    let folder = Path::new(worktree).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "worktree".into());
    project_dir.join("setup-logs").join(format!("{folder}.log"))
}

/// `command` through the platform shell: `cmd /S /C "<command>"` (the line as typed, `/S` takes off only the outer
/// quotes) on Windows, `sh -c <command>` elsewhere.
fn shell(command: &str) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut c = Command::new("cmd");
        c.args(["/S", "/C"]).raw_arg(format!("\"{command}\""));
        c
    }
    #[cfg(not(windows))]
    {
        let mut c = Command::new("sh");
        c.args(["-c", command]);
        c
    }
}

/// Runs `command` in `dir` to its end or `timeout` (then its whole tree is killed), stdout and stderr into `log`.
/// `started` gets its process tree (the registry's, so `forget` can stop it).
fn run(dir: &str, command: &str, log: &Path, timeout: Duration, started: impl FnOnce(Option<Arc<crate::proc::Tree>>, u32)) -> End {
    let file = log.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| std::fs::File::create(log));
    let mut file = match file {
        Ok(f) => f,
        Err(e) => return End::NotStarted(format!("로그 파일을 만들지 못했어요: {e}")),
    };
    let _ = writeln!(file, "$ {command}\n");
    let (out, err) = match (file.try_clone(), file.try_clone()) {
        (Ok(o), Ok(e)) => (o, e),
        (Err(e), _) | (_, Err(e)) => return End::NotStarted(e.to_string()),
    };
    let mut cmd = shell(command);
    cmd.current_dir(dir).stdin(Stdio::null()).stdout(out).stderr(err);
    // as every process Maestro starts: a host Claude session's identity doesn't leak in
    for k in crate::agent::host_session_env(std::env::vars().map(|(k, _)| k)) {
        cmd.env_remove(k);
    }
    crate::proc::contain(&mut cmd);
    crate::cli::quiet(&mut cmd);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return End::NotStarted(e.to_string()),
    };
    let tree = crate::proc::Tree::adopt(&child).map(Arc::new);
    started(tree.clone(), child.id());
    let t = Instant::now();
    let end = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s.code().map_or(End::NoCode, End::Code),
            Ok(None) if t.elapsed() >= timeout => {
                match &tree {
                    Some(t) => t.kill(),
                    None => crate::proc::kill_tree(child.id()),
                }
                let _ = child.kill();
                let _ = child.wait();
                break End::TimedOut;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
            Err(e) => break End::NotStarted(e.to_string()),
        }
    };
    let _ = writeln!(file, "\n[maestro] {}", match &end {
        End::Code(c) => format!("종료 코드 {c}"),
        End::NoCode => "종료 코드 없이 멈췄어요".into(),
        End::TimedOut => "시간이 다 돼 멈췄어요".into(),
        End::NotStarted(e) => e.clone(),
    });
    end
}

/// Runs a new worktree's setup command in the background (`begin` recorded it as run `id`); when it ends, its
/// agent gets the `[maestro]` note, or keeps it for its next start. Nothing is said once `forget` stopped it.
pub fn start_setup(app: AppHandle, root: String, worktree: String, command: String, log: PathBuf, id: u64) {
    std::thread::spawn(move || {
        let key = crate::util::path_key(&worktree);
        let mine = |m: &mut HashMap<String, Prep>| m.get_mut(&key).and_then(|p| p.setup.as_mut()).filter(|r| r.id == id).is_some();
        let end = run(&worktree, &command, &log, SETUP_TIMEOUT, |tree, pid| {
            with(|m| {
                if let Some(r) = m.get_mut(&key).and_then(|p| p.setup.as_mut()).filter(|r| r.id == id) {
                    r.tree = tree;
                    r.pid = Some(pid);
                }
            })
        });
        // stopped by `forget` (the worktree went): nobody to tell
        let current = with(|m| {
            let current = mine(m);
            if current {
                if let Some(p) = m.get_mut(&key) {
                    p.setup = None;
                    if p.quiet() {
                        m.remove(&key);
                    }
                }
            }
            current
        });
        if !current {
            return;
        }
        let shown = log.to_string_lossy().replace('\\', "/");
        let tail = if end == End::Code(0) { String::new() } else { std::fs::read(&log).map(|b| tail_lines(&String::from_utf8_lossy(&b), TAIL_LINES)).unwrap_or_default() };
        let note = done_note(&command, &end, &shown, &tail);
        if let Err(e) = crate::control::tell(&app, &root, &worktree, &note_key(&worktree), &note) {
            eprintln!("[maestro] setup note not delivered: {e}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_stay_inside_the_project() {
        assert_eq!(segments(".env*").unwrap(), [".env*"]);
        assert_eq!(segments(" config/local.json ").unwrap(), ["config", "local.json"]);
        assert_eq!(segments("config\\local.json").unwrap(), ["config", "local.json"], "either slash");
        assert_eq!(segments("./a//b/").unwrap(), ["a", "b"], "./ and doubled slashes are nothing");
        for bad in ["", "  ", "/etc/passwd", "\\\\server\\share\\x", "C:/x", "C:x", "a:b", "../x", "a/../../b", "..", ".git/config", "a/.GIT/x", "."] {
            assert!(segments(bad).is_err(), "{bad:?} must be refused");
        }
        assert!(segments("../x").unwrap_err().contains(".."));
    }

    #[test]
    fn patterns_are_saved_clean() {
        let list = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(clean_patterns(&list(&[" .env* ", "", ".env*", "config\\local.json", "  "])).unwrap(), [".env*", "config/local.json"]);
        assert!(clean_patterns(&list(&[".env", "/abs"])).unwrap_err().contains("/abs"));
        assert!(clean_patterns(&[]).unwrap().is_empty());
    }

    #[test]
    fn star_matches_within_a_segment() {
        assert!(matches(".env*", ".env", false));
        assert!(matches(".env*", ".env.local", false));
        assert!(!matches(".env*", "env", false));
        assert!(!matches(".env*", "x.env", false));
        assert!(matches("*.json", "a.json", false));
        assert!(!matches("*.json", "a.json.bak", false));
        assert!(matches("a*b*c", "abc", false));
        assert!(matches("a*b*c", "aXbYbc", false), "backtracks to a later b");
        assert!(!matches("a*b*c", "acb", false));
        assert!(matches("*", ".hidden", false));
        assert!(matches("**", "x", false));
        assert!(matches("local.json", "local.json", false));
        assert!(!matches("local.json", "local.jsonx", false));
        assert!(matches(".env*", ".ENV.Local", true), "case folded");
        assert!(!matches(".env*", ".ENV", false));
        assert!(matches("설정*", "설정.json", false), "not only ASCII");
    }

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git").arg("-C").arg(dir).args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"]).args(args).output().unwrap().status.success();
        assert!(ok, "git {args:?}");
    }

    fn put(dir: &Path, rel: &str, text: &str) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    #[test]
    fn copies_only_untracked_files_and_never_over_one() {
        let base = crate::util::unique_temp_dir("maestro-prep");
        let (src, dst) = (base.join("repo"), base.join("wt"));
        std::fs::create_dir_all(&src).unwrap();
        git(&src, &["init", "-q"]);
        put(&src, ".gitignore", ".env\n.env.local\nconfig/local.json\n");
        put(&src, ".env.example", "tracked");
        git(&src, &["add", "-A"]);
        git(&src, &["commit", "-q", "-m", "init"]);
        git(&src, &["worktree", "add", "-q", "-b", "w", &dst.to_string_lossy()]);
        put(&src, ".env", "SECRET=1"); // ignored
        put(&src, ".env.local", "LOCAL=src"); // ignored, but the worktree has one already
        put(&src, ".env.untracked", "U=1"); // neither tracked nor ignored
        put(&src, ".env.example", "changed here"); // tracked: the worktree has its own
        put(&src, "config/local.json", "{}");
        std::fs::create_dir_all(src.join(".envdir")).unwrap(); // a folder: never copied
        put(&src, ".envdir/inner", "x");
        put(&dst, ".env.local", "LOCAL=wt");
        let pats: Vec<String> = [".env*", "config/local.json", "config/*.json", "missing/x", "../x"].iter().map(|s| s.to_string()).collect();
        let r = copy_local(&src.to_string_lossy(), &dst.to_string_lossy(), &pats);
        assert_eq!(r.copied, [".env", ".env.untracked", "config/local.json"], "{r:?}");
        assert_eq!(r.unignored, [".env.untracked"], "a git add -A in the worktree would take it");
        assert_eq!(r.failed.len(), 1, "only the unsafe pattern: {r:?}");
        assert!(r.failed[0].contains(".."));
        assert_eq!(std::fs::read_to_string(dst.join(".env")).unwrap(), "SECRET=1");
        assert_eq!(std::fs::read_to_string(dst.join(".env.local")).unwrap(), "LOCAL=wt", "never overwritten");
        assert_eq!(std::fs::read_to_string(dst.join(".env.example")).unwrap(), "tracked", "tracked: the checkout's own");
        assert!(!dst.join(".envdir").exists());
        // again: everything is there now
        assert_eq!(copy_local(&src.to_string_lossy(), &dst.to_string_lossy(), &pats).copied, Vec::<String>::new());
        let _ = std::fs::remove_dir_all(base);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_out_of_the_project_is_not_followed() {
        let base = crate::util::unique_temp_dir("maestro-prep-link");
        let (src, dst) = (base.join("repo"), base.join("wt"));
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dst).unwrap();
        git(&src, &["init", "-q"]);
        put(&base, "outside.txt", "secret");
        std::os::unix::fs::symlink(base.join("outside.txt"), src.join(".env")).unwrap();
        std::os::unix::fs::symlink(&base, src.join("up")).unwrap();
        let r = copy_local(&src.to_string_lossy(), &dst.to_string_lossy(), &[".env*".to_string(), "up/outside.txt".to_string()]);
        assert!(r.copied.is_empty(), "{r:?}");
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn notes_say_how_it_ended() {
        assert_eq!(done_note("npm install", &End::Code(0), "C:/l.log", "ignored"), "[maestro] 준비 명령이 끝났어요: npm install (종료 코드 0). 로그: C:/l.log");
        let n = done_note("npm install", &End::Code(1), "C:/l.log", "npm ERR! x\n");
        assert!(n.starts_with("[maestro] 준비 명령이 실패했어요: npm install (종료 코드 1). 로그: C:/l.log\n마지막 줄:\nnpm ERR! x\n"), "{n}");
        assert!(done_note("x", &End::TimedOut, "l", "").contains("30분"));
        assert!(!done_note("x", &End::Code(2), "l", " ").contains("마지막 줄"), "no tail, no heading");
        assert!(done_note("x", &End::NotStarted("없음".into()), "l", "").contains("실행하지 못했어요: x (없음)"));
        assert_eq!(tail_lines("a\n\nb\r\nc\n", 2), "b\nc");
        assert_eq!(tail_lines("", 3), "");
        assert_eq!(tail_lines(&"가".repeat(301), 1), format!("{}…", "가".repeat(300)));
    }

    #[test]
    fn the_brief_tells_while_it_runs() {
        let wt = "C:/p/prep-brief/worktrees/a";
        assert_eq!(brief(wt), None);
        let copied = Copied { copied: vec![".env".into(), ".env.x".into()], unignored: vec![".env.x".into()], failed: vec![".env.y: 접근 거부".into()] };
        let id = begin(wt, copied, Some(("npm install".into(), "C:/logs/a.log".into())));
        let b = brief("c:\\P\\prep-brief\\worktrees\\a\\").unwrap();
        assert!(b.contains("`npm install`") && b.contains("C:/logs/a.log"), "{b}");
        assert!(b.contains("커밋하지 마세요") && b.contains("- .env.x") && !b.contains("- .env\n"), "only the unignored one: {b}");
        assert!(b.contains("- .env.y: 접근 거부"), "{b}");
        let home = crate::util::unique_temp_dir("maestro-prep-home");
        let full = crate::agent::system_brief(&home, &crate::agent::AgentMeta::default(), wt);
        assert!(full.ends_with(&b), "the agent's brief carries it");
        assert!(id > 0);
        assert!(forget(wt), "a setup was on record");
        assert_eq!(brief(wt), None);
        assert!(!forget(wt));
        // nothing to say: nothing kept
        begin(wt, Copied { copied: vec![".env".into()], ..Default::default() }, None);
        assert_eq!(brief(wt), None);
        assert_eq!(note_key("C:\\P\\a\\"), "setup:c:/p/a", "one key per worktree, whatever the spelling");
    }

    #[test]
    fn runs_through_the_shell_with_its_output_logged() {
        let dir = crate::util::unique_temp_dir("maestro-prep-run");
        std::fs::create_dir_all(&dir).unwrap();
        let d = dir.to_string_lossy().to_string();
        let log = dir.join("logs").join("a.log");
        let mut pid = None;
        assert_eq!(run(&d, "echo hello && exit 3", &log, Duration::from_secs(60), |_, p| pid = Some(p)), End::Code(3));
        assert!(pid.is_some());
        let text = std::fs::read_to_string(&log).unwrap();
        assert!(text.starts_with("$ echo hello && exit 3") && text.contains("hello") && text.contains("종료 코드 3"), "{text}");
        assert_eq!(run(&d, "echo ok", &log, Duration::from_secs(60), |_, _| {}), End::Code(0));
        let slow = if cfg!(windows) { "ping -n 30 127.0.0.1 >NUL" } else { "sleep 30" };
        let t = Instant::now();
        assert_eq!(run(&d, slow, &log, Duration::from_millis(500), |_, _| {}), End::TimedOut);
        assert!(t.elapsed() < Duration::from_secs(20), "killed, not waited out");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn logs_sit_by_worktree_folder() {
        assert_eq!(log_path(Path::new("/h/.maestro/projects/app_1"), "C:/h/.maestro/projects/app_1/worktrees/agent-2"), Path::new("/h/.maestro/projects/app_1/setup-logs/agent-2.log"));
    }
}
