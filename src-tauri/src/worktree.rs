use std::path::{Path, PathBuf};
use std::process::Command;

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    git_raw(dir, args).map(|s| s.trim_end().to_string())
}

fn git_raw(dir: &Path, args: &[&str]) -> Result<String, String> {
    git_env(dir, &[], args)
}

fn git_env(dir: &Path, env: &[(&str, &Path)], args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(dir).args(args);
    cmd.envs(env.iter().copied());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flash from a GUI app
    }
    let out = cmd.output().map_err(|e| format!("git 실행 실패: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// One path spelling everywhere (git prints forward slashes, PathBuf::join adds backslashes on Windows).
pub fn norm(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// Repo root of any path inside a git working tree.
pub fn toplevel(path: &str) -> Result<String, String> {
    git(Path::new(path), &["rev-parse", "--show-toplevel"]).map_err(|e| format!("Git 저장소가 아닙니다: {e}"))
}

/// Whether a picked folder can be a project as is: "ready", "empty" (a repo with no commit yet, so no worktree
/// can branch off it) or "none" (not inside a git repo).
pub fn repo_state(path: &str) -> &'static str {
    match toplevel(path) {
        Err(_) => "none",
        Ok(root) if git(Path::new(&root), &["rev-parse", "--verify", "HEAD"]).is_err() => "empty",
        Ok(_) => "ready",
    }
}

/// Makes a folder usable as a project: `git init` (branch main) if it isn't a repo, and a first commit of what's
/// there if there's none yet. Without a git identity the commit is signed as Maestro, so a fresh machine works too.
pub fn init_repo(path: &str) -> Result<(), String> {
    if toplevel(path).is_err() {
        git(Path::new(path), &["init", "-b", "main"]).map_err(|e| format!("git init 실패: {e}"))?;
    }
    let root = toplevel(path)?;
    let dir = Path::new(&root);
    if git(dir, &["rev-parse", "--verify", "HEAD"]).is_ok() {
        return Ok(());
    }
    git(dir, &["add", "-A"]).map_err(|e| format!("git add 실패: {e}"))?;
    let commit = ["commit", "--allow-empty", "-m", "Initial commit"];
    git(dir, &commit)
        .or_else(|_| git(dir, &[&["-c", "user.name=Maestro", "-c", "user.email=maestro@localhost"][..], &commit[..]].concat()))
        .map(|_| ())
        .map_err(|e| format!("첫 커밋 실패: {e}"))
}

/// origin's default branch, else the current branch, else HEAD.
pub fn default_branch(root: &str) -> String {
    let root = Path::new(root);
    git(root, &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"])
        .map(|s| s.trim_start_matches("origin/").to_string())
        .or_else(|_| git(root, &["symbolic-ref", "--short", "HEAD"]))
        .unwrap_or_else(|_| "HEAD".into())
}

/// FNV-1a: stable across Rust versions, unlike DefaultHasher, so the project dir never moves.
fn fnv1a(s: &str) -> u32 {
    s.bytes().fold(0x811c_9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193))
}

fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect()
}

/// ~/.maestro/projects/<name>_<hash>
pub fn project_dir(home: &Path, root: &str) -> PathBuf {
    let name = Path::new(root).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    home.join(".maestro").join("projects").join(format!("{}_{:08x}", sanitize(&name), fnv1a(root)))
}

/// The branch the project folder has checked out.
pub fn current_branch(root: &str) -> String {
    git(Path::new(root), &["symbolic-ref", "--short", "HEAD"]).unwrap_or_else(|_| "HEAD".into())
}

/// A branch a worktree can start from or check out.
#[derive(serde::Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    pub name: String,
    /// only on a remote: checking it out makes a local tracking branch
    pub remote: bool,
    /// checked out in the project folder or a worktree: git won't check it out a second time
    pub in_use: bool,
}

