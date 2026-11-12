use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    git_raw(dir, args).map(|s| s.trim_end().to_string())
}

fn git_raw(dir: &Path, args: &[&str]) -> Result<String, String> {
    git_env(dir, &[], args)
}

fn git_env(dir: &Path, env: &[(&str, &OsStr)], args: &[&str]) -> Result<String, String> {
    match git_out(dir, env, args)? {
        (0, out, _) => Ok(out),
        (_, _, err) => Err(err.trim().to_string()),
    }
}

/// git's exit code, stdout and stderr. For commands that report by exit code (`merge-tree`, `merge`: 1 = conflict):
/// stderr is localized and never parsed.
fn git_out(dir: &Path, env: &[(&str, &OsStr)], args: &[&str]) -> Result<(i32, String, String), String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(dir).args(args);
    cmd.envs(env.iter().copied());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flash from a GUI app
    }
    let out = cmd.output().map_err(|e| format!("git 실행 실패: {e}"))?;
    Ok((out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned()))
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
    /// its PR was merged (R6): `{ number, method }`, filled in by the command
    pub merged_by_pr: Option<serde_json::Value>,
    /// a discard with nothing to lose still keeps a backup ref of the branch (`git branch -d` refuses it and the
    /// base lacks its tip, as after a squash). Looked at only for a merged PR's branch (`merged_heads` given).
    pub backup: bool,
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

/// `merged_heads`: heads of the worker's PRs the app saw merged (R6). Commits up to one of them count as merged
/// whatever the method (a squash leaves no trace `git cherry` can see) and even before the local base has them.
pub fn check_discard(root: &str, path: &str, branch: &str, base: &str, keep_branch: bool, merged_heads: &[String]) -> Result<DiscardCheck, String> {
    let dir = Path::new(root);
    let changes = status(path)?.into_iter().map(|(code, file)| format!("{code} {file}")).collect();
    // a base that no longer resolves (deleted, or a repo without a default branch): the project folder's HEAD
    let (base, base_commit) = match git(dir, &["rev-parse", "--verify", "--quiet", &format!("{base}^{{commit}}")]) {
        Ok(c) => (base.to_string(), c),
        Err(_) => (current_branch(root), git(dir, &["rev-parse", "HEAD"])?),
    };
    let exists = has_branch(dir, branch); // one git call: each costs ~0.5 s where a scanner holds process starts
    let stays = keep_branch && exists;
    let goes = exists && !stays;
    // `branch` is what the canvas last saw; the agent may have switched the worktree since. HEAD's commits are
    // lost unless a branch that stays has them (a detached worktree's go with its folder, even with keep_branch)
    let head = head_branch(path);
    let mut commits = match head.as_deref() {
        Some(b) if b != branch || !goes => vec![],
        _ => unmerged(Path::new(path), &base_commit, "HEAD", merged_heads)?,
    };
    // ...and a deleted branch HEAD has moved off loses its own
    if goes && head.as_deref() != Some(branch) {
        for c in unmerged(Path::new(path), &base_commit, &format!("refs/heads/{branch}"), merged_heads)? {
            if !commits.contains(&c) {
                commits.push(c);
            }
        }
    }
    let backup = goes && !merged_heads.is_empty() && needs_backup(dir, branch, &base_commit);
    Ok(DiscardCheck { base, base_commit, changes, commits, keep_branch: stays, merged_by_pr: None, backup })
}

/// Whether a clean discard backs `branch` up before `-D`: `git branch -d` refuses it (its tip isn't in its upstream,
/// or in the project folder's HEAD when it has none) and `base` lacks the tip.
fn needs_backup(dir: &Path, branch: &str, base_commit: &str) -> bool {
    let Ok(tip) = git(dir, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]) else { return false };
    let into = git(dir, &["rev-parse", "--verify", "--quiet", &format!("{branch}@{{upstream}}")]).or_else(|_| git(dir, &["rev-parse", "HEAD"]));
    let deletes = into.is_ok_and(|r| git(dir, &["merge-base", "--is-ancestor", &tip, &r]).is_ok());
    !deletes && git(dir, &["merge-base", "--is-ancestor", &tip, base_commit]).is_err()
}

/// The branch a worktree has checked out now; None when detached.
pub(crate) fn head_branch(path: &str) -> Option<String> {
    git(Path::new(path), &["symbolic-ref", "-q", "HEAD"]).ok()?.strip_prefix("refs/heads/").map(str::to_string)
}

