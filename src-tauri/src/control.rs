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
    ("branch list", "브랜치 목록 (사용 중 여부)과 Lead 기준 브랜치"),
    ("library list", "Maestro 문서 (페르소나, 지침, 문서 등 분류별 마크다운): 참조로 걸 수 있는 문서"),
    ("ref list", "[--to 워크트리]  (그 에이전트가 참조하는 문서, 기본은 나)"),
    ("ref add", "--path 파일.md [--to 워크트리]  (참조 연결: 그 에이전트 캔버스에 마크다운 블록이 생겨요)"),
    ("ref remove", "--path 파일.md [--to 워크트리]  (참조 해제)"),
    ("send", "--to 워크트리 --text 내용  (다른 에이전트에게 지시 보내기)"),
    ("read", "[--to 워크트리]  (그 에이전트의 상태와 마지막 답변, 기본은 나)"),
    ("selection", "사용자가 내 캔버스에서 지금 선택한 블록들 (에이전트, 문서, 프롬프트 블록, 그린 구조: 블록·영역·화살표)"),
    ("draw", "--file 구조.json  (내 캔버스에 구조를 그려요. {\"areas\":[{\"title\",\"in\"?}], \"blocks\":[{\"title\",\"body\"?,\"area\"?}], \"arrows\":[{\"from\",\"to\",\"label\"?,\"as\"?,\"body\"?}], \"relations\"?:[{\"name\",\"pattern\"?,\"head\"?,\"color\"?,\"description\"?,\"extends\"?}]}. 같은 이름은 고쳐 써요. as = 선의 관계 이름(없는 이름은 기본 모양으로 선언돼요), label = 선 위 짧은 동사구, body = 긴 설명)"),
    ("relation list", "이 프로젝트의 관계(선의 종류): 내장(지휘·지시·참조·기본 관계)과 선언된 것, 각자의 뜻과 모양. 새로 선언하기 전에 먼저 보고 다시 써요"),
    ("relation define", "--name 이름 [--description 뜻] [--pattern solid|dashed|dotted] [--head one|both|none] [--color blue|green|amber|red|violet|teal] [--extends 관계]  (관계를 선언하거나 고쳐요. 모양은 이 고정된 값들 중에서만, 뜻은 자유. 내장 관계는 색만)"),
    ("merge-request", "워커 | --to 워커 [--verified \"확인한 내용과 결과\"]  (Lead 전용: 워커 브랜치를 기준 브랜치에 합쳐 달라고 사용자에게 요청해요. 사용자가 승인 카드에서 승인하면, 로컬 모드면 Maestro가 --no-ff로 합치고, PR 모드면 앱이 브랜치를 푸시하고 PR을 만들어요. 결과는 [maestro] 병합·PR 메모로 와요)"),
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
            let adapter_id = Some(arg("agent")).filter(|s| !s.is_empty()).or(me.adapter_id.clone()).unwrap_or(crate::catalog::default_id().into());
            let (permission_mode, mapped) = worker_mode(files(app, &root, &me.path)?.try_read_meta(), &adapter_id)?;
            let info = create_worktree(app, &state, CreateAgentReq {
                project_root: root.clone(),
                adapter_id,
                permission_mode,
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
            let mut out = card(&info);
            if let Some(note) = mapped {
                out["permission"] = json!(note);
            }
            let prompt = arg("prompt");
            if !prompt.is_empty() {
                // the worker exists now: a prompt that doesn't get through is a warning, kept for its next start
                let sent = deliver(app, &state, &root, &info.path, &prompt);
                let kept = sent.is_err() && files(app, &root, &info.path).is_ok_and(|f| keep_prompt(&f.notes(), &me.path, &prompt));
                // its start flushed the (then empty) notes already: a start that comes up late (slow, a login
                // finished in the CLI view) takes the kept prompt then
                if let Some(a) = state.get(&info.path).filter(|a| kept && a.alive()) {
                    a.deliver_kept_notes_when_ready();
                }
                let (result, warning) = prompt_result(info.name.as_deref().unwrap_or(&info.branch), sent, kept);
                out["prompt"] = result;
                if let Some(w) = warning {
                    out["warning"] = json!(w);
                }
            }
            Ok(out)
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
        "relation/list" => Ok(crate::relations::listing(&crate::relations::load(&home(app)?, &root))),
        "relation/define" => {
            let home = home(app)?;
            let mut f = crate::relations::load(&home, &root);
            let opt = |k: &str| Some(arg(k)).filter(|v| !v.is_empty());
            let id = crate::relations::define(&mut f, &arg("name"), crate::relations::Def { description: opt("description"), pattern: opt("pattern"), head: opt("head"), color: opt("color"), extends: opt("extends") })?;
            crate::relations::save(&home, &root, &f)?;
            let _ = app.emit("maestro://relations", &root);
            Ok(json!({ "defined": id, "relations": crate::relations::listing(&f) }))
        }
        "draw" => {
            let mut spec = drawing(&me.path, &arg("file"))?;
            declare_drawn_relations(app, &root, &mut spec)?;
            let counts = json!({ "areas": spec["areas"].as_array().map_or(0, |a| a.len()), "blocks": spec["blocks"].as_array().map_or(0, |a| a.len()), "arrows": spec["arrows"].as_array().map_or(0, |a| a.len()) });
            state.drawings.lock().unwrap().entry(me.path.to_lowercase()).or_default().push(spec);
            let _ = app.emit("maestro://draw", &me.path); // my canvas, if it's open, takes it now; otherwise when it opens
            Ok(json!({ "drawn": counts }))
        }
        r if r == "merge-request" || r.starts_with("merge-request/") => {
            let key = merge_target(r, &arg("to"))?;
            let w = target(app, &state, &root, &key)?;
            check_merge_caller(me.is_maestro && crate::is_root(&root, &me.path), crate::is_root(&root, &w.path))?;
            let home = home(app)?;
            let mode = crate::project::merge_mode(&home, &root);
            let base = base_branch(app, &root);
            let tools = crate::github::Tools::system();
            let opts = crate::pr::plan_opts(&tools, &app.state::<crate::pr::Prs>(), &root, &base, &w.path, mode);
            let plan = worktree::plan_merge_with(&root, &base, &w.path, &opts)?;
            let mut req = crate::merge::MergeRequest {
                id: 0,
                root: root.clone(),
                maestro_path: me.path.clone(),
                worker_path: w.path.clone(),
                worker_name: w.name.clone().unwrap_or(w.branch.clone()),
                verified: crate::merge::verified(&arg("verified")),
                requested_at: crate::merge::now_ms(),
                plan,
                pr: None,
                pr_draft: None,
            };
            if mode.is_pr() {
                req.pr_draft = Some(crate::pr::draft(&tools, &req, &home));
            }
            let (branch, base, commits, files, open_pr) = (req.plan.branch.clone(), req.plan.base.clone(), req.plan.commit_count, req.plan.file_count, req.plan.existing_pr);
            let (id, replaced) = app.state::<crate::merge::Merges>().insert(app, req);
            // answered now: the user decides whenever they look at the card
            let mut out = json!({
                "requested": id,
                "worker": card(&w),
                "branch": branch,
                "base": base,
                "commits": commits,
                "files": files,
                "mode": mode,
                "note": if mode.is_pr() { "사용자 승인을 기다려요. 승인되면 앱이 푸시하고 PR을 만들어요. 결과는 [maestro] PR … 메모로 와요. 기다리는 동안 다른 일을 해도 돼요" } else { "사용자 승인을 기다려요. 결과는 [maestro] 병합 … 메모로 와요. 기다리는 동안 다른 일을 해도 돼요" },
            });
            if let Some(n) = open_pr {
                out["pr"] = json!(n);
            }
            if let Some(old) = replaced {
                out["replaced"] = json!(old);
            }
            Ok(out)
        }
        _ => Err(format!("모르는 명령: {}. `maestro --help`를 보세요.", route.replace('/', " "))),
    }
}