/// Local branches, then remote-only ones by their short name (`origin/feat` → `feat`).
pub fn branches(root: &str) -> Result<Vec<Branch>, String> {
    let dir = Path::new(root);
    let local = git(dir, &["for-each-ref", "--format=%(refname:short)", "refs/heads"])?;
    let remote = git(dir, &["for-each-ref", "--format=%(refname:short)", "refs/remotes"])?;
    let in_use: Vec<String> = parse_worktree_list(&git(dir, &["worktree", "list", "--porcelain"])?).into_iter().map(|(_, b)| b).collect();
    Ok(merge_branches(&local, &remote, &in_use))
}

fn merge_branches(local: &str, remote: &str, in_use: &[String]) -> Vec<Branch> {
    let mut out: Vec<Branch> = local.lines().filter(|l| !l.is_empty()).map(|n| Branch { name: n.into(), remote: false, in_use: false }).collect();
    for r in remote.lines() {
        // `origin/HEAD` (shown as `origin` by newer git) is a pointer, not a branch
        let Some((_, name)) = r.split_once('/') else { continue };
        if name != "HEAD" && !out.iter().any(|b| b.name == name) {
            out.push(Branch { name: name.into(), remote: true, in_use: false });
        }
    }
    for b in &mut out {
        b.in_use = in_use.contains(&b.name);
    }
    out
}

/// A worktree in Maestro's project dir. `existing`: check out that branch (a remote-only one becomes a
/// local tracking branch, git's default). Otherwise a new `branch` from `base`, adding -2, -3 … when taken.
pub fn create(root: &str, project_dir: &Path, branch: &str, base: &str, existing: bool) -> Result<(PathBuf, String), String> {
    let root = Path::new(root);
    for n in 1..100 {
        let cand = if n == 1 { branch.to_string() } else { format!("{branch}-{n}") };
        let path = project_dir.join("worktrees").join(sanitize(&cand));
        let taken = !existing && git(root, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{cand}")]).is_ok();
        if taken || path.exists() {
            continue;
        }
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        if existing {
            git(root, &["worktree", "add", &path.to_string_lossy(), branch])?;
            return Ok((path, branch.to_string()));
        }
        git(root, &["worktree", "add", "-b", &cand, &path.to_string_lossy(), base])?;
        return Ok((path, cand));
    }
    Err(format!("사용 가능한 브랜치 이름이 없습니다: {branch}"))
}

/// Maestro-managed worktrees of `root` as (path, branch).
/// Every linked worktree of the repo (path, branch): Maestro's own and any made elsewhere (git, other
/// tools). The main checkout is left out (it's the project folder, where the Maestro runs), and so are
/// entries whose folder is gone (`prunable`).
pub fn list(root: &str) -> Result<Vec<(String, String)>, String> {
    let out = git(Path::new(root), &["worktree", "list", "--porcelain"])?;
    let root = norm(Path::new(root)).to_lowercase();
    Ok(parse_worktree_list(&out).into_iter().filter(|(p, _)| p.to_lowercase() != root).collect())
}

/// The main checkout a worktree (or the main checkout itself) belongs to: the parent of the shared `.git`.
pub fn main_checkout(path: &str) -> Option<String> {
    let common = git(Path::new(path), &["rev-parse", "--path-format=absolute", "--git-common-dir"]).ok()?;
    Path::new(common.trim()).parent().map(norm)
}

/// Made by Maestro (under its project dir) rather than found in the repo.
pub fn is_managed(path: &str, project_dir: &Path) -> bool {
    path.replace('\\', "/").to_lowercase().starts_with(&norm(&project_dir.join("worktrees")).to_lowercase())
}

/// Stable short id for a path (meta file names for worktrees Maestro didn't create).
pub fn path_id(path: &str) -> String {
    format!("{:08x}", fnv1a(&path.replace('\\', "/").to_lowercase()) as u32)
}

fn parse_worktree_list(out: &str) -> Vec<(String, String)> {
    out.replace("\r\n", "\n")
        .split("\n\n")
        .filter_map(|block| {
            let mut path = None;
            let mut branch = None;
            let mut head = "";
            for line in block.lines() {
                if let Some(p) = line.strip_prefix("worktree ") {
                    path = Some(p.replace('\\', "/"));
                } else if let Some(b) = line.strip_prefix("branch refs/heads/") {
                    branch = Some(b.to_string());
                } else if let Some(h) = line.strip_prefix("HEAD ") {
                    head = h;
                } else if line.starts_with("prunable") {
                    return None; // its folder was deleted outside git
                }
            }
            let branch = branch.unwrap_or_else(|| format!("(detached {})", &head[..head.len().min(7)]));
            path.map(|p| (p, branch))
        })
        .collect()
}

/// What discarding a worktree would lose: files not committed (untracked ones too) and commits of its branch
/// that `base` lacks. Ones `base` has an equivalent of (cherry-picked, rebased, squash-merged) count as merged.
#[derive(serde::Serialize, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiscardCheck {
    /// the branch the commits were compared with
    pub base: String,
    /// `base` as a commit, so the worktree resolves it the same as the project folder
    #[serde(skip)]
    base_commit: String,
    /// `git status --porcelain` lines ("XY path")
    pub changes: Vec<String>,
    /// "abc1234 subject", oldest first
    pub commits: Vec<String>,
    /// the worktree checked out a branch that existed before: the branch stays whatever happens
    pub keep_branch: bool,
}

