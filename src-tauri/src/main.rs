#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod acp;
mod acp_runtime;
mod acp_setup;
mod agent;
mod antigravity;
mod approval;
mod catalog;
mod claude;
mod cli;
mod commands;
mod control;
mod durable;
mod events;
mod exec;
mod github;
mod guard;
mod library;
mod merge;
mod pr;
mod proc;
mod project;
mod pty;
mod runtime;
mod stream;
mod transcript;
mod tui_wait;
mod util;
mod workspace;
mod worktree;

use agent::{Agent, AgentMeta, AgentState, Files, LaunchFailure};
use approval::{Approval, Approvals};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter as _, Manager, State};

/// Running agents keyed by lowercased worktree path (Windows paths are case-insensitive).
#[derive(Default)]
pub(crate) struct AppState {
    pub(crate) agents: Mutex<HashMap<String, Arc<Agent>>>,
    /// lowercased worktree path → held while an agent for it starts (`start_lock`)
    starting: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    /// lowercased worktree path → lowercased project root, for agents (setup terminals have none)
    roots: Mutex<HashMap<String, String>>,
    away: Mutex<Away>,
    /// lowercased worktree path → the blocks the user selected on that agent's canvas (`maestro selection`)
    pub(crate) selection: Mutex<HashMap<String, serde_json::Value>>,
    /// lowercased worktree path → structure an agent drew with `maestro draw`, waiting for its canvas to take it
    pub(crate) drawings: Mutex<HashMap<String, Vec<serde_json::Value>>>,
    /// the app's 에이전트 보기 (Settings → 일반, `set_view_default`): what a start nobody asked a view of runs on.
    /// Memory only; chat until the main window says otherwise
    view_default: Mutex<agent::View>,
}

/// Which projects are on screen (per window: splits can show several, pop-out windows more), when each other
/// one was left, and after how long its idle agents stop.
#[derive(Default)]
struct Away {
    on_screen: HashMap<String, HashSet<String>>,
    minutes: u64,
    left: HashMap<String, std::time::Instant>,
}

impl Away {
    fn visible(&self) -> HashSet<String> {
        self.on_screen.values().flatten().cloned().collect()
    }

    /// Window `label` now shows `roots` (empty: the window is gone). Projects nobody shows any more start their
    /// away clock; ones shown again stop it.
    fn show(&mut self, label: &str, roots: HashSet<String>) {
        let before = self.visible();
        if roots.is_empty() {
            self.on_screen.remove(label);
        } else {
            self.on_screen.insert(label.to_string(), roots);
        }
        let after = self.visible();
        for r in before.difference(&after) {
            self.left.insert(r.clone(), std::time::Instant::now());
        }
        for r in &after {
            self.left.remove(r);
        }
    }
}

impl AppState {
    pub(crate) fn get(&self, path: &str) -> Option<Arc<Agent>> {
        self.agents.lock().unwrap().get(&path.to_lowercase()).cloned()
    }

    /// One start per worktree at a time, from the alive check to `agents.insert`. Two commands (chat view, CLI view,
    /// `maestro send`) racing would otherwise both spawn: an orphan process, and the stream's epoch taken by the
    /// second `Emitter`, so the registered agent's events would go nowhere. Other worktrees start in parallel.
    fn start_lock(&self, path: &str) -> Arc<Mutex<()>> {
        self.starting.lock().unwrap().entry(path.to_lowercase()).or_default().clone()
    }
}

pub(crate) fn home(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().home_dir().map_err(|e| e.to_string())
}

pub(crate) fn files(app: &AppHandle, project_root: &str, worktree_path: &str) -> Result<Files, String> {
    let pdir = worktree::project_dir(&home(app)?, project_root);
    if is_root(project_root, worktree_path) {
        // the Maestro agent works on the project folder itself
        return Ok(Files { meta: pdir.join("agents").join("maestro.json") });
    }
    if !worktree::is_managed(worktree_path, &pdir) {
        // a worktree made outside Maestro: its folder name may clash with ours, so key it by path
        return Ok(Files { meta: pdir.join("agents").join(format!("ext-{}.json", worktree::path_id(worktree_path))) });
    }
    Ok(Files::new(&pdir, worktree_path))
}

/// The project folder itself, i.e. where the Maestro agent runs (not a worktree).
pub(crate) fn is_root(project_root: &str, path: &str) -> bool {
    project_root.replace('\\', "/").eq_ignore_ascii_case(&path.replace('\\', "/"))
}

/// The running agent for a worktree, starting (or resuming) it first if needed: on the runtime of the app's
/// 에이전트 보기 (`set_view_default`; the chat's is ACP when it can). No per-agent memory: a look at one agent's
/// terminal doesn't make its next start (a delegation, a resume after away) a terminal one.
pub(crate) fn ensure(app: &AppHandle, state: &AppState, project_root: &str, worktree_path: &str) -> Result<(Arc<Agent>, bool), String> {
    ensure_with(app, state, project_root, worktree_path, None)
}

/// `ensure`, with the runtime asked for (`want`) instead of the app's default view's.
pub(crate) fn ensure_with(app: &AppHandle, state: &AppState, project_root: &str, worktree_path: &str, want: Option<acp_setup::Want>) -> Result<(Arc<Agent>, bool), String> {
    if let Some(a) = state.get(worktree_path).filter(|a| a.alive()) {
        return Ok((a, false));
    }
    let lock = state.start_lock(worktree_path);
    let _one = lock.lock().unwrap_or_else(|e| e.into_inner());
    // started by whoever held the lock before us
    if let Some(a) = state.get(worktree_path).filter(|a| a.alive()) {
        return Ok((a, false));
    }
    let f = files(app, project_root, worktree_path)?;
    let meta = f.read_meta().ok_or("이 워크트리에는 에이전트 설정이 없습니다")?;
    let want = start_want(want, *state.view_default.lock().unwrap());
    // the ACP runtime starts in the background: this returns at once either way
    let a = Agent::spawn(app.clone(), &home(app)?, worktree_path, &f, meta, want)?;
    state.agents.lock().unwrap().insert(worktree_path.to_lowercase(), a.clone());
    state.roots.lock().unwrap().insert(worktree_path.to_lowercase(), project_root.to_lowercase());
    a.clone().deliver_kept_notes(); // worker notes that came while it was off
    Ok((a, true))
}

/// The runtime a start runs on: the one asked for, else the app's default view's (the chat: ACP when it can).
/// `AgentMeta::last_view` is never looked at.
pub(crate) fn start_want(explicit: Option<acp_setup::Want>, default: agent::View) -> acp_setup::Want {
    explicit.unwrap_or(match default {
        agent::View::Chat => acp_setup::Want::Auto,
        agent::View::Cli => acp_setup::Want::Cli,
    })
}

/// The app's 에이전트 보기 ("chat" | "cli"), from the main window at launch and on every change: starts that
/// no view asked for (`ensure`) run on it.
#[tauri::command]
fn set_view_default(state: State<AppState>, view: String) -> Result<(), String> {
    *state.view_default.lock().unwrap() = agent::View::parse(&view)?;
    Ok(())
}

/// Stops the idle agents of projects left at least `minutes` ago. Running ones, ones waiting on an approval
/// and ones with a shell still going (a background Bash) stay. They resume (`--resume`) when next needed.
fn reap_away(app: &AppHandle) {
    let state = app.state::<AppState>();
    let (active, limit, left) = {
        let a = state.away.lock().unwrap();
        if a.minutes == 0 {
            return;
        }
        (a.visible(), Duration::from_secs(a.minutes * 60), a.left.clone())
    };
    let waiting: Vec<String> = app.state::<Approvals>().pending().iter().map(|p| p.worktree_path.to_lowercase()).collect();
    let roots = state.roots.lock().unwrap().clone();
    let due: Vec<Arc<Agent>> = state.agents.lock().unwrap().iter()
        .filter(|(k, a)| {
            let Some(root) = roots.get(*k) else { return false };
            // a project never opened this run counts as left at launch
            !active.contains(root)
                && left.get(root).is_none_or(|t| t.elapsed() >= limit)
                && *a.state.lock().unwrap() == AgentState::Idle
                && a.quiet_for() >= limit
                && !waiting.contains(k)
                && !a.switching_now()
        })
        .map(|(_, a)| a.clone())
        .collect();
    if due.is_empty() {
        return;
    }
    // can't see the process tree → can't rule out a background shell, so keep them all
    let Some(procs) = agent::processes() else { return };
    for a in due.iter().filter(|a| !a.runs_shell(&procs)) {
        a.kill();
    }
}

/// Structure the agent `owner` drew (`maestro draw`) since its canvas last looked; taking it empties the queue.
#[tauri::command]
fn take_canvas_drawings(state: State<AppState>, owner: String) -> Vec<serde_json::Value> {
    state.drawings.lock().unwrap().remove(&owner.to_lowercase()).unwrap_or_default()
}

/// What the user selected on agent `owner`'s canvas, for `maestro selection`; an empty list clears it.
#[tauri::command]
fn set_canvas_selection(state: State<AppState>, owner: String, items: serde_json::Value) {
    let mut s = state.selection.lock().unwrap();
    if items.as_array().map_or(true, |a| a.is_empty()) {
        s.remove(&owner.to_lowercase());
    } else {
        s.insert(owner.to_lowercase(), items);
    }
}

/// The projects on screen in the calling window (each window reports its own; a closed one is dropped).
#[tauri::command]
fn set_away(window: tauri::Window, state: State<AppState>, active_roots: Vec<String>, minutes: u64) {
    let mut a = state.away.lock().unwrap();
    a.show(window.label(), active_roots.iter().map(|r| r.to_lowercase()).collect());
    a.minutes = minutes;
}

// ---------- repos (~/.maestro/repos.json) ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RepoInfo {
    root: String,
    name: String,
    default_branch: String,
}

fn repo_info(root: &str) -> RepoInfo {
    RepoInfo {
        root: root.to_string(),
        name: Path::new(root).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| root.into()),
        default_branch: worktree::default_branch(root),
    }
}

