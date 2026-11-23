//! 프로젝트 추가's other two ways (docs/features/ui/add-project.md): a repo cloned from a URL (`clone`), or a new
//! empty one (`create`). Both make a folder that wasn't there and return it; the sidebar then adds it like a picked
//! folder (`add_repo`).

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::exec::{self, Cmd};
use crate::worktree;

/// How long a clone may run before it's killed (a big repo on a slow line; a hang never waits on a password).
const CLONE_SECS: f64 = 600.0;

const BAD_URL: &str = "https://, http://, ssh:// 주소나 git@호스트:경로만 복제할 수 있어요";

/// The URL as `git clone` gets it (trimmed): https://, http://, ssh:// or the scp-like `user@host:path`. Never a
/// local path, `file://` or another transport (`ext::` runs a command), and never anything git or ssh could read
/// as an option.
pub fn clone_url(url: &str) -> Result<&str, String> {
    let u = url.trim();
    if u.is_empty() {
        return Err("URL을 입력해 주세요".into());
    }
    if u.starts_with('-') || u.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(BAD_URL.into());
    }
    let lower = u.to_ascii_lowercase();
    let ok = match ["https://", "http://", "ssh://"].iter().find(|s| lower.starts_with(**s)) {
        Some(scheme) => {
            // user@host[:port]: a host there, and neither part an option (ssh://-oProxyCommand=…)
            let auth = u[scheme.len()..].split(['/', '?', '#']).next().unwrap_or("");
            let host = auth.rsplit('@').next().unwrap_or("");
            !auth.starts_with('-') && !host.is_empty() && !host.starts_with('-') && !host.starts_with(':')
        }
        None => scp_like(u),
    };
    if ok { Ok(u) } else { Err(BAD_URL.into()) }
}

/// git's scp-like ssh form `user@host:path`: no slash before the colon (git would read a path), a user and a host
/// that aren't options, and some path.
fn scp_like(u: &str) -> bool {
    let Some((head, path)) = u.split_once(':') else { return false };
    let Some((user, host)) = head.split_once('@') else { return false };
    !head.contains(['/', '\\']) && !user.is_empty() && !user.starts_with('-') && !host.is_empty() && !host.starts_with('-') && !path.is_empty()
}

/// The folder a clone of `url` gets when the name is left as filled in: its last path part without `.git`, as git
/// names it (`https://github.com/a/repo.git` → `repo`, `git@host:repo` → `repo`). "" when that's no usable name.
/// src/projectForm.ts `repoNameFromUrl` does the same for the dialog.
pub fn repo_name_from_url(url: &str) -> String {
    let u = url.trim();
    let u = u.split(['?', '#']).next().unwrap_or("");
    let u = u.trim_end_matches(['/', '\\']);
    let u = u.strip_suffix(".git").unwrap_or(u).trim_end_matches(['/', '\\']);
    let last = u.rsplit(['/', '\\', ':']).next().unwrap_or("");
    if check_name(last).is_ok() { last.to_string() } else { String::new() }
}

/// A new project folder's name (trimmed): one folder (no separators, no `..`), and one Windows can make.
pub fn check_name(name: &str) -> Result<&str, String> {
    let n = name.trim();
    if n.is_empty() {
        return Err("폴더 이름을 입력해 주세요".into());
    }
    let bad = |c: char| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control();
    if n == "." || n.contains("..") || n.starts_with('-') || n.ends_with('.') || n.chars().any(bad) || reserved(n) {
        return Err(format!("폴더 이름으로 쓸 수 없어요: {n}"));
    }
    Ok(n)
}

/// Windows' device names (CON, NUL, COM1, …), with or without an extension: no folder can have them.
fn reserved(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT")) && stem.len() == 4 && stem.as_bytes()[3].is_ascii_digit() && stem.as_bytes()[3] != b'0')
}

/// Where the new project goes: `name` (checked) in `parent`, a folder that exists; the folder itself mustn't yet.
fn destination(parent: &str, name: &str) -> Result<PathBuf, String> {
    let name = check_name(name)?;
    let parent = parent.trim();
    if parent.is_empty() {
        return Err("위치를 골라 주세요".into());
    }
    let dir = Path::new(parent);
    if !dir.is_absolute() || !dir.is_dir() {
        return Err(format!("위치 폴더가 없어요: {parent}"));
    }
    let dest = dir.join(name);
    // a dangling link counts too: the folder can't be made there
    if dest.symlink_metadata().is_ok() {
        return Err(format!("이미 있는 폴더예요: {}", dest.display()));
    }
    Ok(dest)
}