impl DiscardCheck {
    pub fn clean(&self) -> bool {
        self.changes.is_empty() && self.commits.is_empty()
    }
}

/// How to discard: `Clean` only when nothing would be lost, `Keep` the work on its branch, `Discard` it
/// (a backup ref stays).
#[derive(serde::Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DiscardChoice {
    Clean,
    Keep,
    Discard,
}

/// Where trashed work goes: `git branch <name> refs/maestro/trash/<…>` brings it back.
const TRASH: &str = "refs/maestro/trash";

pub fn check_discard(root: &str, path: &str, branch: &str, base: &str, keep_branch: bool) -> Result<DiscardCheck, String> {
    let dir = Path::new(root);
    let changes = status(path)?.into_iter().map(|(code, file)| format!("{code} {file}")).collect();
    // a base that no longer resolves (deleted, or a repo without a default branch): the project folder's HEAD
    let (base, base_commit) = match git(dir, &["rev-parse", "--verify", "--quiet", &format!("{base}^{{commit}}")]) {
        Ok(c) => (base.to_string(), c),
        Err(_) => (current_branch(root), git(dir, &["rev-parse", "HEAD"])?),
    };
    let stays = keep_branch && has_branch(dir, branch);
    let goes = has_branch(dir, branch) && !stays;
    // `branch` is what the canvas last saw; the agent may have switched the worktree since. HEAD's commits are
    // lost unless a branch that stays has them (a detached worktree's go with its folder, even with keep_branch)
    let head = head_branch(path);
    let mut commits = match head.as_deref() {
        Some(b) if b != branch || !goes => vec![],
        _ => unmerged(Path::new(path), &base_commit, "HEAD")?,
    };
    // ...and a deleted branch HEAD has moved off loses its own
    if goes && head.as_deref() != Some(branch) {
        for c in unmerged(Path::new(path), &base_commit, &format!("refs/heads/{branch}"))? {
            if !commits.contains(&c) {
                commits.push(c);
            }
        }
    }
    Ok(DiscardCheck { base, base_commit, changes, commits, keep_branch: stays })
}

/// The branch a worktree has checked out now; None when detached.
fn head_branch(path: &str) -> Option<String> {
    git(Path::new(path), &["symbolic-ref", "-q", "HEAD"]).ok()?.strip_prefix("refs/heads/").map(str::to_string)
}