/// The permission mode a worker made by `maestro worktree create` gets: its Maestro's, or the nearest one its agent
/// takes that is no looser (`catalog::not_looser`), with a note when it changed. A Maestro meta that can't be read
/// is an error, never a default: a default could be looser than what the user chose.
fn worker_mode(caller: Result<Option<crate::agent::AgentMeta>, String>, adapter_id: &str) -> Result<(String, Option<String>), String> {
    let mode = caller.map_err(|e| format!("내 에이전트 설정을 읽지 못해 권한을 정할 수 없어요: {e}"))?.ok_or("내 에이전트 설정이 없어 권한을 정할 수 없어요")?.permission_mode;
    let got = crate::catalog::not_looser(adapter_id, &mode)?;
    let note = (got != mode).then(|| format!("{adapter_id}은(는) {mode} 권한이 없어 더 엄격한 {got}(으)로 만들었어요"));
    Ok((got.to_string(), note))
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
        return Err("merge-request는 이 프로젝트의 Lead만 쓸 수 있어요. 워커는 직접 합치거나 푸시하지 말고 작업이 끝났다고 답하세요".into());
    }
    if target_is_root {
        return Err("프로젝트 폴더는 합칠 대상이 아니에요".into());
    }
    Ok(())
}

/// A worker's turn ended (done or failed): paste a note with that turn's answer into its parent's prompt, so the
/// parent can carry on without polling. A parent that is off (not restarted for this) or at the trust dialog gets it
/// kept on disk and pasted when it starts or the dialog is accepted (`Agent::deliver_kept_notes`). Errors are dropped.
pub fn worker_done(app: &AppHandle, path: &str, turn_id: &str, outcome: &events::TurnOutcome) {
    let state = app.state::<AppState>();
    let Ok((root, me)) = locate(app, &state, path) else { return };
    let Ok(all) = worktree_infos(app, &state, &root) else { return };
    let Some(parent) = parent_of(&all, &me.path) else { return };
    let name = me.name.as_deref().unwrap_or(&me.branch);
    let evs = history_of(app, &root, &me.path).unwrap_or_default();
    let note = worker_note(name, turn_id, outcome, &evs);
    let _ = tell(app, &root, &parent.path, name, &note);
}

