//! Per-project settings kept next to the agents (`<project_dir>/project.json`), apart from the Maestro's meta so they
//! outlive it. Today only the merge mode (R6): "local" (default, R5's `--no-ff` merge in the project folder) or "pr"
//! (the app pushes the worker's branch and opens a pull request with gh).

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

#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSettings {
    #[serde(default)]
    pub merge_mode: MergeMode,
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

#[cfg(test)]
mod tests {
    use super::*;

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