/// Commits on `tip` that `base` lacks, as "abc1234 subject". `git cherry` leaves out ones with an equivalent
/// patch in `base`; a branch squash-merged as a whole (its combined change, as one commit on the merge base,
/// has an equivalent in `base`) has none left.
fn unmerged(dir: &Path, base: &str, tip: &str) -> Result<Vec<String>, String> {
    if git(dir, &["rev-list", "--count", &format!("{base}..{tip}")])? == "0" {
        return Ok(vec![]);
    }
    let left: Vec<String> = git(dir, &["cherry", "-v", base, tip])?
        .lines()
        .filter_map(|l| l.strip_prefix("+ "))
        .map(|l| match l.split_once(' ') {
            Some((sha, subject)) => format!("{} {subject}", &sha[..sha.len().min(7)]),
            None => l[..l.len().min(7)].to_string(),
        })
        .collect();
    if left.is_empty() {
        return Ok(left);
    }
    // squash-merged (or merged in pieces): merging it into `base` would change nothing (git 2.38+); older git
    // (no `merge-tree --write-tree`) or a conflict: its combined change as one commit, checked with `git cherry`
    let squashed = match git(dir, &["merge-tree", "--write-tree", base, tip]) {
        Ok(out) => out.lines().next() == git(dir, &["rev-parse", &format!("{base}^{{tree}}")]).ok().as_deref(),
        Err(_) => git(dir, &["merge-base", base, tip])
            .and_then(|mb| commit_tree(dir, &[&format!("{tip}^{{tree}}"), "-p", &mb, "-m", "maestro squash check"]))
            .and_then(|c| git(dir, &["cherry", base, &c]))
            .is_ok_and(|out| out.starts_with('-')),
    };
    Ok(if squashed { vec![] } else { left })
}

fn has_branch(root: &Path, branch: &str) -> bool {
    git(root, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).is_ok()
}

/// `git commit-tree`, signed as Maestro when there's no git identity (as `init_repo` does).
fn commit_tree(dir: &Path, args: &[&str]) -> Result<String, String> {
    let a = [&["commit-tree"][..], args].concat();
    git(dir, &a).or_else(|_| git(dir, &[&["-c", "user.name=Maestro", "-c", "user.email=maestro@localhost"][..], &a[..]].concat()))
}

/// A commit of everything in the folder that isn't committed (untracked files too, ignored ones not) on top of
/// HEAD, made through a throwaway index so the worktree's own stays as it was. None when nothing is uncommitted.
fn snapshot(path: &str, message: &str) -> Result<Option<String>, String> {
    if status(path)?.is_empty() {
        return Ok(None);
    }
    let dir = Path::new(path);
    let index = std::env::temp_dir().join(format!("maestro-index-{:08x}-{}", fnv1a(path), std::process::id()));
    let env = [("GIT_INDEX_FILE", index.as_path())];
    let tree = git_env(dir, &env, &["read-tree", "HEAD"])
        .and_then(|_| git_env(dir, &env, &["add", "-A"]))
        .and_then(|_| git_env(dir, &env, &["write-tree"]));
    let _ = std::fs::remove_file(&index);
    commit_tree(dir, &[tree?.trim(), "-p", "HEAD", "-m", message]).map(Some)
}

/// Points `refs/maestro/trash/<branch>` (-2, -3 … when taken) at `commit` and returns that ref.
fn backup(root: &Path, branch: &str, commit: &str) -> Result<String, String> {
    for n in 1..1000 {
        let name = if n == 1 { format!("{TRASH}/{}", sanitize(branch)) } else { format!("{TRASH}/{}-{n}", sanitize(branch)) };
        if git(root, &["rev-parse", "--verify", "--quiet", &name]).is_err() {
            git(root, &["update-ref", &name, commit])?;
            return Ok(name);
        }
    }
    Err(format!("백업 ref 이름이 없습니다: {branch}"))
}