fn repos_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(home(app)?.join(".maestro").join("repos.json"))
}

/// The format of `repos.json` this build writes (and the newest it reads; durable.rs).
pub(crate) const REPOS_VERSION: u64 = 1;

/// Add and remove read, change and write the whole list: one at a time, or a click could undo another's.
static REPOS: Mutex<()> = Mutex::new(());

pub(crate) fn read_repos(app: &AppHandle) -> Result<Vec<String>, String> {
    match durable::read_json(&repos_file(app)?, REPOS_VERSION) {
        Ok(v) => Ok(v.unwrap_or_default()),
        Err(e @ durable::StateError::Corrupt { .. }) => Err(format!("{e}. 프로젝트를 다시 추가하면 손상된 파일은 옆에 옮겨 두고 새로 만들어요")),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
async fn list_repos(app: AppHandle) -> Result<Vec<RepoInfo>, String> {
    Ok(read_repos(&app)?.iter().map(|r| repo_info(r)).collect())
}

/// "ready", "empty" (no commit yet) or "none" (not a git repo): the sidebar asks before `init_repo`.
#[tauri::command]
async fn repo_state(path: String) -> &'static str {
    worktree::repo_state(&path)
}

/// `git init` and/or a first commit, so the folder can be added as a project.
#[tauri::command]
async fn init_repo(path: String) -> Result<(), String> {
    worktree::init_repo(&path)
}

#[tauri::command]
async fn add_repo(app: AppHandle, path: String) -> Result<RepoInfo, String> {
    let root = worktree::toplevel(&path)?;
    let _one = REPOS.lock().unwrap_or_else(|e| e.into_inner());
    let file = repos_file(&app)?;
    // a corrupt list starts over (the write moves it aside); a newer Maestro's is left alone
    let mut roots: Vec<String> = durable::read_for_write(&file, REPOS_VERSION)?;
    // the same repo spelled differently (c: vs C:, \ vs /) keeps its first spelling: the project folder is named by it
    let root = match roots.iter().find(|r| same_root(r, &root)) {
        Some(known) => known.clone(),
        None => {
            roots.push(root.clone());
            durable::write_json(&file, &roots, REPOS_VERSION)?;
            root
        }
    };
    Ok(repo_info(&root))
}

/// Two spellings of one repo root: slashes either way, and case ignored where the file system ignores it.
fn same_root(a: &str, b: &str) -> bool {
    let (a, b) = (a.replace('\\', "/"), b.replace('\\', "/"));
    let (a, b) = (a.trim_end_matches('/'), b.trim_end_matches('/'));
    if cfg!(any(windows, target_os = "macos")) { a.eq_ignore_ascii_case(b) } else { a == b }
}

/// Forgets the repo in the sidebar; worktrees on disk are left alone.
#[tauri::command]
fn remove_repo(app: AppHandle, root: String) -> Result<(), String> {
    let _one = REPOS.lock().unwrap_or_else(|e| e.into_inner());
    let roots: Vec<String> = read_repos(&app)?.into_iter().filter(|r| *r != root).collect();
    durable::write_json(&repos_file(&app)?, &roots, REPOS_VERSION)
}

// ---------- worktrees & agents ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorktreeInfo {
    pub(crate) path: String,
    pub(crate) branch: String,
    pub(crate) adapter_id: Option<String>,
    permission_mode: Option<String>,
    pub(crate) name: Option<String>,
    description: Option<String>,
    /// passed as `--model` on every start; None = the CLI's own default
    model: Option<String>,
    pub(crate) state: AgentState,
    /// the project's Maestro (coordinator) on the project folder, not a worktree
    pub(crate) is_maestro: bool,
    /// child worktree paths (its workers in the project hierarchy)
    pub(crate) workers: Vec<String>,
    /// Markdown files it refers to
    pub(crate) refs: Vec<String>,
    /// a worktree made outside Maestro (found in the repo); discarding it only detaches the agent
    pub(crate) external: bool,
    /// titles of the personas linked to it
    pub(crate) personas: Vec<String>,
    /// Maestro only: where workers branch off and merge back
    base_branch: Option<String>,
    /// Maestro only: the project's merge mode ("local" | "pr")
    merge_mode: Option<project::MergeMode>,
    /// titles of the other (non-persona) documents it refers to
    docs: Vec<String>,
    /// "acp" | "pty" while the agent runs; None when it's off
    runtime: Option<catalog::Runtime>,
    /// why the chat runs on the terminal (`FallbackReason::view`: kind, message, ...)
    runtime_note: Option<serde_json::Value>,
}

fn info(state: &AppState, f: &Files, path: String, branch: String) -> WorktreeInfo {
    let meta = f.read_meta().unwrap_or_default();
    let some = |s: String| Some(s).filter(|s| !s.is_empty());
    let owner = state.get(&path).filter(|a| a.alive()).map(|a| a.owner());
    WorktreeInfo {
        runtime: owner.as_ref().map(|o| o.kind),
        runtime_note: owner.and_then(|o| o.note).map(|n| n.view()),
        state: state.get(&path).map(|a| a.state.lock().unwrap().clone()).unwrap_or(AgentState::Idle),
        adapter_id: some(meta.adapter_id),
        permission_mode: some(meta.permission_mode),
        name: meta.name,
        description: meta.description,
        model: meta.model.filter(|m| !m.is_empty()),
        is_maestro: meta.role.as_deref() == Some("maestro"),
        workers: meta.workers,
        refs: meta.refs,
        external: false,
        personas: vec![],
        base_branch: meta.base_branch,
        merge_mode: None,
        docs: vec![],
        path,
        branch,
    }
}