/// Commits on `tip` that `base` lacks, as "abc1234 subject". `git cherry` leaves out ones with an equivalent
/// patch in `base`; a branch squash-merged as a whole (its combined change, as one commit on the merge base,
/// has an equivalent in `base`) has none left. Commits reachable from one of `merged_heads` (the heads of PRs
/// that were merged, R6) are merged too; heads this repo doesn't have are ignored.
fn unmerged(dir: &Path, base: &str, tip: &str, merged_heads: &[String]) -> Result<Vec<String>, String> {
    let heads: Vec<String> = merged_heads.iter().filter(|h| git(dir, &["cat-file", "-e", &format!("{h}^{{commit}}")]).is_ok()).map(|h| format!("^{h}")).collect();
    let mut count = vec!["rev-list".to_string(), "--count".into(), tip.into(), format!("^{base}")];
    count.extend(heads.iter().cloned());
    if git(dir, &count.iter().map(String::as_str).collect::<Vec<_>>())? == "0" {
        return Ok(vec![]);
    }
    // what the merged PR heads don't cover
    let after: Option<std::collections::HashSet<String>> = if heads.is_empty() {
        None
    } else {
        let mut a = vec!["rev-list".to_string(), tip.into()];
        a.extend(heads.iter().cloned());
        Some(git(dir, &a.iter().map(String::as_str).collect::<Vec<_>>())?.lines().map(String::from).collect())
    };
    let left: Vec<String> = git(dir, &["cherry", "-v", base, tip])?
        .lines()
        .filter_map(|l| l.strip_prefix("+ "))
        .filter(|l| after.as_ref().is_none_or(|a| a.contains(l.split(' ').next().unwrap_or(""))))
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
    let env = [("GIT_INDEX_FILE", index.as_os_str())];
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
///
/// `merged_heads`: as `check_discard` takes them (a squash-merged PR's branch goes the backup + `-D` way).
pub fn discard(root: &str, path: &str, branch: &str, base: &str, keep_branch: bool, choice: DiscardChoice, merged_heads: &[String]) -> Result<Option<String>, String> {
    let dir = Path::new(root);
    let named = has_branch(dir, branch);
    let delete_branch = named && !keep_branch;
    match choice {
        DiscardChoice::Clean => {
            let check = check_discard(root, path, branch, base, keep_branch, merged_heads)?;
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

// ---------- merge requests (merge.rs): planned when the conductor asks, merged after the user approves ----------

/// A file a merge request changes: git's name-status letter (A M D R T), its path (and the old one for a rename),
/// lines added and removed (None: binary).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    pub code: String,
    pub path: String,
    pub old_path: Option<String>,
    pub add: Option<u32>,
    pub del: Option<u32>,
}

/// What a merge request would merge, as git saw it when the conductor asked. The approval merges exactly this
/// (`tip_sha` into `base` while it is still `base_sha`), never whatever the branch names by then.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MergePlan {
    pub branch: String,
    pub base: String,
    pub base_sha: String,
    pub tip_sha: String,
    pub merge_base: String,
    /// "abc1234 subject", oldest first, at most MAX_COMMITS (`commit_count` has them all)
    pub commits: Vec<String>,
    pub commit_count: usize,
    /// at most MAX_FILES (`file_count`, `adds` and `dels` count them all)
    pub files: Vec<ChangedFile>,
    pub file_count: usize,
    pub adds: u32,
    pub dels: u32,
    /// false: `merge-tree --write-tree` isn't there (git older than 2.38), so no conflict was predicted
    pub conflicts_checked: bool,
    /// untracked files in the worker: not committed, so not part of the merge (a warning only)
    pub worker_untracked: usize,
    /// tracked changes in the project folder when asked: a warning, checked again on approval
    pub project_dirty: Vec<String>,
    /// the project's merge mode when the conductor asked: the approval does what was planned
    #[serde(default)]
    pub mode: crate::project::MergeMode,
    /// PR mode: tracked files the worker hasn't committed (the app commits them only if the user ticks it on the card)
    #[serde(default)]
    pub worker_dirty: Vec<String>,
    /// PR mode: the remote the branch goes to
    #[serde(default)]
    pub remote: Option<String>,
    /// what the commits and conflicts were computed against: `refs/remotes/<remote>/<base>` in PR mode when it
    /// exists, else `refs/heads/<base>`
    #[serde(default)]
    pub cmp_ref: String,
    /// PR mode: the worker's open PR, which the approval updates instead of making another
    #[serde(default)]
    pub existing_pr: Option<u64>,
}

/// What `plan_merge_with` needs beyond the folders: the mode and, in PR mode, what the app knows of the worker's PRs.
#[derive(Default, Clone, Debug)]
pub struct PlanOpts {
    pub mode: crate::project::MergeMode,
    pub remote: Option<String>,
    /// heads of the worker's PRs that were merged (their commits are on the base already, whatever the method)
    pub merged_heads: Vec<String>,
    /// the worker's last PR was squash-merged: (number, the squash commit)
    pub squashed: Option<(u64, String)>,
    pub existing_pr: Option<u64>,
}

const MAX_COMMITS: usize = 100;
const MAX_FILES: usize = 500;
/// No editor, no prompt: the app merges with nobody at a terminal.
const MERGE_ENV: [(&str, &str); 3] = [("GIT_MERGE_AUTOEDIT", "no"), ("GIT_EDITOR", "true"), ("GIT_TERMINAL_PROMPT", "0")];
/// What may be under way in a repo, as `rev-parse --git-path` names it (a worktree's own gitdir too).
pub(crate) const OPERATIONS: [(&str, &str); 7] = [
    ("MERGE_HEAD", "merge"),
    ("rebase-merge", "rebase"),
    ("rebase-apply", "rebase/am"),
    ("CHERRY_PICK_HEAD", "cherry-pick"),
    ("REVERT_HEAD", "revert"),
    ("BISECT_LOG", "bisect"),
    ("index.lock", "index.lock"),
];

/// How an approved merge ended. Each is final: the request goes and the conductor gets a note.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// `commit` is the merge commit, on the base branch with the recorded base as its first parent; `later`: commits
    /// the worker's branch got after the request (not merged)
    Merged { commit: String, later: usize },
    /// the base branch moved since the request (`now` is where it is)
    Stale { now: String },
    /// the merge would conflict, or did and was aborted: the project folder is as it was
    Conflict { files: Vec<String> },
    /// git failed; `touched`: after a merge started (the folder may be mid-merge), else nothing was changed
    Failed { message: String, touched: bool },
}

/// Why an approved merge can't run now. Nothing was changed: the request stays and the user can try again.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    /// the project folder has another branch checked out
    NotOnBase(String),
    /// tracked files the project folder hasn't committed
    Dirty(Vec<String>),
    /// another git operation is under way there
    Busy(&'static str),
    /// git refused without starting (an untracked file in the way, a repo hook, a locked file): its message
    Git(String),
}

/// "a, b, c 외 N개"
pub fn short_list(items: &[String]) -> String {
    let mut s = items.iter().take(5).cloned().collect::<Vec<_>>().join(", ");
    if items.len() > 5 {
        s = format!("{s} 외 {}개", items.len() - 5);
    }
    s
}

/// `git status --porcelain=v1 -z` entries as exact paths (no quoting); a rename or copy gives both its paths.
pub(crate) fn parse_status_z(out: &str) -> Vec<String> {
    let mut paths = vec![];
    let mut it = out.split('\0').filter(|e| !e.is_empty());
    while let Some(e) = it.next() {
        if e.len() < 4 {
            continue;
        }
        let code = &e[..2];
        paths.push(e[3..].to_string());
        if code.contains('R') || code.contains('C') {
            if let Some(old) = it.next() {
                paths.push(old.to_string());
            }
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

/// Tracked changes as exact paths (`-z`: a name with spaces or non-ASCII letters comes back as it is), sorted.
pub(crate) fn tracked_paths(dir: &Path) -> Result<Vec<String>, String> {
    Ok(parse_status_z(&git_raw(dir, &["status", "--porcelain=v1", "-z", "--untracked-files=no"])?))
}

/// Tracked changes (`status` without untracked files) as paths.
fn tracked_changes(dir: &Path) -> Result<Vec<String>, String> {
    let out = git_raw(dir, &["status", "--porcelain=v1", "--untracked-files=no"])?;
    Ok(out.lines().filter(|l| l.len() > 3).map(|l| l[3..].rsplit(" -> ").next().unwrap_or(&l[3..]).trim_matches('"').to_string()).collect())
}

pub(crate) fn git_path_exists(dir: &Path, line: &str) -> bool {
    let p = Path::new(line.trim());
    if p.is_absolute() {
        p.exists()
    } else {
        dir.join(p).exists()
    }
}

/// The git operation under way in `dir`, if any (one git call for all of them).
fn in_progress(dir: &Path) -> Result<Option<&'static str>, String> {
    let mut args = vec!["rev-parse"];
    for (p, _) in OPERATIONS {
        args.extend(["--git-path", p]);
    }
    let out = git(dir, &args)?;
    Ok(OPERATIONS.iter().zip(out.lines()).find(|(_, line)| git_path_exists(dir, line)).map(|((_, what), _)| *what))
}

fn merge_head(dir: &Path) -> bool {
    git(dir, &["rev-parse", "--git-path", "MERGE_HEAD"]).is_ok_and(|p| git_path_exists(dir, &p))
}

/// `-c user.name=Maestro …` when the repo has no git identity (as `init_repo` and `commit_tree` sign), decided
/// before merging so a merge is never run twice.
fn identity_args(dir: &Path, env: &[(&str, &OsStr)]) -> Vec<&'static str> {
    let has = |k: &str| matches!(git_out(dir, env, &["config", k]), Ok((0, out, _)) if !out.trim().is_empty());
    if has("user.name") && has("user.email") {
        vec![]
    } else {
        vec!["-c", "user.name=Maestro", "-c", "user.email=maestro@localhost"]
    }
}

/// Files merging `b` into `a` would conflict on, without touching any file (git 2.38+). Some(empty): it merges
/// cleanly; None: this git can't tell.
fn conflict_precheck(dir: &Path, env: &[(&str, &OsStr)], a: &str, b: &str) -> Option<Vec<String>> {
    match git_out(dir, env, &["merge-tree", "--write-tree", "--name-only", "--no-messages", a, b]) {
        Ok((0, _, _)) => Some(vec![]),
        Ok((1, out, _)) => {
            // the merged tree's id, then the conflicted files (a blank line ends them)
            let mut files: Vec<String> = vec![];
            for l in out.lines().skip(1).take_while(|l| !l.is_empty()) {
                if !files.iter().any(|f| f == l) {
                    files.push(l.to_string());
                }
            }
            if files.is_empty() {
                files.push("(알 수 없음)".into());
            }
            Some(files)
        }
        _ => None,
    }
}

/// The tree merging `b` into `a` makes, without touching any file (git 2.38+); None: a conflict, or this git can't.
fn merged_tree(dir: &Path, env: &[(&str, &OsStr)], a: &str, b: &str) -> Option<String> {
    match git_out(dir, env, &["merge-tree", "--write-tree", "--no-messages", a, b]) {
        Ok((0, out, _)) => out.lines().next().map(|l| l.trim().to_string()).filter(|t| !t.is_empty()),
        _ => None,
    }
}

/// `diff -z --name-only` as paths.
fn names(dir: &Path, env: &[(&str, &OsStr)], args: &[&str]) -> Option<Vec<String>> {
    let mut a = vec!["diff", "--name-only", "-z", "--no-renames", "--no-ext-diff"];
    a.extend_from_slice(args);
    match git_out(dir, env, &a) {
        Ok((0, out, _)) => Some(out.split('\0').filter(|p| !p.is_empty()).map(String::from).collect()),
        _ => None,
    }
}

/// Files the merge would add that are already on disk in the project folder, though HEAD doesn't track them (the base
/// is HEAD, so a path new in `tip` isn't in HEAD): ignored files (`.env`, local config) git would overwrite without a
/// word, and untracked ones. A parent folder that's a file there counts too. What's on disk because HEAD tracks it
/// (a case-only rename on a case-insensitive disk, a tracked file that becomes a folder) is git's to replace.
fn in_the_way(dir: &Path, env: &[(&str, &OsStr)], base: &str, tip: &str) -> Result<Vec<String>, String> {
    let added = names(dir, env, &["--diff-filter=A", base, tip]).ok_or("병합할 파일 목록을 읽지 못했어요")?;
    if added.is_empty() {
        return Ok(vec![]);
    }
    let fold = |p: &str| if cfg!(any(windows, target_os = "macos")) { p.to_lowercase() } else { p.to_string() };
    let listed = match git_out(dir, env, &["ls-tree", "-r", "-z", "--name-only", "--full-tree", base]) {
        Ok((0, out, _)) => out,
        _ => return Err("기준 브랜치의 파일 목록을 읽지 못했어요".into()),
    };
    // the paths HEAD tracks and their folders, spelled the way the disk compares them
    let mut tracked = std::collections::HashSet::new();
    for f in listed.split('\0').filter(|p| !p.is_empty()) {
        let mut p = fold(f);
        tracked.insert(p.clone());
        while let Some(k) = p.rfind('/') {
            p.truncate(k);
            if !tracked.insert(p.clone()) {
                break;
            }
        }
    }
    let untracked = |rel: &Path| !tracked.contains(&fold(&rel.to_string_lossy().replace('\\', "/")));
    let on_disk = |rel: &str| std::fs::symlink_metadata(dir.join(rel)).is_ok() && untracked(Path::new(rel));
    let parent_is_file = |rel: &str| {
        let mut p = Path::new(rel).parent();
        while let Some(d) = p.filter(|d| !d.as_os_str().is_empty()) {
            if std::fs::symlink_metadata(dir.join(d)).is_ok_and(|m| !m.is_dir()) {
                return untracked(d);
            }
            p = d.parent();
        }
        false
    };
    Ok(added.into_iter().filter(|f| on_disk(f) || parent_is_file(f)).collect())
}

/// What a merge that failed before recording MERGE_HEAD left in the folder (it was clean before).
#[derive(Debug, PartialEq)]
enum Leftover {
    /// nothing: git refused before writing
    None,
    /// only the merge's own writes, all staged (a checkout cut short): `reset --merge` takes back just those
    Merge,
    /// a change that isn't the merge's (someone staged or edited meanwhile): left alone
    Foreign(Vec<String>),
    /// can't tell (no merged tree, unstaged merge writes): left alone
    Unknown,
}

fn leftover(dir: &Path, env: &[(&str, &OsStr)], p: &MergePlan) -> Leftover {
    let dirty = tracked_changes(dir).unwrap_or_default();
    if dirty.is_empty() {
        return Leftover::None;
    }
    let Some(tree) = merged_tree(dir, env, &p.base_sha, &p.tip_sha) else { return Leftover::Unknown };
    // staged vs HEAD, staged vs the merged tree, the files vs staged, the files vs the merged tree
    let (Some(staged), Some(staged_other), Some(unstaged), Some(files_other)) =
        (names(dir, env, &["--cached", "HEAD"]), names(dir, env, &["--cached", &tree]), names(dir, env, &[]), names(dir, env, &[&tree]))
    else {
        return Leftover::Unknown;
    };
    let foreign = staged.iter().any(|f| staged_other.contains(f)) || unstaged.iter().any(|f| files_other.contains(f));
    if foreign {
        Leftover::Foreign(dirty)
    } else if unstaged.is_empty() && !staged.is_empty() {
        Leftover::Merge
    } else {
        Leftover::Unknown
    }
}

/// `diff from to` as files with their line counts (renames found).
fn changed_files(dir: &Path, from: &str, to: &str) -> Result<Vec<ChangedFile>, String> {
    let names = git_raw(dir, &["diff", "-z", "--no-color", "--no-ext-diff", "--name-status", "-M", from, to])?;
    let nums = git_raw(dir, &["diff", "-z", "--no-color", "--no-ext-diff", "--numstat", "-M", from, to])?;
    let mut counts = std::collections::HashMap::new();
    let mut it = nums.split('\0');
    while let Some(entry) = it.next().filter(|e| !e.is_empty()) {
        let mut parts = entry.splitn(3, '\t');
        let (a, d, p) = (parts.next().unwrap_or("-"), parts.next().unwrap_or("-"), parts.next().unwrap_or(""));
        // a rename: "a\td\t", then the old path and the new one
        let path = if p.is_empty() { it.next().and(it.next()).unwrap_or("").to_string() } else { p.to_string() };
        counts.insert(path, (a.parse().ok(), d.parse().ok()));
    }
    let mut out = vec![];
    let mut it = names.split('\0');
    while let Some(status) = it.next().filter(|s| !s.is_empty()) {
        let code = status[..1].to_string();
        let (old_path, path) = if code == "R" || code == "C" { (it.next().map(String::from), it.next().unwrap_or("").to_string()) } else { (None, it.next().unwrap_or("").to_string()) };
        let (add, del) = counts.get(&path).copied().unwrap_or((None, None));
        out.push(ChangedFile { code, path, old_path, add, del });
    }
    Ok(out)
}

/// A further request from a branch whose last PR went in as one new commit (squash).
pub fn squash_refusal(n: u64) -> String {
    format!("PR #{n}이 squash로 합쳐진 브랜치예요. 워커가 원격 기준을 받아 합치거나(git merge) 새 워커로 시작한 뒤 다시 요청하세요.")
}

/// Checks a merge request when the conductor asks: the project folder on a local `base`, the worker on a branch of
/// its own with everything committed and something `base` lacks, no conflict predicted. Records what it would merge.
#[cfg_attr(not(test), allow(dead_code))]
pub fn plan_merge(root: &str, base: &str, worker_path: &str) -> Result<MergePlan, String> {
    plan_merge_with(root, base, worker_path, &PlanOpts::default())
}

/// `plan_merge` for either mode. PR mode doesn't touch or check the project folder (the merge happens on GitHub),
/// records the worker's uncommitted tracked files instead of refusing, compares with the remote-tracking base when
/// there is one (no fetch here), and leaves out commits of PRs that were already merged.
pub fn plan_merge_with(root: &str, base: &str, worker_path: &str, opts: &PlanOpts) -> Result<MergePlan, String> {
    let dir = Path::new(root);
    let pr = opts.mode.is_pr();
    let not_local = || format!("기준 브랜치 '{base}'가 로컬 브랜치가 아니에요");
    let local = git(dir, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{base}^{{commit}}")]);
    let tracking = opts.remote.as_ref().filter(|_| pr).map(|r| format!("refs/remotes/{r}/{base}"));
    let (cmp_ref, base_sha) = match tracking.and_then(|t| git(dir, &["rev-parse", "--verify", "--quiet", &format!("{t}^{{commit}}")]).ok().map(|s| (t, s))) {
        Some(found) => found,
        None => (format!("refs/heads/{base}"), local.map_err(|_| not_local())?),
    };
    if !pr {
        let cur = current_branch(root);
        if cur != base {
            return Err(format!("프로젝트 폴더가 기준 브랜치({base})가 아니라 {cur}에 있어요"));
        }
    }
    let branch = head_branch(worker_path).ok_or("워커가 브랜치에 있지 않아요 (detached)")?;
    if branch == base {
        return Err(format!("워커가 기준 브랜치({base})에 있어요: 합칠 워커 브랜치가 없어요"));
    }
    let dirty = if pr { tracked_paths(Path::new(worker_path))? } else { tracked_changes(Path::new(worker_path))? };
    if !dirty.is_empty() && !pr {
        return Err(format!("워커에 커밋하지 않은 변경이 {}개 있어요 ({}): 커밋한 뒤 다시 요청하세요", dirty.len(), short_list(&dirty)));
    }
    let worker_untracked = status(worker_path)?.iter().filter(|(c, _)| c == "??").count();
    let tip_sha = git(dir, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}^{{commit}}")]).map_err(|_| format!("브랜치 '{branch}'를 찾지 못했어요"))?;
    // heads of merged PRs this repo has (they were pushed from here)
    let heads: Vec<String> = opts.merged_heads.iter().filter(|h| git(dir, &["cat-file", "-e", &format!("{h}^{{commit}}")]).is_ok()).map(|h| format!("^{h}")).collect();
    let mut range: Vec<String> = vec![tip_sha.clone(), format!("^{base_sha}")];
    range.extend(heads.iter().cloned());
    let range_args: Vec<&str> = range.iter().map(String::as_str).collect();
    let commit_count: usize = git(dir, &[&["rev-list", "--count"][..], &range_args].concat())?.parse().unwrap_or(0);
    if commit_count == 0 && dirty.is_empty() {
        return Err(if heads.is_empty() { format!("'{branch}'는 이미 {base}에 들어 있어요") } else { format!("'{branch}'의 커밋은 이미 PR로 {base}에 합쳐졌어요") });
    }
    // a commit not here yet (no fetch here: the folder wasn't fast-forwarded, the merge was seen by the poller) is
    // checked again by the approval right after it fetches the base (pr.rs, S2)
    if let Some((n, commit)) = opts.squashed.as_ref().filter(|_| pr && commit_count > 0) {
        let known = git(dir, &["cat-file", "-e", &format!("{commit}^{{commit}}")]).is_ok();
        if known && git(dir, &["merge-base", "--is-ancestor", commit, &tip_sha]).is_err() {
            return Err(squash_refusal(*n));
        }
    }
    let merge_base = git(dir, &["merge-base", &base_sha, &tip_sha])?;
    // every commit `--no-ff` (or the PR) brings, patch-equivalent ones too (not `unmerged`, which leaves those out)
    let mut commits: Vec<String> = git(dir, &[&["log", "--reverse", "--no-color", "--no-show-signature", "--format=%h %s"][..], &range_args].concat())?.lines().map(String::from).collect();
    commits.truncate(MAX_COMMITS);
    let mut files = changed_files(dir, &merge_base, &tip_sha)?;
    let file_count = files.len();
    let (adds, dels) = files.iter().fold((0u32, 0u32), |(a, d), f| (a.saturating_add(f.add.unwrap_or(0)), d.saturating_add(f.del.unwrap_or(0))));
    files.truncate(MAX_FILES);
    let conflicts = conflict_precheck(dir, &[], &base_sha, &tip_sha);
    if let Some(c) = conflicts.as_ref().filter(|c| !c.is_empty()) {
        return Err(format!("{base}와 충돌이 예상돼요: {}. 워커 워크트리에서 {base}를 합쳐 충돌을 푼 뒤 다시 요청하세요", short_list(c)));
    }
    let project_dirty = if pr { vec![] } else { tracked_changes(dir)? };
    Ok(MergePlan {
        branch,
        base: base.to_string(),
        base_sha,
        tip_sha,
        merge_base,
        commits,
        commit_count,
        files,
        file_count,
        adds,
        dels,
        conflicts_checked: conflicts.is_some(),
        worker_untracked,
        project_dirty,
        mode: opts.mode,
        worker_dirty: if pr { dirty } else { vec![] },
        remote: opts.remote.clone().filter(|_| pr),
        cmp_ref,
        existing_pr: opts.existing_pr.filter(|_| pr),
    })
}

#[cfg(test)]
thread_local! {
    /// lets a test reach the real merge's conflict handling (the precheck would stop it first)
    static SKIP_PRECHECK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn skip_precheck() -> bool {
    #[cfg(test)]
    return SKIP_PRECHECK.with(|c| c.get());
    #[cfg(not(test))]
    false
}

/// Merges an approved request into the project folder (the caller holds `Merges::lock_root`): `git merge --no-ff` of
/// the recorded tip, only while the base is still the recorded commit, on a clean folder with nothing under way and no
/// file on disk where the merge would add one. A failed merge takes back only its own changes (`recover`); never
/// `reset --hard`.
pub fn merge_into_base(root: &str, req: &crate::merge::MergeRequest) -> Result<Outcome, Block> {
    merge_into_base_env(root, req, &[])
}

fn merge_into_base_env(root: &str, req: &crate::merge::MergeRequest, env: &[(&str, &OsStr)]) -> Result<Outcome, Block> {
    let dir = Path::new(root);
    let p = &req.plan;
    let g = |args: &[&str]| git_env(dir, env, args).map(|s| s.trim_end().to_string());
    // the folder on the base branch, at the recorded commit, with no tracked change: checked first, and again right
    // before `git merge` (which merges into whatever HEAD is then)
    let on_base = || -> Result<Option<Outcome>, Block> {
        let cur = current_branch(root);
        if cur != p.base {
            return Err(Block::NotOnBase(cur));
        }
        let now = g(&["rev-parse", "--verify", "--quiet", &format!("refs/heads/{}^{{commit}}", p.base)]).unwrap_or_default();
        let head = g(&["rev-parse", "--verify", "--quiet", "HEAD^{commit}"]).unwrap_or_default();
        if now != p.base_sha || head != p.base_sha {
            return Ok(Some(Outcome::Stale { now: if now.is_empty() { head } else { now } }));
        }
        let dirty = tracked_changes(dir).map_err(Block::Git)?;
        if !dirty.is_empty() {
            return Err(Block::Dirty(dirty));
        }
        Ok(None)
    };
    if let Some(o) = on_base()? {
        return Ok(o);
    }
    if let Some(what) = in_progress(dir).map_err(Block::Git)? {
        return Err(Block::Busy(what));
    }
    if g(&["cat-file", "-e", &format!("{}^{{commit}}", p.tip_sha)]).is_err() {
        return Ok(Outcome::Failed { message: format!("요청한 커밋 {}이(가) 저장소에 없어요", short_sha(&p.tip_sha)), touched: false });
    }
    // cheap and touches no file: a conflict stops here with the folder as it was
    if !skip_precheck() {
        if let Some(files) = conflict_precheck(dir, env, &p.base_sha, &p.tip_sha).filter(|c| !c.is_empty()) {
            return Ok(Outcome::Conflict { files });
        }
    }
    // git overwrites ignored files without asking (even with --no-overwrite-ignore on this path): the user's own
    // `.env` or local config in the project folder would be lost
    let blocking = in_the_way(dir, env, &p.base_sha, &p.tip_sha).map_err(Block::Git)?;
    if !blocking.is_empty() {
        return Err(Block::Git(format!("합치면 프로젝트 폴더에 있는 추적하지 않는 파일(무시된 파일 포함)을 덮어써요: {}. 옮기거나 지운 뒤 다시 병합을 누르세요", short_list(&blocking))));
    }
    let menv: Vec<(&str, &OsStr)> = MERGE_ENV.iter().map(|(k, v)| (*k, OsStr::new(*v))).chain(env.iter().copied()).collect();
    let m1 = format!("Merge branch '{}' into {}", p.branch, p.base);
    let m2 = format!("Maestro merge-request #{} (워커 {}): 사용자 승인", req.id, req.worker_name);
    let mut args: Vec<&str> = identity_args(dir, env);
    args.extend(["merge", "--no-ff", "--no-edit", "--no-overwrite-ignore", "-m", &m1, "-m", &m2, &p.tip_sha]);
    // the checks above take a few git calls: someone may have committed or switched branches meanwhile
    if let Some(o) = on_base()? {
        return Ok(o);
    }
    let (code, _, err) = git_out(dir, &menv, &args).map_err(Block::Git)?;
    if code != 0 {
        let err = Some(err.trim().to_string()).filter(|e| !e.is_empty()).unwrap_or_else(|| format!("git merge가 실패했어요 (종료 코드 {code})"));
        return recover(dir, &menv, p, err);
    }
    let commit = g(&["rev-parse", "HEAD"]).unwrap_or_default();
    // still a moment between the last check and git reading HEAD: the merge commit must sit on the base branch, on
    // top of the recorded commit, or it isn't the merge the user approved
    let landed = current_branch(root);
    if landed != p.base {
        return Ok(Outcome::Failed { message: format!("합치는 사이 프로젝트 폴더의 브랜치가 바뀌어 병합 커밋 {}이(가) 기준 브랜치({})가 아니라 {landed}에 들어갔어요", short_sha(&commit), p.base), touched: true });
    }
    if g(&["rev-parse", "HEAD^1"]).ok().as_deref() != Some(p.base_sha.as_str()) {
        return Ok(Outcome::Failed {
            message: format!("합치는 사이 기준 브랜치({})가 움직여 병합 커밋 {}이(가) 요청 때의 {} 위가 아닌 다른 커밋 위에 만들어졌어요", p.base, short_sha(&commit), short_sha(&p.base_sha)),
            touched: true,
        });
    }
    if g(&["rev-parse", "HEAD^2"]).ok().as_deref() != Some(p.tip_sha.as_str()) {
        eprintln!("[maestro] merge #{}: the second parent isn't the requested tip", req.id);
    }
    let later = g(&["rev-list", "--count", &format!("{}..refs/heads/{}", p.tip_sha, p.branch)]).ok().and_then(|n| n.parse().ok()).unwrap_or(0);
    Ok(Outcome::Merged { commit, later })
}

fn short_sha(s: &str) -> &str {
    &s[..s.len().min(7)]
}

/// After `git merge` failed: takes back only what the merge itself did. A merge under way (MERGE_HEAD) is aborted;
/// one that never started (HEAD still the base) is left alone unless every change is its own staged write; anything
/// else (someone's staged work, a half-written tree it can't tell apart) stays as it is.
fn recover(dir: &Path, menv: &[(&str, &OsStr)], p: &MergePlan, err: String) -> Result<Outcome, Block> {
    let g = |args: &[&str]| git_env(dir, menv, args).map(|s| s.trim_end().to_string());
    let merging = merge_head(dir);
    let conflicted: Vec<String> = if merging { g(&["diff", "--name-only", "--diff-filter=U"]).map(|o| o.lines().map(String::from).collect()).unwrap_or_default() } else { vec![] };
    if merging {
        let _ = git_out(dir, menv, &["merge", "--abort"]);
    } else if g(&["rev-parse", "HEAD"]).ok().as_deref() == Some(p.base_sha.as_str()) {
        match leftover(dir, menv, p) {
            Leftover::None => return Err(Block::Git(err)),
            Leftover::Merge => {
                let _ = git_out(dir, menv, &["reset", "--merge"]);
            }
            Leftover::Foreign(files) => return Err(Block::Dirty(files)),
            Leftover::Unknown => return Ok(Outcome::Failed { message: err, touched: true }),
        }
    }
    let restored = g(&["rev-parse", "HEAD"]).ok().as_deref() == Some(p.base_sha.as_str()) && !merge_head(dir) && tracked_changes(dir).is_ok_and(|t| t.is_empty());
    match (restored, conflicted.is_empty()) {
        (true, false) => Ok(Outcome::Conflict { files: conflicted }),
        // nothing left: a repo hook said no, a file locked by an editor
        (true, true) => Err(Block::Git(err)),
        (false, _) => Ok(Outcome::Failed { message: err, touched: true }),
    }
}

/// A file's content at a commit (`rel` from the repo's top, forward slashes); Err when it isn't there.
pub fn show_at(root: &str, rev: &str, rel: &str) -> Result<String, String> {
    git_raw(Path::new(root), &["show", &format!("{rev}:{rel}")])
}

/// `git config --get alias.<name>` (the guard follows aliases).
pub(crate) fn alias(dir: &str, name: &str) -> Option<String> {
    git(Path::new(dir), &["config", "--get", &format!("alias.{name}")]).ok().filter(|a| !a.trim().is_empty())
}

/// Whether `arg` names a commit in `dir`'s repo.
pub(crate) fn is_rev(dir: &str, arg: &str) -> bool {
    !arg.starts_with('-') && git(Path::new(dir), &["rev-parse", "--verify", "--quiet", &format!("{arg}^{{commit}}")]).is_ok()
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
        let check = || check_discard(&root, &wt, "wt", "main", false, &[]).unwrap();
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
        assert!(check_discard(&root, &wt, "wt", "main", true, &[]).unwrap().clean());
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
        assert_eq!(check_discard(&root, &wt, "wt", "gone", false, &[]).unwrap().base, "main");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clean_discard_removes_folder_and_branch_only_when_nothing_is_lost() {
        let (dir, root, wt) = repo_with_worktree("clean");
        std::fs::write(Path::new(&wt).join("new.txt"), "x").unwrap();
        assert!(discard(&root, &wt, "wt", "main", false, DiscardChoice::Clean, &[]).is_err());
        assert!(Path::new(&wt).join("new.txt").exists(), "refused: nothing touched");
        std::fs::remove_file(Path::new(&wt).join("new.txt")).unwrap();
        assert_eq!(discard(&root, &wt, "wt", "main", false, DiscardChoice::Clean, &[]).unwrap(), None);
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
        let saved = discard(&root, &wt, "wt", "main", false, DiscardChoice::Discard, &[]).unwrap().unwrap();
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
        assert_eq!(discard(&root, &wt, "wt", "main", false, DiscardChoice::Discard, &[]).unwrap().unwrap(), "refs/maestro/trash/wt-2");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keep_commits_uncommitted_work_to_the_branch() {
        let (dir, root, wt) = repo_with_worktree("keep");
        std::fs::write(Path::new(&wt).join("untracked.txt"), "loose\n").unwrap();
        assert_eq!(discard(&root, &wt, "wt", "main", false, DiscardChoice::Keep, &[]).unwrap().as_deref(), Some("wt"));
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
        let c = check_discard(&root, &wt, "wt", "main", false, &[]).unwrap();
        assert_eq!(c.commits.len(), 1);
        assert!(c.commits[0].ends_with(" on wt"));
        assert!(discard(&root, &wt, "wt", "main", false, DiscardChoice::Clean, &[]).is_err());
        // keep: the uncommitted work goes on `other`, `wt` isn't moved
        std::fs::write(w.join("loose.txt"), "loose\n").unwrap();
        assert_eq!(discard(&root, &wt, "wt", "main", false, DiscardChoice::Keep, &[]).unwrap().as_deref(), Some("other"));
        assert_eq!(git(r, &["show", "other:loose.txt"]).unwrap(), "loose");
        assert_eq!(git(r, &["rev-parse", "wt"]).unwrap(), on_wt);
        // discard: `wt`'s commit, not in HEAD's backup, gets one of its own before `wt` is deleted
        git(r, &["worktree", "add", &wt, "wt"]).unwrap();
        git(w, &["switch", "--detach", "main"]).unwrap();
        let saved = discard(&root, &wt, "wt", "main", false, DiscardChoice::Discard, &[]).unwrap().unwrap();
        assert_eq!(saved, "refs/maestro/trash/wt, refs/maestro/trash/wt-2");
        assert_eq!(git(r, &["rev-parse", "refs/maestro/trash/wt-2"]).unwrap(), on_wt);
        assert!(!has_branch(r, "wt"));
        // a detached worktree's commits count even when its branch stays
        git(r, &["worktree", "add", "-b", "kept", &wt, "main"]).unwrap();
        git(w, &["switch", "--detach"]).unwrap();
        std::fs::write(w.join("c.txt"), "c\n").unwrap();
        commit_all(&wt, "detached");
        let c = check_discard(&root, &wt, "kept", "main", true, &[]).unwrap();
        assert!(c.keep_branch && c.commits.len() == 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn request(root: &str, wt: &str, plan: MergePlan) -> crate::merge::MergeRequest {
        crate::merge::MergeRequest { id: 7, root: root.into(), maestro_path: root.into(), worker_path: wt.into(), worker_name: "w".into(), verified: None, requested_at: 0, plan, pr: None, pr_draft: None }
    }

    fn planned(root: &str, wt: &str) -> crate::merge::MergeRequest {
        request(root, wt, plan_merge(root, "main", wt).unwrap())
    }

    fn put(dir: &str, file: &str, text: &str) {
        std::fs::write(Path::new(dir).join(file), text).unwrap();
    }

    fn rev(dir: &str, r: &str) -> String {
        git(Path::new(dir), &["rev-parse", r]).unwrap()
    }

    #[test]
    fn plan_merge_lists_commits_and_files() {
        let (dir, root, wt) = repo_with_worktree("plan");
        put(&root, "c.txt", "one\ntwo\nthree\nfour\nfive\nsix\n");
        put(&root, "d.txt", "gone\n");
        commit_all(&root, "more files");
        git(Path::new(&wt), &["reset", "--hard", "main"]).unwrap();
        put(&wt, "a.txt", "a\nb\n");
        put(&wt, "b.txt", "new\n");
        commit_all(&wt, "first");
        std::fs::remove_file(Path::new(&wt).join("d.txt")).unwrap();
        std::fs::rename(Path::new(&wt).join("c.txt"), Path::new(&wt).join("e.txt")).unwrap();
        std::fs::write(Path::new(&wt).join("bin.dat"), [0u8, 1, 2, 0, 3]).unwrap();
        commit_all(&wt, "second");
        let p = plan_merge(&root, "main", &wt).unwrap();
        assert_eq!((p.branch.as_str(), p.base.as_str()), ("wt", "main"));
        assert_eq!(p.base_sha, rev(&root, "main"));
        assert_eq!(p.tip_sha, rev(&root, "wt"));
        assert_eq!(p.merge_base, p.base_sha);
        assert_eq!(p.commit_count, 2);
        assert!(p.commits[0].ends_with(" first") && p.commits[1].ends_with(" second"), "{:?}", p.commits);
        let f = |path: &str| p.files.iter().find(|f| f.path == path).unwrap_or_else(|| panic!("{path}: {:?}", p.files)).clone();
        assert_eq!((f("a.txt").code.as_str(), f("a.txt").add, f("a.txt").del), ("M", Some(1), Some(0)));
        assert_eq!((f("b.txt").code.as_str(), f("b.txt").add), ("A", Some(1)));
        assert_eq!((f("d.txt").code.as_str(), f("d.txt").del), ("D", Some(1)));
        assert_eq!((f("e.txt").code.as_str(), f("e.txt").old_path.as_deref()), ("R", Some("c.txt")));
        assert_eq!((f("bin.dat").code.as_str(), f("bin.dat").add, f("bin.dat").del), ("A", None, None));
        assert_eq!(p.file_count, 5);
        assert_eq!((p.adds, p.dels), (2, 1));
        assert!(p.conflicts_checked && p.project_dirty.is_empty() && p.worker_untracked == 0);
        // the diff dialog's sides: the recorded merge base and tip
        assert_eq!(show_at(&root, &p.merge_base, "c.txt").unwrap(), "one\ntwo\nthree\nfour\nfive\nsix\n");
        assert_eq!(show_at(&root, &p.tip_sha, "e.txt").unwrap(), "one\ntwo\nthree\nfour\nfive\nsix\n");
        assert!(show_at(&root, &p.tip_sha, "d.txt").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn plan_refuses() {
        let (dir, root, wt) = repo_with_worktree("refuse");
        let err = |base: &str| plan_merge(&root, base, &wt).unwrap_err();
        assert!(err("main").contains("이미 main에 들어 있어요"), "{}", err("main"));
        put(&wt, "a.txt", "dirty\n");
        assert!(err("main").contains("커밋하지 않은 변경이 1개"), "{}", err("main"));
        git(Path::new(&wt), &["checkout", "--", "a.txt"]).unwrap();
        put(&wt, "b.txt", "b\n");
        commit_all(&wt, "work");
        git(Path::new(&wt), &["switch", "--detach"]).unwrap();
        assert!(err("main").contains("detached"));
        git(Path::new(&wt), &["switch", "wt"]).unwrap();
        assert!(err("nope").contains("로컬 브랜치가 아니에요"));
        assert!(err("HEAD").contains("로컬 브랜치가 아니에요"));
        git(Path::new(&root), &["branch", "other"]).unwrap();
        assert!(err("other").contains("기준 브랜치(other)가 아니라 main"), "{}", err("other"));
        // the same lines changed on both sides
        put(&wt, "a.txt", "worker\n");
        commit_all(&wt, "worker edit");
        put(&root, "a.txt", "main\n");
        commit_all(&root, "main edit");
        assert!(err("main").contains("충돌이 예상돼요: a.txt"), "{}", err("main"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn worker_untracked_only_warns() {
        let (dir, root, wt) = repo_with_worktree("untracked");
        put(&wt, "b.txt", "b\n");
        commit_all(&wt, "work");
        put(&wt, "loose.txt", "x");
        put(&root, "a.txt", "edited in the project folder\n");
        let p = plan_merge(&root, "main", &wt).unwrap();
        assert_eq!(p.worker_untracked, 1);
        assert_eq!(p.project_dirty, ["a.txt"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn merge_no_ff_two_parents() {
        let (dir, root, wt) = repo_with_worktree("noff");
        put(&wt, "b.txt", "b\n");
        commit_all(&wt, "work");
        // a fast-forward would be possible: --no-ff still makes a merge commit
        let req = planned(&root, &wt);
        let Ok(Outcome::Merged { commit, later: 0 }) = merge_into_base(&root, &req) else { panic!() };
        assert_eq!(commit, rev(&root, "HEAD"));
        assert_ne!(commit, req.plan.tip_sha);
        assert_eq!(rev(&root, "HEAD^1"), req.plan.base_sha);
        assert_eq!(rev(&root, "HEAD^2"), req.plan.tip_sha);
        assert_eq!(rev(&root, "main"), commit);
        let msg = git(Path::new(&root), &["log", "-1", "--format=%B"]).unwrap();
        assert!(msg.starts_with("Merge branch 'wt' into main") && msg.contains("Maestro merge-request #7"), "{msg}");
        assert!(Path::new(&root).join("b.txt").exists());
        assert!(tracked_changes(Path::new(&root)).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn merge_merges_the_recorded_tip() {
        let (dir, root, wt) = repo_with_worktree("tip");
        put(&wt, "b.txt", "b\n");
        commit_all(&wt, "asked about");
        let req = planned(&root, &wt);
        put(&wt, "late.txt", "late\n");
        commit_all(&wt, "after the request");
        let Ok(Outcome::Merged { later: 1, .. }) = merge_into_base(&root, &req) else { panic!() };
        assert_eq!(rev(&root, "HEAD^2"), req.plan.tip_sha);
        assert!(Path::new(&root).join("b.txt").exists() && !Path::new(&root).join("late.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn moved_base_is_stale() {
        let (dir, root, wt) = repo_with_worktree("stale");
        put(&wt, "b.txt", "b\n");
        commit_all(&wt, "work");
        let req = planned(&root, &wt);
        put(&root, "c.txt", "c\n");
        commit_all(&root, "base moved");
        let moved = rev(&root, "main");
        assert_eq!(merge_into_base(&root, &req), Ok(Outcome::Stale { now: moved.clone() }));
        assert_eq!(rev(&root, "HEAD"), moved);
        assert!(!merge_head(Path::new(&root)) && !Path::new(&root).join("b.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn project_folder_blocks() {
        let (dir, root, wt) = repo_with_worktree("blocks");
        put(&wt, "b.txt", "b\n");
        commit_all(&wt, "work");
        let req = planned(&root, &wt);
        // tracked edits: nothing touched, the request can be tried again
        put(&root, "a.txt", "edited\n");
        assert_eq!(merge_into_base(&root, &req), Err(Block::Dirty(vec!["a.txt".into()])));
        assert_eq!(std::fs::read_to_string(Path::new(&root).join("a.txt")).unwrap(), "edited\n");
        git(Path::new(&root), &["checkout", "--", "a.txt"]).unwrap();
        // something else under way
        let mh = Path::new(&root).join(".git").join("MERGE_HEAD");
        std::fs::write(&mh, format!("{}\n", req.plan.tip_sha)).unwrap();
        assert_eq!(merge_into_base(&root, &req), Err(Block::Busy("merge")));
        std::fs::remove_file(&mh).unwrap();
        // another branch checked out
        git(Path::new(&root), &["switch", "-c", "side"]).unwrap();
        assert_eq!(merge_into_base(&root, &req), Err(Block::NotOnBase("side".into())));
        git(Path::new(&root), &["switch", "main"]).unwrap();
        // untracked files alone don't block
        put(&root, "notes.txt", "mine\n");
        assert!(matches!(merge_into_base(&root, &req), Ok(Outcome::Merged { .. })));
        assert_eq!(std::fs::read_to_string(Path::new(&root).join("notes.txt")).unwrap(), "mine\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The worker and main changed the same line: a request recorded as is (the precheck would refuse to plan it).
    fn conflicting(tag: &str) -> (PathBuf, String, crate::merge::MergeRequest) {
        let (dir, root, wt) = repo_with_worktree(tag);
        put(&wt, "a.txt", "worker\n");
        commit_all(&wt, "worker edit");
        put(&root, "a.txt", "main\n");
        commit_all(&root, "main edit");
        let plan = MergePlan { branch: "wt".into(), base: "main".into(), base_sha: rev(&root, "main"), tip_sha: rev(&root, "wt"), ..Default::default() };
        let req = request(&root, &wt, plan);
        (dir, root, req)
    }

    #[test]
    fn conflict_aborts_clean() {
        let (dir, root, req) = conflicting("conflict");
        SKIP_PRECHECK.with(|c| c.set(true));
        let out = merge_into_base(&root, &req);
        SKIP_PRECHECK.with(|c| c.set(false));
        assert_eq!(out, Ok(Outcome::Conflict { files: vec!["a.txt".into()] }));
        assert!(!merge_head(Path::new(&root)));
        assert!(tracked_changes(Path::new(&root)).unwrap().is_empty());
        assert_eq!(rev(&root, "HEAD"), req.plan.base_sha);
        // core.autocrlf may check it out with CRLF
        assert_eq!(std::fs::read_to_string(Path::new(&root).join("a.txt")).unwrap().trim_end(), "main");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn files_in_the_way_block() {
        let (dir, root, wt) = repo_with_worktree("inway");
        let (r, w) = (Path::new(&root), Path::new(&wt));
        put(&root, ".gitignore", ".env\n");
        commit_all(&root, "ignore .env");
        git(w, &["reset", "--hard", "main"]).unwrap();
        // the worker force-adds an ignored file and adds an ordinary one
        put(&wt, ".env", "worker\n");
        put(&wt, "b.txt", "b\n");
        std::fs::create_dir_all(w.join("sub")).unwrap();
        put(&wt, "sub/c.txt", "c\n");
        git(w, &["add", "-f", ".env", "b.txt", "sub/c.txt"]).unwrap();
        git(w, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-m", "env"]).unwrap();
        let req = planned(&root, &wt);
        // the user's own .env (ignored), a loose b.txt, and a file where the worker has a folder
        put(&root, ".env", "secret\n");
        put(&root, "b.txt", "mine\n");
        put(&root, "sub", "a file\n");
        let Err(Block::Git(msg)) = merge_into_base(&root, &req) else { panic!() };
        assert!(msg.contains(".env") && msg.contains("b.txt") && msg.contains("sub/c.txt"), "{msg}");
        assert_eq!(std::fs::read_to_string(r.join(".env")).unwrap(), "secret\n");
        assert_eq!(std::fs::read_to_string(r.join("b.txt")).unwrap(), "mine\n");
        assert_eq!(rev(&root, "HEAD"), req.plan.base_sha);
        // moved out of the way: it merges
        for f in [".env", "b.txt", "sub"] {
            std::fs::remove_file(r.join(f)).unwrap();
        }
        assert!(matches!(merge_into_base(&root, &req), Ok(Outcome::Merged { .. })));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tracked_files_are_not_in_the_way() {
        let (dir, root, wt) = repo_with_worktree("tracked");
        let w = Path::new(&wt);
        put(&root, "f", "a file\n");
        commit_all(&root, "add f");
        git(w, &["reset", "--hard", "main"]).unwrap();
        // a case-only rename (the old name is on a case-insensitive disk) and a tracked file that becomes a folder
        git(w, &["mv", "a.txt", "A.txt"]).unwrap();
        git(w, &["rm", "-q", "f"]).unwrap();
        std::fs::create_dir_all(w.join("f")).unwrap();
        put(&wt, "f/y", "y\n");
        commit_all(&wt, "rename and nest");
        let req = planned(&root, &wt);
        assert_eq!(in_the_way(Path::new(&root), &[], &req.plan.base_sha, &req.plan.tip_sha), Ok(vec![]));
        assert!(matches!(merge_into_base(&root, &req), Ok(Outcome::Merged { .. })));
        let files = git(Path::new(&root), &["ls-files"]).unwrap();
        assert!(files.lines().any(|l| l == "A.txt") && !files.lines().any(|l| l == "a.txt") && files.lines().any(|l| l == "f/y"), "{files}");
        assert!(Path::new(&root).join("f").is_dir());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn failed_merge_leaves_someone_elses_staged_work() {
        let (dir, root, wt) = repo_with_worktree("foreign");
        let r = Path::new(&root);
        put(&wt, "b.txt", "b\n");
        commit_all(&wt, "work");
        let req = planned(&root, &wt);
        // staged after the clean check: git refuses the merge without touching anything
        put(&root, "a.txt", "staged\n");
        git(r, &["add", "a.txt"]).unwrap();
        let (code, _, _) = git_out(r, &[], &["-c", "user.name=t", "-c", "user.email=t@t", "merge", "--no-ff", "--no-edit", &req.plan.tip_sha]).unwrap();
        assert_ne!(code, 0);
        assert_eq!(recover(r, &[], &req.plan, "refused".into()), Err(Block::Dirty(vec!["a.txt".into()])));
        // core.autocrlf may write it back with CRLF
        assert_eq!(std::fs::read_to_string(r.join("a.txt")).unwrap().trim_end(), "staged");
        assert_eq!(git(r, &["show", ":a.txt"]).unwrap(), "staged");
        assert_eq!(tracked_changes(r).unwrap(), ["a.txt"]);
        // a checkout cut short: only the merge's own staged writes, taken back
        git(r, &["reset", "-q", "a.txt"]).unwrap();
        git(r, &["checkout", "--", "a.txt"]).unwrap();
        git(r, &["checkout", &req.plan.tip_sha, "--", "b.txt"]).unwrap();
        assert_eq!(recover(r, &[], &req.plan, "cut short".into()), Err(Block::Git("cut short".into())));
        assert!(tracked_changes(r).unwrap().is_empty() && !r.join("b.txt").exists());
        // nothing there: nothing done
        assert_eq!(recover(r, &[], &req.plan, "hook".into()), Err(Block::Git("hook".into())));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn precheck_reports_conflict() {
        let (dir, root, req) = conflicting("precheck");
        assert_eq!(merge_into_base(&root, &req), Ok(Outcome::Conflict { files: vec!["a.txt".into()] }));
        assert_eq!(rev(&root, "HEAD"), req.plan.base_sha);
        assert_eq!(conflict_precheck(Path::new(&root), &[], &req.plan.base_sha, &req.plan.base_sha), Some(vec![]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn identity_fallback() {
        let (dir, root, wt) = repo_with_worktree("identity");
        put(&wt, "b.txt", "b\n");
        commit_all(&wt, "work");
        let req = planned(&root, &wt);
        // no git identity anywhere: the merge is signed as Maestro
        let empty = dir.join("empty.gitconfig");
        std::fs::write(&empty, "").unwrap();
        let env = [("GIT_CONFIG_GLOBAL", empty.as_os_str()), ("GIT_CONFIG_NOSYSTEM", OsStr::new("1"))];
        assert_eq!(identity_args(Path::new(&root), &env).len(), 4);
        assert!(matches!(merge_into_base_env(&root, &req, &env), Ok(Outcome::Merged { .. })));
        assert_eq!(git(Path::new(&root), &["log", "-1", "--format=%an <%ae>"]).unwrap(), "Maestro <maestro@localhost>");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn merged_worker_discards_clean() {
        let (dir, root, wt) = repo_with_worktree("merged");
        put(&wt, "b.txt", "b\n");
        commit_all(&wt, "work");
        assert_eq!(check_discard(&root, &wt, "wt", "main", false, &[]).unwrap().commits.len(), 1);
        assert!(matches!(merge_into_base(&root, &planned(&root, &wt)), Ok(Outcome::Merged { .. })));
        assert!(check_discard(&root, &wt, "wt", "main", false, &[]).unwrap().clean());
        // merged with a merge commit: no backup ref, and the dialog says none
        let tip = git(Path::new(&wt), &["rev-parse", "HEAD"]).unwrap();
        assert!(!check_discard(&root, &wt, "wt", "main", false, &[tip.clone()]).unwrap().backup);
        assert_eq!(discard(&root, &wt, "wt", "main", false, DiscardChoice::Clean, &[]).unwrap(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_squash_merged_branch_says_it_keeps_a_backup() {
        let (dir, root, wt) = repo_with_worktree("squashbackup");
        put(&wt, "b.txt", "b\n");
        commit_all(&wt, "work");
        let tip = git(Path::new(&wt), &["rev-parse", "HEAD"]).unwrap();
        git(Path::new(&root), &["merge", "--squash", "wt"]).unwrap();
        git(Path::new(&root), &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-m", "squash"]).unwrap();
        let c = check_discard(&root, &wt, "wt", "main", false, &[tip.clone()]).unwrap();
        assert!(c.clean() && c.backup, "{c:?}");
        assert_eq!(discard(&root, &wt, "wt", "main", false, DiscardChoice::Clean, &[tip]).unwrap().as_deref(), Some("refs/maestro/trash/wt"));
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