/// Removes a Maestro-made worktree folder; returns where its work went (the kept branch or a backup ref),
/// None when nothing needed saving.
/// - `Clean`: refused unless `check_discard` finds nothing to lose; plain `git worktree remove` and `git branch -d`.
///   When `-d` won't (it only knows merges into HEAD), an ancestor of `base` goes with `-D`, a squash-merged
///   branch is backed up first.
/// - `Keep`: uncommitted work becomes a commit on the branch, the branch stays, only the folder goes.
/// - `Discard`: the branch tip, with uncommitted work on top, is saved as a backup ref, then folder and branch go.
///
/// `keep_branch` (it checked out a branch that existed before): the branch is never deleted.
pub fn discard(root: &str, path: &str, branch: &str, base: &str, keep_branch: bool, choice: DiscardChoice) -> Result<Option<String>, String> {
    let dir = Path::new(root);
    let named = has_branch(dir, branch);
    let delete_branch = named && !keep_branch;
    match choice {
        DiscardChoice::Clean => {
            let check = check_discard(root, path, branch, base, keep_branch)?;
            if !check.clean() {
                return Err("이 워크트리에 아직 지우면 잃는 작업이 있어요. 다시 폐기를 눌러 확인해 주세요".into());
            }
            git(dir, &["worktree", "remove", path])?;
            if !delete_branch || git(dir, &["branch", "-d", branch]).is_ok() {
                return Ok(None);
            }
            let tip = git(dir, &["rev-parse", &format!("refs/heads/{branch}")])?;
            let saved = if git(dir, &["merge-base", "--is-ancestor", &tip, &check.base_commit]).is_ok() { None } else { Some(backup(dir, branch, &tip)?) };
            git(dir, &["branch", "-D", branch])?;
            Ok(saved)
        }
        DiscardChoice::Keep => {
            // on the branch the worktree has now, which may not be `branch` (the agent switched)
            let head = head_branch(path);
            let snap = snapshot(path, &format!("maestro: 커밋하지 않은 작업 보관 ({})", head.as_deref().unwrap_or(branch)))?;
            let saved = match (head, snap) {
                (Some(b), Some(c)) => {
                    // moves the branch only if it is still where the snapshot sits on
                    let parent = git(dir, &["rev-parse", &format!("{c}^")])?;
                    git(dir, &["update-ref", &format!("refs/heads/{b}"), &c, &parent])?;
                    b
                }
                (Some(b), None) => b,
                // detached: no branch to keep it on
                (None, snap) => {
                    let tip = match snap {
                        Some(c) => c,
                        None => git(Path::new(path), &["rev-parse", "HEAD"])?,
                    };
                    backup(dir, branch, &tip)?
                }
            };
            git(dir, &["worktree", "remove", "--force", path])?;
            Ok(Some(saved))
        }
        DiscardChoice::Discard => {
            let tip = match snapshot(path, &format!("maestro: 버린 워크트리의 커밋하지 않은 작업 ({branch})"))? {
                Some(c) => c,
                None => git(Path::new(path), &["rev-parse", "HEAD"])?,
            };
            let mut saved = backup(dir, branch, &tip)?;
            // HEAD may have moved off the branch (the agent switched): what only the branch has gets its own backup
            let own = if delete_branch { Some(git(dir, &["rev-parse", &format!("refs/heads/{branch}")])?) } else { None };
            if let Some(own) = own.filter(|own| git(dir, &["merge-base", "--is-ancestor", own, &tip]).is_err()) {
                saved = format!("{saved}, {}", backup(dir, branch, &own)?);
            }
            git(dir, &["worktree", "remove", "--force", path])?;
            if delete_branch {
                git(dir, &["branch", "-D", branch])?;
            }
            Ok(Some(saved))
        }
    }
}

