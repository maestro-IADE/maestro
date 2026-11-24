//! Per-project settings kept next to the agents (`<project_dir>/project.json`), apart from the Maestro's meta so they
//! outlive it: the merge mode (R6), "local" (default, R5's `--no-ff` merge in the project folder) or "pr" (the app
//! pushes the worker's branch and opens a pull request with gh); and 워크트리 준비 (prep.rs), the local files copied
//! into each new worktree and the setup command run there.
//! Also the paths of project groups (`~/.maestro/groups.json`, main.rs): which group a project sits in, and the repos
//! right inside a folder.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MergeMode {
    #[default]
    Local,
    Pr,
}

impl MergeMode {
    pub fn parse(s: &str) -> Result<MergeMode, String> {
        match s {
            "local" => Ok(MergeMode::Local),
            "pr" => Ok(MergeMode::Pr),
            _ => Err(format!("알 수 없는 병합 방식: {s}")),
        }
    }

    pub fn is_pr(self) -> bool {
        self == MergeMode::Pr
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSettings {
    #[serde(default)]
    pub merge_mode: MergeMode,
    /// patterns of the local files copied into each new worktree (prep.rs); a file from before it gets `.env*`
    #[serde(default = "default_copy_files")]
    pub copy_files: Vec<String>,
    /// run in each new worktree in the background; "" = none
    #[serde(default)]
    pub setup_command: String,
    /// the user trusted the project in Maestro (`trusted_in`); left out while false, so files stay as they were
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub trusted: bool,
}

fn default_copy_files() -> Vec<String> {
    vec![crate::prep::DEFAULT_COPY.to_string()]
}

impl Default for ProjectSettings {
    fn default() -> Self {
        ProjectSettings { merge_mode: MergeMode::default(), copy_files: default_copy_files(), setup_command: String::new(), trusted: false }
    }
}

/// 워크트리 준비 as the settings show it (`project_prep`, `set_project_prep`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Prep {
    pub copy_files: Vec<String>,
    pub setup_command: String,
}

pub fn file(home: &Path, root: &str) -> PathBuf {
    crate::worktree::project_dir(home, root).join("project.json")
}

/// The format of `project.json` this build writes (and the newest it reads; durable.rs).
const FILE_VERSION: u64 = 1;

/// A missing file means the defaults (local); so does one it can't use (the user is told).
pub fn read(home: &Path, root: &str) -> ProjectSettings {
    crate::durable::read_or_default(&file(home, root), FILE_VERSION)
}

pub fn merge_mode(home: &Path, root: &str) -> MergeMode {
    read(home, root).merge_mode
}

/// Err when the file is a newer Maestro's (it's left as it is).
pub fn write_mode(home: &Path, root: &str, mode: MergeMode) -> Result<(), String> {
    let f = file(home, root);
    let mut s: ProjectSettings = crate::durable::read_for_write(&f, FILE_VERSION)?;
    s.merge_mode = mode;
    crate::durable::write_json(&f, &s, FILE_VERSION)
}

pub fn prep(home: &Path, root: &str) -> Prep {
    let s = read(home, root);
    Prep { copy_files: s.copy_files, setup_command: s.setup_command }
}

/// Changes either setting (None: kept) and answers what is saved: patterns cleaned (`prep::clean_patterns`), the
/// command trimmed. Err: an unsafe pattern, a command of more than one line, or a newer Maestro's file.
pub fn write_prep(home: &Path, root: &str, copy_files: Option<Vec<String>>, setup_command: Option<String>) -> Result<Prep, String> {
    let copy_files = copy_files.map(|l| crate::prep::clean_patterns(&l)).transpose()?;
    let setup_command = setup_command.map(|c| c.trim().to_string());
    // `cmd /C` would run only the first line
    if setup_command.as_deref().is_some_and(|c| c.contains(['\n', '\r'])) {
        return Err("준비 명령은 한 줄로 써 주세요".into());
    }
    let f = file(home, root);
    let mut s: ProjectSettings = crate::durable::read_for_write(&f, FILE_VERSION)?;
    if let Some(l) = copy_files {
        s.copy_files = l;
    }
    if let Some(c) = setup_command {
        s.setup_command = c;
    }
    crate::durable::write_json(&f, &s, FILE_VERSION)?;
    Ok(Prep { copy_files: s.copy_files, setup_command: s.setup_command })
}

/// The project (its data folder `pdir`, `worktree::project_dir`) was trusted in Maestro: one "신뢰하고 시작" for any
/// of its agents, or made by 새 프로젝트. Its agents aren't asked again, the way Claude Code keeps a repository's
/// trust for all its worktrees. An acceptance from before this (an agent's own `acpTrusted`) still counts for that
/// agent alone: the data folder is the path's, and a leftover meta may be another repository's. Unreadable = not
/// trusted.
pub fn trusted_in(pdir: &Path) -> bool {
    crate::durable::read_json::<ProjectSettings>(&pdir.join("project.json"), FILE_VERSION).ok().flatten().is_some_and(|s| s.trusted)
}

/// The project at `root` is no longer trusted (`trusted_in`): a clone into that path, or the project taken off the
/// list. Its data folder is keyed by the path alone, so what comes there next would find the old answer. Nothing to
/// do when there's no file.
pub fn untrust(home: &Path, root: &str) -> Result<(), String> {
    let f = file(home, root);
    if !f.exists() {
        return Ok(());
    }
    let mut s: ProjectSettings = crate::durable::read_for_write(&f, FILE_VERSION)?;
    if !s.trusted {
        return Ok(());
    }
    s.trusted = false;
    crate::durable::write_json(&f, &s, FILE_VERSION)
}

/// Keeps the project trusted (`trusted_in`). Err when the file is a newer Maestro's (it's left as it is).
pub fn set_trusted_in(pdir: &Path) -> Result<(), String> {
    let f = pdir.join("project.json");
    let mut s: ProjectSettings = crate::durable::read_for_write(&f, FILE_VERSION)?;
    if s.trusted {
        return Ok(());
    }
    s.trusted = true;
    crate::durable::write_json(&f, &s, FILE_VERSION)
}

/// A project 새 프로젝트 just made in `dir` starts trusted (`trusted_in`): Maestro made all there is in it. Its root
/// is taken as `add_repo` takes it (`toplevel`), so the data folder is the one its agents use.
pub fn trust_new(home: &Path, dir: &str) -> Result<(), String> {
    let root = crate::worktree::toplevel(dir)?;
    set_trusted_in(&crate::worktree::project_dir(home, &root))
}

/// A path with forward slashes and no trailing one, lower case where the file system ignores case.
fn norm(p: &str) -> String {
    let p = p.replace('\\', "/");
    let p = p.trim_end_matches('/');
    if cfg!(any(windows, target_os = "macos")) { p.to_lowercase() } else { p.to_string() }
}

/// The group (one of `groups`, as spelled there) that project `root` sits in: the deepest group folder holding it.
/// A group's own folder is not inside itself.
pub fn group_of<'a>(root: &str, groups: &'a [String]) -> Option<&'a str> {
    let r = norm(root);
    groups
        .iter()
        .filter(|g| r.starts_with(&(norm(g) + "/")))
        .max_by_key(|g| norm(g).len())
        .map(String::as_str)
}