/// The project's worktrees, plus its Maestro (first) when one was created.
pub(crate) fn worktree_infos(app: &AppHandle, state: &AppState, project_root: &str) -> Result<Vec<WorktreeInfo>, String> {
    let pdir = worktree::project_dir(&home(app)?, project_root);
    let mut out = vec![];
    let mf = files(app, project_root, project_root)?;
    if mf.read_meta().is_some() {
        let mut m = info(state, &mf, project_root.replace('\\', "/"), worktree::current_branch(project_root));
        m.merge_mode = Some(project::merge_mode(&home(app)?, project_root));
        out.push(m);
    }
    for (path, branch) in worktree::list(project_root)? {
        let mut w = info(state, &files(app, project_root, &path)?, path, branch);
        w.external = !worktree::is_managed(&w.path, &pdir);
        out.push(w);
    }
    let home = home(app)?;
    for w in &mut out {
        let (personas, docs): (Vec<&String>, Vec<&String>) = w.refs.iter().partition(|r| library::is_persona(&home, r));
        let titles = |v: Vec<&String>| v.into_iter().map(|r| library::title_of(Path::new(r))).collect();
        (w.personas, w.docs) = (titles(personas), titles(docs));
    }
    Ok(out)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateMaestroReq {
    project_root: String,
    adapter_id: String,
    permission_mode: String,
    model: String,
    /// where its workers branch off ("" = the repo's default branch)
    #[serde(default)]
    base_branch: String,
}

/// What a new agent the user picked must be before anything is made: a known agent Maestro can run, with a mode it
/// takes.
fn check_new_agent(adapter_id: &str, permission_mode: &str) -> Result<(), String> {
    let def = catalog::require(adapter_id)?;
    if !def.launchable() {
        return Err(format!("{}은(는) 아직 에이전트로 실행할 수 없어요", def.name));
    }
    catalog::validate_permission(adapter_id, permission_mode)
}

/// The Maestro's project: a Git repo with a commit (it merges into a branch), and its base branch a local one.
fn check_maestro_project(project_root: &str, base_branch: &str) -> Result<(), String> {
    match worktree::repo_state(project_root) {
        "none" => return Err("Git 저장소가 아니에요".into()),
        "empty" => return Err("아직 커밋이 없는 저장소예요. 첫 커밋을 만든 뒤 다시 해 주세요".into()),
        _ => {}
    }
    check_base_branch(project_root, base_branch)
}

/// A base branch ("" = the repo's default) is a local branch of the project: merges go into it there.
fn check_base_branch(project_root: &str, base_branch: &str) -> Result<(), String> {
    if !base_branch.is_empty() && !worktree::has_local_branch(project_root, base_branch) {
        return Err(format!("이 프로젝트에 없는 브랜치예요: {base_branch}"));
    }
    Ok(())
}

/// Writes an agent's meta (`write`) and starts it (`start`); a start that fails takes the meta away again, so no
/// half-made agent stays and the same one can be made again (`Files::check_free` passes). A failed write leaves the
/// file as it was. `create_worktree` does the same with its worktree.
fn create_with_rollback<T>(meta: &Path, write: impl FnOnce() -> Result<(), String>, start: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    write()?;
    start().inspect_err(|_| {
        let _ = std::fs::remove_file(meta);
    })
}

/// The project's Maestro: a coordinator agent on the project folder itself (main branch), started right away.
/// Everything is checked before its meta is written.
#[tauri::command]
async fn create_maestro(app: AppHandle, state: State<'_, AppState>, req: CreateMaestroReq) -> Result<WorktreeInfo, String> {
    let f = files(&app, &req.project_root, &req.project_root)?;
    f.check_free("이 프로젝트에는 이미 마에스트로가 있어요")?;
    let base = req.base_branch.trim().to_string();
    check_new_agent(&req.adapter_id, &req.permission_mode)?;
    check_maestro_project(&req.project_root, &base)?;
    let meta = AgentMeta {
        adapter_id: req.adapter_id,
        permission_mode: req.permission_mode,
        model: Some(req.model.trim().to_string()).filter(|m| !m.is_empty()),
        name: Some("Maestro".into()),
        role: Some("maestro".into()),
        base_branch: Some(base).filter(|b| !b.is_empty()),
        ..Default::default()
    };
    create_with_rollback(&f.meta, || f.write_meta(&meta), || ensure(&app, &state, &req.project_root, &req.project_root))?;
    worktree_infos(&app, &state, &req.project_root)?.into_iter().find(|w| w.is_maestro).ok_or("마에스트로를 만들지 못했어요".into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AttachReq {
    project_root: String,
    worktree_path: String,
    adapter_id: String,
    permission_mode: String,
    model: String,
}

/// Puts an agent on a worktree that has none yet (one made outside Maestro), started right away.
#[tauri::command]
async fn attach_agent(app: AppHandle, state: State<'_, AppState>, req: AttachReq) -> Result<(), String> {
    let f = files(&app, &req.project_root, &req.worktree_path)?;
    f.check_free("이미 에이전트가 있는 워크트리예요")?;
    check_new_agent(&req.adapter_id, &req.permission_mode)?;
    let meta = AgentMeta {
        adapter_id: req.adapter_id,
        permission_mode: req.permission_mode,
        model: Some(req.model.trim().to_string()).filter(|m| !m.is_empty()),
        ..Default::default()
    };
    create_with_rollback(&f.meta, || f.write_meta(&meta), || ensure(&app, &state, &req.project_root, &req.worktree_path).map(|_| ()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateAgentReq {
    project_root: String,
    worktree_path: String,
    #[serde(flatten)]
    patch: agent::AgentPatch,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateAgentResult {
    info: WorktreeInfo,
    applied: agent::Applied,
}

/// The agent settings dialog: name, description, permission mode, model and (the Maestro's) base branch of an
/// agent made before. Saved at once; a running agent keeps what it started with until it restarts
/// (`applied: "restart"`, `restart_agent`). Under the start lock, so no start reads the meta half-way, and as one
/// `update_meta`, so a turn writing its conversation id meanwhile neither undoes the change nor is undone by it.
#[tauri::command]
async fn update_agent(app: AppHandle, state: State<'_, AppState>, req: UpdateAgentReq) -> Result<UpdateAgentResult, String> {
    let lock = state.start_lock(&req.worktree_path);
    let applied = {
        let _one = lock.lock().unwrap_or_else(|e| e.into_inner());
        let f = files(&app, &req.project_root, &req.worktree_path)?;
        let alive = state.get(&req.worktree_path).is_some_and(|a| a.alive());
        f.update_meta(|m| {
            let before = m.clone();
            let is_maestro = before.role.as_deref() == Some("maestro");
            let after = before.patched(&req.patch, is_maestro)?;
            // only what changes is checked: an agent made before a rule keeps working
            if after.permission_mode != before.permission_mode {
                catalog::validate_permission(&after.adapter_id, &after.permission_mode)?;
            }
            if after.base_branch != before.base_branch {
                check_base_branch(&req.project_root, after.base_branch.as_deref().unwrap_or(""))?;
            }
            *m = after;
            Ok(agent::applied(alive, &before, m))
        })?
        .ok_or("이 워크트리에는 에이전트가 없어요")?
    };
    let key = util::path_key(&req.worktree_path);
    let info = worktree_infos(&app, &state, &req.project_root)?.into_iter().find(|w| util::path_key(&w.path) == key).ok_or("저장했지만 에이전트 목록에서 찾지 못했어요")?;
    Ok(UpdateAgentResult { info, applied })
}

/// Restarts an idle agent on the same conversation and view (`Agent::restart`), so a changed mode or model takes
/// effect. Refused while it's off, works, waits on the user or switches views (`agent::can_restart`).
#[tauri::command]
async fn restart_agent(app: AppHandle, project_root: String, worktree_path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        // its meta first: an agent discarded meanwhile has nothing to restart
        files(&app, &project_root, &worktree_path)?.read_meta().ok_or("이 워크트리에는 에이전트가 없어요")?;
        let a = state.get(&worktree_path).filter(|a| a.alive());
        let key = util::path_key(&worktree_path);
        let approval = app.state::<Approvals>().pending().iter().any(|p| util::path_key(&p.worktree_path) == key);
        let st = a.as_ref().map_or(AgentState::Terminated, |a| a.state.lock().unwrap().clone());
        let waiting = approval || a.as_ref().is_some_and(|a| a.tui_wait().is_some());
        agent::can_restart(a.is_some(), &st, waiting, a.as_ref().is_some_and(|a| a.switching_now()))?;
        let a = a.ok_or(agent::RESTART_OFF)?;
        a.restart()?;
        a.deliver_kept_notes();
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------- hierarchy (parent → child worktrees) and references (Markdown → worktree) ----------

/// Every agent of the project (Maestro first, then worktrees) with its files.
fn agent_files(app: &AppHandle, project_root: &str) -> Result<Vec<(String, Files)>, String> {
    let mut out = vec![(project_root.replace('\\', "/"), files(app, project_root, project_root)?)];
    for (p, _) in worktree::list(project_root)? {
        let f = files(app, project_root, &p)?;
        out.push((p, f));
    }
    Ok(out.into_iter().filter(|(_, f)| f.meta.exists()).collect())
}

/// Makes `child` a worker of `parent`: a tree, so the child leaves any previous parent, and a link
/// that would loop back (child is an ancestor of parent) is refused. The Maestro is never a child.
pub(crate) fn link(app: &AppHandle, project_root: &str, parent: &str, child: &str) -> Result<(), String> {
    if is_root(project_root, child) {
        return Err("마에스트로는 다른 블록의 워커가 될 수 없어요".into());
    }
    let agents = agent_files(app, project_root)?;
    let metas: Vec<(String, &Files, AgentMeta)> = agents.iter().filter_map(|(p, f)| f.read_meta().map(|m| (p.clone(), f, m))).collect();
    let tree: Vec<(String, Vec<String>)> = metas.iter().map(|(p, _, m)| (p.clone(), m.workers.clone())).collect();
    for ((_, f, m), workers) in metas.iter().zip(relink(&tree, parent, child)?) {
        if workers != m.workers {
            // only the workers: whatever else changed since the read above stays
            f.update_meta(|cur| {
                cur.workers = workers;
                Ok(())
            })?;
        }
    }
    Ok(())
}

/// The hierarchy after making `child` a worker of `parent`: (agent path, its workers) in, new workers per
/// agent out. A tree, so the child leaves its previous parent; a link that would loop back is refused.
fn relink(tree: &[(String, Vec<String>)], parent: &str, child: &str) -> Result<Vec<Vec<String>>, String> {
    let same = |a: &str, b: &str| a.replace('\\', "/").eq_ignore_ascii_case(&b.replace('\\', "/"));
    if same(parent, child) {
        return Err("자기 자신과는 연결할 수 없어요".into());
    }
    if !tree.iter().any(|(p, _)| same(p, parent)) {
        return Err("상위 블록에 에이전트 설정이 없어요".into());
    }
    let parent_of = |x: &str| tree.iter().find(|(_, ws)| ws.iter().any(|w| same(w, x))).map(|(p, _)| p.clone());
    let mut up = Some(parent.to_string());
    while let Some(p) = up {
        if same(&p, child) {
            return Err("연결하면 고리가 생겨요 (하위 블록을 상위로 둘 수 없어요)".into());
        }
        up = parent_of(&p);
    }
    Ok(tree
        .iter()
        .map(|(p, ws)| {
            let mut ws: Vec<String> = ws.iter().filter(|w| !same(w, child)).cloned().collect();
            if same(p, parent) {
                ws.push(child.replace('\\', "/"));
            }
            ws
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_start_follows_the_app_view_not_the_agent() {
        use acp_setup::Want;
        // asked for: that one wins
        assert_eq!(start_want(Some(Want::Cli), agent::View::Chat), Want::Cli);
        assert_eq!(start_want(Some(Want::Auto), agent::View::Cli), Want::Auto);
        // nobody asked: the app's default view
        assert_eq!(start_want(None, agent::View::Chat), Want::Auto);
        assert_eq!(start_want(None, agent::View::Cli), Want::Cli);
        // an old meta that remembered the terminal still reads, and changes nothing
        let old: agent::AgentMeta = serde_json::from_str(r#"{"adapterId":"claude-code","permissionMode":"default","lastView":"cli"}"#).unwrap();
        assert_eq!(old.last_view.as_deref(), Some("cli"));
        assert_eq!(start_want(None, agent::View::Chat), Want::Auto);
        // the default before the main window says anything
        assert_eq!(*AppState::default().view_default.lock().unwrap(), agent::View::Chat);
    }

    #[test]
    fn marked_path_reads_between_markers() {
        assert_eq!(marked_path("hi\n__MAESTRO_PATH__/a:/b__MAESTRO_PATH__bye"), Some("/a:/b"));
        assert_eq!(marked_path("no markers"), None);
        assert_eq!(marked_path("__MAESTRO_PATH____MAESTRO_PATH__"), None);
        assert_eq!(marked_path("__MAESTRO_PATH__/cut"), None);
    }

    #[cfg(unix)]
    #[test]
    fn login_shell_probe_is_bounded() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("maestro-shellprobe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = |name: &str, body: &str| {
            let p = dir.join(name);
            std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            p.to_string_lossy().to_string()
        };
        // an rc that never finishes: startup goes on with the inherited PATH
        let slow = script("slow", "sleep 30");
        let t = std::time::Instant::now();
        assert_eq!(login_shell_path(&slow, 1.0), None);
        assert!(t.elapsed() < std::time::Duration::from_secs(10), "{:?}", t.elapsed());
        // a greeting around the markers, and the probe marker reaches the rc
        let ok = script("ok", "printf 'motd\\n__MAESTRO_PATH__/x:%s__MAESTRO_PATH__' \"$MAESTRO_SHELL_PROBE\"");
        assert_eq!(login_shell_path(&ok, 5.0).as_deref(), Some("/x:1"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn away_clock_counts_projects_no_window_shows() {
        let set = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<HashSet<_>>();
        let mut a = Away::default();
        a.show("main", set(&["a", "b"]));
        a.show("ws-1", set(&["b", "c"]));
        assert!(a.left.is_empty());
        a.show("main", set(&["a"])); // b still in the pop-out
        assert!(!a.left.contains_key("b"));
        a.show("ws-1", HashSet::new()); // pop-out closed: b and c are away now
        assert!(a.left.contains_key("b") && a.left.contains_key("c") && !a.left.contains_key("a"));
        a.show("main", set(&["a", "c"])); // c back on screen
        assert!(!a.left.contains_key("c"));
    }

    #[test]
    fn repo_root_spellings_match() {
        assert!(same_root("C:/dev/app", "C:/dev/app/"));
        assert!(same_root("C:\\dev\\app", "C:/dev/app"));
        assert!(!same_root("C:/dev/app", "C:/dev/app2"));
        #[cfg(windows)]
        assert!(same_root("c:/Dev/App", "C:/dev/app"));
    }

    #[test]
    fn editor_paths_stay_inside_the_worktree() {
        let base = std::env::temp_dir().join(format!("maestro-inside-{}", std::process::id()));
        std::fs::create_dir_all(base.join("src")).unwrap();
        std::fs::write(base.join("src/a.ts"), "x").unwrap();
        let b = base.to_string_lossy();
        assert!(inside(&b, "src/a.ts").is_ok());
        assert!(inside(&b, "src/new.ts").is_ok()); // not created yet
        assert!(inside(&b, "../outside.txt").is_err());
        assert!(inside(&b, "src/../../outside.txt").is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn relink_keeps_a_tree() {
        let t = |v: &[(&str, &[&str])]| v.iter().map(|(p, ws)| (p.to_string(), ws.iter().map(|w| w.to_string()).collect())).collect::<Vec<(String, Vec<String>)>>();
        let tree = t(&[("M", &["A"]), ("A", &["B"]), ("B", &[]), ("C", &[])]);
        // C moves under A
        assert_eq!(relink(&tree, "A", "C").unwrap(), [vec!["A"], vec!["B", "C"], vec![], vec![]]);
        // B moves from A to M: one parent only
        assert_eq!(relink(&tree, "M", "B").unwrap(), [vec!["A", "B"], vec![], vec![], vec![]]);
        // A under its own descendant B would loop
        assert!(relink(&tree, "B", "A").is_err());
        assert!(relink(&tree, "A", "a").is_err()); // self, case-insensitive
        assert!(relink(&tree, "X", "C").is_err()); // unknown parent
    }

    #[test]
    fn a_failed_start_takes_the_meta_away() {
        let dir = util::unique_temp_dir("maestro-rollback");
        let f = Files { meta: dir.join("agents").join("maestro.json") };
        let meta = AgentMeta { adapter_id: "claude-code".into(), permission_mode: "default".into(), role: Some("maestro".into()), ..Default::default() };
        let r: Result<(), String> = create_with_rollback(&f.meta, || f.write_meta(&meta), || Err("시작 실패".into()));
        assert_eq!(r, Err("시작 실패".into()));
        assert!(!f.meta.exists());
        // nothing half-made: the same one can be made again
        assert!(f.check_free("이미 있어요").is_ok());
        assert_eq!(create_with_rollback(&f.meta, || f.write_meta(&meta), || Ok(7)), Ok(7));
        assert_eq!(f.read_meta().map(|m| m.permission_mode), Some("default".into()));
        assert!(f.check_free("이미 있어요").is_err());
        // a write that fails starts nothing
        let mut started = false;
        let r: Result<(), String> = create_with_rollback(&f.meta, || Err("못 씀".into()), || {
            started = true;
            Ok(())
        });
        assert!(r.is_err() && !started && f.meta.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_maestro_needs_a_repo_with_a_commit_and_a_real_base_branch() {
        let dir = util::unique_temp_dir("maestro-check");
        let (plain, empty, repo) = (dir.join("plain"), dir.join("empty"), dir.join("repo"));
        for d in [&plain, &empty, &repo] {
            std::fs::create_dir_all(d).unwrap();
        }
        let s = |p: &PathBuf| p.to_string_lossy().replace('\\', "/");
        std::fs::write(repo.join("a.txt"), "a").unwrap();
        worktree::init_repo(&s(&repo)).unwrap(); // branch main, one commit
        assert!(std::process::Command::new("git").arg("-C").arg(&empty).args(["init", "-b", "main"]).output().unwrap().status.success());
        assert!(check_maestro_project(&s(&plain), "").is_err());
        assert!(check_maestro_project(&s(&empty), "").is_err());
        assert!(check_maestro_project(&s(&repo), "").is_ok());
        assert!(check_maestro_project(&s(&repo), "main").is_ok());
        assert_eq!(check_maestro_project(&s(&repo), "nope"), Err("이 프로젝트에 없는 브랜치예요: nope".into()));
        // the agent and its mode are checked too, before anything is written
        assert!(check_new_agent("claude-code", "default").is_ok());
        assert!(check_new_agent("antigravity", "default").is_err());
        assert!(check_new_agent("codex", "default").is_err()); // not launchable yet
        assert!(check_new_agent("nope", "default").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[tauri::command]
fn link_worktrees(app: AppHandle, project_root: String, parent: String, child: String) -> Result<(), String> {
    link(&app, &project_root, &parent, &child)
}

#[tauri::command]
fn unlink_worktrees(app: AppHandle, project_root: String, parent: String, child: String) -> Result<(), String> {
    let f = files(&app, &project_root, &parent)?;
    f.update_meta(|m| {
        m.workers.retain(|w| !w.eq_ignore_ascii_case(&child));
        Ok(())
    })
    .map(|_| ())
}

/// Adds or removes a Markdown reference on one agent; with `worktree` empty, removes `md` from every agent
/// (its last block was deleted from the canvas).
#[tauri::command]
fn set_ref(app: AppHandle, state: State<AppState>, project_root: String, worktree: String, md: String, on: bool) -> Result<(), String> {
    set_reference(&app, &state, &project_root, &worktree, &md, on)
}

/// Shared by the canvases and `maestro ref add/remove`.
pub(crate) fn set_reference(app: &AppHandle, state: &AppState, project_root: &str, worktree: &str, md: &str, on: bool) -> Result<(), String> {
    let (app, worktree, md) = (app.clone(), worktree.to_string(), md.to_string());
    set_ref_meta(&app, project_root, &worktree, &md, on)?;
    // a persona takes effect at the agent's next start (system brief); a running agent hears it now
    let home = home(&app)?;
    if !worktree.is_empty() && library::is_persona(&home, &md) {
        if let Some(agent) = state.get(&worktree).filter(|a| a.alive()) {
            let title = library::title_of(Path::new(&md));
            let text = if on {
                format!("[Maestro] 이 워크트리의 페르소나가 '{title}'(으)로 정해졌어요. 지금부터 아래 지침을 따르세요.\n\n{}", std::fs::read_to_string(&md).unwrap_or_default().trim())
            } else {
                format!("[Maestro] 페르소나 '{title}'이(가) 해제됐어요. 그 지침은 더 이상 따르지 않아도 돼요.")
            };
            std::thread::spawn(move || {
                if agent.wait_ready() == Ok(false) {
                    let _ = agent.send(&text, &[]);
                }
            });
        }
    }
    Ok(())
}

fn set_ref_meta(app: &AppHandle, project_root: &str, worktree: &str, md: &str, on: bool) -> Result<(), String> {
    let targets = if worktree.is_empty() { agent_files(app, project_root)?.into_iter().map(|(_, f)| f).collect() } else { vec![files(app, project_root, worktree)?] };
    for f in targets {
        // an unreadable meta is skipped (the user is told), as before
        if f.read_meta().is_none() {
            continue;
        }
        f.update_meta(|m| {
            let had = m.refs.iter().any(|r| r.eq_ignore_ascii_case(md));
            if on && !had {
                m.refs.push(md.replace('\\', "/"));
            } else if !on && had {
                m.refs.retain(|r| !r.eq_ignore_ascii_case(md));
            }
            Ok(())
        })?;
    }
    Ok(())
}

/// Where new worktrees branch off by default: the Maestro's base branch, else the repo's default branch.
pub(crate) fn base_branch(app: &AppHandle, project_root: &str) -> String {
    files(app, project_root, project_root).ok().and_then(|f| f.read_meta()).and_then(|m| m.base_branch).unwrap_or_else(|| worktree::default_branch(project_root))
}

#[tauri::command]
async fn list_branches(project_root: String) -> Result<Vec<worktree::Branch>, String> {
    worktree::branches(&project_root)
}

#[tauri::command]
async fn list_worktrees(app: AppHandle, state: State<'_, AppState>, project_root: String) -> Result<Vec<WorktreeInfo>, String> {
    worktree_infos(&app, &state, &project_root)
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreateAgentReq {
    pub(crate) project_root: String,
    pub(crate) adapter_id: String,
    pub(crate) permission_mode: String,
    pub(crate) branch_name: String,
    pub(crate) base_branch: String,
    /// check out `branch_name` as it is (an existing branch) instead of making it new from `base_branch`
    #[serde(default)]
    pub(crate) existing_branch: bool,
    pub(crate) name: String,
    pub(crate) description: String,
    /// "" = the CLI's own default
    pub(crate) model: String,
}

/// New worktree + its agent, started right away.
#[tauri::command]
async fn create_agent(app: AppHandle, state: State<'_, AppState>, req: CreateAgentReq) -> Result<WorktreeInfo, String> {
    // before the worktree is made: a mode the agent doesn't take would otherwise run as another (agy)
    check_new_agent(&req.adapter_id, &req.permission_mode)?;
    create_worktree(&app, &state, req)
}

/// Shared by the new-worktree dialog and `maestro worktree create`.
pub(crate) fn create_worktree(app: &AppHandle, state: &AppState, req: CreateAgentReq) -> Result<WorktreeInfo, String> {
    let app = app.clone();
    let pdir = worktree::project_dir(&home(&app)?, &req.project_root);
    let keep_branch = req.existing_branch;
    let (path, branch) = worktree::create(&req.project_root, &pdir, &req.branch_name, &req.base_branch, keep_branch)?;
    let path = worktree::norm(&path);
    let f = Files::new(&pdir, &path);
    let some = |s: String| Some(s.trim().to_string()).filter(|s| !s.is_empty());
    let meta = AgentMeta {
        adapter_id: req.adapter_id,
        permission_mode: req.permission_mode,
        name: some(req.name),
        description: some(req.description),
        model: some(req.model),
        keep_branch,
        ..Default::default()
    };
    // a `maestro send` to the new path can't start a second agent while this one starts
    let lock = state.start_lock(&path);
    let _one = lock.lock().unwrap_or_else(|e| e.into_inner());
    let started = f.write_meta(&meta).and_then(|_| Agent::spawn(app.clone(), &home(&app)?, &path, &f, meta, acp_setup::Want::Auto));
    match started {
        Ok(a) => {
            state.agents.lock().unwrap().insert(path.to_lowercase(), a);
            Ok(info(state, &f, path, branch))
        }
        Err(e) => {
            // don't leave an orphan worktree behind a failed start
            let _ = worktree::discard(&req.project_root, &path, &branch, &req.base_branch, keep_branch, worktree::DiscardChoice::Clean, &[]);
            let _ = std::fs::remove_file(&f.meta);
            Err(e)
        }
    }
}

/// Chat input: pasted into the agent's terminal, starting (or resuming) it first if needed.
#[tauri::command]
async fn send_message(app: AppHandle, state: State<'_, AppState>, project_root: String, worktree_path: String, text: String, files_attached: Vec<String>) -> Result<(), String> {
    let (agent, _) = ensure(&app, &state, &project_root, &worktree_path)?;
    tauri::async_runtime::spawn_blocking(move || {
        // a TUI that is still starting would drop the paste; one showing the trust dialog would answer it
        match agent.wait_ready() {
            Ok(false) => agent.send(&text, &files_attached),
            Ok(true) => Err("폴더 신뢰 확인이 먼저 필요해요".to_string()),
            Err(f) => Err(f.message().to_string()),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The CLI view mounts a terminal: make sure there is one to show.
#[tauri::command]
async fn ensure_agent(app: AppHandle, state: State<'_, AppState>, project_root: String, worktree_path: String) -> Result<(), String> {
    ensure(&app, &state, &project_root, &worktree_path).map(|_| ())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Readiness {
    trust_prompt: bool,
    /// why the agent didn't come up (login needed, exited, timed out); None once it's up
    failure: Option<LaunchFailure>,
    /// "acp" | "pty": what runs it now (after a fallback: the terminal)
    runtime: catalog::Runtime,
    /// why the chat runs on the terminal (`FallbackReason::view`)
    runtime_note: Option<serde_json::Value>,
}

/// The chat view waits on this (showing a loader) until the agent takes prompts.
#[tauri::command]
async fn agent_ready(app: AppHandle, state: State<'_, AppState>, project_root: String, worktree_path: String) -> Result<Readiness, String> {
    let (agent, _) = ensure(&app, &state, &project_root, &worktree_path)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (trust_prompt, failure) = match agent.wait_ready() {
            Ok(t) => (t, None),
            Err(f) => (false, Some(f)),
        };
        // up now after a start that wasn't (a retry after a login, a slow start): notes kept meanwhile go now
        if !trust_prompt && failure.is_none() {
            agent.clone().deliver_kept_notes();
        }
        let o = agent.owner();
        Readiness { trust_prompt, failure, runtime: o.kind, runtime_note: o.note.map(|n| n.view()) }
    })
    .await
    .map_err(|e| e.to_string())
}

/// No agent running: it starts on `view`'s runtime (the CLI's terminal, else the chat's). Nothing is remembered:
/// the next start nobody asks a view of follows the app's default (`start_want`).
fn start_on_view(app: &AppHandle, state: &AppState, project_root: &str, worktree_path: &str, view: agent::View) -> Result<agent::SwitchResult, String> {
    let want = if view == agent::View::Cli { acp_setup::Want::Cli } else { acp_setup::Want::Auto };
    let (a, _) = ensure_with(app, state, project_root, worktree_path, Some(want))?;
    Ok(a.switch_done())
}

/// The chat ↔ CLI switch (`Agent::switch`): `view` "chat" | "cli", `when` "ifIdle" | "afterTurn" | "interrupt".
/// No agent running: it starts on the view's runtime.
#[tauri::command]
async fn switch_runtime(app: AppHandle, project_root: String, worktree_path: String, view: String, when: String) -> Result<agent::SwitchResult, String> {
    let (view, when) = (agent::View::parse(&view)?, agent::When::parse(&when)?);
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        // "off": it went off while the switch waited (a crash mid-turn): started anew, like no agent at all
        if let Some(r) = state.get(&worktree_path).filter(|a| a.alive()).map(|a| a.switch(view, when)) {
            if !matches!(&r, Ok(r) if r.status == "off") {
                return r;
            }
        }
        start_on_view(&app, &state, &project_root, &worktree_path, view)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// A view mounts (the chat's boot, the CLI's terminal): no agent running → it starts on the view's runtime, as
/// `switch_runtime` would; a running one is left as it is, whichever runtime holds the conversation (only the
/// toggle and the CLI view's "CLI로 전환" move it: two places may show the same worktree in different views, and a
/// mount or a zoom must not take the conversation from the other one). Answers what runs it now ("done").
#[tauri::command]
async fn start_view(app: AppHandle, project_root: String, worktree_path: String, view: String) -> Result<agent::SwitchResult, String> {
    let view = agent::View::parse(&view)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        if let Some(a) = state.get(&worktree_path).filter(|a| a.alive()) {
            return Ok(a.switch_done());
        }
        start_on_view(&app, &state, &project_root, &worktree_path, view)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// After a "stuck" switch, with the user's yes: the conversation copied (`session/fork`) and opened in `view`.
/// "off": the old process went off while the copy was made; the agent starts once, on the copy, like
/// `switch_runtime` starts one.
#[tauri::command]
async fn fork_session(app: AppHandle, project_root: String, worktree_path: String, view: String) -> Result<agent::SwitchResult, String> {
    let view = agent::View::parse(&view)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let a = state.get(&worktree_path).filter(|a| a.alive()).ok_or("에이전트가 실행 중이 아니에요")?;
        let r = a.fork(view)?;
        if r.status != "off" {
            return Ok(r);
        }
        start_on_view(&app, &state, &project_root, &worktree_path, view)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Settings: installs the chat runtime's adapter (`acp_setup::install`). Progress comes as "acp://install"
/// `{ id, kind: "line" | "done" | "failed", text }`.
#[tauri::command]
fn acp_install(app: AppHandle, id: String) -> Result<(), String> {
    let def = catalog::require(&id)?;
    let data = acp_setup::app_data().ok_or("앱 데이터 폴더를 찾지 못했어요")?.to_path_buf();
    let a = app.clone();
    acp_setup::start_install(def, data, move |kind, text| {
        let _ = a.emit("acp://install", serde_json::json!({ "id": def.id, "kind": kind, "text": text }));
    })
}

#[tauri::command]
fn acp_install_cancel(id: String) {
    acp_setup::cancel_install(&id);
}

/// Settings: the chat runtime for an agent, "auto" | "pty". Running agents keep theirs until their next start.
#[tauri::command]
fn set_chat_runtime(app: AppHandle, id: String, mode: String) -> Result<(), String> {
    let pref = match mode.as_str() {
        "auto" => acp_setup::Pref::Auto,
        "pty" => acp_setup::Pref::Pty,
        _ => return Err(format!("알 수 없는 방식: {mode}")),
    };
    acp_setup::set_pref(&home(&app)?, &id, pref)?;
    acp_setup::invalidate();
    Ok(())
}

#[tauri::command]
fn accept_trust(state: State<AppState>, worktree_path: String) -> Result<(), String> {
    let agent = state.get(&worktree_path).ok_or("에이전트가 실행 중이 아니에요")?;
    agent.accept_trust()?;
    agent.deliver_kept_notes(); // worker notes held back while the trust dialog was up
    Ok(())
}

/// Interrupts the current turn (Esc in the TUI); the agent keeps running.
#[tauri::command]
fn stop_agent(state: State<AppState>, worktree_path: String) -> Result<(), String> {
    state.get(&worktree_path).map_or(Ok(()), |a| a.interrupt())
}

/// Short summary of the latest answer for the project canvas, made by the assistant model from settings
/// (Haiku by default) and cached in the agent's meta until the answer or the model changes.
/// None when there is nothing to summarize yet.
#[tauri::command]
async fn summarize_agent(app: AppHandle, project_root: String, worktree_path: String, assistant_agent: String, assistant_model: String) -> Result<Option<String>, String> {
    let f = files(&app, &project_root, &worktree_path)?;
    let home = home(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        // one at a time: a second caller (StrictMode's double effect, two cards) then finds the fresh cache
        // instead of starting its own model call and overwriting the first summary with a different wording
        static SUMMARIZING: Mutex<()> = Mutex::new(());
        let _one = SUMMARIZING.lock().unwrap_or_else(|e| e.into_inner());
        let Some(meta) = f.read_meta() else { return Ok(None) };
        // every agent's answers are summarized by Haiku (needs Claude Code installed; otherwise the card shows the raw text)
        let live = app.state::<AppState>().get(&worktree_path).map(|a| a.shared_runtime());
        let text = events::recent_turn_text(&runtime::history(live.as_deref(), &home, &meta, &worktree_path), 4000);
        if text.is_empty() {
            return Ok(None);
        }
        let key = {
            use std::hash::{DefaultHasher, Hash, Hasher};
            let mut h = DefaultHasher::new();
            // a different assistant model or prompt makes a fresh summary
            (agent::SUMMARY_PROMPT, &assistant_agent, &assistant_model, &text).hash(&mut h);
            format!("{:x}", h.finish())
        };
        if meta.summary_key.as_deref() == Some(&key) {
            return Ok(meta.summary);
        }
        let summary = agent::summarize(&text, &assistant_agent, &assistant_model)?;
        // re-read: the model call takes seconds, and meanwhile the live tail may have followed a /clear or a
        // newer agy conversation (cli_session_id). Writing the copy read above would put the old id back.
        f.update_meta(|fresh| {
            fresh.summary = Some(summary.clone());
            fresh.summary_key = Some(key);
            Ok(())
        })?;
        Ok(Some(summary))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Every agent Maestro knows (catalog.rs): names, logos, install, runtimes, features. Static; no detection.
#[tauri::command]
fn agent_catalog() -> &'static [catalog::AgentDef] {
    &catalog::AGENTS
}

/// Models an agent offers: Claude's aliases, or what `agy models` lists for the account.
#[tauri::command]
async fn agent_models(id: String) -> Result<Vec<cli::ModelChoice>, String> {
    let def = catalog::require(&id)?;
    tauri::async_runtime::spawn_blocking(move || cli::models(def)).await.map_err(|e| e.to_string())?
}

/// The command a new worktree with this agent would run, for the settings tile.
#[tauri::command]
fn launch_preview(id: String, permission_mode: String, model: String) -> String {
    agent::launch_preview(&id, &permission_mode, &model)
}

/// Settings' setup terminal for an agent CLI: install if needed, then run it to log in. Lives in the
/// agents map under `setup:<id>`, so the pty_* commands and the CLI view's XTerm work on it unchanged.
#[tauri::command]
async fn setup_start(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<String, String> {
    let key = format!("setup:{id}");
    if state.get(&key).is_some_and(|a| a.alive()) {
        return Ok(key);
    }
    let argv = cli::setup_argv(&id)?;
    let a = Agent::run(app.clone(), &key, &argv, &home(&app)?, &[])?;
    state.agents.lock().unwrap().insert(key.clone(), Arc::new(a));
    Ok(key)
}

#[tauri::command]
fn setup_stop(state: State<AppState>, id: String) {
    if let Some(a) = state.agents.lock().unwrap().remove(&format!("setup:{id}")) {
        a.kill();
    }
}

#[tauri::command]
async fn cli_status(recheck: Option<bool>) -> Result<cli::CliStatus, String> {
    let recheck = recheck.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || cli::status(recheck)).await.map_err(|e| e.to_string())
}

/// Permission prompts waiting for an answer (for blocks mounted after the event fired).
#[tauri::command]
fn list_approvals(approvals: State<Approvals>) -> Vec<Approval> {
    approvals.pending()
}

/// Agents whose terminal waits for an answer only the CLI view can give (for blocks mounted after the event fired).
#[tauri::command]
fn list_tui_waits(state: State<AppState>) -> Vec<tui_wait::TuiWait> {
    state.agents.lock().unwrap().values().filter_map(|a| a.tui_wait()).collect()
}

/// choice: "allow" | "always" | "deny" | "cli"
#[tauri::command]
fn answer_approval(approvals: State<Approvals>, id: u64, choice: String) {
    approvals.answer(id, &choice);
}

/// Merge requests waiting for the user (for cards mounted after the event fired, and after a restart).
#[tauri::command]
fn list_merges(merges: State<merge::Merges>) -> Vec<merge::MergeRequest> {
    merges.list()
}

/// The merge card's answer: approve (the app merges with `--no-ff`, or in PR mode pushes and opens the PR with
/// `pr`'s title and body) or reject (with an optional reason). Err: it can't go ahead now (the project folder isn't
/// clean, the remote moved, gh isn't logged in …) or was answered already; the card stays.
#[tauri::command]
async fn answer_merge(app: AppHandle, id: u64, approve: bool, reason: Option<String>, pr: Option<pr::PrOptions>) -> Result<merge::MergeResult, String> {
    tauri::async_runtime::spawn_blocking(move || merge::answer(&app, id, approve, reason, pr)).await.map_err(|e| e.to_string())?
}

// ---------- PR mode (R6): the project's merge mode, pull requests, CI ----------

#[tauri::command]
fn project_merge_mode(app: AppHandle, root: String) -> Result<project::MergeMode, String> {
    Ok(project::merge_mode(&home(&app)?, &root))
}

/// The 프로젝트 setting: "local" | "pr". Err: why PR mode can't be used (the mode stays) or a PR action runs.
#[tauri::command]
async fn set_merge_mode(app: AppHandle, root: String, mode: String) -> Result<(), String> {
    let mode = project::MergeMode::parse(&mode)?;
    tauri::async_runtime::spawn_blocking(move || {
        let host = pr::AppHost::new(&app)?;
        pr::set_merge_mode(&host, &app.state::<merge::Merges>(), &root, mode)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Whether PR mode can be used for the project and why not (cached 5 minutes; `recheck` asks again).
#[tauri::command]
async fn pr_readiness(app: AppHandle, root: String, recheck: Option<bool>) -> Result<github::PrReadiness, String> {
    let base = base_branch(&app, &root);
    tauri::async_runtime::spawn_blocking(move || github::readiness(&github::Tools::system(), &root, &base, recheck.unwrap_or(false), merge::now_ms())).await.map_err(|e| e.to_string())
}

/// Every pull request the app knows (cards mounted after the event fired, and after a restart).
#[tauri::command]
fn list_prs(prs: State<pr::Prs>) -> Vec<pr::PrRecord> {
    prs.list().into_iter().filter(|r| !r.hidden).collect()
}

/// "다시 확인": the project's PRs (every project's with no root) are polled now.
#[tauri::command]
fn refresh_prs(root: Option<String>) {
    pr::refresh(root.as_deref());
}

/// The PrCard's merge: "merge" | "squash", pinned to the head the card showed; `ack`: the red CI state and the change
/// request the user confirmed "그래도 병합" for (anything else found then is refused).
#[tauri::command]
async fn merge_pr(app: AppHandle, worker_path: String, method: String, head_sha: String, ack: Option<pr::MergeAck>) -> Result<pr::PrRecord, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let host = pr::AppHost::new(&app)?;
        pr::merge_pr(&host, &app.state::<merge::Merges>(), &app.state::<pr::Prs>(), &worker_path, &method, &head_sha, &ack.unwrap_or_default())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// "기준 브랜치 받기": fast-forward the project folder to the merged base when it can be.
#[tauri::command]
async fn pr_sync_base(app: AppHandle, worker_path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let host = pr::AppHost::new(&app)?;
        pr::pr_sync_base(&host, &app.state::<merge::Merges>(), &app.state::<pr::Prs>(), &worker_path)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// A failed CI job's log, trimmed, as text for the log dialog (never stored or sent by itself).
#[tauri::command]
async fn pr_failed_log(app: AppHandle, worker_path: String, job_id: Option<u64>) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let host = pr::AppHost::new(&app)?;
        pr::failed_log(&host, &app.state::<pr::Prs>(), &worker_path, job_id)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn open_pr(prs: State<pr::Prs>, worker_path: String) -> Result<(), String> {
    pr::open_pr(&prs, &worker_path)
}

/// "닫기" on a merged or closed PR's card.
#[tauri::command]
fn dismiss_pr(app: AppHandle, prs: State<pr::Prs>, worker_path: String) {
    if prs.update(&worker_path, |r| r.hidden = true).is_some() {
        let _ = app.emit("maestro://pr-done", serde_json::json!({ "workerPath": worker_path }));
    }
}

/// One file of a merge request for the diff dialog: at the recorded merge base and at the requested tip.
#[tauri::command]
async fn merge_file(app: AppHandle, id: u64, path: String) -> Result<merge::MergeFileDiff, String> {
    let req = app.state::<merge::Merges>().get(id).ok_or("이미 처리된 요청이에요")?;
    tauri::async_runtime::spawn_blocking(move || merge::file_diff(&req, &path)).await.map_err(|e| e.to_string())?
}

/// The chat composer's `/` menu for this worktree's agent: its built-ins, then its custom commands and skills.
/// Under the ACP runtime, what the adapter doesn't take is marked for the CLI view, and what only it reports is
/// added.
#[tauri::command]
async fn list_slash_commands(app: AppHandle, state: State<'_, AppState>, worktree_path: String, adapter_id: String) -> Result<Vec<commands::SlashCommand>, String> {
    let list = commands::list(catalog::require(&adapter_id)?, &home(&app)?, Path::new(&worktree_path));
    let live = state.get(&worktree_path).filter(|a| a.alive()).and_then(|a| a.commands());
    Ok(commands::with_runtime(list, live))
}

/// The terminal's scrollback, its byte count and PTY epoch (none running: empty, under the current epoch).
#[tauri::command]
fn pty_snapshot(state: State<AppState>, worktree_path: String) -> runtime::PtySnapshot {
    // nothing running: no bar, the CLI view starts its terminal
    state.get(&worktree_path).map(|a| a.snapshot()).unwrap_or_else(|| runtime::PtySnapshot { epoch: stream::pty_epoch(&worktree_path), terminal: true, ..Default::default() })
}

#[tauri::command]
fn pty_write(state: State<AppState>, worktree_path: String, data: String) -> Result<(), String> {
    state.get(&worktree_path).map_or(Ok(()), |a| a.write(data.as_bytes()))
}

#[tauri::command]
fn pty_resize(state: State<AppState>, worktree_path: String, cols: u16, rows: u16) -> Result<(), String> {
    state.get(&worktree_path).map_or(Ok(()), |a| a.resize(cols, rows))
}

/// Chat history as common events (the running runtime's own record, else the agent CLI's transcript), with the
/// stream epoch it belongs to.
#[tauri::command]
async fn get_transcript(app: AppHandle, project_root: String, worktree_path: String) -> Result<events::Transcript, String> {
    let f = files(&app, &project_root, &worktree_path)?;
    let home = home(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        // looked up on every try: a restart resets the stream before the new run is registered
        let live = || app.state::<AppState>().get(&worktree_path).map(|a| a.shared_runtime());
        runtime::snapshot(live, &home, &f, &worktree_path)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Made by Maestro, so discarding removes its folder; the Maestro and worktrees made outside are only detached.
fn owns_folder(app: &AppHandle, project_root: &str, worktree_path: &str) -> Result<bool, String> {
    Ok(!is_root(project_root, worktree_path) && worktree::is_managed(worktree_path, &worktree::project_dir(&home(app)?, project_root)))
}

/// What discarding would lose (uncommitted files, commits the base branch lacks); the trash button asks this
/// first. Nothing for a worktree whose folder Maestro wouldn't remove.
#[tauri::command]
async fn check_discard(app: AppHandle, project_root: String, worktree_path: String, branch: String) -> Result<worktree::DiscardCheck, String> {
    if !owns_folder(&app, &project_root, &worktree_path)? {
        return Ok(worktree::DiscardCheck::default());
    }
    let prs = app.state::<pr::Prs>();
    if prs.get(&worktree_path).is_some_and(|r| r.merge_pending()) {
        return Err(pr::PR_MERGE_PENDING.into());
    }
    let keep_branch = files(&app, &project_root, &worktree_path)?.read_meta().is_some_and(|m| m.keep_branch);
    let mut c = worktree::check_discard(&project_root, &worktree_path, &branch, &base_branch(&app, &project_root), keep_branch, &prs.merged_heads(&worktree_path, &branch))?;
    if let Some(rec) = prs.get(&worktree_path) {
        c.merged_by_pr = match (&rec.merged, rec.state.as_str()) {
            (Some(m), _) => Some(serde_json::json!({ "number": rec.number, "method": m.method })),
            (None, "OPEN") => Some(serde_json::json!({ "number": rec.number, "open": true })),
            _ => None,
        };
    }
    Ok(c)
}

/// Kill the agent, remove the worktree as `choice` says (`worktree::discard`) and delete the agent's files.
/// Returns where the work went (kept branch or backup ref). The Maestro and outside worktrees only lose the agent.
#[tauri::command]
async fn discard_worktree(app: AppHandle, state: State<'_, AppState>, project_root: String, worktree_path: String, branch: String, choice: worktree::DiscardChoice) -> Result<Option<String>, String> {
    // not while the app pushes, commits or opens a PR for it, or a PR it may have opened isn't found yet; for the
    // Maestro, the same for any of its project's requests and while a PR merge click runs (they'd lose their card
    // and their note mid-way)
    let merges = app.state::<merge::Merges>();
    if merges.pr_unresolved(&worktree_path) {
        return Err(pr::PR_UNRESOLVED.into());
    }
    // a PR GitHub will still merge (the user's click, a merge queue, auto-merge): its record must stay to note it
    if !is_root(&project_root, &worktree_path) && app.state::<pr::Prs>().get(&worktree_path).is_some_and(|r| r.merge_pending()) {
        return Err(pr::PR_MERGE_PENDING.into());
    }
    if is_root(&project_root, &worktree_path) {
        let root = merge::key(&project_root);
        let merging = app.state::<pr::Prs>().list().into_iter().any(|r| merge::key(&r.root) == root && (r.merge_attempt.is_some() || merges.pr_busy(&r.worker_path)));
        if merges.pr_active_in(&project_root) || merging {
            return Err(pr::PR_UNRESOLVED.into());
        }
    }
    let gone = state.agents.lock().unwrap().remove(&worktree_path.to_lowercase());
    if let Some(a) = gone {
        // for good: no switch or fallback starts it again
        a.retire();
        std::thread::sleep(Duration::from_millis(300)); // let the process release file handles (Windows)
    }
    let mut saved = None;
    // the Maestro, or a worktree made outside Maestro: never touch the folder or its branch
    if owns_folder(&app, &project_root, &worktree_path)? {
        let keep_branch = files(&app, &project_root, &worktree_path)?.read_meta().is_some_and(|m| m.keep_branch);
        let heads = app.state::<pr::Prs>().merged_heads(&worktree_path, &branch);
        saved = worktree::discard(&project_root, &worktree_path, &branch, &base_branch(&app, &project_root), keep_branch, choice, &heads)?;
    }
    forget_agent(&app, &project_root, &worktree_path)?;
    // its merge request goes with it (no note: nothing was merged or refused); a discarded Maestro takes the requests
    // it asked for too, since no card could show them and nobody would hear the answer
    merges.withdraw_worker(&app, &worktree_path);
    if is_root(&project_root, &worktree_path) {
        merges.withdraw_maestro(&app, &worktree_path);
        // its PRs stay with their workers, no longer with a Maestro (no note, no card for a later one)
        for r in app.state::<pr::Prs>().orphan_maestro(&worktree_path) {
            let _ = app.emit("maestro://pr", serde_json::json!(r));
        }
    } else if app.state::<pr::Prs>().remove_worker(&worktree_path).is_some() {
        // its PR record goes (the PR and its remote branch stay on GitHub)
        let _ = app.emit("maestro://pr-done", serde_json::json!({ "workerPath": worktree_path }));
    }
    Ok(saved)
}

/// Drops an agent's link from every worker list and deletes all its files (meta and what spawning writes next to it).
fn forget_agent(app: &AppHandle, project_root: &str, worktree_path: &str) -> Result<(), String> {
    let same = |w: &String| w.replace('\\', "/").eq_ignore_ascii_case(&worktree_path.replace('\\', "/"));
    for (_, f) in agent_files(app, project_root)? {
        let _ = f.update_meta(|m| {
            m.workers.retain(|w| !same(w));
            Ok(())
        });
    }
    let meta = files(app, project_root, worktree_path)?.meta;
    for ext in ["settings.json", "brief.md", "notes.json"] {
        let _ = std::fs::remove_file(meta.with_extension(ext));
    }
    let _ = std::fs::remove_dir_all(meta.with_extension("brief"));
    // its previous copy too (a `.corrupt-…` one moved aside stays for the user to look at)
    let _ = std::fs::remove_file(durable::backup_path(&meta));
    let _ = std::fs::remove_file(meta);
    Ok(())
}

// ---------- right panel: files & changes ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DirEntry {
    name: String,
    is_dir: bool,
}

#[tauri::command]
async fn list_dir(path: String) -> Result<Vec<DirEntry>, String> {
    let mut out: Vec<DirEntry> = std::fs::read_dir(&path)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name() != ".git")
        .map(|e| DirEntry { name: e.file_name().to_string_lossy().into(), is_dir: e.file_type().map(|t| t.is_dir()).unwrap_or(false) })
        .collect();
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(out)
}

#[derive(Serialize)]
struct FileChange {
    code: String,
    path: String,
}

#[tauri::command]
async fn git_status(worktree_path: String) -> Result<Vec<FileChange>, String> {
    Ok(worktree::status(&worktree_path)?.into_iter().map(|(code, path)| FileChange { code, path }).collect())
}


// ---------- code editor ----------

/// `rel` inside `base`, refusing anything that resolves outside it (`..`, absolute paths, links out).
fn inside(base: &str, rel: &str) -> Result<PathBuf, String> {
    let root = std::fs::canonicalize(base).map_err(|e| e.to_string())?;
    let p = root.join(rel);
    // a new file has no canonical path yet; its folder must still be inside
    let real = std::fs::canonicalize(&p).or_else(|_| p.parent().map_or(Err(()), |d| std::fs::canonicalize(d).map_err(|_| ()))).map_err(|_| format!("{rel}: 경로를 찾을 수 없어요"))?;
    if !real.starts_with(&root) {
        return Err(format!("{rel}: 워크트리 밖의 파일이에요"));
    }
    Ok(p)
}

fn mtime(p: &Path) -> u64 {
    std::fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as u64)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TextFile {
    text: String,
    mtime: u64,
}

const EDIT_MAX: u64 = 5 << 20;

/// A text file for the editor; refuses big or binary files (a NUL byte).
#[tauri::command]
async fn read_text_file(base: String, rel: String) -> Result<TextFile, String> {
    let p = inside(&base, &rel)?;
    if std::fs::metadata(&p).map_err(|e| e.to_string())?.len() > EDIT_MAX {
        return Err("5MB가 넘는 파일은 열지 않아요".into());
    }
    let bytes = std::fs::read(&p).map_err(|e| e.to_string())?;
    if bytes.contains(&0) {
        return Err("바이너리 파일이라 열 수 없어요".into());
    }
    Ok(TextFile { text: String::from_utf8_lossy(&bytes).into_owned(), mtime: mtime(&p) })
}

/// 0 when missing; the editor polls it to notice an agent's edits.
#[tauri::command]
async fn file_mtime(base: String, rel: String) -> Result<u64, String> {
    Ok(mtime(&inside(&base, &rel)?))
}

/// Saves unless the file changed on disk since `expected` (its mtime when read), so an agent's edit isn't overwritten.
#[tauri::command]
async fn write_text_file(base: String, rel: String, text: String, expected: u64, force: bool) -> Result<u64, String> {
    let p = inside(&base, &rel)?;
    if !force && mtime(&p) != expected {
        return Err("CHANGED".into());
    }
    std::fs::write(&p, text).map_err(|e| e.to_string())?;
    Ok(mtime(&p))
}

/// The file as of HEAD for the diff view; "" when it's new.
#[tauri::command]
async fn file_at_head(base: String, rel: String) -> Result<String, String> {
    // a deleted file has no path left to resolve, so check the spelling (git only reads the repo anyway)
    if Path::new(&rel).is_absolute() || rel.split(['/', '\\']).any(|s| s == "..") {
        return Err(format!("{rel}: 워크트리 밖의 파일이에요"));
    }
    Ok(worktree::show_head(&base, &rel).unwrap_or_default())
}

// ---------- canvas ----------

// ---------- documents (the sidebar's 문서 tab): Maestro's own Markdown, shared by every project ----------

#[tauri::command]
async fn library_list(app: AppHandle) -> Result<Vec<library::LibFolder>, String> {
    library_folders(&app)
}

/// The persona folder first, then the document categories (also `maestro library list`).
pub(crate) fn library_folders(app: &AppHandle) -> Result<Vec<library::LibFolder>, String> {
    Ok(library::all(&home(app)?))
}

/// The playbooks (the sidebar's 플레이북 tab), as one folder.
#[tauri::command]
async fn playbook_list(app: AppHandle) -> Result<Vec<library::LibFolder>, String> {
    Ok(library::playbooks(&home(&app)?))
}

/// A new persona in the shared persona folder; returns its file path.
#[tauri::command]
fn persona_create(app: AppHandle, name: String) -> Result<String, String> {
    library::create_persona(&home(&app)?, &name)
}

#[tauri::command]
fn docs_create(app: AppHandle, folder: String, name: String) -> Result<String, String> {
    library::create_doc(&home(&app)?, &folder, &name)
}

/// Copies Markdown files into a category: adding to Maestro's documents (using one is only a reference).
#[tauri::command]
fn docs_import(app: AppHandle, folder: String, files: Vec<String>) -> Result<(), String> {
    library::import_docs(&home(&app)?, &folder, &files)
}

#[tauri::command]
fn docs_delete(app: AppHandle, path: String) -> Result<(), String> {
    library::delete_doc(&home(&app)?, &path)
}

#[tauri::command]
fn docs_category(app: AppHandle, name: String) -> Result<(), String> {
    library::create_category(&home(&app)?, &name)
}

/// A Markdown file's block title (its top `#` heading), for files dropped from the file tree.
#[tauri::command]
fn md_title(path: String) -> String {
    library::title_of(Path::new(&path))
}

/// One file per canvas: `overview` (the project's worktree cards) or a worktree's folder name.
fn canvas_path(app: &AppHandle, project_root: &str, key: &str) -> Result<PathBuf, String> {
    let key: String = key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    Ok(worktree::project_dir(&home(app)?, project_root).join("canvases").join(format!("{key}.json")))
}

/// The canvas's own format version ("1.0.0" in the file, written by the frontend): its major is checked here.
const CANVAS_VERSION: u64 = 1;

/// A corrupt canvas reads as its `.bak`, if that one is good; a newer Maestro's is an Err (the canvas then never saves
/// over it).
#[tauri::command]
fn load_canvas_state(app: AppHandle, project_root: String, key: String) -> Result<Option<String>, String> {
    durable::read_text(&canvas_path(&app, &project_root, &key)?, CANVAS_VERSION).map_err(String::from)
}

#[tauri::command]
fn save_canvas_state(app: AppHandle, project_root: String, key: String, state_json: String) -> Result<(), String> {
    durable::replace_text(&canvas_path(&app, &project_root, &key)?, &state_json, CANVAS_VERSION)
}

/// An app opened from Finder / Dock (or a desktop launcher) gets a bare PATH, without Homebrew, nvm or npm's
/// global bin, so `claude`, `npm` and friends look missing. Take the PATH the user's login shell sets up.
#[cfg(not(windows))]
fn use_shell_path() {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    if let Some(path) = login_shell_path(&shell, SHELL_PROBE_SECS) {
        std::env::set_var("PATH", path);
    }
}

/// How long the login shell may take to print its PATH before startup goes on with the inherited one.
#[cfg_attr(windows, allow(dead_code))]
const SHELL_PROBE_SECS: f64 = 10.0;

#[cfg_attr(windows, allow(dead_code))]
const PATH_MARK: &str = "__MAESTRO_PATH__";

/// The text between the first two markers (an interactive shell may print a greeting around it); None when it's
/// empty or the closing marker is missing (cut off).
#[cfg_attr(windows, allow(dead_code))]
fn marked_path(stdout: &str) -> Option<&str> {
    let mut it = stdout.split(PATH_MARK);
    it.next();
    let p = it.next()?;
    it.next()?;
    (!p.is_empty()).then_some(p)
}

/// The PATH `shell -ilc` sets up, bounded by `exec::run`: a slow rc, or one that leaves a process holding stdout, can't
/// stall startup. The rc sees `MAESTRO_SHELL_PROBE=1`, so it can skip heavy parts.
#[cfg_attr(windows, allow(dead_code))]
fn login_shell_path(shell: &str, secs: f64) -> Option<String> {
    let mut cmd = exec::Cmd::new(shell, &["-ilc", &format!("printf '{PATH_MARK}%s{PATH_MARK}' \"$PATH\"")]).timeout(secs);
    cmd.env_extra.push(("MAESTRO_SHELL_PROBE".into(), "1".into()));
    marked_path(&exec::run(cmd).stdout).map(String::from)
}

/// A second launch brings the running window forward (the pop-out `ws-*` windows, hidden while pre-warmed, are left).
fn focus_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// One Maestro at a time: two would both poll PRs, rewrite `~/.maestro`'s lists over each other and copy the CLI.
/// Release builds by default; dev builds (`tauri dev`, often run by an agent inside the installed Maestro, under the
/// same identifier) are exempt, or they would only focus the installed one. MAESTRO_SINGLE_INSTANCE=1/0 turns it on/off.
/// Dev and release still share `~/.maestro`.
fn single_instance() -> bool {
    std::env::var("MAESTRO_SINGLE_INSTANCE").map(|v| v != "0").unwrap_or(!cfg!(debug_assertions))
}

fn main() {
    #[cfg(not(windows))]
    use_shell_path();
    let mut builder = tauri::Builder::default();
    // first of all plugins (its docs): a second process exits here, before any setup below runs
    if single_instance() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| focus_main(app)));
    }
    builder
        .plugin(tauri_plugin_dialog::init())
        // auto update (UpdateNotice.tsx): the updater checks and installs, process relaunches afterwards
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(AppState::default())
        .manage(workspace::Workspace::default())
        .setup(|app| {
            // state files that can't be used (durable.rs) are told in a warning dialog, from the loads below on
            let h = app.handle().clone();
            durable::set_notice_sink(move |msg| {
                use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
                h.dialog().message(msg).title("Maestro 저장 파일").kind(MessageDialogKind::Warning).show(|_| {});
            });
            if let Ok(dir) = app.path().app_local_data_dir() {
                acp_setup::set_app_data(dir);
            }
            let server = Approvals::start(app.handle().clone())?;
            // dev builds only: lets you try the CLI from any terminal (set MAESTRO_URL, MAESTRO_WORKTREE and that
            // worktree's MAESTRO_TOKEN, which an agent started there has in its environment)
            #[cfg(debug_assertions)]
            eprintln!("[maestro] MAESTRO_URL={}", server.addr());
            app.manage(server);
            app.manage(merge::Merges::load(&home(app.handle())?));
            app.manage(pr::Prs::load(&home(app.handle())?));
            // PR mode: requests and merges left mid-way are reconciled (reads only), then CI is polled
            let h = app.handle().clone();
            std::thread::spawn(move || {
                if let Ok(host) = pr::AppHost::new(&h) {
                    pr::resume(&host, &h.state::<merge::Merges>(), &h.state::<pr::Prs>());
                }
                pr::start_poller(h);
            });
            agent::install_cli(&home(app.handle())?);
            workspace::warm_later(app.handle().clone());
            library::seed(&home(app.handle())?);
            let h = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(30));
                reap_away(&h);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_repos,
            add_repo,
            repo_state,
            init_repo,
            remove_repo,
            list_worktrees,
            list_branches,
            create_agent,
            send_message,
            ensure_agent,
            agent_ready,
            switch_runtime,
            start_view,
            fork_session,
            acp_install,
            acp_install_cancel,
            set_chat_runtime,
            accept_trust,
            list_slash_commands,
            list_approvals,
            list_tui_waits,
            list_merges,
            answer_merge,
            merge_file,
            project_merge_mode,
            set_merge_mode,
            pr_readiness,
            list_prs,
            refresh_prs,
            merge_pr,
            pr_sync_base,
            pr_failed_log,
            open_pr,
            dismiss_pr,
            summarize_agent,
            create_maestro,
            update_agent,
            restart_agent,
            library_list,
            playbook_list,
            docs_create,
            docs_import,
            docs_delete,
            docs_category,
            persona_create,
            md_title,
            attach_agent,
            link_worktrees,
            unlink_worktrees,
            set_ref,
            cli_status,
            setup_start,
            setup_stop,
            agent_catalog,
            agent_models,
            launch_preview,
            answer_approval,
            stop_agent,
            set_away,
            set_view_default,
            set_canvas_selection,
            take_canvas_drawings,
            pty_snapshot,
            pty_write,
            pty_resize,
            get_transcript,
            check_discard,
            discard_worktree,
            list_dir,
            git_status,
            workspace::ws_get,
            workspace::ws_commit,
            workspace::ws_draft_set,
            workspace::ws_draft_get,
            workspace::ws_window_ready,
            workspace::ws_take_window,
            workspace::ws_show_window,
            read_text_file,
            write_text_file,
            file_mtime,
            file_at_head,
            load_canvas_state,
            save_canvas_state,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build Maestro")
        .run(|app, event| match event {
            // no orphan agents after the window closes
            tauri::RunEvent::Exit => {
                // in parallel: each kill waits for taskkill
                let all: Vec<Arc<Agent>> = app.state::<AppState>().agents.lock().unwrap().values().cloned().collect();
                std::thread::scope(|s| {
                    for a in &all {
                        s.spawn(move || a.kill());
                    }
                });
            }
            tauri::RunEvent::WindowEvent { label, event: tauri::WindowEvent::Destroyed, .. } => {
                // a closed window shows nothing any more; closing the main window ends the app, pop-outs included
                app.state::<AppState>().away.lock().unwrap().show(&label, HashSet::new());
                app.state::<workspace::Workspace>().window_closed(app, &label);
                if label == "main" {
                    app.exit(0);
                }
            }
            _ => {}
        });
}