/// The note for a worker's ended turn, with only that turn's text: a failed turn never shows an earlier answer. A
/// done turn whose id isn't in the history at all (the re-read raced a restart) falls back to the latest answer.
fn worker_note(name: &str, turn_id: &str, outcome: &events::TurnOutcome, evs: &[events::RuntimeEvent]) -> String {
    let answer = match outcome {
        events::TurnOutcome::Failed { .. } => events::turn_answer(evs, turn_id),
        _ if evs.iter().any(|e| e.turn_id() == Some(turn_id)) => events::turn_answer(evs, turn_id),
        _ => events::last_answer(evs),
    };
    done_note(name, outcome, answer)
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

/// The kept-note key of a first prompt that `worktree create --prompt` couldn't deliver, per caller (like
/// `merge:<path>`), so it never collides with a worker-name key of this agent's own workers.
fn prompt_key(caller: &str) -> String {
    format!("prompt:{}", caller.to_lowercase())
}

/// Keeps an undelivered first prompt in the new worker's own notes, pasted as-is (it's the worker's instruction, not
/// a `[maestro]` note) by `Agent::deliver_kept_notes` once its prompt is up. Kept only, no retry now: delivery just
/// failed. Returns whether it is on disk.
fn keep_prompt(file: &Path, caller: &str, prompt: &str) -> bool {
    pass_notes(file, Some((&prompt_key(caller), prompt)), None);
    file.exists()
}

/// `worktree create`'s `prompt` field and warning: delivered, or not (kept for the worker's next start, or not even that).
fn prompt_result(name: &str, sent: Result<(), String>, kept: bool) -> (Value, Option<String>) {
    match sent {
        Ok(()) => (json!({ "delivered": true }), None),
        Err(e) if kept => (
            json!({ "delivered": false, "error": e, "kept": true }),
            Some(format!("워커 '{name}'은(는) 만들었지만 첫 지시를 보내지 못했어요: {e}. 지시는 보관했다가 이 워커의 입력창이 뜨면 보내요(늦게 끝난 시작이나 CLI 보기에서 마친 로그인 뒤, 신뢰 확인 뒤, 다시 시작할 때. maestro send로 다른 지시를 보내면 그보다 먼저 보내요). 같은 --name으로 다시 만들지 말고 같은 지시를 다시 보내지도 마세요.")),
        ),
        Err(e) => (
            json!({ "delivered": false, "error": e, "kept": false }),
            Some(format!("워커 '{name}'은(는) 만들었지만 첫 지시를 보내지 못했고 보관하지도 못했어요: {e}. 다시 만들지 말고, 워커가 뜨면 maestro send --to {name} 으로 지시를 보내세요.")),
        ),
    }
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

/// A queue emptied on delivery: written atomically, no version or `.bak` (durable.rs).
fn write_notes(file: &Path, notes: &[(String, String)]) {
    if let Err(e) = crate::util::atomic_write(file, &serde_json::to_string_pretty(notes).unwrap()) {
        eprintln!("[maestro] worker notes not saved: {e}");
    }
}

/// `[maestro] 워커 … 완료` with the turn's answer, or `[maestro] 워커 … 실패 (…)` with the error (never 완료).
fn done_note(name: &str, outcome: &events::TurnOutcome, answer: Option<String>) -> String {
    let clip = |s: &str, max: usize| s.char_indices().nth(max).map_or(s.len(), |(i, _)| i);
    let answer_text = |answer: &str| {
        let cut = &answer[..clip(answer, 1500)];
        let more = if cut.len() < answer.len() { format!("\n… (잘림, 전체는 `maestro read --to {name}`)") } else { String::new() };
        format!("{cut}{more}")
    };
    match outcome {
        events::TurnOutcome::Failed { error, message } => {
            let msg = message.trim();
            let mut note = format!("[maestro] 워커 '{name}' 실패 ({error}): {}", &msg[..clip(msg, 300)]);
            // Claude's API errors are also the turn's text: don't repeat them
            if let Some(a) = answer.filter(|a| !a.trim().is_empty() && a.trim() != msg) {
                note += &format!("\n이번 턴 답변:\n{}", answer_text(&a));
            }
            note + "\n작업이 끝나지 않았어요. 워커를 확인하고 다시 시키거나 사용자에게 알리세요."
        }
        // Cancelled/Command/Replaced don't finish a turn (`events::finishes_turn`): formatted as done if they ever do
        _ => match answer {
            Some(a) => format!("[maestro] 워커 '{name}' 완료. 마지막 답변:\n{}", answer_text(&a)),
            None => format!("[maestro] 워커 '{name}' 완료. 이번 턴에는 글 답변이 없어요."),
        },
    }
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
    Ok(json!({ "areas": areas, "blocks": blocks, "arrows": arrows, "relations": list("relations") }))
}

/// A drawing's relations: the ones it declares (`relations`), and the one each arrow names (`as`, declared in the
/// default look when it's new), turned into the relation id the canvas stores (`relation`). Saved before the drawing
/// goes to the canvas, so the canvas knows every relation its new lines use.
fn declare_drawn_relations(app: &AppHandle, root: &str, spec: &mut Value) -> Result<(), String> {
    let declared = spec["relations"].as_array().cloned().unwrap_or_default();
    let named = spec["arrows"].as_array().map_or(false, |a| a.iter().any(|x| x["as"].as_str().is_some_and(|n| !n.trim().is_empty())));
    if declared.is_empty() && !named {
        return Ok(());
    }
    let home = home(app)?;
    let mut f = crate::relations::load(&home, root);
    for r in &declared {
        let s = |k: &str| r[k].as_str().map(str::to_string);
        crate::relations::define(&mut f, r["name"].as_str().unwrap_or(""), crate::relations::Def { description: s("description"), pattern: s("pattern"), head: s("head"), color: s("color"), extends: s("extends") })?;
    }
    for a in spec["arrows"].as_array_mut().into_iter().flatten() {
        if let Some(n) = a["as"].as_str().map(str::trim).filter(|n| !n.is_empty()).map(String::from) {
            a["relation"] = json!(crate::relations::resolve_or_declare(&mut f, &n)?);
        }
    }
    crate::relations::save(&home, root, &f)?;
    let _ = app.emit("maestro://relations", root);
    Ok(())
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
                // the Lead also answers to its old name, so notes and habits from before the rename still reach it
                || (w.is_maestro && (k == "lead" || k == "maestro"))
                || w.branch.to_lowercase() == k
                || w.path.to_lowercase() == worktree::norm(std::path::Path::new(&k))
                || w.path.rsplit('/').next().is_some_and(|f| f.to_lowercase() == k)
        })
        .ok_or(format!("이 프로젝트에 '{key}' 워크트리가 없어요"))
}

