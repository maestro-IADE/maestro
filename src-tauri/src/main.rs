#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent;
mod antigravity;
mod approval;
mod catalog;
mod cli;
mod commands;
mod control;
mod library;
mod workspace;
mod worktree;

use agent::{Agent, AgentMeta, AgentState, Files, LaunchFailure, Tail};
use approval::{Approval, Approvals};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Manager, State};

/// Running agents keyed by lowercased worktree path (Windows paths are case-insensitive).
#[derive(Default)]
pub(crate) struct AppState {
    pub(crate) agents: Mutex<HashMap<String, Arc<Agent>>>,
    /// lowercased worktree path → lowercased project root, for agents (setup terminals have none)
    roots: Mutex<HashMap<String, String>>,
    away: Mutex<Away>,
    /// lowercased worktree path → the blocks the user selected on that agent's canvas (`maestro selection`)
    pub(crate) selection: Mutex<HashMap<String, serde_json::Value>>,
    /// lowercased worktree path → structure an agent drew with `maestro draw`, waiting for its canvas to take it
    pub(crate) drawings: Mutex<HashMap<String, Vec<serde_json::Value>>>,
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

/// The running agent for a worktree, starting (or resuming) it first if needed.
pub(crate) fn ensure(app: &AppHandle, state: &AppState, project_root: &str, worktree_path: &str) -> Result<(Arc<Agent>, bool), String> {
    if let Some(a) = state.get(worktree_path).filter(|a| a.alive()) {
        return Ok((a, false));
    }
    let f = files(app, project_root, worktree_path)?;
    let meta = f.read_meta().ok_or("이 워크트리에는 에이전트 설정이 없습니다")?;
    let a = Arc::new(Agent::spawn(app.clone(), &home(app)?, worktree_path, &f, meta, &app.state::<Approvals>())?);
    state.agents.lock().unwrap().insert(worktree_path.to_lowercase(), a.clone());
    state.roots.lock().unwrap().insert(worktree_path.to_lowercase(), project_root.to_lowercase());
    a.clone().deliver_kept_notes(); // worker notes that came while it was off
    Ok((a, true))
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

pub(crate) fn read_repos(app: &AppHandle) -> Result<Vec<String>, String> {
    match std::fs::read_to_string(repos_file(app)?) {
        Ok(s) => serde_json::from_str(&s).map_err(|e| format!("repos.json 손상: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(e.to_string()),
    }
}

/// Write .tmp then rename, so a crash never leaves a half-written file.
fn atomic_write(path: &Path, text: &str) -> Result<(), String> {
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
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
    let mut roots = read_repos(&app)?;
    // the same repo spelled differently (c: vs C:, \ vs /) keeps its first spelling: the project folder is named by it
    let root = match roots.iter().find(|r| same_root(r, &root)) {
        Some(known) => known.clone(),
        None => {
            roots.push(root.clone());
            atomic_write(&repos_file(&app)?, &serde_json::to_string_pretty(&roots).unwrap())?;
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
    let roots: Vec<String> = read_repos(&app)?.into_iter().filter(|r| *r != root).collect();
    atomic_write(&repos_file(&app)?, &serde_json::to_string_pretty(&roots).unwrap())
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
    /// titles of the other (non-persona) documents it refers to
    docs: Vec<String>,
}

fn info(state: &AppState, f: &Files, path: String, branch: String) -> WorktreeInfo {
    let meta = f.read_meta().unwrap_or_default();
    let some = |s: String| Some(s).filter(|s| !s.is_empty());
    WorktreeInfo {
        state: state.get(&path).map(|a| a.state.lock().unwrap().clone()).unwrap_or(AgentState::Idle),
        adapter_id: some(meta.adapter_id),
        permission_mode: some(meta.permission_mode),
        name: meta.name,
        description: meta.description,
        is_maestro: meta.role.as_deref() == Some("maestro"),
        workers: meta.workers,
        refs: meta.refs,
        external: false,
        personas: vec![],
        base_branch: meta.base_branch,
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
        out.push(info(state, &mf, project_root.replace('\\', "/"), worktree::current_branch(project_root)));
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

/// The project's Maestro: a coordinator agent on the project folder itself (main branch), started right away.
#[tauri::command]
async fn create_maestro(app: AppHandle, state: State<'_, AppState>, req: CreateMaestroReq) -> Result<WorktreeInfo, String> {
    let f = files(&app, &req.project_root, &req.project_root)?;
    if f.read_meta().is_some() {
        return Err("이 프로젝트에는 이미 마에스트로가 있어요".into());
    }
    let meta = AgentMeta {
        adapter_id: req.adapter_id,
        permission_mode: req.permission_mode,
        model: Some(req.model).filter(|m| !m.is_empty()),
        name: Some("Maestro".into()),
        role: Some("maestro".into()),
        base_branch: Some(req.base_branch).filter(|b| !b.is_empty()),
        ..Default::default()
    };
    f.write_meta(&meta)?;
    ensure(&app, &state, &req.project_root, &req.project_root)?;
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
    if f.read_meta().is_some() {
        return Err("이미 에이전트가 있는 워크트리예요".into());
    }
    f.write_meta(&AgentMeta {
        adapter_id: req.adapter_id,
        permission_mode: req.permission_mode,
        model: Some(req.model).filter(|m| !m.is_empty()),
        ..Default::default()
    })?;
    ensure(&app, &state, &req.project_root, &req.worktree_path).map(|_| ())
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
            f.write_meta(&AgentMeta { workers, ..m.clone() })?;
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
}

#[tauri::command]
fn link_worktrees(app: AppHandle, project_root: String, parent: String, child: String) -> Result<(), String> {
    link(&app, &project_root, &parent, &child)
}

#[tauri::command]
fn unlink_worktrees(app: AppHandle, project_root: String, parent: String, child: String) -> Result<(), String> {
    let f = files(&app, &project_root, &parent)?;
    let Some(mut m) = f.read_meta() else { return Ok(()) };
    m.workers.retain(|w| !w.eq_ignore_ascii_case(&child));
    f.write_meta(&m)
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
        let Some(mut m) = f.read_meta() else { continue };
        let had = m.refs.iter().any(|r| r.eq_ignore_ascii_case(&md));
        if on && !had {
            m.refs.push(md.replace('\\', "/"));
        } else if !on && had {
            m.refs.retain(|r| !r.eq_ignore_ascii_case(&md));
        } else {
            continue;
        }
        f.write_meta(&m)?;
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
    let started = f.write_meta(&meta).and_then(|_| Agent::spawn(app.clone(), &home(&app)?, &path, &f, meta, &app.state::<Approvals>()));
    match started {
        Ok(a) => {
            state.agents.lock().unwrap().insert(path.to_lowercase(), Arc::new(a));
            Ok(info(state, &f, path, branch))
        }
        Err(e) => {
            // don't leave an orphan worktree behind a failed start
            let _ = worktree::discard(&req.project_root, &path, &branch, &req.base_branch, keep_branch, worktree::DiscardChoice::Clean);
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
    /// why the TUI didn't come up (login needed, exited, timed out); None once it's up
    failure: Option<LaunchFailure>,
}

/// The chat view waits on this (showing a loader) until the agent's TUI is up.
#[tauri::command]
async fn agent_ready(app: AppHandle, state: State<'_, AppState>, project_root: String, worktree_path: String) -> Result<Readiness, String> {
    let (agent, _) = ensure(&app, &state, &project_root, &worktree_path)?;
    tauri::async_runtime::spawn_blocking(move || match agent.wait_ready() {
        Ok(trust_prompt) => Readiness { trust_prompt, failure: None },
        Err(f) => Readiness { trust_prompt: false, failure: Some(f) },
    })
    .await
    .map_err(|e| e.to_string())
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
        let Some(mut meta) = f.read_meta() else { return Ok(None) };
        // every agent's answers are summarized by Haiku (needs Claude Code installed; otherwise the card shows the raw text)
        let Some(sid) = agent::conversation_id(&meta, &home, &worktree_path) else { return Ok(None) };
        let events = Tail::read_all(agent::transcript_file(&meta.adapter_id, &home, &worktree_path, &sid), &meta.adapter_id);
        let text = agent::recent_turn_text(&events, 4000);
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
        meta.summary = Some(summary.clone());
        meta.summary_key = Some(key);
        f.write_meta(&meta)?;
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
async fn cli_status() -> Result<cli::CliStatus, String> {
    tauri::async_runtime::spawn_blocking(cli::status).await.map_err(|e| e.to_string())
}

/// Permission prompts waiting for an answer (for blocks mounted after the event fired).
#[tauri::command]
fn list_approvals(approvals: State<Approvals>) -> Vec<Approval> {
    approvals.pending()
}

/// choice: "allow" | "always" | "deny" | "cli"
#[tauri::command]
fn answer_approval(approvals: State<Approvals>, id: u64, choice: String) {
    approvals.answer(id, &choice);
}

/// The chat composer's `/` menu for this worktree's agent: its built-ins, then its custom commands and skills.
#[tauri::command]
async fn list_slash_commands(app: AppHandle, worktree_path: String, adapter_id: String) -> Result<Vec<commands::SlashCommand>, String> {
    Ok(commands::list(catalog::require(&adapter_id)?, &home(&app)?, Path::new(&worktree_path)))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    data_b64: String,
    seq: u64,
}

#[tauri::command]
fn pty_snapshot(state: State<AppState>, worktree_path: String) -> Snapshot {
    let (data_b64, seq) = state.get(&worktree_path).map(|a| a.snapshot()).unwrap_or_default();
    Snapshot { data_b64, seq }
}

#[tauri::command]
fn pty_write(state: State<AppState>, worktree_path: String, data: String) -> Result<(), String> {
    state.get(&worktree_path).map_or(Ok(()), |a| a.write(data.as_bytes()))
}

#[tauri::command]
fn pty_resize(state: State<AppState>, worktree_path: String, cols: u16, rows: u16) -> Result<(), String> {
    state.get(&worktree_path).map_or(Ok(()), |a| a.resize(cols, rows))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TranscriptRes {
    events: Vec<Value>,
    seq: u64,
}

/// Chat history straight from the agent CLI's own transcript.
#[tauri::command]
async fn get_transcript(app: AppHandle, project_root: String, worktree_path: String) -> Result<TranscriptRes, String> {
    let meta = files(&app, &project_root, &worktree_path)?.read_meta().unwrap_or_default();
    let home = home(&app)?;
    let events = match agent::conversation_id(&meta, &home, &worktree_path) {
        Some(id) => Tail::read_all(agent::transcript_file(&meta.adapter_id, &home, &worktree_path, &id), &meta.adapter_id),
        None => vec![],
    };
    Ok(TranscriptRes { seq: events.len() as u64, events })
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
    let keep_branch = files(&app, &project_root, &worktree_path)?.read_meta().is_some_and(|m| m.keep_branch);
    worktree::check_discard(&project_root, &worktree_path, &branch, &base_branch(&app, &project_root), keep_branch)
}

/// Kill the agent, remove the worktree as `choice` says (`worktree::discard`) and delete the agent's files.
/// Returns where the work went (kept branch or backup ref). The Maestro and outside worktrees only lose the agent.
#[tauri::command]
async fn discard_worktree(app: AppHandle, state: State<'_, AppState>, project_root: String, worktree_path: String, branch: String, choice: worktree::DiscardChoice) -> Result<Option<String>, String> {
    if let Some(a) = state.agents.lock().unwrap().remove(&worktree_path.to_lowercase()) {
        a.kill();
        std::thread::sleep(Duration::from_millis(300)); // let the process release file handles (Windows)
    }
    let mut saved = None;
    // the Maestro, or a worktree made outside Maestro: never touch the folder or its branch
    if owns_folder(&app, &project_root, &worktree_path)? {
        let keep_branch = files(&app, &project_root, &worktree_path)?.read_meta().is_some_and(|m| m.keep_branch);
        saved = worktree::discard(&project_root, &worktree_path, &branch, &base_branch(&app, &project_root), keep_branch, choice)?;
    }
    forget_agent(&app, &project_root, &worktree_path)?;
    Ok(saved)
}

/// Drops an agent's link from every worker list and deletes all its files (meta and what spawning writes next to it).
fn forget_agent(app: &AppHandle, project_root: &str, worktree_path: &str) -> Result<(), String> {
    let same = |w: &String| w.replace('\\', "/").eq_ignore_ascii_case(&worktree_path.replace('\\', "/"));
    for (_, f) in agent_files(app, project_root)? {
        if let Some(mut m) = f.read_meta().filter(|m| m.workers.iter().any(same)) {
            m.workers.retain(|w| !same(w));
            let _ = f.write_meta(&m);
        }
    }
    let meta = files(app, project_root, worktree_path)?.meta;
    for ext in ["settings.json", "brief.md", "notes.json"] {
        let _ = std::fs::remove_file(meta.with_extension(ext));
    }
    let _ = std::fs::remove_dir_all(meta.with_extension("brief"));
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

#[tauri::command]
fn load_canvas_state(app: AppHandle, project_root: String, key: String) -> Result<Option<String>, String> {
    match std::fs::read_to_string(canvas_path(&app, &project_root, &key)?) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
fn save_canvas_state(app: AppHandle, project_root: String, key: String, state_json: String) -> Result<(), String> {
    atomic_write(&canvas_path(&app, &project_root, &key)?, &state_json)
}

/// An app opened from Finder / Dock (or a desktop launcher) gets a bare PATH, without Homebrew, nvm or npm's
/// global bin, so `claude`, `npm` and friends look missing. Take the PATH the user's login shell sets up.
#[cfg(not(windows))]
fn use_shell_path() {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    // markers: an interactive shell may print a greeting around it
    let Ok(out) = std::process::Command::new(shell).args(["-ilc", "printf '__MAESTRO_PATH__%s__MAESTRO_PATH__' \"$PATH\""]).output() else { return };
    let text = String::from_utf8_lossy(&out.stdout);
    if let Some(path) = text.split("__MAESTRO_PATH__").nth(1).filter(|p| !p.is_empty()) {
        std::env::set_var("PATH", path);
    }
}

fn main() {
    #[cfg(not(windows))]
    use_shell_path();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .manage(workspace::Workspace::default())
        .setup(|app| {
            let server = Approvals::start(app.handle().clone())?;
            // dev builds only: lets you try the CLI from any terminal (set these plus MAESTRO_WORKTREE)
            #[cfg(debug_assertions)]
            eprintln!("[maestro] MAESTRO_URL={} MAESTRO_TOKEN={}", server.addr(), server.token());
            app.manage(server);
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
            accept_trust,
            list_slash_commands,
            list_approvals,
            summarize_agent,
            create_maestro,
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
                for a in app.state::<AppState>().agents.lock().unwrap().values() {
                    a.kill();
                }
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
