//! Developer tools: the top bar's "dev 모드" (dev builds) runs parts of the app without an agent, to check them
//! (docs/features/ui/dev-tools.md). The first test runs the real `maestro` CLI, one command after another, against a
//! blank test canvas. The caller is a stand-in under an open project (not a worktree) that `control::locate` answers
//! for, so every route runs exactly as it would for an agent; no model is involved anywhere.

use crate::{agent, approval::Approvals, home, relations, util, AppState, WorktreeInfo};
use serde::Serialize;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tauri::{AppHandle, Emitter, Manager, State};

/// The stand-in caller's folder name under the project (never made on disk), and so its canvas file's name.
pub const TEST_DIR: &str = "__maestro_cli_test__";
/// What the relations the test declares are named with: taken away again when the test starts over or ends.
pub const TEST_RELATION: &str = "시험-";

/// The stand-in caller of project `root`: what the CLI test runs `maestro` as (its MAESTRO_WORKTREE).
pub fn caller(root: &str) -> String {
    format!("{}/{TEST_DIR}", root.replace('\\', "/").trim_end_matches('/'))
}

/// The project a stand-in caller belongs to; None for anything else (every real worktree).
pub fn root_of(caller: &str) -> Option<String> {
    let c = caller.replace('\\', "/");
    c.trim_end_matches('/').strip_suffix(&format!("/{TEST_DIR}")).filter(|r| !r.is_empty()).map(String::from)
}

/// The stand-in as a worktree of `root`, for the CLI's routes: no agent behind it, no workers, no references.
pub fn stand_in(app: &AppHandle, state: &AppState, root: &str) -> Result<WorktreeInfo, String> {
    let path = caller(root);
    let mut w = crate::info(state, &crate::files(app, root, &path)?, path, "dev".into());
    w.name = Some("CLI 시험".into());
    Ok(w)
}

/// The `maestro` command agents run (`~/.maestro/bin/maestro`, put there when the app starts).
fn cli_path(app: &AppHandle) -> Result<PathBuf, String> {
    let p = agent::cli_dir(&home(app)?).join(if cfg!(windows) { "maestro.exe" } else { "maestro" });
    if p.is_file() {
        Ok(p)
    } else {
        Err(format!("maestro CLI가 없어요: {}", p.display()))
    }
}