/// The folders right inside `dir` that are git repos of their own (a `.git` folder, not a file: worktrees and
/// submodules have a file), in name order. Hidden folders are skipped.
pub fn git_children(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')) && p.join(".git").is_dir())
        .collect();
    out.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_is_kept_for_the_project() {
        let pdir = crate::util::unique_temp_dir("maestro-project-trust");
        std::fs::create_dir_all(&pdir).unwrap();
        assert!(!trusted_in(&pdir), "nothing there");
        // a project.json from before (prep only) isn't trusted, and keeps what it had
        std::fs::write(pdir.join("project.json"), r#"{"version":1,"mergeMode":"pr","copyFiles":[".env*"],"setupCommand":"npm ci"}"#).unwrap();
        assert!(!trusted_in(&pdir));
        set_trusted_in(&pdir).unwrap();
        assert!(trusted_in(&pdir));
        let s: ProjectSettings = crate::durable::read_json(&pdir.join("project.json"), FILE_VERSION).unwrap().unwrap();
        assert_eq!((s.merge_mode, s.setup_command.as_str(), s.trusted), (MergeMode::Pr, "npm ci", true));
        // other settings written later keep it
        let home = crate::util::unique_temp_dir("maestro-project-trust-home");
        let root = "C:/code/app";
        let p = crate::worktree::project_dir(&home, root);
        set_trusted_in(&p).unwrap();
        write_prep(&home, root, None, Some("echo hi".into())).unwrap();
        write_mode(&home, root, MergeMode::Local).unwrap();
        assert!(trusted_in(&p));
        // untrusted, the field isn't written at all (files stay as older builds wrote them)
        let q = crate::util::unique_temp_dir("maestro-project-trust-plain");
        crate::durable::write_json(&q.join("project.json"), &ProjectSettings::default(), FILE_VERSION).unwrap();
        assert!(!std::fs::read_to_string(q.join("project.json")).unwrap().contains("trusted"));
        for d in [pdir, home, q] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn an_agent_trusted_before_trusts_only_itself() {
        // before 2026-11-24 an acceptance stayed in that agent's meta: a leftover one may be another repository's
        let pdir = crate::util::unique_temp_dir("maestro-project-legacy-trust");
        std::fs::create_dir_all(pdir.join("agents")).unwrap();
        std::fs::write(pdir.join("agents").join("maestro.json"), r#"{"version":1,"adapterId":"claude-code","acpTrusted":true}"#).unwrap();
        assert!(!trusted_in(&pdir));
        let _ = std::fs::remove_dir_all(pdir);
    }

    #[test]
    fn a_clone_or_a_removed_project_is_no_longer_trusted() {
        let home = crate::util::unique_temp_dir("maestro-project-untrust");
        let root = "C:/code/app";
        let pdir = crate::worktree::project_dir(&home, root);
        untrust(&home, root).unwrap(); // no file: nothing made
        assert!(!pdir.join("project.json").exists());
        set_trusted_in(&pdir).unwrap();
        write_prep(&home, root, None, Some("npm ci".into())).unwrap();
        untrust(&home, root).unwrap();
        assert!(!trusted_in(&pdir));
        assert_eq!(prep(&home, root).setup_command, "npm ci", "the rest stays");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_new_project_starts_trusted_and_a_clone_doesnt() {
        let home = crate::util::unique_temp_dir("maestro-project-new-home");
        let parent = crate::util::unique_temp_dir("maestro-project-new");
        let dir = crate::newproject::create(&parent.to_string_lossy(), "app").unwrap();
        trust_new(&home, &dir).unwrap();
        let root = crate::worktree::toplevel(&dir).unwrap();
        assert!(trusted_in(&crate::worktree::project_dir(&home, &root)));
        // another repo next to it (as a clone or an opened folder would be) isn't
        let other = crate::newproject::create(&parent.to_string_lossy(), "other").unwrap();
        assert!(!trusted_in(&crate::worktree::project_dir(&home, &crate::worktree::toplevel(&other).unwrap())));
        for d in [home, parent] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn a_project_sits_in_the_deepest_group_holding_it() {
        let groups = vec!["C:\\work".to_string(), "C:/work/team/".to_string(), "D:/other".to_string()];
        assert_eq!(group_of("C:/work/app", &groups), Some("C:\\work"), "slashes either way");
        assert_eq!(group_of("C:/work/team/api", &groups), Some("C:/work/team/"), "the deeper group wins");
        assert_eq!(group_of("C:/workshop/app", &groups), None, "a name that only starts the same is not inside");
        assert_eq!(group_of("C:/work", &groups), None, "a group's own folder is not in it");
        assert_eq!(group_of("E:/app", &groups), None);
        if cfg!(windows) {
            assert_eq!(group_of("c:/WORK/app", &groups), Some("C:\\work"), "case doesn't matter on Windows");
        }
    }

    #[test]
    fn a_folder_lists_its_own_repos_only() {
        let dir = crate::util::unique_temp_dir("maestro-group");
        for d in ["b-repo/.git", "A-repo/.git", "plain", ".hidden/.git", "deep/inner/.git"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        std::fs::create_dir_all(dir.join("worktree")).unwrap();
        std::fs::write(dir.join("worktree/.git"), "gitdir: elsewhere").unwrap();
        std::fs::write(dir.join("file.txt"), "x").unwrap();
        let names: Vec<String> = git_children(&dir).iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["A-repo", "b-repo"], "repos right inside, in name order; no worktree, hidden or deeper one");
        assert!(git_children(&dir.join("missing")).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn mode_defaults_to_local_and_persists() {
        let home = std::env::temp_dir().join(format!("maestro-project-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        assert_eq!(merge_mode(&home, "C:/p/app"), MergeMode::Local);
        write_mode(&home, "C:/p/app", MergeMode::Pr).unwrap();
        assert_eq!(merge_mode(&home, "C:/p/app"), MergeMode::Pr);
        assert_eq!(merge_mode(&home, "C:/p/other"), MergeMode::Local);
        assert_eq!(std::fs::read_to_string(file(&home, "C:/p/app")).unwrap().contains("\"mergeMode\": \"pr\""), true);
        assert!(MergeMode::parse("x").is_err());
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn prep_defaults_to_env_files_and_persists() {
        let home = crate::util::unique_temp_dir("maestro-project-prep");
        let root = "C:/p/app";
        assert_eq!(prep(&home, root), Prep { copy_files: vec![".env*".into()], setup_command: String::new() }, "no file yet");
        // a file from before 워크트리 준비: the default patterns, its merge mode kept
        let f = file(&home, root);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, r#"{"version":1,"mergeMode":"pr"}"#).unwrap();
        assert_eq!(prep(&home, root).copy_files, [".env*"]);
        let p = write_prep(&home, root, None, Some(" npm install ".into())).unwrap();
        assert_eq!(p, Prep { copy_files: vec![".env*".into()], setup_command: "npm install".into() });
        let p = write_prep(&home, root, Some(vec![" .env ".into(), "".into(), "config\\local.json".into()]), None).unwrap();
        assert_eq!(p.copy_files, [".env", "config/local.json"]);
        assert_eq!(p.setup_command, "npm install", "the other field kept");
        assert_eq!(merge_mode(&home, root), MergeMode::Pr, "and the merge mode");
        // emptied by the user: stays empty, not the default again
        assert!(write_prep(&home, root, Some(vec![]), None).unwrap().copy_files.is_empty());
        assert!(prep(&home, root).copy_files.is_empty());
        assert!(write_prep(&home, root, Some(vec!["../x".into()]), None).unwrap_err().contains(".."));
        assert!(write_prep(&home, root, None, Some("a\nb".into())).is_err());
        assert!(prep(&home, root).copy_files.is_empty(), "a refused change writes nothing");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_newer_file_is_left_alone() {
        let home = crate::util::unique_temp_dir("maestro-project-v");
        let f = file(&home, "C:/p/app");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        let newer = r#"{"version":2,"mergeMode":"pr","more":1}"#;
        std::fs::write(&f, newer).unwrap();
        assert_eq!(merge_mode(&home, "C:/p/app"), MergeMode::Local);
        let e = write_mode(&home, "C:/p/app", MergeMode::Local).unwrap_err();
        assert!(e.contains("v2"), "{e}");
        assert_eq!(std::fs::read_to_string(&f).unwrap(), newer);
        let _ = std::fs::remove_dir_all(home);
    }
}
