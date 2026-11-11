//! What the `maestro` CLI can do, run by the local server (approval.rs) for an agent inside Maestro.
//! The caller is the worktree the CLI ran in (MAESTRO_WORKTREE, set when Maestro starts an agent), so
//! every command is scoped to that worktree's project. Deliberately no delete/discard: agents can add
//! worktrees and talk to each other, not tear anything down. `merge-request` only asks: the app merges after the
//! user approves the card (merge.rs).

use crate::{base_branch, events, runtime, create_worktree, ensure, files, home, library, library_folders, link, read_repos, set_reference, worktree, worktree_infos, AppState, CreateAgentReq, WorktreeInfo};
use serde_json::{json, Value};
use std::path::Path;
use tauri::{AppHandle, Emitter, Manager};

/// What `maestro --help` lists; `maestro <route words> --flag value` maps onto these.
pub const ROUTES: &[(&str, &str)] = &[
    ("whoami", "내 워크트리, 상위 블록, 내 워커들, 나에게 연결된 참조 문서"),
    ("worktree list", "이 프로젝트의 워크트리와 에이전트 상태"),
    ("worktree create", "--name 이름 [--prompt 첫 지시] [--agent claude-code|antigravity] [--model 모델] [--description 설명] [--branch 새 브랜치 --base 기준 | --checkout 기존 브랜치]"),
    ("branch list", "브랜치 목록 (사용 중 여부)과 마에스트로 기준 브랜치"),
    ("library list", "Maestro 문서 (페르소나, 지침, 문서 등 분류별 마크다운): 참조로 걸 수 있는 문서"),
    ("ref list", "[--to 워크트리]  (그 에이전트가 참조하는 문서, 기본은 나)"),
    ("ref add", "--path 파일.md [--to 워크트리]  (참조 연결: 그 에이전트 캔버스에 마크다운 블록이 생겨요)"),
    ("ref remove", "--path 파일.md [--to 워크트리]  (참조 해제)"),
    ("send", "--to 워크트리 --text 내용  (다른 에이전트에게 지시 보내기)"),
    ("read", "[--to 워크트리]  (그 에이전트의 상태와 마지막 답변, 기본은 나)"),
    ("selection", "사용자가 내 캔버스에서 지금 선택한 블록들 (에이전트, 문서, 프롬프트 블록, 그린 구조: 블록·영역·화살표)"),
    ("draw", "--file 구조.json  (내 캔버스에 구조를 그려요. {\"areas\":[{\"title\",\"in\"?}], \"blocks\":[{\"title\",\"body\"?,\"area\"?}], \"arrows\":[{\"from\",\"to\",\"label\"?}]}. 같은 이름은 고쳐 써요)"),
    ("merge-request", "워커 | --to 워커 [--verified \"확인한 내용과 결과\"]  (마에스트로 전용: 워커 브랜치를 기준 브랜치에 합쳐 달라고 사용자에게 요청해요. 사용자가 승인 카드에서 승인하면 Maestro가 --no-ff로 합치고, 결과는 [maestro] 병합 메모로 와요)"),
];