/// Takes away what the test left in project `root`: its canvas file, drawings not taken yet, its selection, and the
/// relations it declared (named `시험-…`).
fn clear(app: &AppHandle, state: &AppState, root: &str) -> Result<(), String> {
    let me = caller(root).to_lowercase();
    if let Ok(p) = crate::canvas_path(app, root, TEST_DIR) {
        let _ = std::fs::remove_file(p);
    }
    state.drawings.lock().unwrap().remove(&me);
    state.chat_asks.lock().unwrap().remove(&me);
    state.selection.lock().unwrap().remove(&me);
    let home = home(app)?;
    // (the files its commands pointed at: drawings, and the pictures and pages it showed)
    let _ = std::fs::remove_dir_all(home.join(".maestro").join("dev"));
    let names: Vec<String> = relations::load(&home, root).relations.iter().filter(|r| r.name.starts_with(TEST_RELATION)).map(|r| r.name.clone()).collect();
    for n in &names {
        let _ = relations::update(&home, root, |f| relations::remove(f, n));
    }
    if !names.is_empty() {
        let _ = app.emit("maestro://relations", root);
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestCanvas {
    /// the stand-in caller: the test canvas is this "agent's"
    scope: String,
    /// the CLI the test runs
    cli: String,
}

/// Starts the CLI test for project `root`, or starts it over: a blank canvas for the stand-in caller.
#[tauri::command]
pub fn dev_cli_begin(app: AppHandle, state: State<AppState>, root: String) -> Result<TestCanvas, String> {
    if !crate::read_repos(&app)?.iter().any(|r| util::path_key(r) == util::path_key(&root)) {
        return Err("Maestro에 있는 프로젝트가 아니에요".into());
    }
    clear(&app, &state, &root)?;
    Ok(TestCanvas { scope: caller(&root), cli: cli_path(&app)?.display().to_string() })
}

/// Ends the CLI test for project `root`: nothing of it stays behind.
#[tauri::command]
pub fn dev_cli_end(app: AppHandle, state: State<AppState>, root: String) -> Result<(), String> {
    clear(&app, &state, &root)
}

#[derive(Serialize)]
pub struct CliRun {
    /// the process's exit code (0 = the command worked)
    code: i32,
    stdout: String,
    stderr: String,
    ms: u64,
}

/// Runs the real `maestro` CLI as the stand-in caller of `root`: `args` are what comes after `maestro`. `file`: what
/// a `--file` flag points at (written to a file of its own, whose path is added as `--file <path>`): JSON, or with
/// `name` a file of that name (a picture or a page for `maestro show`, where the extension matters).
#[tauri::command]
pub async fn dev_cli_run(app: AppHandle, root: String, args: Vec<String>, file: Option<String>, name: Option<String>) -> Result<CliRun, String> {
    tauri::async_runtime::spawn_blocking(move || run(&app, &root, args, file, name)).await.map_err(|e| e.to_string())?
}

/// The file name a test file is written under: `name`'s last part (never a path out of the test's folder).
fn file_name(name: Option<&str>) -> String {
    let n = name.unwrap_or("").rsplit(['/', '\\']).next().unwrap_or("").trim().trim_matches('.').to_string();
    if n.is_empty() { "cli-test.json".into() } else { n }
}

fn run(app: &AppHandle, root: &str, mut args: Vec<String>, file: Option<String>, name: Option<String>) -> Result<CliRun, String> {
    let home = home(app)?;
    if let Some(json) = file {
        let dir = home.join(".maestro").join("dev");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let p = dir.join(file_name(name.as_deref()));
        std::fs::write(&p, json).map_err(|e| e.to_string())?;
        args.push("--file".into());
        args.push(p.to_string_lossy().replace('\\', "/"));
    }
    let approvals = app.state::<Approvals>();
    let mut cmd = Command::new(cli_path(app)?);
    // what an agent's terminal has: where the app listens, the caller and the caller's own token
    cmd.args(&args).envs(agent::maestro_env(&home, &caller(root), &approvals)).current_dir(&home).stdin(Stdio::null());
    let start = std::time::Instant::now();
    let out = crate::cli::quiet(&mut cmd).output().map_err(|e| format!("maestro CLI를 실행하지 못했어요: {e}"))?;
    Ok(CliRun {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        ms: start.elapsed().as_millis() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_files_stay_in_the_test_folder() {
        assert_eq!(file_name(None), "cli-test.json");
        assert_eq!(file_name(Some("시험-그림.svg")), "시험-그림.svg");
        assert_eq!(file_name(Some("../../x/evil.html")), "evil.html");
        assert_eq!(file_name(Some("a\\b.png")), "b.png");
        assert_eq!(file_name(Some("..")), "cli-test.json");
    }

    #[test]
    fn stand_in_caller_names_its_project_and_nothing_else_does() {
        assert_eq!(caller("C:\\p\\repo\\"), "C:/p/repo/__maestro_cli_test__");
        assert_eq!(root_of(&caller("C:/p/repo")).as_deref(), Some("C:/p/repo"));
        assert_eq!(root_of("C:\\p\\repo\\__maestro_cli_test__\\").as_deref(), Some("C:/p/repo"));
        // a real worktree, the project itself, a look-alike folder further in
        assert_eq!(root_of("C:/Users/u/.maestro/projects/repo_1/agent-1"), None);
        assert_eq!(root_of("C:/p/repo"), None);
        assert_eq!(root_of("C:/p/repo/__maestro_cli_test__/src"), None);
        assert_eq!(root_of("/__maestro_cli_test__"), None);
    }
}