/// URL로 복제: `git clone` of `url` into folder `name` (empty: the URL's, `repo_name_from_url`) in `parent`. Returns
/// the new folder.
pub fn clone(url: &str, parent: &str, name: &str) -> Result<String, String> {
    let url = clone_url(url)?;
    let name = if name.trim().is_empty() { repo_name_from_url(url) } else { name.to_string() };
    clone_into(url, &destination(parent, &name)?)
}

/// `git clone -- <url> <dest>` with no prompt (a password or host key it would ask for fails it) and a 10 minute
/// limit. `dest` is made here first, so a folder something else makes at the same moment is never taken or removed;
/// on failure it goes again and git's last lines say why.
fn clone_into(url: &str, dest: &Path) -> Result<String, String> {
    std::fs::create_dir(dest).map_err(|e| format!("폴더를 만들지 못했어요: {e}"))?;
    let d = dest.to_string_lossy().into_owned();
    let out = exec::run(Cmd::git(dest.parent().unwrap_or(dest), &["clone", "--", url, &d]).network().timeout(CLONE_SECS));
    if out.ok() {
        return Ok(d);
    }
    remove_made(dest);
    Err(clone_error(&out))
}

/// Takes away a folder made here; a killed git's files can stay locked a moment, so it tries three times.
fn remove_made(dest: &Path) {
    for _ in 0..3 {
        if std::fs::remove_dir_all(dest).is_ok() || !dest.exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

fn clone_error(out: &exec::Out) -> String {
    if out.timed_out {
        return "10분 안에 끝나지 않아 복제를 멈췄어요".into();
    }
    if out.spawn_err == Some(std::io::ErrorKind::NotFound) {
        return "git을 찾지 못했어요. Git을 설치해 주세요".into();
    }
    let why = last_lines(&out.stderr, 4);
    if why.is_empty() { "복제하지 못했어요".into() } else { format!("복제하지 못했어요\n{why}") }
}

/// git's last `n` lines that say something (not the "Cloning into …" it starts with).
fn last_lines(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().map(str::trim_end).filter(|l| !l.trim().is_empty() && !l.starts_with("Cloning into")).collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// 새 프로젝트: folder `name` in `parent`, its own git repo (branch main, even inside another repo's folder) with a
/// first commit, as `init_repo` makes it. Returns the new folder; a failed one is taken away again.
pub fn create(parent: &str, name: &str) -> Result<String, String> {
    let dest = destination(parent, name)?;
    std::fs::create_dir(&dest).map_err(|e| format!("폴더를 만들지 못했어요: {e}"))?;
    let d = dest.to_string_lossy().into_owned();
    if let Err(e) = worktree::init_new_repo(&d) {
        remove_made(&dest);
        return Err(e);
    }
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clone_urls_are_remote_only() {
        for ok in [
            "https://github.com/a/repo.git",
            "  http://host:8080/repo  ",
            "HTTPS://GitHub.com/a/b",
            "ssh://git@host:22/a/repo.git",
            "ssh://host/repo",
            "git@github.com:a/repo.git",
            "org-1@ssh.dev.azure.com:v3/o/p/r",
            "https://user:token@host/repo.git",
        ] {
            assert!(clone_url(ok).is_ok(), "{ok}");
        }
        assert_eq!(clone_url("  https://h/r  ").unwrap(), "https://h/r", "trimmed");
        for bad in [
            "",
            "   ",
            "-u/x",
            "--upload-pack=calc",
            "file:///C:/repo",
            "C:\\code\\repo",
            "C:/code/repo",
            "/home/me/repo",
            "../repo",
            "\\\\server\\share\\repo",
            "ext::sh -c calc",
            "ext::calc",
            "fd::3",
            "https://",
            "https:///repo",
            "https://-oProxyCommand=calc/x",
            "ssh://-oProxyCommand=calc/x",
            "ssh://-u@host/x",
            "ssh://user@-host/x",
            "https://host/a b",
            "git@host:",
            "git@:repo",
            "@host:repo",
            "-git@host:repo",
            "git@-host:repo",
            "a/git@host:repo",
            "host:repo",
            "git://host/repo",
        ] {
            assert!(clone_url(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_clone_is_named_by_its_urls_last_part() {
        for (url, name) in [
            ("https://github.com/a/repo.git", "repo"),
            ("https://github.com/a/repo", "repo"),
            ("https://github.com/a/repo/", "repo"),
            ("https://github.com/a/repo.git/", "repo"),
            ("https://host/a/repo.git?x=1#top", "repo"),
            ("git@github.com:a/my-app.git", "my-app"),
            ("git@host:repo.git", "repo"),
            ("ssh://git@host:22/team/tool.js.git", "tool.js"),
            ("  https://h/r.git  ", "r"),
            ("https://github.com", "github.com"),
            ("https://h/a/..", ""),
            ("https://h/con.git", ""),
            ("", ""),
        ] {
            assert_eq!(repo_name_from_url(url), name, "{url:?}");
        }
    }

    #[test]
    fn folder_names_are_one_plain_folder() {
        for ok in ["repo", " my app ", "한글-폴더", "a.b", "v1.2.3", "COM0", "console"] {
            assert!(check_name(ok).is_ok(), "{ok:?}");
        }
        assert_eq!(check_name(" repo ").unwrap(), "repo");
        for bad in ["", "  ", ".", "..", "a..b", "a/b", "a\\b", "C:", "a:b", "a*", "a?", "a|b", "<a>", "\"a\"", "-a", "a.", "con", "NUL.txt", "com1", "LPT9", "a\tb"] {
            assert!(check_name(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn last_lines_skip_the_start_line() {
        let err = "Cloning into 'C:/x/repo'...\nremote: Repository not found.\n\nfatal: repository 'https://h/r/' not found\n";
        assert_eq!(last_lines(err, 4), "remote: Repository not found.\nfatal: repository 'https://h/r/' not found");
        assert_eq!(last_lines("a\nb\nc\nd\ne\n", 2), "d\ne");
        assert_eq!(last_lines("Cloning into 'x'...\n", 4), "");
    }

    /// A fresh temp folder for one test.
    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("maestro-newproject-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn create_makes_its_own_repo_with_a_commit() {
        let dir = temp("create");
        // the parent is a repo itself: the new project must still be its own, not a folder of that one
        worktree::init_repo(&dir.to_string_lossy()).unwrap();
        let made = create(&dir.to_string_lossy(), " app ").unwrap();
        assert!(Path::new(&made).ends_with("app"));
        assert_eq!(worktree::repo_state(&made), "ready");
        assert_eq!(worktree::norm(&PathBuf::from(worktree::toplevel(&made).unwrap())).to_lowercase(), worktree::norm(Path::new(&made)).to_lowercase());
        assert_eq!(worktree::current_branch(&made), "main");
        // taken now, and a bad name or place is refused before anything is made
        assert!(create(&dir.to_string_lossy(), "app").unwrap_err().contains("이미 있는"));
        assert!(create(&dir.to_string_lossy(), "../app2").is_err());
        assert!(create("relative/place", "app3").is_err());
        assert!(create(&dir.join("missing").to_string_lossy(), "app4").is_err());
        assert!(!dir.join("app2").exists() && !dir.join("missing").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_clone_leaves_no_folder() {
        let dir = temp("clone");
        // a source that isn't there: git fails at once, with no network
        let dest = dir.join("gone");
        let err = clone_into(&dir.join("no-such-repo").to_string_lossy(), &dest).unwrap_err();
        assert!(err.starts_with("복제하지 못했어요\n"), "{err}");
        assert!(!dest.exists());
        // a folder already there is refused untouched
        std::fs::create_dir(dir.join("kept")).unwrap();
        std::fs::write(dir.join("kept/a.txt"), "a").unwrap();
        assert!(clone("https://host/kept.git", &dir.to_string_lossy(), "").unwrap_err().contains("이미 있는"));
        assert!(dir.join("kept/a.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clone_into_copies_the_repo() {
        let dir = temp("cloned");
        let src = dir.join("src");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("a.txt"), "a").unwrap();
        worktree::init_repo(&src.to_string_lossy()).unwrap();
        let made = clone_into(&src.to_string_lossy(), &dir.join("copy")).unwrap();
        assert_eq!(worktree::repo_state(&made), "ready");
        assert!(dir.join("copy/a.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