pub fn handle(app: &AppHandle, route: &str, caller: &str, input: &Value) -> Result<Value, String> {
    if route == "help" {
        return Ok(json!(ROUTES.iter().map(|(r, d)| json!({ "command": r, "usage": d })).collect::<Vec<_>>()));
    }
    if caller.is_empty() {
        return Err("maestro 명령은 Maestro가 띄운 에이전트 안에서 실행해야 해요 (MAESTRO_WORKTREE 없음)".into());
    }
    let state = app.state::<AppState>();
    let (root, me) = locate(app, &state, caller)?;
    let arg = |k: &str| input[k].as_str().unwrap_or("").trim().to_string();
    match route {
        "whoami" => {
            let all = worktree_infos(app, &state, &root)?;
            Ok(json!({
                "project": root,
                "worktree": card(&me),
                "parent": parent_of(&all, &me.path).map(|p| card(p)),
                "workers": all.iter().filter(|w| me.workers.iter().any(|p| p.eq_ignore_ascii_case(&w.path))).map(card).collect::<Vec<_>>(),
                // documents linked to you on the canvas: read them before working
                "references": me.refs.iter().map(|r| json!({ "path": r, "title": library::title_of(std::path::Path::new(r)) })).collect::<Vec<_>>(),
            }))
        }
        "worktree/list" => {
            let all = worktree_infos(app, &state, &root)?;
            Ok(json!(all
                .iter()
                .map(|w| {
                    let mut c = card(w);
                    c["role"] = json!(if w.is_maestro { "maestro" } else { "" });
                    c["parent"] = json!(parent_of(&all, &w.path).map(|p| p.name.clone().unwrap_or(p.branch.clone())));
                    c
                })
                .collect::<Vec<_>>()))
        }
        "worktree/create" => {
            let name = arg("name");
            if name.is_empty() {
                return Err("--name 이 필요해요".into());
            }
            let n = worktree_infos(app, &state, &root)?.len() + 1;
            let or = |v: String, d: String| if v.is_empty() { d } else { v };
            let checkout = arg("checkout");
            let info = create_worktree(app, &state, CreateAgentReq {
                project_root: root.clone(),
                adapter_id: Some(arg("agent")).filter(|s| !s.is_empty()).or(me.adapter_id.clone()).unwrap_or(crate::catalog::default_id().into()),
                permission_mode: files(app, &root, &me.path)?.read_meta().map(|m| m.permission_mode).unwrap_or("acceptEdits".into()),
                existing_branch: !checkout.is_empty(),
                branch_name: or(checkout, or(arg("branch"), format!("agent-{n}"))),
                base_branch: or(arg("base"), base_branch(app, &root)),
                name,
                description: arg("description"),
                model: arg("model"),
            })?;
            // made by you → it's your worker (a line from your card on the project canvas)
            link(app, &root, &me.path, &info.path)?;
            let _ = app.emit("maestro://worktrees", &root); // the UI reloads this project's worktrees
            let prompt = arg("prompt");
            if !prompt.is_empty() {
                deliver(app, &state, &root, &info.path, &prompt)?;
            }
            Ok(card(&info))
        }
        "branch/list" => Ok(json!({ "base": base_branch(app, &root), "branches": worktree::branches(&root)? })),
        "library/list" => Ok(json!(library_folders(app)?)),
        "ref/list" => {
            let to = match arg("to").as_str() {
                "" => me,
                key => target(app, &state, &root, key)?,
            };
            Ok(json!({ "worktree": card(&to), "references": refs_of(app, &to) }))
        }
        "ref/add" | "ref/remove" => {
            let to = target(app, &state, &root, &Some(arg("to")).filter(|t| !t.is_empty()).unwrap_or(me.path.clone()))?;
            let on = route == "ref/add";
            let path = md_path(&me.path, &arg("path"), on)?;
            set_reference(app, &state, &root, &to.path, &path, on)?;
            let _ = app.emit("maestro://worktrees", &root); // canvases add or drop the Markdown block
            let to = target(app, &state, &root, &to.path)?;
            Ok(json!({ "worktree": card(&to), "references": refs_of(app, &to) }))
        }
        "send" => {
            let text = arg("text");
            if text.is_empty() {
                return Err("--text 가 필요해요".into());
            }
            let to = target(app, &state, &root, &arg("to"))?;
            deliver(app, &state, &root, &to.path, &text)?;
            Ok(json!({ "sent": card(&to) }))
        }
        "read" => {
            let to = match arg("to").as_str() {
                "" => me,
                key => target(app, &state, &root, key)?,
            };
            Ok(json!({ "worktree": card(&to), "lastAnswer": last_answer(app, &root, &to.path) }))
        }
        "selection" => {
            let picked = state.selection.lock().unwrap().get(&me.path.to_lowercase()).cloned();
            Ok(json!({ "selected": picked.unwrap_or_else(|| json!([])) }))
        }
        "draw" => {
            let spec = drawing(&me.path, &arg("file"))?;
            let counts = json!({ "areas": spec["areas"].as_array().map_or(0, |a| a.len()), "blocks": spec["blocks"].as_array().map_or(0, |a| a.len()), "arrows": spec["arrows"].as_array().map_or(0, |a| a.len()) });
            state.drawings.lock().unwrap().entry(me.path.to_lowercase()).or_default().push(spec);
            let _ = app.emit("maestro://draw", &me.path); // my canvas, if it's open, takes it now; otherwise when it opens
            Ok(json!({ "drawn": counts }))
        }
        r if r == "merge-request" || r.starts_with("merge-request/") => {
            let key = merge_target(r, &arg("to"))?;
            let w = target(app, &state, &root, &key)?;
            check_merge_caller(me.is_maestro && crate::is_root(&root, &me.path), crate::is_root(&root, &w.path))?;
            let plan = worktree::plan_merge(&root, &base_branch(app, &root), &w.path)?;
            let req = crate::merge::MergeRequest {
                id: 0,
                root: root.clone(),
                maestro_path: me.path.clone(),
                worker_path: w.path.clone(),
                worker_name: w.name.clone().unwrap_or(w.branch.clone()),
                verified: crate::merge::verified(&arg("verified")),
                requested_at: crate::merge::now_ms(),
                plan,
            };
            let (branch, base, commits, files) = (req.plan.branch.clone(), req.plan.base.clone(), req.plan.commit_count, req.plan.file_count);
            let (id, replaced) = app.state::<crate::merge::Merges>().insert(app, req);
            // answered now: the user decides whenever they look at the card
            let mut out = json!({
                "requested": id,
                "worker": card(&w),
                "branch": branch,
                "base": base,
                "commits": commits,
                "files": files,
                "note": "사용자 승인을 기다려요. 결과는 [maestro] 병합 … 메모로 와요. 기다리는 동안 다른 일을 해도 돼요",
            });
            if let Some(old) = replaced {
                out["replaced"] = json!(old);
            }
            Ok(out)
        }
        _ => Err(format!("모르는 명령: {}. `maestro --help`를 보세요.", route.replace('/', " "))),
    }
}