/// Start/resume the agent, wait for its prompt, paste the text. Notes it kept (a first prompt from `worktree
/// create`, worker notes) that are still waiting go first, so the text never overtakes them.
fn deliver(app: &AppHandle, state: &AppState, root: &str, path: &str, text: &str) -> Result<(), String> {
    let (agent, _) = ensure(app, state, root, path)?;
    match agent.wait_ready() {
        Ok(false) => {}
        Ok(true) => return Err("그 워크트리는 폴더 신뢰 확인이 먼저 필요해요 (Maestro 화면에서 확인)".into()),
        Err(f) => return Err(f.message().into()),
    }
    agent.send_kept_notes();
    agent.send(text, &[])
}

/// The agent's conversation as the same common events the chat view shows (`maestro read`, worker-done notes).
fn history_of(app: &AppHandle, root: &str, path: &str) -> Option<Vec<events::RuntimeEvent>> {
    let meta = files(app, root, path).ok()?.read_meta()?;
    let home = home(app).ok()?;
    let live = app.state::<AppState>().get(path);
    let rt = live.map(|a| a.shared_runtime());
    Some(runtime::history(rt.as_deref(), &home, &meta, path))
}

/// The agent's latest answer (`maestro read`).
fn last_answer(app: &AppHandle, root: &str, path: &str) -> Option<String> {
    events::last_answer(&history_of(app, root, path)?)
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
        assert!(check_merge_caller(false, false).unwrap_err().contains("Lead만"));
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

    fn done() -> events::TurnOutcome {
        events::TurnOutcome::Done { stop_reason: "end_turn".into() }
    }
    fn failed(error: &str, message: &str) -> events::TurnOutcome {
        events::TurnOutcome::Failed { error: error.into(), message: message.into() }
    }
    fn text(turn: &str, msg: &str, t: &str) -> events::RuntimeEvent {
        events::RuntimeEvent::AgentText { turn_id: turn.into(), item_id: format!("{msg}#0"), message_id: Some(msg.into()), text: t.into() }
    }
    fn prompt(turn: &str) -> events::RuntimeEvent {
        events::RuntimeEvent::UserMessage { turn_id: turn.into(), item_id: turn.into(), kind: events::UserKind::Prompt, text: "go".into(), files: vec![] }
    }
    fn ended(turn: &str, outcome: events::TurnOutcome) -> events::RuntimeEvent {
        events::RuntimeEvent::TurnEnded { turn_id: turn.into(), outcome }
    }

    #[test]
    fn done_note_truncates_long_answers() {
        assert_eq!(done_note("api", &done(), Some("끝".into())), "[maestro] 워커 'api' 완료. 마지막 답변:\n끝");
        let long = done_note("api", &done(), Some("가".repeat(2000)));
        assert!(long.contains(&"가".repeat(1500)) && !long.contains(&"가".repeat(1501)) && long.contains("maestro read --to api"));
        assert_eq!(done_note("api", &done(), None), "[maestro] 워커 'api' 완료. 이번 턴에는 글 답변이 없어요.");
        // a failed turn's partial text is cut the same way
        let long = done_note("api", &failed("acp", "boom"), Some("가".repeat(2000)));
        assert!(long.contains(&"가".repeat(1500)) && !long.contains(&"가".repeat(1501)) && !long.contains("완료"));
    }

    #[test]
    fn failed_turn_note_has_no_earlier_answer() {
        let evs = vec![prompt("a"), text("a", "u1", "old answer"), ended("a", done()), prompt("b"), ended("b", failed("auth", "x"))];
        let note = worker_note("api", "b", &failed("auth", "로그인이 필요해요. 터미널에서 로그인하세요"), &evs);
        assert!(note.starts_with("[maestro] 워커 'api' 실패 (auth): 로그인이 필요해요"), "{note}");
        assert!(!note.contains("완료") && !note.contains("old answer"), "{note}");
        // a failed turn missing from the history still shows nothing earlier
        assert!(!worker_note("api", "zz", &failed("exited", "꺼졌어요"), &evs).contains("old answer"));
    }

    #[test]
    fn failed_claude_api_turn_keeps_its_own_text_once() {
        let evs = vec![prompt("a"), text("a", "u1", "old answer"), prompt("b"), text("b", "u2", "API Error: 429"), ended("b", failed("rate_limit", "API Error: 429"))];
        let note = worker_note("api", "b", &failed("rate_limit", "API Error: 429"), &evs);
        assert_eq!(note.matches("API Error: 429").count(), 1, "{note}");
        assert!(!note.contains("완료") && !note.contains("old answer"));
        // other text of the failed turn is shown under its own heading
        let evs = vec![prompt("b"), text("b", "u2", "반쯤 했어요")];
        let note = worker_note("api", "b", &failed("acp", "adapter crashed"), &evs);
        assert!(note.contains("이번 턴 답변:\n반쯤 했어요") && note.ends_with("사용자에게 알리세요."), "{note}");
    }

    #[test]
    fn done_note_reads_only_the_ended_turn() {
        let evs = vec![prompt("a"), text("a", "u1", "old answer"), ended("a", done()), prompt("b"), text("b", "u2", "new"), ended("b", done()), prompt("c"), ended("c", done())];
        assert_eq!(worker_note("api", "b", &done(), &evs), "[maestro] 워커 'api' 완료. 마지막 답변:\nnew");
        assert_eq!(worker_note("api", "c", &done(), &evs), "[maestro] 워커 'api' 완료. 이번 턴에는 글 답변이 없어요.");
    }

    #[test]
    fn create_prompt_failure_is_a_warning() {
        let (r, w) = prompt_result("api", Ok(()), false);
        assert_eq!(r, json!({ "delivered": true }));
        assert!(w.is_none());
        let (r, w) = prompt_result("api", Err("폴더 신뢰 확인이 먼저 필요해요".into()), true);
        assert_eq!((r["delivered"].clone(), r["kept"].clone()), (json!(false), json!(true)));
        assert!(r["error"].as_str().unwrap().contains("폴더 신뢰 확인"));
        let w = w.unwrap();
        assert!(w.contains("'api'") && w.contains("다시 만들지"), "{w}");
        // it goes when the worker's prompt comes up, not only on a restart
        assert!(w.contains("입력창이 뜨면") && !w.contains("깨울"), "{w}");
        let (r, w) = prompt_result("api", Err("에이전트가 꺼져 있어요".into()), false);
        assert_eq!(r["kept"], json!(false));
        assert!(w.unwrap().contains("maestro send --to api"));
    }

    #[test]
    fn kept_prompt_is_delivered_on_next_start() {
        let dir = std::env::temp_dir().join(format!("maestro-kept-prompt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("w.notes.json");
        assert!(keep_prompt(&file, "C:/P/Root", "첫 지시 (old)"));
        assert!(keep_prompt(&file, "C:/P/Root", "첫 지시"));
        assert_eq!(read_notes(&file), [("prompt:c:/p/root".to_string(), "첫 지시".to_string())]);
        // a note kept afterwards goes after the prompt (oldest first)
        pass_notes(&file, Some(("sub", "done")), None);
        let sent = std::cell::RefCell::new(vec![]);
        let ok = |t: &str, _: &[(String, String)]| -> Result<(), String> {
            sent.borrow_mut().push(t.to_string());
            Ok(())
        };
        // `deliver_kept_notes` on the next start
        assert!(pass_notes(&file, None, Some(&ok)));
        assert_eq!(*sent.borrow(), ["첫 지시\n\ndone"]);
        assert!(!file.exists());
        // the key doesn't collide with a worker named "prompt"
        assert_ne!(prompt_key("C:/P/Root"), "prompt");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_worker_takes_its_maestros_mode_never_a_looser_one() {
        let meta = |mode: &str| Ok(Some(crate::agent::AgentMeta { adapter_id: "claude-code".into(), permission_mode: mode.into(), ..Default::default() }));
        assert_eq!(worker_mode(meta("default"), "claude-code"), Ok(("default".into(), None)));
        // agy would run "default" as accept-edits: it gets plan, and the Maestro is told
        let (mode, note) = worker_mode(meta("default"), "antigravity").unwrap();
        assert_eq!(mode, "plan");
        assert!(note.is_some_and(|n| n.contains("plan")));
        // a Maestro meta that can't be read is no reason to fall back to accept-edits
        assert!(worker_mode(Err("손상".into()), "claude-code").is_err());
        assert!(worker_mode(Ok(None), "claude-code").is_err());
    }

    #[test]
    fn done_note_falls_back_when_turn_unknown() {
        let evs = vec![prompt("a"), text("a", "u1", "old answer"), ended("a", done())];
        assert_eq!(worker_note("api", "zz", &done(), &evs), "[maestro] 워커 'api' 완료. 마지막 답변:\nold answer");
    }
}