/// `git status --porcelain` as (XY code, path).
pub fn status(path: &str) -> Result<Vec<(String, String)>, String> {
    let out = git(Path::new(path), &["status", "--porcelain=v1", "-uall"])?;
    Ok(out
        .lines()
        .filter(|l| l.len() > 3)
        .map(|l| {
            let file = l[3..].rsplit(" -> ").next().unwrap_or(&l[3..]).trim_matches('"');
            (l[..2].to_string(), file.to_string())
        })
        .collect())
}

/// A file's content at HEAD (`rel` relative to `path`); Err when it isn't in HEAD.
pub fn show_head(path: &str, rel: &str) -> Result<String, String> {
    // untrimmed: a dropped trailing newline would show as a change in the diff
    git_raw(Path::new(path), &["show", &format!("HEAD:./{}", rel.replace('\\', "/"))])
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_makes_a_plain_folder_a_project() {
        let dir = std::env::temp_dir().join(format!("maestro-init-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "hi").unwrap();
        let p = dir.to_string_lossy().to_string();
        assert_eq!(repo_state(&p), "none");
        init_repo(&p).unwrap();
        assert_eq!(repo_state(&p), "ready");
        // the files there are in the first commit, so worktrees start with them
        assert!(git(&dir, &["ls-files"]).unwrap().contains("a.txt"));
        init_repo(&p).unwrap(); // again: nothing to do
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A repo (branch main, one commit) under a fresh temp folder, with a worktree on new branch `wt`.
    fn repo_with_worktree(tag: &str) -> (PathBuf, String, String) {
        let dir = std::env::temp_dir().join(format!("maestro-discard-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("repo")).unwrap();
        let root = norm(&dir.join("repo"));
        std::fs::write(dir.join("repo/a.txt"), "a\n").unwrap();
        commit_all(&root, "init");
        let wt = norm(&dir.join("wt"));
        git(Path::new(&root), &["worktree", "add", "-b", "wt", &wt, "main"]).unwrap();
        (dir, root, wt)
    }

    fn commit_all(dir: &str, message: &str) {
        let d = Path::new(dir);
        if git(d, &["rev-parse", "--git-dir"]).is_err() {
            git(d, &["init", "-b", "main"]).unwrap();
        }
        git(d, &["add", "-A"]).unwrap();
        git(d, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-m", message]).unwrap();
    }

    #[test]
    fn discard_check_verdicts() {
        let (dir, root, wt) = repo_with_worktree("check");
        let check = || check_discard(&root, &wt, "wt", "main", false).unwrap();
        assert!(check().clean());
        // untracked file
        std::fs::write(Path::new(&wt).join("new.txt"), "x").unwrap();
        assert_eq!(check().changes, ["?? new.txt"]);
        assert!(check().commits.is_empty());
        // a commit main lacks
        commit_all(&wt, "feature one");
        let c = check();
        assert!(c.changes.is_empty());
        assert_eq!(c.commits.len(), 1);
        assert!(c.commits[0].ends_with(" feature one"));
        // ...unless the branch stays (an existing branch checked out)
        assert!(check_discard(&root, &wt, "wt", "main", true).unwrap().clean());
        // cherry-picked onto main: an equivalent patch counts as merged
        let sha = git(Path::new(&wt), &["rev-parse", "HEAD"]).unwrap();
        git(Path::new(&root), &["-c", "user.name=t", "-c", "user.email=t@t", "cherry-pick", &sha]).unwrap();
        assert!(check().clean());
        // two more commits, squash-merged into main as one
        std::fs::write(Path::new(&wt).join("b.txt"), "b\n").unwrap();
        commit_all(&wt, "two");
        std::fs::write(Path::new(&wt).join("c.txt"), "c\n").unwrap();
        commit_all(&wt, "three");
        assert_eq!(check().commits.len(), 2);
        git(Path::new(&root), &["merge", "--squash", "wt"]).unwrap();
        git(Path::new(&root), &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-m", "squash"]).unwrap();
        assert!(check().clean());
        // a base that doesn't resolve falls back to the project folder's branch
        assert_eq!(check_discard(&root, &wt, "wt", "gone", false).unwrap().base, "main");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clean_discard_removes_folder_and_branch_only_when_nothing_is_lost() {
        let (dir, root, wt) = repo_with_worktree("clean");
        std::fs::write(Path::new(&wt).join("new.txt"), "x").unwrap();
        assert!(discard(&root, &wt, "wt", "main", false, DiscardChoice::Clean).is_err());
        assert!(Path::new(&wt).join("new.txt").exists(), "refused: nothing touched");
        std::fs::remove_file(Path::new(&wt).join("new.txt")).unwrap();
        assert_eq!(discard(&root, &wt, "wt", "main", false, DiscardChoice::Clean).unwrap(), None);
        assert!(!Path::new(&wt).exists());
        assert!(!has_branch(Path::new(&root), "wt"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn discard_leaves_a_backup_ref_with_uncommitted_work() {
        let (dir, root, wt) = repo_with_worktree("backup");
        std::fs::write(Path::new(&wt).join("b.txt"), "committed\n").unwrap();
        commit_all(&wt, "work");
        let tip = git(Path::new(&wt), &["rev-parse", "HEAD"]).unwrap();
        std::fs::write(Path::new(&wt).join("a.txt"), "edited\n").unwrap();
        std::fs::write(Path::new(&wt).join("untracked.txt"), "loose\n").unwrap();
        let saved = discard(&root, &wt, "wt", "main", false, DiscardChoice::Discard).unwrap().unwrap();
        assert_eq!(saved, "refs/maestro/trash/wt");
        assert!(!Path::new(&wt).exists() && !has_branch(Path::new(&root), "wt"));
        let r = Path::new(&root);
        // the backup is the branch tip plus a commit of the uncommitted files
        assert_eq!(git(r, &["rev-parse", &format!("{saved}^")]).unwrap(), tip);
        assert_eq!(git(r, &["show", &format!("{saved}:a.txt")]).unwrap(), "edited");
        assert_eq!(git(r, &["show", &format!("{saved}:untracked.txt")]).unwrap(), "loose");
        // a second discard of the same branch name doesn't overwrite the first backup
        git(r, &["worktree", "add", "-b", "wt", &wt, "main"]).unwrap();
        std::fs::write(Path::new(&wt).join("x.txt"), "x").unwrap();
        assert_eq!(discard(&root, &wt, "wt", "main", false, DiscardChoice::Discard).unwrap().unwrap(), "refs/maestro/trash/wt-2");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keep_commits_uncommitted_work_to_the_branch() {
        let (dir, root, wt) = repo_with_worktree("keep");
        std::fs::write(Path::new(&wt).join("untracked.txt"), "loose\n").unwrap();
        assert_eq!(discard(&root, &wt, "wt", "main", false, DiscardChoice::Keep).unwrap().as_deref(), Some("wt"));
        assert!(!Path::new(&wt).exists());
        assert_eq!(git(Path::new(&root), &["show", "wt:untracked.txt"]).unwrap(), "loose");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn discard_follows_a_branch_switch_inside_the_worktree() {
        // the agent commits on `wt`, then switches to a new branch: the canvas still says `wt`
        let (dir, root, wt) = repo_with_worktree("switch");
        let (r, w) = (Path::new(&root), Path::new(&wt));
        std::fs::write(w.join("b.txt"), "b\n").unwrap();
        commit_all(&wt, "on wt");
        let on_wt = git(w, &["rev-parse", "HEAD"]).unwrap();
        git(w, &["switch", "-c", "other", "main"]).unwrap();
        // `wt` goes with the folder, so its commit counts; `other` stays, so HEAD's don't
        let c = check_discard(&root, &wt, "wt", "main", false).unwrap();
        assert_eq!(c.commits.len(), 1);
        assert!(c.commits[0].ends_with(" on wt"));
        assert!(discard(&root, &wt, "wt", "main", false, DiscardChoice::Clean).is_err());
        // keep: the uncommitted work goes on `other`, `wt` isn't moved
        std::fs::write(w.join("loose.txt"), "loose\n").unwrap();
        assert_eq!(discard(&root, &wt, "wt", "main", false, DiscardChoice::Keep).unwrap().as_deref(), Some("other"));
        assert_eq!(git(r, &["show", "other:loose.txt"]).unwrap(), "loose");
        assert_eq!(git(r, &["rev-parse", "wt"]).unwrap(), on_wt);
        // discard: `wt`'s commit, not in HEAD's backup, gets one of its own before `wt` is deleted
        git(r, &["worktree", "add", &wt, "wt"]).unwrap();
        git(w, &["switch", "--detach", "main"]).unwrap();
        let saved = discard(&root, &wt, "wt", "main", false, DiscardChoice::Discard).unwrap().unwrap();
        assert_eq!(saved, "refs/maestro/trash/wt, refs/maestro/trash/wt-2");
        assert_eq!(git(r, &["rev-parse", "refs/maestro/trash/wt-2"]).unwrap(), on_wt);
        assert!(!has_branch(r, "wt"));
        // a detached worktree's commits count even when its branch stays
        git(r, &["worktree", "add", "-b", "kept", &wt, "main"]).unwrap();
        git(w, &["switch", "--detach"]).unwrap();
        std::fs::write(w.join("c.txt"), "c\n").unwrap();
        commit_all(&wt, "detached");
        let c = check_discard(&root, &wt, "kept", "main", true).unwrap();
        assert!(c.keep_branch && c.commits.len() == 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn project_dir_is_stable_and_safe() {
        let a = project_dir(Path::new("/h"), "C:/dev/my app");
        assert_eq!(a, project_dir(Path::new("/h"), "C:/dev/my app"));
        assert!(a.to_string_lossy().contains("my_app_"));
        assert_ne!(a, project_dir(Path::new("/h"), "D:/dev/my app"));
    }

    #[test]
    fn merges_local_and_remote_branches() {
        let b = merge_branches("main\ndev\n", "origin\norigin/HEAD\norigin/dev\norigin/feat/x\nupstream/fix", &["main".into()]);
        let names: Vec<(&str, bool, bool)> = b.iter().map(|b| (b.name.as_str(), b.remote, b.in_use)).collect();
        assert_eq!(names, [("main", false, true), ("dev", false, false), ("feat/x", true, false), ("fix", true, false)]);
    }

    #[test]
    fn parses_porcelain_worktree_list() {
        let out = "worktree C:/dev/app\nHEAD abc\nbranch refs/heads/main\n\nworktree C:/h/.maestro/wt/a\nHEAD def\nbranch refs/heads/feat/x\n\nworktree C:/h/d\nHEAD 1352154abc\ndetached\n\nworktree C:/gone\nHEAD 999\nbranch refs/heads/old\nprunable gitdir file points to non-existent location\n";
        assert_eq!(
            parse_worktree_list(out),
            vec![
                ("C:/dev/app".into(), "main".into()),
                ("C:/h/.maestro/wt/a".into(), "feat/x".into()),
                ("C:/h/d".into(), "(detached 1352154)".into()),
            ]
        );
        assert!(is_managed("C:\\h\\.maestro\\projects\\p_1\\worktrees\\agent-1", Path::new("C:/h/.maestro/projects/p_1")));
        assert!(!is_managed("C:/Users/u/Desktop/claude-1-1", Path::new("C:/h/.maestro/projects/p_1")));
        assert_eq!(path_id("C:\\A\\b"), path_id("c:/a/B"));
        // this crate's own checkout resolves to itself
        let here = env!("CARGO_MANIFEST_DIR");
        let main = main_checkout(here).unwrap();
        assert!(norm(Path::new(here)).to_lowercase().starts_with(&main.to_lowercase()));
    }
}