/// The worker a merge-request names: the words after `merge-request` (the CLI joins them with `/`, so a branch like
/// `feat/x` comes back whole), else `--to`.
fn merge_target(route: &str, to: &str) -> Result<String, String> {
    let rest = route.strip_prefix("merge-request").unwrap_or("").trim_start_matches('/');
    let key = if rest.is_empty() { to.trim() } else { rest };
    if key.is_empty() {
        return Err("합칠 워커를 지정해 주세요: maestro merge-request --to <워커>".into());
    }
    Ok(key.to_string())
}

/// Only the project's Maestro asks for merges, and never for the project folder itself.
fn check_merge_caller(me_is_maestro: bool, target_is_root: bool) -> Result<(), String> {
    if !me_is_maestro {
        return Err("merge-request는 이 프로젝트의 마에스트로만 쓸 수 있어요. 워커는 직접 합치거나 푸시하지 말고 작업이 끝났다고 답하세요".into());
    }
    if target_is_root {
        return Err("프로젝트 폴더는 합칠 대상이 아니에요".into());
    }
    Ok(())
}

/// A worker's turn ended: paste a note with its answer into its parent's prompt, so the parent can carry on
/// without polling. A parent that is off (not restarted for this) or at the trust dialog gets it kept on disk
/// and pasted when it starts or the dialog is accepted (`Agent::deliver_kept_notes`). Errors are dropped.
pub fn worker_done(app: &AppHandle, path: &str) {
    let state = app.state::<AppState>();
    let Ok((root, me)) = locate(app, &state, path) else { return };
    let Ok(all) = worktree_infos(app, &state, &root) else { return };
    let Some(parent) = parent_of(&all, &me.path) else { return };
    let name = me.name.as_deref().unwrap_or(&me.branch);
    let note = done_note(name, last_answer(app, &root, &me.path));
    let _ = tell(app, &root, &parent.path, name, &note);
}

/// Pastes a `[maestro]` note into an agent's prompt, or keeps it until the agent can take it (`pass_notes`): worker
/// done notes (key: the worker's name) and merge results (key: `merge:<worker path>`). One kept note per key.
pub(crate) fn tell(app: &AppHandle, root: &str, to_path: &str, key: &str, note: &str) -> Result<(), String> {
    let f = files(app, root, to_path)?;
    let state = app.state::<AppState>();
    // Ok(false) from wait_ready: its prompt is up (true: the trust dialog; Err: it didn't start)
    match state.get(to_path).filter(|a| a.wait_ready() == Ok(false) && a.alive()) {
        Some(agent) => pass_notes(&f.notes(), Some((key, note)), Some(&|t: &str, kept: &[(String, String)]| agent.send_note(t, kept))),
        None => pass_notes(&f.notes(), Some((key, note)), None),
    };
    Ok(())
}

/// Hands an agent its kept notes (oldest first) plus `new` (worker, note), as one paste through `send`. `send`
/// None (it can't take them now) or failing: they stay in `file` for next time, one per worker (its latest; `maestro
/// read` still has every answer), so none is dropped and the file stays as small as the worker list. `send` gets
/// the notes it carries too: a runtime that queues the paste and exits before sending it keeps them again
/// (`keep_notes`). Returns whether anything was sent.
pub(crate) fn pass_notes(file: &Path, new: Option<(&str, &str)>, send: Option<&dyn Fn(&str, &[(String, String)]) -> Result<(), String>>) -> bool {
    // one taker at a time (a start and a finishing worker can race), so no note is pasted twice
    let _one = NOTES.lock().unwrap_or_else(|e| e.into_inner());
    let mut notes = read_notes(file);
    if let Some((worker, note)) = new {
        notes.retain(|(w, _)| w != worker);
        notes.push((worker.to_string(), note.to_string()));
    }
    if notes.is_empty() {
        return false;
    }
    let text = notes.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>().join("\n\n");
    if send.is_some_and(|send| send(&text, &notes).is_ok()) {
        let _ = std::fs::remove_file(file);
        return true;
    }
    write_notes(file, &notes);
    false
}

/// Notes a runtime took but couldn't deliver (it exited first): back in the file, for the next start.
pub(crate) fn keep_notes(file: &Path, entries: &[(String, String)]) {
    let _one = NOTES.lock().unwrap_or_else(|e| e.into_inner());
    let mut notes = read_notes(file);
    for (w, n) in entries {
        // a newer note of the same worker, kept meanwhile, wins
        if !notes.iter().any(|(x, _)| x == w) {
            notes.push((w.clone(), n.clone()));
        }
    }
    if !notes.is_empty() {
        write_notes(file, &notes);
    }
}

static NOTES: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn read_notes(file: &Path) -> Vec<(String, String)> {
    std::fs::read_to_string(file).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn write_notes(file: &Path, notes: &[(String, String)]) {
    let _ = std::fs::create_dir_all(file.parent().unwrap_or(Path::new("."))).and_then(|_| std::fs::write(file, serde_json::to_string_pretty(notes).unwrap()));
}

fn done_note(name: &str, answer: Option<String>) -> String {
    const MAX: usize = 1500;
    let answer = answer.unwrap_or_default();
    let cut = answer.char_indices().nth(MAX).map_or(answer.as_str(), |(i, _)| &answer[..i]);
    let more = if cut.len() < answer.len() { format!("\n… (잘림, 전체는 `maestro read --to {name}`)") } else { String::new() };
    format!("[maestro] 워커 '{name}' 완료. 마지막 답변:\n{cut}{more}")
}

fn refs_of(app: &AppHandle, w: &WorktreeInfo) -> Vec<Value> {
    let home = home(app).ok();
    w.refs
        .iter()
        .map(|r| json!({ "path": r, "title": library::title_of(Path::new(r)), "persona": home.as_deref().is_some_and(|h| library::is_persona(h, r)) }))
        .collect()
}

/// `--path` as an absolute, forward-slash path; relative paths are from the caller's worktree. Adding
/// needs an existing Markdown file; removing takes the path as given (the file may be gone).
fn md_path(caller: &str, path: &str, must_exist: bool) -> Result<String, String> {
    if path.is_empty() {
        return Err("--path 로 마크다운 파일을 지정해 주세요 (`maestro library list`)".into());
    }
    let p = Path::new(path);
    let p = if p.is_absolute() { p.to_path_buf() } else { Path::new(caller).join(p) };
    if must_exist {
        if !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("md")) {
            return Err(format!("마크다운(.md) 파일만 참조할 수 있어요: {path}"));
        }
        if !p.is_file() {
            return Err(format!("파일이 없어요: {}", worktree::norm(&p)));
        }
    }
    Ok(worktree::norm(&p))
}

/// A structure spec from `--file` (relative to the caller's worktree): areas, blocks and arrows, each named by
/// `title`. Checked here so a typo comes back to the agent instead of drawing half a picture.
fn drawing(caller: &str, file: &str) -> Result<Value, String> {
    if file.is_empty() {
        return Err("--file 로 구조 JSON 파일을 지정해 주세요 (`maestro --help`의 draw 형식)".into());
    }
    let p = Path::new(file);
    let p = if p.is_absolute() { p.to_path_buf() } else { Path::new(caller).join(p) };
    let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", worktree::norm(&p)))?;
    let spec: Value = serde_json::from_str(&text).map_err(|e| format!("구조 JSON을 읽지 못했어요: {e}"))?;
    let list = |k: &str| spec[k].as_array().cloned().unwrap_or_default();
    let (areas, blocks, arrows) = (list("areas"), list("blocks"), list("arrows"));
    if areas.is_empty() && blocks.is_empty() {
        return Err("areas나 blocks가 하나는 있어야 해요".into());
    }
    let title = |v: &Value| v["title"].as_str().map(str::trim).unwrap_or("").to_string();
    let names: Vec<String> = areas.iter().chain(&blocks).map(title).collect();
    if let Some(i) = names.iter().position(|n| n.is_empty()) {
        return Err(format!("{}번째 영역/블록에 title이 없어요", i + 1));
    }
    let known = |n: &str| names.iter().any(|x| x == n);
    for a in &arrows {
        for end in ["from", "to"] {
            let n = a[end].as_str().unwrap_or("");
            if !known(n) {
                return Err(format!("화살표의 {end} '{n}'이(가) 이 파일의 영역/블록 이름에 없어요"));
            }
        }
    }
    for b in areas.iter().chain(&blocks) {
        for k in ["area", "in"] {
            if let Some(n) = b[k].as_str().filter(|n| !n.is_empty()) {
                if !areas.iter().any(|a| title(a) == n) {
                    return Err(format!("'{}'의 {k} '{n}'이(가) 영역 이름에 없어요", title(b)));
                }
            }
        }
    }
    Ok(json!({ "areas": areas, "blocks": blocks, "arrows": arrows }))
}

fn parent_of<'a>(all: &'a [WorktreeInfo], path: &str) -> Option<&'a WorktreeInfo> {
    all.iter().find(|w| w.workers.iter().any(|p| p.eq_ignore_ascii_case(path)))
}

fn card(w: &WorktreeInfo) -> Value {
    json!({ "name": w.name.clone().unwrap_or(w.branch.clone()), "branch": w.branch, "agent": w.adapter_id, "status": w.state, "path": w.path })
}

/// The caller's project and its own worktree. git names the caller's repo in one call; scanning every
/// registered repo is only the fallback (each git call costs ~0.3 s on Windows).
pub(crate) fn locate(app: &AppHandle, state: &AppState, caller: &str) -> Result<(String, WorktreeInfo), String> {
    let me = worktree::norm(std::path::Path::new(caller));
    let repos = read_repos(app)?;
    let guess = worktree::main_checkout(caller).and_then(|m| repos.iter().find(|r| r.replace('\\', "/").eq_ignore_ascii_case(&m)).cloned());
    for root in guess.iter().chain(repos.iter().filter(|r| Some(*r) != guess.as_ref())) {
        if let Some(w) = worktree_infos(app, state, root)?.into_iter().find(|w| w.path.eq_ignore_ascii_case(&me)) {
            return Ok((root.clone(), w));
        }
    }
    Err(format!("Maestro가 모르는 워크트리예요: {caller}"))
}

/// A worktree of this project by name, branch, folder or path.
fn target(app: &AppHandle, state: &AppState, root: &str, key: &str) -> Result<WorktreeInfo, String> {
    if key.is_empty() {
        return Err("--to 로 워크트리를 지정해 주세요 (`maestro worktree list`)".into());
    }
    let k = key.to_lowercase();
    worktree_infos(app, state, root)?
        .into_iter()
        .find(|w| {
            w.name.as_deref().is_some_and(|n| n.to_lowercase() == k)
                || w.branch.to_lowercase() == k
                || w.path.to_lowercase() == worktree::norm(std::path::Path::new(&k))
                || w.path.rsplit('/').next().is_some_and(|f| f.to_lowercase() == k)
        })
        .ok_or(format!("이 프로젝트에 '{key}' 워크트리가 없어요"))
}

/// Start/resume the agent, wait for its prompt, paste the text.
fn deliver(app: &AppHandle, state: &AppState, root: &str, path: &str, text: &str) -> Result<(), String> {
    let (agent, _) = ensure(app, state, root, path)?;
    match agent.wait_ready() {
        Ok(false) => {}
        Ok(true) => return Err("그 워크트리는 폴더 신뢰 확인이 먼저 필요해요 (Maestro 화면에서 확인)".into()),
        Err(f) => return Err(f.message().into()),
    }
    agent.send(text, &[])
}

/// The agent's latest answer, from the same common events the chat view shows (`maestro read`, worker-done notes).
fn last_answer(app: &AppHandle, root: &str, path: &str) -> Option<String> {
    let meta = files(app, root, path).ok()?.read_meta()?;
    let home = home(app).ok()?;
    let live = app.state::<AppState>().get(path);
    let rt = live.map(|a| a.shared_runtime());
    events::last_answer(&runtime::history(rt.as_deref(), &home, &meta, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawing_checks_names() {
        let dir = std::env::temp_dir().join(format!("maestro-draw-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let caller = worktree::norm(&dir);
        let put = |s: &str| std::fs::write(dir.join("s.json"), s).unwrap();
        put(r#"{"areas":[{"title":"서버"}],"blocks":[{"title":"API","area":"서버"},{"title":"화면"}],"arrows":[{"from":"화면","to":"API","label":"REST"}]}"#);
        let ok = drawing(&caller, "s.json").unwrap();
        assert_eq!(ok["blocks"].as_array().unwrap().len(), 2);
        put(r#"{"blocks":[{"title":"화면"}],"arrows":[{"from":"화면","to":"없음"}]}"#);
        assert!(drawing(&caller, "s.json").unwrap_err().contains("없음"));
        put(r#"{"blocks":[{"title":"API","area":"서버"}]}"#);
        assert!(drawing(&caller, "s.json").unwrap_err().contains("서버"));
        assert!(drawing(&caller, "").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn md_path_resolves_and_checks() {
        let dir = std::env::temp_dir().join("maestro-md-path-test");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("spec.md"), "# Spec").unwrap();
        let caller = worktree::norm(&dir);
        assert_eq!(md_path(&caller, "spec.md", true).unwrap(), format!("{caller}/spec.md"));
        assert!(md_path(&caller, "missing.md", true).is_err());
        assert!(md_path(&caller, "notes.txt", true).is_err());
        assert!(md_path(&caller, "", true).is_err());
        // removing a reference whose file is gone still works
        assert_eq!(md_path(&caller, "gone.md", false).unwrap(), format!("{caller}/gone.md"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn notes_are_kept_until_the_parent_can_take_them() {
        let dir = std::env::temp_dir().join(format!("maestro-notes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("agents").join("maestro.notes.json");
        let sent = std::cell::RefCell::new(vec![]);
        let carried = std::cell::RefCell::new(vec![]);
        let ok = |t: &str, kept: &[(String, String)]| -> Result<(), String> {
            sent.borrow_mut().push(t.to_string());
            carried.borrow_mut().push(kept.len());
            Ok(())
        };
        let broken = |_: &str, _: &[(String, String)]| -> Result<(), String> { Err("pty closed".into()) };
        // parent off: kept, nothing sent
        assert!(!pass_notes(&file, Some(("a", "one")), None));
        // a failed paste keeps it too
        assert!(!pass_notes(&file, Some(("b", "two")), Some(&broken)));
        assert!(file.exists() && sent.borrow().is_empty());
        // parent up: the kept notes, oldest first, then the new one, in one paste; the file goes
        assert!(pass_notes(&file, Some(("c", "three")), Some(&ok)));
        assert_eq!(*sent.borrow(), ["one\n\ntwo\n\nthree"]);
        assert_eq!(*carried.borrow(), [3]);
        assert!(!file.exists());
        // nothing kept, nothing new: nothing sent
        assert!(!pass_notes(&file, None, Some(&ok)));
        // taken but never delivered (the runtime exited first): kept again; a newer note of the worker wins
        pass_notes(&file, Some(("b", "two again")), None);
        keep_notes(&file, &[("a".into(), "one".into()), ("b".into(), "two".into())]);
        assert!(pass_notes(&file, None, Some(&ok)));
        assert_eq!(sent.borrow()[1], "two again\n\none");
        // many workers: none dropped; a worker that finishes again keeps only its latest, moved last
        for i in 0..20 {
            pass_notes(&file, Some((&i.to_string(), &format!("w{i}"))), None);
        }
        pass_notes(&file, Some(("0", "w0 again")), None);
        assert!(pass_notes(&file, None, Some(&ok)));
        let all = (1..20).map(|i| format!("w{i}")).chain(["w0 again".to_string()]).collect::<Vec<_>>().join("\n\n");
        assert_eq!(sent.borrow()[2], all);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn merge_target_from_route_or_flag() {
        assert_eq!(merge_target("merge-request/api", "").unwrap(), "api");
        assert_eq!(merge_target("merge-request/feat/x", "other").unwrap(), "feat/x");
        assert_eq!(merge_target("merge-request", " web ").unwrap(), "web");
        assert!(merge_target("merge-request", "").unwrap_err().contains("--to"));
    }

    #[test]
    fn check_merge_caller_only_maestro() {
        assert!(check_merge_caller(true, false).is_ok());
        assert!(check_merge_caller(false, false).unwrap_err().contains("마에스트로만"));
        assert!(check_merge_caller(true, true).unwrap_err().contains("프로젝트 폴더"));
    }

    #[test]
    fn merge_notes_keep_worker_done() {
        // a worker's done note and its merge result are kept side by side (different keys), each its latest
        let dir = std::env::temp_dir().join(format!("maestro-merge-notes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("maestro.notes.json");
        pass_notes(&file, Some(("api", "done")), None);
        pass_notes(&file, Some(("merge:c:/w/api", "merged")), None);
        pass_notes(&file, Some(("merge:c:/w/api", "merged again")), None);
        let sent = std::cell::RefCell::new(String::new());
        assert!(pass_notes(&file, None, Some(&|t: &str, _: &[(String, String)]| -> Result<(), String> {
            *sent.borrow_mut() = t.to_string();
            Ok(())
        })));
        assert_eq!(*sent.borrow(), "done\n\nmerged again");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn done_note_truncates_long_answers() {
        assert_eq!(done_note("api", Some("끝".into())), "[maestro] 워커 'api' 완료. 마지막 답변:\n끝");
        let long = done_note("api", Some("가".repeat(2000)));
        assert!(long.contains(&"가".repeat(1500)) && !long.contains(&"가".repeat(1501)) && long.contains("maestro read --to api"));
    }
}
