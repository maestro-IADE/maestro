//! What the `maestro` CLI can do, run by the local server (approval.rs) for an agent inside Maestro.
//! The caller is the worktree the CLI ran in (MAESTRO_WORKTREE, set when Maestro starts an agent), so
//! every command is scoped to that worktree's project. Deliberately no delete/discard: agents can add
//! worktrees and talk to each other, not tear anything down. `stop` only interrupts one of the caller's own workers
//! mid-turn, like the 중단 button: the agent stays up and its worktree as it was. `merge-request` only asks: the app
//! merges after the user approves the card (merge.rs).

use crate::agent::AgentState;
use crate::{base_branch, events, runtime, create_worktree, ensure, files, home, library, library_folders, link, read_repos, set_reference, worktree, worktree_infos, AppState, CreateAgentReq, WorktreeInfo};
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;
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
    ("stop", "--to 워커  (내 워커의 지금 턴을 멈춰요. 화면의 중단 버튼과 같아요: 워커는 켜진 채 남고 하던 일은 끝나지 않은 채 멈춰요. 내 워커(whoami의 workers)만 멈출 수 있어요. 엉뚱한 방향으로 가는 워커를 멈춘 뒤 maestro send로 바로잡을 지시를 보내요. 상태를 보려고 멈추지는 마세요(maestro read). 멈췄다는 [maestro] 메모는 내 다음 maestro send 지시 앞에 붙어 한 메시지로 가요. 답: worker, stopped, before, now (running|idle|off))"),
    ("selection", "사용자가 내 캔버스에서 지금 선택한 블록과 선 (에이전트, 문서, 프롬프트 블록, 그린 구조: 블록·영역과 그 화살표, 선: from·to·relation·label·body, 질문 블록: 질문·보기·답. by가 user면 사용자가 직접 놓은 질문)"),
    ("canvas", "[--brief true]  (내 캔버스를 읽어요: 영역·블록(id, 본문, 위치. 단계·상태가 있는 영역은 mode, steps|states, current, 그 안의 것은 있는 때 in_steps|in_states)·화살표(관계, 라벨)·다른 블록(에이전트, 문서, 프롬프트)과 선, 내 워커들의 상태(phase: running|asking|failed|merge-pending|idle|off)와 완료 보고(## 보고 블록, 없으면 null), 그림 검사(lint). 마지막으로 저장된 캔버스라 방금 그린 것은 잠시 뒤 보여요. --brief true면 본문은 첫 줄만)"),
    ("draw", "--file 구조.json [--say 설명]  (내 캔버스에 구조를 그려요. {\"layout\"?:\"grid|flow|tree\", \"areas\":[{\"title\",\"in\"?,\"layout\"?,\"color\"?,\"steps\"?|\"states\"?,\"step\"?|\"state\"?,\"stack\"?}], \"blocks\":[{\"title\",\"body\"?,\"area\"?,\"color\"?,\"step\"?|\"state\"?}], \"arrows\":[{\"from\",\"to\",\"label\"?,\"as\"?,\"body\"?,\"step\"?|\"state\"?}], \"relations\"?:[{\"name\",\"pattern\"?,\"head\"?,\"color\"?,\"description\"?,\"extends\"?}]}. 영역·블록의 color = 테두리 색(blue|green|amber|red|violet|teal): 함께 묶이는 것에 같은 색을 줘요. 영역에 steps(단계 수나 이름 목록: 순서대로 넘기는 타임라인) 또는 states(상태 이름 목록: 이름으로 고르는 상황)를 주면 단계·상태가 있는 영역이 돼요(둘 중 하나만). 그 안의 블록·영역·화살표에 step(1부터 세는 번호, 여러 개면 목록) 또는 state(이름)를 주면 그때만 보이고, 안 주면 늘 보여요. 영역에 stack: true를 주면 묶음이 돼요: 든 블록이 준 순서대로 세로 한 줄로 붙어 하나로 읽혀요(여러 조각이 한 덩어리일 때. steps/states와 함께는 못 써요). 좌표는 주지 않아요: layout이 배치를 정해요(grid = 줄지어, flow = 화살표 방향으로 왼쪽→오른쪽 단계, tree = 위→아래 단계). 옆으로 길어지면 앱이 아래로 이어요(한 줄은 블록 네 개쯤, 긴 flow는 아래로 꺾여 되돌아와요). 새 그림은 캔버스의 빈 자리에 놓여요(위부터, 왼쪽부터). 맨 위 layout은 맨 바깥 영역·블록, 영역의 layout은 그 안. 같은 이름은 고쳐 써요. as = 선의 관계 이름(없는 이름은 기본 모양으로 선언돼요), label = 선 위 짧은 동사구, body = 긴 설명. 답의 lint에 무시된 칸·겹친 이름·이어지지 않은 블록 같은 검사 결과가 와요)"),
    ("show", "--file 파일 [--title 제목] [--say 설명]  (내 폴더의 이미지(png·jpg·gif·webp·svg)나 HTML 파일을 내 캔버스에 보기 블록으로 띄워요. 차트, 화면 캡처, 만든 페이지처럼 그림으로 봐야 하는 결과를 보여 줄 때 써요. 상대 경로는 내 폴더 기준. 파일을 다시 쓰면 블록이 스스로 다시 읽어요. HTML은 한 파일로 완결돼야 해요(옆 파일을 상대 경로로 못 불러요). 10MB까지. 답: shown(경로), kind(image|html). 치울 때는 erase --view 파일)"),
    ("erase", "--title 이름 | --from 이름 --to 이름 | --question 질문id | --view 파일 | --file 지울것.json | --all true  (--view: 그 파일(또는 제목)의 보기 블록.) (내 캔버스에서 그린 것을 지워요. --title: 그 이름의 블록이나 영역(영역 안의 것은 남아 한 단계 바깥으로), --from/--to: 그 화살표, --question: 그 질문 블록(ask가 준 id, 또는 질문 글 그대로. 묶음의 id(set)를 주면 묶음 전체. 답을 적용한 뒤 치울 때 써요), --file: {\"areas\":[이름], \"blocks\":[이름], \"arrows\":[{\"from\",\"to\"}], \"questions\":[id]}, --all true: 캔버스의 그림·질문·보기 블록 전부(그린 블록·영역·화살표, 질문 블록과 묶음, 보기 블록. 답을 기다리는 질문도 지워지고 그 답은 오지 않아요: 답의 unanswered가 그 수예요). 에이전트·문서·프롬프트 블록과 그 선은 지우지 않아요. 사용자는 Ctrl+Z로 되돌릴 수 있어요)"),
    ("ask", "[--in chat] [--say 설명] --title 질문 [--choices \"보기1|보기2|…\"] [--multi true] [--other true] [--suggest true] [--body 설명] | --file 질문들.json  (--file: 질문 여러 개를 한 묶음으로 물어요. {\"title\"?: 묶음 이름, \"questions\": [{\"title\", \"choices\"?: [보기…], \"multi\"?, \"other\"?, \"body\"?}, …]} 2~10개. 사용자가 하나씩 넘기며 답하고 마지막에 한 번에 보내서, 답이 메모 하나로 와요. 물을 것이 둘 이상이면 따로따로 묻지 말고 이걸 써요. 답의 set이 묶음 id예요.) (사용자에게 묻는 질문 블록을 내 캔버스에 놓아요. --choices가 있으면 객관식(보기 2~8개, | 로 나눔, --multi true면 여러 개 고르기, --other true면 '기타' 글칸도), 없으면 주관식. 사용자가 답하면 [maestro] 질문 … 답 메모로 와요. 물은 뒤에는 답이 필요 없는 일만 하거나 턴을 끝내세요. 답을 적용했으면 maestro erase --question <id>로 그 질문을 치워요. --in chat: 캔버스가 아니라 채팅에 선택지로 띄워요(번호가 붙은 보기와 '직접 입력'). 어떻게 진행할지 고르게 할 때 써요: 첫 보기는 '그대로 진행', 그 뒤에 Maestro로 할 수 있는 다른 흐름을 권해요. 답하면 선택지는 저절로 사라져요. --suggest true: Maestro의 방식을 권하는 선택지라는 표시예요. 채팅에서 일반 질문과 다르게 '추천'으로 보여요. 묶음(--file)은 캔버스에만 놓여요)"),
    ("step", "--area 영역 [--to 번호|next|prev] [--say 설명]  (내 캔버스에서 단계가 있는 영역을 다른 단계로 넘겨요. --to 없으면 next. 끝에서는 멈춰요. 답에 넘어간 단계(current)와 전체 수(of)가 와요)"),
    ("state", "--area 영역 --to 상태이름 [--say 설명]  (내 캔버스에서 상태가 있는 영역을 그 상태로 바꿔요. 상태 이름은 maestro canvas의 그 영역 states에 있어요)"),
    ("say", "--text 설명  (내 캔버스에 한 줄 설명을 띄워요: 지금 화면이 무엇인지 사용자에게 말해요. draw·show·ask·step·state·focus·tidy의 --say(그림 파일의 \"say\")와 같은 것이에요: 그 명령이 놓는 것과 함께 뜨고 잠시 뒤 사라져요. 설명 하나가 한 걸음이에요: 사용자가 '눌러서 넘기기'로 보면 설명마다 다음을 눌러야 그 그림이 나타나요. 줄바꿈 없는 120자 안의 한 줄. 설명이 붙은 명령 어디에나 --play 제목을 더하면 그 걸음부터 새 플레이가 그 제목으로 시작해요: 첫 걸음이 아니라 플레이 전체를 줄인 30자 안의 제목. --source \"파일|파일\"은 그 플레이가 설명하는 원본(코드, 문서, 메모리 파일)이에요: 사용자가 플레이를 보다가 그림을 고치면 끝날 때 그 원본에 반영할지 물어요)"),
    ("focus", "--title 이름 [--say 설명]  (내 캔버스의 화면을 그 블록·영역·질문·보기 블록으로 옮겨요. 설명하는 순서대로 화면을 넘길 때 써요)"),
    ("tidy", "[--area 영역] [--say 설명]  (내 캔버스의 배치를 한 번 맞춰요: 영역 안의 블록을 줄 맞추고 크기를 맞추고, 그린 것들을 빈틈 없이 다시 놓아요. --area: 그 영역 안만. 여러 번 나눠 그린 뒤 마지막에 한 번 써요. 에이전트·프롬프트 블록은 옮기지 않아요)"),
    ("relation list", "이 프로젝트의 관계(선의 종류): 내장(지휘·지시·참조·기본 관계)과 선언된 것, 각자의 뜻과 모양. 새로 선언하기 전에 먼저 보고 다시 써요"),
    ("relation remove", "--name 이름  (선언한 관계를 지워요. 그 관계로 그린 선은 남아 기본 관계로 돌아가요. 내장 관계는 못 지워요)"),
    ("relation define", "--name 이름 [--description 뜻] [--pattern solid|dashed|dotted] [--head one|both|none] [--color blue|green|amber|red|violet|teal] [--extends 관계]  (관계를 선언하거나 고쳐요. 모양은 이 고정된 값들 중에서만, 뜻은 자유. 내장 관계는 색만)"),
    ("merge-request", "워커 | --to 워커 [--verified \"확인한 내용과 결과\"]  (Lead 전용: 워커 브랜치를 기준 브랜치에 합쳐 달라고 사용자에게 요청해요. 사용자가 승인 카드에서 승인하면, 로컬 모드면 Maestro가 --no-ff로 합치고, PR 모드면 앱이 브랜치를 푸시하고 PR을 만들어요. 결과는 [maestro] 병합·PR 메모로 와요)"),
];

/// Where a question goes (`maestro ask --in …`): on the canvas (nothing, or `canvas`), or in the chat as a choice
/// (`chat`). Anything else comes back as an error naming the two.
fn ask_in_chat(place: &str) -> Result<bool, String> {
    match place.trim() {
        "" | "canvas" => Ok(false),
        "chat" => Ok(true),
        other => Err(format!("--in 은 chat 또는 canvas예요 ({other})")),
    }
}

/// The most choices that wait in one agent's chat: an older one gives way to a newer one past this.
const CHAT_ASKS_MAX: usize = 5;

/// Something for an agent's canvas, queued: the open canvas takes it now, a closed one when it opens. `say`: the
/// line the agent says with it (canvas.rs `check_say`), shown on the canvas as it comes in.
fn enqueue(app: &AppHandle, state: &AppState, path: &str, say: &Option<String>, tag: &Value, mut item: Value) {
    if let Some(s) = say {
        item["say"] = json!(s);
    }
    // (`--play`: this step starts a play by that name; `--source`: what the play explains. canvas-flows.md)
    if let Some(p) = tag["play"].as_str() {
        item["play"] = json!(p);
    }
    if tag["source"].as_array().is_some_and(|a| !a.is_empty()) {
        item["source"] = tag["source"].clone();
    }
    // (numbered, so the chat can name the one it throws away while it still waits here: `drop_queued`)
    item["seq"] = json!(state.draw_seq.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1);
    state.drawings.lock().unwrap().entry(path.to_lowercase()).or_default().push(item);
    let _ = app.emit("maestro://draw", path);
}

/// Takes the queued things numbered `seqs` out of `owner`'s queue (the chat's 버리기, while the canvas hasn't taken
/// them); how many went. Ones the canvas took meanwhile are simply not there any more.
pub(crate) fn drop_queued(queues: &mut std::collections::HashMap<String, Vec<Value>>, owner: &str, seqs: &[u64]) -> usize {
    let Some(list) = queues.get_mut(&owner.to_lowercase()) else { return 0 };
    let before = list.len();
    list.retain(|x| !x["seq"].as_u64().is_some_and(|n| seqs.contains(&n)));
    before - list.len()
}

/// The note for something the user threw away instead of letting it onto the canvas. `what`: the thing in a few
/// words, as the chat showed it.
fn discard_note(what: &str) -> String {
    match what.trim() {
        "" => "[maestro] 사용자가 캔버스에 올리려던 것을 올리지 않고 버렸어요. 그건 캔버스에 없어요.".to_string(),
        w => format!("[maestro] 사용자가 캔버스에 올리려던 것을 올리지 않고 버렸어요: {w}
그건 캔버스에 없어요."),
    }
}

/// The user threw away something agent `owner` put up for its canvas: a note says so (into its prompt, or kept until
/// it can take it), since the agent would otherwise go on as if it were there. `n`: the thing's number, so two
/// thrown away in a row are two notes.
pub fn discarded(app: &AppHandle, owner: &str, n: u64, what: &str) -> Result<(), String> {
    let note = discard_note(what);
    // the dev tools' test canvas (dev.rs) has no agent behind it: the note is shown on the test screen, not sent
    if crate::dev::root_of(owner).is_some() {
        let _ = app.emit("maestro://dev-note", json!({ "owner": owner, "note": note }));
        return Ok(());
    }
    let state = app.state::<AppState>();
    let (root, me) = locate(app, &state, owner)?;
    tell(app, &root, &me.path, &format!("discard:{n}"), &note)
}

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
    // the line said with what a canvas command puts there (`--say`)
    let say = crate::canvas::check_say(&arg("say"))?;
    // the name of the play this step starts (`--play`)
    let play = crate::canvas::check_play(&arg("play"))?;
    // and what it explains (`--source`: code, documents, memory), to carry the user's edits back to
    let tag = json!({ "play": play, "source": crate::canvas::check_sources(&arg("source"))? });
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
            // a worker takes its maker's 추천 수준, as it takes its permission mode
            let suggest = files(app, &root, &me.path)?.try_read_meta().ok().flatten().and_then(|m| m.suggest).unwrap_or_default();
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
                suggest,
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
            // my stop of it, if one waits (`maestro stop`), goes in the same message as this instruction: one turn,
            // not a turn for the note alone whose end would reach me as a 완료 note before the correction's
            let stops = files(app, &root, &to.path).ok().map(|f| f.stops());
            let stop = stops.as_deref().and_then(|f| take_stop_note(f, &me.path));
            let sent = deliver(app, &state, &root, &to.path, &with_stop_note(stop.as_deref(), &text));
            if let (Err(_), Some(file), Some(note)) = (&sent, &stops, &stop) {
                // not delivered: it waits for the next instruction again
                keep_stop_note(file, &me.path, note);
            }
            sent?;
            Ok(json!({ "sent": card(&to) }))
        }
        "read" => {
            let to = match arg("to").as_str() {
                "" => me,
                key => target(app, &state, &root, key)?,
            };
            Ok(json!({ "worktree": card(&to), "lastAnswer": last_answer(app, &root, &to.path) }))
        }
        // a worker of mine going the wrong way: its turn stops as the 중단 button stops it, never more (no discarding),
        // and the correction follows with `send`
        "stop" => {
            let to = target(app, &state, &root, &arg("to"))?;
            let name = to.name.clone().unwrap_or(to.branch.clone());
            check_stop(&me.path, &me.workers, &to.path, &name)?;
            // never started for this: an agent that is off has no turn to stop
            let live = state.get(&to.path).filter(|a| a.alive());
            let before = live.as_ref().map_or(AgentState::Terminated, |a| a.state.lock().unwrap().clone());
            let word = |st: &AgentState| worker_phase(st, false, false, false);
            let mut worker = card(&to);
            let Some(agent) = live.filter(|_| before == AgentState::Running) else {
                worker["status"] = json!(before);
                return Ok(json!({ "worker": worker, "stopped": false, "before": word(&before), "now": word(&before), "note": "이미 쉬고 있었어요. 멈출 턴이 없어요" }));
            };
            agent.interrupt().map_err(|e| format!("'{name}'을(를) 멈추지 못했어요: {e}"))?;
            // only once the interrupt went through, and in its own file (`Files::stops`), never the notes queue that a
            // start, an opened view or a restart pastes on its own: only my next `send` takes it
            let by = me.name.clone().unwrap_or(me.branch.clone());
            let kept = files(app, &root, &to.path).is_ok_and(|f| keep_stop_note(&f.stops(), &me.path, &stop_note(&by)));
            let now = settle(&agent, STOP_WAIT);
            worker["status"] = json!(now);
            let mut out = json!({
                "worker": worker,
                "stopped": true,
                "before": word(&before),
                "now": word(&now),
                "note": if now == AgentState::Running {
                    format!("멈추라고 보냈지만 턴이 아직 끝나지 않았어요. 잠시 뒤 `maestro read --to {name}`로 확인하세요. 바로잡을 지시는 `maestro send --to {name} --text …`로 보내요")
                } else {
                    format!("멈췄어요. 바로잡을 지시는 `maestro send --to {name} --text …`로 보내요. 멈췄다는 [maestro] 메모가 그 지시 앞에 붙어 한 메시지로 가요")
                },
            });
            if !kept {
                out["warning"] = json!(format!("'{name}'은(는) 멈췄지만 멈췄다는 메모를 보관하지 못했어요. 다음 지시에 멈춘 이유를 함께 적으세요"));
            }
            Ok(out)
        }
        "selection" => {
            let picked = state.selection.lock().unwrap().get(&me.path.to_lowercase()).cloned();
            Ok(json!({ "selected": picked.unwrap_or_else(|| json!([])) }))
        }
        "relation/list" => Ok(crate::relations::listing(&crate::relations::load(&home(app)?, &root))),
        "relation/remove" => {
            let (id, f) = crate::relations::update(&home(app)?, &root, |f| crate::relations::remove(f, &arg("name")))?;
            let _ = app.emit("maestro://relations", &root);
            Ok(json!({ "removed": id, "relations": crate::relations::listing(&f) }))
        }
        "erase" => {
            let canvas = std::fs::read_to_string(crate::canvas_path(app, &root, &canvas_key(&me))?).ok();
            let spec = erasing(&me.path, input, canvas.as_deref())?;
            let counts = erased(&spec, canvas.as_deref());
            enqueue(app, &state, &me.path, &say, &tag, json!({ "erase": spec }));
            Ok(json!({ "erased": counts }))
        }
        "relation/define" => {
            let opt = |k: &str| Some(arg(k)).filter(|v| !v.is_empty());
            let def = crate::relations::Def { description: opt("description"), pattern: opt("pattern"), head: opt("head"), color: opt("color"), extends: opt("extends") };
            let (id, f) = crate::relations::update(&home(app)?, &root, |f| crate::relations::define(f, &arg("name"), def))?;
            let _ = app.emit("maestro://relations", &root);
            Ok(json!({ "defined": id, "relations": crate::relations::listing(&f) }))
        }
        "draw" => {
            let (mut spec, raw) = drawing(&me.path, &arg("file"))?;
            // checked against the canvas as last saved and the relations before this drawing declares any
            let saved = saved_canvas(app, &root, &me);
            let nodes = crate::canvas::nodes_of(&saved, &|_| None);
            // steps and states: a block's `step` / `state` must be one its area has, here or on the canvas already
            crate::canvas::check_phases(&raw, &nodes)?;
            crate::canvas::check_stacks_hold(&raw, &nodes)?;
            let lint = crate::canvas::lint_drawing(&raw, &nodes, &crate::canvas::arrows_of(&nodes, &saved), &crate::relations::load(&home(app)?, &root));
            declare_drawn_relations(app, &root, &mut spec)?;
            let counts = json!({ "areas": spec["areas"].as_array().map_or(0, |a| a.len()), "blocks": spec["blocks"].as_array().map_or(0, |a| a.len()), "arrows": spec["arrows"].as_array().map_or(0, |a| a.len()) });
            // (the line said with it: `--say`, or "say" in the file)
            let said = if say.is_some() { say } else { crate::canvas::check_say(raw["say"].as_str().unwrap_or(""))? };
            enqueue(app, &state, &me.path, &said, &tag, spec); // my canvas, if it's open, takes it now; otherwise when it opens
            Ok(json!({ "drawn": counts, "lint": crate::canvas::lint_json(&lint) }))
        }
        // an area of mine with steps or states goes to another one (zones.md): queued like a drawing, so the open
        // canvas moves now and a closed one when it opens
        "step" | "state" => {
            let saved = saved_canvas(app, &root, &me);
            let nodes = crate::canvas::nodes_of(&saved, &|_| None);
            let (told, answer) = crate::canvas::phase_move(route, &arg("area"), &arg("to"), &nodes)?;
            enqueue(app, &state, &me.path, &say, &tag, json!({ "phase": told }));
            Ok(answer)
        }
        // several questions at once (`--file`): one set on the canvas, gone through a question at a time, answered
        // together in one note
        // a file of mine shown on my canvas (view-block.md): queued like a drawing
        "show" => {
            let stamp = crate::merge::now_ms();
            let view = showing(&me.path, &arg("file"), &arg("title"), &format!("view_{stamp}_{}", ASKED.fetch_add(1, std::sync::atomic::Ordering::Relaxed)))?;
            let out = json!({ "shown": view["path"], "kind": view["kind"], "note": "캔버스에 보기 블록을 놓았어요. 파일을 다시 쓰면 블록이 스스로 다시 읽어요(다시 show 할 필요 없어요). 같은 파일을 다시 show 하면 블록은 하나로 남고 제목만 바뀌어요. 치울 때는 `maestro erase --view <파일>`" });
            enqueue(app, &state, &me.path, &say, &tag, json!({ "show": view }));
            Ok(out)
        }
        "ask" if !arg("file").is_empty() => {
            if ask_in_chat(&arg("in"))? {
                return Err("질문 묶음(--file)은 캔버스에만 놓여요. 채팅에는 질문을 하나씩 띄워요 (--in chat --title … --choices …)".into());
            }
            let p = Path::new(&arg("file")).to_path_buf();
            let p = if p.is_absolute() { p } else { Path::new(&me.path).join(p) };
            let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", worktree::norm(&p)))?;
            let spec: Value = serde_json::from_str(&text).map_err(|e| format!("질문 JSON을 읽지 못했어요: {e}"))?;
            let stamp = crate::merge::now_ms();
            let set = question_set(&spec, &mut || format!("{stamp}_{}", ASKED.fetch_add(1, std::sync::atomic::Ordering::Relaxed)))?;
            let ids: Vec<Value> = set["questions"].as_array().into_iter().flatten().map(|q| q["id"].clone()).collect();
            let out = json!({ "asked": ids, "set": set["id"], "kind": "묶음", "note": format!("캔버스에 질문 {}개를 한 묶음으로 놓았어요. 사용자가 하나씩 답하고 마지막에 보내면 [maestro] 질문 {}개에 … 답 메모 하나로 와요. 답이 있어야 하는 일은 기다리고, 사용자에게 캔버스에 질문을 올렸다고 말하세요. 답을 적용한 뒤에는 `maestro erase --question <set의 id>`로 묶음을 치우세요", ids.len(), ids.len()) });
            enqueue(app, &state, &me.path, &say, &tag, json!({ "ask_set": set }));
            Ok(out)
        }
        "ask" => {
            let flag = |k: &str| matches!(arg(k).as_str(), "true" | "1" | "yes");
            let id = format!("q_{}_{}", crate::merge::now_ms(), ASKED.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
            let q = question(&id, &arg("title"), &arg("body"), &arg("choices"), flag("multi"), flag("other"))?;
            let kind = if q["choices"].as_array().is_some_and(|c| !c.is_empty()) { "객관식" } else { "주관식" };
            // in the chat, as a choice (`--in chat`): for a fork in the conversation (how to go on), where the user
            // is reading anyway. It waits there until answered or closed
            if ask_in_chat(&arg("in"))? {
                // `--suggest true`: one of Maestro's ways offered (the brief's "추천"), drawn apart from a question
                let mut q = q;
                if flag("suggest") {
                    q["suggest"] = json!(true);
                }
                {
                    let mut asks = state.chat_asks.lock().unwrap();
                    let list = asks.entry(me.path.to_lowercase()).or_default();
                    list.push(q);
                    if list.len() > CHAT_ASKS_MAX {
                        list.remove(0);
                    }
                }
                let _ = app.emit("maestro://asks", &me.path);
                return Ok(json!({ "asked": id, "kind": kind, "in": "chat", "note": "채팅에 선택지를 띄웠어요. 사용자가 하나를 고르거나 직접 적으면 [maestro] 질문 … 답 메모가 입력으로 와요. 답이 있어야 하는 일은 기다리고, 채팅에 선택지를 올렸다고 답에 말하세요. 답하면 선택지는 저절로 사라져요(지울 필요 없어요)." }));
            }
            enqueue(app, &state, &me.path, &say, &tag, json!({ "ask": q })); // my canvas, if it's open, shows it now; otherwise when it opens
            Ok(json!({ "asked": id, "kind": kind, "note": "캔버스에 질문 블록을 놓았어요. 사용자가 답하면 [maestro] 질문 … 답 메모가 입력으로 와요. 답이 있어야 하는 일은 기다리고, 사용자에게 캔버스에 질문을 올렸다고 말하세요. 답을 적용한 뒤에는 `maestro erase --question <이 id>`로 질문 블록을 치우세요" }))
        }
        // a line on its own (canvas-controls.md "안내"): what the user is looking at, said on the canvas
        "say" => {
            let text = crate::canvas::check_say(&arg("text"))?.ok_or("--text 로 한 줄 설명을 주세요 (`maestro say --text \"…\"`)")?;
            enqueue(app, &state, &me.path, &None, &tag, json!({ "say": text }));
            Ok(json!({ "said": text }))
        }
        // the view of my canvas goes to something on it. The name is looked up when the canvas gets to it (what was
        // drawn a moment ago may not be saved yet), so a name that isn't there is a note here, not an error
        "focus" => {
            let title = arg("title");
            if title.is_empty() {
                return Err("--title 로 보여 줄 블록이나 영역 이름을 주세요 (`maestro focus --title 이름`)".into());
            }
            let nodes = crate::canvas::nodes_of(&saved_canvas(app, &root, &me), &|_| None);
            let found = crate::canvas::has_named(&nodes, &title, &["block", "area", "question", "view"]);
            enqueue(app, &state, &me.path, &say, &tag, json!({ "focus": { "title": title } }));
            let mut out = json!({ "focus": title });
            if !found {
                out["note"] = json!("저장된 캔버스에 그 이름이 아직 없어요. 방금 그린 것이면 그것이 놓인 뒤에 화면이 그리로 가요. 이름이 틀렸으면 아무 일도 일어나지 않아요 (`maestro canvas`로 이름을 확인하세요)");
            }
            Ok(out)
        }
        // the layout of my canvas (or of one area) put straight once: rows lined up, blocks the same size, what was
        // drawn packed again (structure-drawing.md "배치 정리")
        "tidy" => {
            let area = arg("area");
            let nodes = crate::canvas::nodes_of(&saved_canvas(app, &root, &me), &|_| None);
            let found = area.is_empty() || crate::canvas::has_named(&nodes, &area, &["area"]);
            enqueue(app, &state, &me.path, &say, &tag, json!({ "tidy": { "area": if area.is_empty() { Value::Null } else { json!(area) } } }));
            let mut out = json!({ "tidied": if area.is_empty() { json!("canvas") } else { json!(area) } });
            if !found {
                out["note"] = json!("저장된 캔버스에 그 영역이 아직 없어요. 방금 그린 것이면 그것이 놓인 뒤에 맞춰요. 이름이 틀렸으면 아무 일도 일어나지 않아요");
            }
            Ok(out)
        }
        "canvas" => {
            let saved = saved_canvas(app, &root, &me);
            let all = worktree_infos(app, &state, &root)?;
            let name_of = |p: &str| all.iter().find(|w| crate::util::path_key(&w.path) == crate::util::path_key(p)).map(|w| w.name.clone().unwrap_or(w.branch.clone()));
            let brief = matches!(arg("brief").as_str(), "true" | "1" | "yes");
            let mut out = crate::canvas::read(&saved, &crate::relations::load(&home(app)?, &root), &name_of, brief);
            let nodes = crate::canvas::nodes_of(&saved, &name_of);
            out["lint"] = crate::canvas::lint_json(&crate::canvas::lint_canvas(&nodes, &saved));
            out["workers"] = json!(all.iter().filter(|w| me.workers.iter().any(|p| p.eq_ignore_ascii_case(&w.path))).map(|w| worker_view(app, &state, &root, w)).collect::<Vec<_>>());
            if saved.get("nodes").is_none() {
                out["note"] = json!("저장된 캔버스가 아직 없어요 (캔버스를 한 번 열면 저장돼요)");
            }
            Ok(out)
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

static ASKED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
const CHOICES_MAX: usize = 8;

/// A question `maestro ask` puts on the caller's canvas (docs/features/canvas/question-block.md): multiple choice
/// when `choices` (`|`-separated, 2 to 8, no repeats) is given, free text otherwise. `multi` (pick several) needs
/// choices; `other` adds a free-text 기타 to the choices (free text already is one, so it's dropped there).
fn question(id: &str, title: &str, body: &str, choices: &str, multi: bool, other: bool) -> Result<Value, String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("--title 로 질문을 적어 주세요".into());
    }
    let list: Vec<String> = choices.split('|').map(str::trim).filter(|c| !c.is_empty()).map(String::from).collect();
    if list.len() == 1 {
        return Err("보기는 둘 이상이어야 해요 (--choices \"A|B\"). 보기 없이 물으면 주관식이에요".into());
    }
    if list.len() > CHOICES_MAX {
        return Err(format!("보기는 {CHOICES_MAX}개까지예요 ({}개)", list.len()));
    }
    if let Some(c) = list.iter().enumerate().find_map(|(i, c)| list[..i].contains(c).then_some(c)) {
        return Err(format!("같은 보기가 두 번 있어요: {c}"));
    }
    if multi && list.is_empty() {
        return Err("--multi 는 --choices 와 함께 써요".into());
    }
    let mut q = json!({ "id": id, "title": title, "choices": list, "multi": multi, "other": other && !list.is_empty() });
    if !body.trim().is_empty() {
        q["body"] = json!(body.trim());
    }
    Ok(q)
}

/// `maestro show`: the file an agent puts on its canvas as a view block (view-block.md). A relative path is the
/// caller's own folder's; the file has to be there, be an image or a page, and not be huge.
fn showing(caller: &str, file: &str, title: &str, id: &str) -> Result<Value, String> {
    if file.trim().is_empty() {
        return Err(format!("--file 로 보여 줄 파일을 지정해 주세요 ({})", crate::canvas::VIEW_KINDS));
    }
    let p = Path::new(file.trim());
    let p = if p.is_absolute() { p.to_path_buf() } else { Path::new(caller).join(p) };
    let shown = worktree::norm(&p);
    let kind = crate::canvas::view_kind(&shown).ok_or_else(|| format!("{shown}: 보기 블록은 이미지나 HTML 파일만 보여 줘요 ({})", crate::canvas::VIEW_KINDS))?;
    let meta = std::fs::metadata(&p).map_err(|_| format!("{shown}: 파일이 없어요 (먼저 파일을 만들고 show 하세요)"))?;
    if !meta.is_file() {
        return Err(format!("{shown}: 파일이 아니에요"));
    }
    if meta.len() > crate::canvas::VIEW_MAX {
        return Err(format!("{shown}: 10MB가 넘는 파일은 보여 주지 않아요"));
    }
    let mut view = json!({ "id": id, "path": shown, "kind": kind });
    if !title.trim().is_empty() {
        view["title"] = json!(title.trim());
    }
    Ok(view)
}

/// The most questions one set takes (each is a step the user goes through).
const SET_MAX: usize = 10;

/// Several questions asked at once (`maestro ask --file`, question-block.md "질문 묶음"): `{"title"?, "questions":
/// [{"title", "choices"?, "multi"?, "other"?, "body"?}, …]}` (or just the list), 2 to 10 of them, each checked like a
/// single question. `choices` is a list or the `A|B` text. `fresh` gives a new id's tail.
fn question_set(spec: &Value, fresh: &mut dyn FnMut() -> String) -> Result<Value, String> {
    let list = spec["questions"].as_array().or(spec.as_array()).ok_or("질문 묶음은 {\"questions\": [{\"title\": …}, …]} 꼴이에요")?;
    if list.len() < 2 {
        return Err("묶음은 질문이 둘 이상일 때 써요. 하나면 `maestro ask --title …`로 물어요".into());
    }
    if list.len() > SET_MAX {
        return Err(format!("한 묶음은 질문 {SET_MAX}개까지예요 ({}개)", list.len()));
    }
    let mut questions = vec![];
    for (i, q) in list.iter().enumerate() {
        let choices = match &q["choices"] {
            Value::Array(a) => a.iter().filter_map(|c| c.as_str()).collect::<Vec<_>>().join("|"),
            other => other.as_str().unwrap_or("").to_string(),
        };
        let on = |k: &str| q[k].as_bool().unwrap_or(false) || matches!(q[k].as_str(), Some("true" | "1" | "yes"));
        let made = question(&format!("q_{}", fresh()), q["title"].as_str().unwrap_or(""), q["body"].as_str().unwrap_or(""), &choices, on("multi"), on("other")).map_err(|e| format!("{}번째 질문: {e}", i + 1))?;
        if questions.iter().any(|x: &Value| x["title"] == made["title"]) {
            return Err(format!("{}번째 질문: 같은 질문이 묶음에 두 번 있어요 ({})", i + 1, made["title"].as_str().unwrap_or("")));
        }
        questions.push(made);
    }
    let mut set = json!({ "id": format!("qs_{}", fresh()), "questions": questions });
    if let Some(t) = spec["title"].as_str().map(str::trim).filter(|t| !t.is_empty()) {
        set["title"] = json!(t);
    }
    Ok(set)
}

/// One answer of a set, as the canvas hands it over: the question, what was picked and written, and (`edited`) the
/// question as answered when the user changed it first.
pub struct SetAnswer {
    pub title: String,
    pub edited: bool,
    pub body: String,
    pub choices: Vec<String>,
    pub picked: Vec<String>,
    pub text: String,
}

/// The one note a set's answers go to the agent in: a numbered line per question.
fn answers_note(items: &[SetAnswer]) -> String {
    let mut out = format!("[maestro] 질문 {}개에 사용자가 답했어요.", items.len());
    for (i, a) in items.iter().enumerate() {
        let text = a.text.trim();
        let mut parts = vec![];
        if !a.picked.is_empty() {
            parts.push(format!("고른 것: {}", a.picked.join(", ")));
        }
        if !text.is_empty() {
            parts.push(format!("{}: {text}", if a.picked.is_empty() { "답" } else { "덧붙인 말" }));
        }
        let changed = if a.edited {
            let mut c = String::from(" (사용자가 고친 질문");
            if !a.body.trim().is_empty() {
                c.push_str(&format!(". 설명: {}", a.body.trim()));
            }
            if !a.choices.is_empty() {
                c.push_str(&format!(". 보기: {}", a.choices.join(" | ")));
            }
            c.push(')');
            c
        } else {
            String::new()
        };
        out.push_str(&format!("\n{}. '{}'{changed} → {}", i + 1, a.title.trim(), parts.join(" · ")));
    }
    out
}

/// The user sent the answers of question set `set` on agent `owner`'s canvas: one note with all of them goes into
/// that agent's prompt, or is kept until it can take it.
pub fn answer_set(app: &AppHandle, owner: &str, set: &str, items: &[SetAnswer]) -> Result<(), String> {
    if items.is_empty() {
        return Err("답이 없어요".into());
    }
    if let Some(i) = items.iter().position(|a| a.picked.is_empty() && a.text.trim().is_empty()) {
        return Err(format!("{}번째 질문에 아직 답하지 않았어요", i + 1));
    }
    let note = answers_note(items);
    if crate::dev::root_of(owner).is_some() {
        let _ = app.emit("maestro://dev-note", json!({ "owner": owner, "note": note }));
        return Ok(());
    }
    let state = app.state::<AppState>();
    let (root, me) = locate(app, &state, owner)?;
    tell(app, &root, &me.path, &format!("answers:{set}"), &note)
}

/// A question the user changed on the canvas before answering it: the note carries the question as it was answered
/// (its notes and every choice), since that is no longer what the agent asked.
pub struct Edited<'a> {
    pub body: &'a str,
    pub choices: &'a [String],
}

/// The note an answer pastes into the agent's prompt: what was picked and written, and for a question the user
/// changed first (`edited`), the question as answered (its notes and every choice).
fn answer_note(title: &str, edited: Option<Edited>, picked: &[String], text: &str) -> String {
    let text = text.trim();
    let mut out = match &edited {
        None => format!("[maestro] 질문 '{}'에 사용자가 답했어요.", title.trim()),
        Some(_) => format!("[maestro] 사용자가 질문을 고쳐서 답했어요. 고친 질문: '{}'", title.trim()),
    };
    if let Some(m) = edited {
        if !m.body.trim().is_empty() {
            out.push_str(&format!("\n설명: {}", m.body.trim()));
        }
        if !m.choices.is_empty() {
            out.push_str(&format!("\n보기: {}", m.choices.join(" | ")));
        }
    }
    if !picked.is_empty() {
        out.push_str(&format!("\n고른 것: {}", picked.join(", ")));
    }
    if !text.is_empty() {
        out.push_str(&format!("\n{}: {text}", if picked.is_empty() { "답" } else { "덧붙인 말" }));
    }
    out
}

/// The user answered question `id` on agent `owner`'s canvas: the answer goes into that agent's prompt, or is kept
/// until it can take it (one kept note per question).
pub fn answer_question(app: &AppHandle, owner: &str, id: &str, title: &str, edited: Option<Edited>, picked: &[String], text: &str) -> Result<(), String> {
    if title.trim().is_empty() {
        return Err("질문이 비어 있어요. 블록을 더블클릭해 질문을 적어 주세요".into());
    }
    if picked.is_empty() && text.trim().is_empty() {
        return Err("답이 비어 있어요".into());
    }
    let note = answer_note(title, edited, picked, text);
    // the dev tools' test canvas (dev.rs) has no agent behind it: the note is shown on the test screen, not sent
    if crate::dev::root_of(owner).is_some() {
        let _ = app.emit("maestro://dev-note", json!({ "owner": owner, "note": note }));
        return Ok(());
    }
    let state = app.state::<AppState>();
    let (root, me) = locate(app, &state, owner)?;
    tell(app, &root, &me.path, &format!("answer:{id}"), &note)
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

/// Who may stop whom (`maestro stop`): an agent stops only its own workers, the ones in its `workers` (linked to it
/// by `maestro worktree create` or on the canvas). Not itself, its parent or a sibling, and not a worker's worker:
/// that one is its own parent's to stop.
fn check_stop(me: &str, my_workers: &[String], target: &str, target_name: &str) -> Result<(), String> {
    let same = |a: &str, b: &str| crate::util::path_key(a) == crate::util::path_key(b);
    if same(me, target) {
        return Err("자기 자신은 멈출 수 없어요. maestro stop은 내 워커의 턴을 멈춰요".into());
    }
    if !my_workers.iter().any(|w| same(w, target)) {
        return Err(format!("'{target_name}'은(는) 내 워커가 아니에요. maestro stop은 내 워커(maestro whoami의 workers)만 멈출 수 있어요"));
    }
    Ok(())
}

/// How long `maestro stop` waits for the stopped turn to end before it answers with the state it has then (the
/// terminal's Esc settles in about two seconds: `Agent::interrupt`).
const STOP_WAIT: Duration = Duration::from_secs(5);

/// The agent's state once it leaves `Running`, or as it is when `wait` is up.
fn settle(agent: &crate::agent::Agent, wait: Duration) -> AgentState {
    let until = std::time::Instant::now() + wait;
    loop {
        let st = agent.state.lock().unwrap().clone();
        if st != AgentState::Running || std::time::Instant::now() >= until {
            return st;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The key of a stop in the stopped worker's stop file, per caller (like `prompt:<path>`). A second stop by the same
/// caller replaces the first.
fn stop_key(caller: &str) -> String {
    format!("stop:{}", caller.to_lowercase())
}

/// What a stopped worker reads in front of its lead's next instruction (`with_stop_note`): who stopped it, and that
/// the stopped work isn't done. It never comes alone, so it points at the instruction below it.
fn stop_note(by: &str) -> String {
    format!("[maestro] 상위 에이전트 '{by}'이(가) 하던 턴을 중단했어요. 그 일은 끝나지 않은 채 멈췄어요. 멈춘 일을 그대로 이어 하지 말고 아래 '{by}'의 지시를 따르세요.")
}

/// Keeps a stop note in the stopped worker's stop file (`Files::stops`), not pasted now: a paste would start a turn
/// right after the stop. Only the caller's next `send` takes it (`take_stop_note`). Returns whether it is on disk.
fn keep_stop_note(file: &Path, caller: &str, note: &str) -> bool {
    let _one = NOTES.lock().unwrap_or_else(|e| e.into_inner());
    let key = stop_key(caller);
    let mut stops = read_notes(file);
    stops.retain(|(k, _)| *k != key);
    stops.push((key, note.to_string()));
    write_notes(file, &stops);
    file.exists()
}

/// Takes the caller's stop note out of the worker's stop file, for the instruction that goes now. Another caller's
/// stays: it goes with that one's instruction.
fn take_stop_note(file: &Path, caller: &str) -> Option<String> {
    let _one = NOTES.lock().unwrap_or_else(|e| e.into_inner());
    let key = stop_key(caller);
    let mut stops = read_notes(file);
    let at = stops.iter().position(|(k, _)| *k == key)?;
    let (_, note) = stops.remove(at);
    if stops.is_empty() {
        let _ = std::fs::remove_file(file);
    } else {
        write_notes(file, &stops);
    }
    Some(note)
}

/// The instruction as a stopped worker gets it: the stop note and the instruction in one message, so they make one
/// turn (`deliver` would run two prompts as two turns).
fn with_stop_note(note: Option<&str>, text: &str) -> String {
    match note {
        Some(note) => format!("{note}\n\n{text}"),
        None => text.to_string(),
    }
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
fn drawing(caller: &str, file: &str) -> Result<(Value, Value), String> {
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
    crate::canvas::check_layouts(&spec)?;
    crate::canvas::check_colors(&spec)?;
    crate::canvas::check_stacks(&spec)?;
    let mut out = json!({ "areas": areas, "blocks": blocks, "arrows": arrows, "relations": list("relations") });
    if let Some(l) = spec["layout"].as_str() {
        out["layout"] = json!(l.trim());
    }
    Ok((out, spec))
}

/// The caller's own canvas as last saved (`{}` when it was never saved or can't be read).
fn saved_canvas(app: &AppHandle, root: &str, me: &WorktreeInfo) -> Value {
    crate::canvas_path(app, root, &canvas_key(me)).ok().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(json!({}))
}

/// A worker as `maestro canvas` shows it: what it's doing now, derived from the runtime (never from what anyone drew
/// or wrote), and the done report at the end of its last answer (`report: null` when there is none or it's broken).
fn worker_view(app: &AppHandle, state: &AppState, root: &str, w: &WorktreeInfo) -> Value {
    let key = crate::util::path_key(&w.path);
    let asking = app.state::<crate::approval::Approvals>().pending().iter().any(|p| crate::util::path_key(&p.worktree_path) == key)
        || state.get(&w.path).is_some_and(|a| a.tui_wait().is_some());
    let history = history_of(app, root, &w.path).unwrap_or_default();
    let failed = history.iter().rev().find_map(|e| match e {
        events::RuntimeEvent::TurnEnded { outcome, .. } => Some(matches!(outcome, events::TurnOutcome::Failed { .. })),
        _ => None,
    }) == Some(true);
    let merge = app.state::<crate::merge::Merges>().of_worker(&w.path).is_some();
    let phase = worker_phase(&w.state, asking, failed, merge);
    let report = events::last_answer(&history).as_deref().and_then(crate::canvas::parse_report);
    json!({ "name": w.name.clone().unwrap_or(w.branch.clone()), "path": w.path, "phase": phase, "report": report })
}

/// One word for what a worker is up to: a question waiting on the user beats running, a failed last turn and a merge
/// waiting for approval beat idle.
fn worker_phase(st: &crate::agent::AgentState, asking: bool, failed: bool, merge: bool) -> &'static str {
    use crate::agent::AgentState::*;
    match st {
        _ if asking => "asking",
        Running => "running",
        _ if failed => "failed",
        _ if merge => "merge-pending",
        Idle => "idle",
        Terminated => "off",
    }
}

/// The canvas file an agent's own canvas is saved as (Canvas.tsx `canvasKey`): the Lead's is the project canvas.
fn canvas_key(me: &WorktreeInfo) -> String {
    if me.is_maestro { "overview".into() } else { Path::new(&me.path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default() }
}

/// What `erase` takes away: names checked against the canvas as last saved (a typo comes back to the agent instead of
/// silently erasing nothing). `--question` takes a question block, by the id `ask` answered with or by the question
/// itself: how an agent clears a question once it has used the answer. `--all` takes everything an agent can put on
/// the canvas: every drawn block, area and arrow, every question block (one still waiting too) and every view block.
fn erasing(caller: &str, input: &Value, canvas: Option<&str>) -> Result<Value, String> {
    let arg = |k: &str| input[k].as_str().unwrap_or("").trim().to_string();
    let all = matches!(arg("all").as_str(), "true" | "1" | "yes");
    let mut spec = if !arg("file").is_empty() {
        let p = Path::new(caller).join(arg("file"));
        let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", worktree::norm(&p)))?;
        serde_json::from_str::<Value>(&text).map_err(|e| format!("지울 것 JSON을 읽지 못했어요: {e}"))?
    } else {
        json!({})
    };
    let names = |k: &str, s: &Value| -> Vec<String> { s[k].as_array().into_iter().flatten().filter_map(|v| v.as_str()).map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).collect() };
    let (mut areas, mut blocks) = (names("areas", &spec), names("blocks", &spec));
    let mut arrows: Vec<(String, String)> = spec["arrows"].as_array().into_iter().flatten().map(|a| (a["from"].as_str().unwrap_or("").trim().to_string(), a["to"].as_str().unwrap_or("").trim().to_string())).collect();
    let title = arg("title");
    if !arg("from").is_empty() || !arg("to").is_empty() {
        arrows.push((arg("from"), arg("to")));
    }
    let mut questions = names("questions", &spec);
    if !arg("question").is_empty() {
        questions.push(arg("question"));
    }
    let mut shown = names("views", &spec);
    if !arg("view").is_empty() {
        shown.push(arg("view"));
    }
    let drawn_named = !title.is_empty() || !areas.is_empty() || !blocks.is_empty() || !arrows.is_empty();
    if all && (drawn_named || !questions.is_empty() || !shown.is_empty()) {
        return Err("--all 은 혼자 써요 (다른 지울 것과 함께 쓰지 않아요)".into());
    }
    if !all && !drawn_named && questions.is_empty() && shown.is_empty() {
        return Err("지울 것을 주세요: --title 이름, --from 이름 --to 이름, --question 질문id, --view 파일, --file 지울것.json, 또는 --all true".into());
    }
    if arrows.iter().any(|(f, t)| f.is_empty() || t.is_empty()) {
        return Err("화살표는 --from 과 --to 를 함께 줘요".into());
    }
    // what's drawn on the canvas now: (title, kind) of blocks and areas, and arrows as (from title, to title)
    let saved: Value = canvas.and_then(|c| serde_json::from_str(c).ok()).unwrap_or(json!({}));
    let nodes = saved["nodes"].as_array().cloned().unwrap_or_default();
    let title_of = |n: &Value| n["data"]["title"].as_str().unwrap_or("").trim().to_string();
    let drawn: Vec<(String, &str, String)> = nodes
        .iter()
        .filter_map(|n| match n["type"].as_str() {
            Some("struct_block") => Some((title_of(n), "block", n["id"].as_str().unwrap_or("").to_string())),
            // (the agents' area, `team`, is the app's: it holds the agents' cards and can't be erased from here)
            Some("zone") if n["data"]["team"] != true => Some((title_of(n), "area", n["id"].as_str().unwrap_or("").to_string())),
            _ => None,
        })
        .collect();
    // view blocks (`maestro show`), each named by its file (as shown, or the path given to show) or its title
    let mut views: Vec<String> = vec![];
    for v in &shown {
        let given = worktree::norm(Path::new(v));
        let mine = worktree::norm(&Path::new(caller).join(v));
        let hit: Vec<String> = nodes
            .iter()
            .filter(|n| n["type"] == "view_block")
            .filter(|n| {
                let p = n["data"]["path"].as_str().unwrap_or("");
                p.eq_ignore_ascii_case(&given) || p.eq_ignore_ascii_case(&mine) || n["data"]["title"].as_str().map(str::trim) == Some(v.as_str())
            })
            .filter_map(|n| n["id"].as_str().map(str::to_string))
            .collect();
        if hit.is_empty() {
            return Err(format!("'{v}' 보기 블록이 내 캔버스에 없어요 (show에 준 파일 경로나 제목으로 지워요. 방금 놓았다면 잠시 뒤 다시 해 보세요)"));
        }
        for id in hit {
            if !views.contains(&id) {
                views.push(id);
            }
        }
    }
    if drawn.is_empty() && drawn_named {
        return Err("내 캔버스에 그린 블록이나 영역이 없어요 (방금 그렸다면 잠시 뒤 다시 해 보세요)".into());
    }
    // the question blocks on the canvas, each named by its id or by what it asks; they go by id. A set's id (or the
    // title it was asked under, when no question is called that) takes the whole set: its questions and the area
    // they sit in
    let asked: Vec<(String, String)> = nodes.iter().filter(|n| n["type"] == "question_block").map(|n| (n["data"]["qid"].as_str().unwrap_or("").to_string(), title_of(n))).collect();
    let set_of = |q: &str, by_title: bool| nodes.iter().filter(|n| n["type"] == "zone" && n["data"]["qset"].is_string()).find(|n| if by_title { title_of(n) == q } else { n["data"]["qset"] == q }).and_then(|n| n["data"]["qset"].as_str().map(str::to_string));
    let mut qids: Vec<String> = vec![];
    let mut sets: Vec<String> = vec![];
    for q in &questions {
        let one = asked.iter().find(|(id, t)| id == q || t == q);
        if let Some(set) = set_of(q, false).or_else(|| if one.is_none() { set_of(q, true) } else { None }) {
            for n in nodes.iter().filter(|n| n["type"] == "question_block" && n["data"]["set"] == set.as_str()) {
                let id = n["data"]["qid"].as_str().unwrap_or("").to_string();
                if !qids.contains(&id) {
                    qids.push(id);
                }
            }
            if !sets.contains(&set) {
                sets.push(set);
            }
            continue;
        }
        match one {
            Some((id, _)) if !qids.contains(id) => qids.push(id.clone()),
            Some(_) => {}
            None => return Err(format!("'{q}' 질문 블록이 내 캔버스에 없어요 (id는 ask의 답과 `maestro canvas`의 questions에 있어요. 방금 물었다면 잠시 뒤 다시 해 보세요)")),
        }
    }
    let has = |t: &str, kind: &str| drawn.iter().any(|(n, k, _)| n == t && *k == kind);
    if !title.is_empty() {
        match (has(&title, "block"), has(&title, "area")) {
            (true, _) => blocks.push(title.clone()),
            (false, true) => areas.push(title.clone()),
            _ => return Err(format!("'{title}' 블록이나 영역이 내 캔버스에 없어요")),
        }
    }
    for b in &blocks {
        if !has(b, "block") {
            return Err(format!("'{b}' 블록이 내 캔버스에 없어요"));
        }
    }
    for a in &areas {
        if !has(a, "area") {
            return Err(format!("'{a}' 영역이 내 캔버스에 없어요"));
        }
    }
    let id_of = |t: &str| drawn.iter().filter(|(n, _, _)| n == t).map(|(_, _, id)| id.clone()).collect::<Vec<_>>();
    let edges = saved["edges"].as_array().cloned().unwrap_or_default();
    for (f, t) in &arrows {
        let (fs, ts) = (id_of(f), id_of(t));
        let found = edges.iter().any(|e| e["data"]["kind"] == "arrow" && fs.iter().any(|x| e["source"] == x.as_str()) && ts.iter().any(|x| e["target"] == x.as_str()));
        if !found {
            return Err(format!("'{f}' → '{t}' 화살표가 내 캔버스에 없어요"));
        }
    }
    spec = json!({
        "all": all,
        "areas": areas,
        "blocks": blocks,
        "arrows": arrows.iter().map(|(f, t)| json!({ "from": f, "to": t })).collect::<Vec<_>>(),
        "questions": qids,
        "sets": sets,
        "views": views,
    });
    Ok(spec)
}

/// What an erase took, as counts for the answer. With `--all` the lists are empty (the canvas takes everything it
/// has by then), so the counts are read from the canvas as last saved, and `unanswered` says how many of the agent's
/// questions went without an answer: no note will come for those.
fn erased(spec: &Value, canvas: Option<&str>) -> Value {
    let len = |k: &str| spec[k].as_array().map_or(0, |a| a.len());
    if spec["all"] != true {
        return json!({ "areas": len("areas"), "blocks": len("blocks"), "arrows": len("arrows"), "questions": len("questions"), "views": len("views"), "all": false });
    }
    let saved: Value = canvas.and_then(|c| serde_json::from_str(c).ok()).unwrap_or(json!({}));
    let nodes = |f: &dyn Fn(&Value) -> bool| saved["nodes"].as_array().map_or(0, |a| a.iter().filter(|n| f(n)).count());
    let asked = |n: &Value| n["type"] == "question_block";
    json!({
        // (a question set's area goes with its questions and isn't counted as an area of its own)
        "areas": nodes(&|n| n["type"] == "zone" && n["data"]["team"] != true && !n["data"]["qset"].is_string()),
        "blocks": nodes(&|n| n["type"] == "struct_block"),
        "arrows": saved["edges"].as_array().map_or(0, |a| a.iter().filter(|e| e["data"]["kind"] == "arrow").count()),
        "questions": nodes(&asked),
        "unanswered": nodes(&|n| asked(n) && n["data"]["mine"] != true && !n["data"]["answer"].is_object()),
        "views": nodes(&|n| n["type"] == "view_block"),
        "all": true,
    })
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
    // all or nothing: a refused relation or arrow leaves the file as it was (nothing half-declared)
    let arrows = spec["arrows"].as_array().cloned().unwrap_or_default();
    let (ids, _) = crate::relations::update(&home(app)?, root, |f| {
        for r in &declared {
            let s = |k: &str| r[k].as_str().map(str::to_string);
            crate::relations::define(f, r["name"].as_str().unwrap_or(""), crate::relations::Def { description: s("description"), pattern: s("pattern"), head: s("head"), color: s("color"), extends: s("extends") })?;
        }
        let mut ids = vec![];
        for a in &arrows {
            let Some(n) = a["as"].as_str().map(str::trim).filter(|n| !n.is_empty()) else {
                ids.push(None);
                continue;
            };
            let id = crate::relations::resolve_or_declare(f, n)?;
            if let Some(b) = crate::relations::behavior_of(f, &id) {
                return Err(format!("'{n}'은(는) {b} 동작이 있는 관계라 그림의 화살표에는 못 써요 (그림에는 동작 없는 관계를 써요)"));
            }
            ids.push(Some(id));
        }
        Ok(ids)
    })?;
    for (a, id) in spec["arrows"].as_array_mut().into_iter().flatten().zip(ids) {
        if let Some(id) = id {
            a["relation"] = json!(id);
        }
    }
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
    // the dev tools' stand-in caller (dev.rs): not a worktree, the CLI test's blank canvas under a project of ours
    if let Some(root) = crate::dev::root_of(caller) {
        if let Some(root) = read_repos(app)?.into_iter().find(|r| crate::util::path_key(r) == crate::util::path_key(&root)) {
            let me = crate::dev::stand_in(app, state, &root)?;
            return Ok((root, me));
        }
    }
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
        Ok(true) => return Err("그 워크트리는 프로젝트 신뢰 확인이 먼저 필요해요 (사용자가 Maestro 화면에서 신뢰하면 이어서 가요)".into()),
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
    fn question_checks_its_choices() {
        let q = question("q1", " 어느 DB? ", "", "Postgres | SQLite|", false, true).unwrap();
        assert_eq!(q, json!({ "id": "q1", "title": "어느 DB?", "choices": ["Postgres", "SQLite"], "multi": false, "other": true }));
        // free text: no choices, so no 기타 either
        let q = question("q2", "이름은?", " 짧게 ", "", false, true).unwrap();
        assert_eq!((q["choices"].clone(), q["other"].clone(), q["body"].clone()), (json!([]), json!(false), json!("짧게")));
        assert!(question("q", " ", "", "", false, false).unwrap_err().contains("--title"));
        assert!(question("q", "x", "", "A", false, false).unwrap_err().contains("둘 이상"));
        assert!(question("q", "x", "", "A|B|A", false, false).unwrap_err().contains("두 번"));
        assert!(question("q", "x", "", "1|2|3|4|5|6|7|8|9", false, false).unwrap_err().contains("8개"));
        assert!(question("q", "x", "", "", true, false).unwrap_err().contains("--multi"));
    }

    #[test]
    fn thrown_away_things_leave_the_queue_by_number() {
        let mut q = std::collections::HashMap::new();
        q.insert("c:/w/api".to_string(), vec![json!({ "seq": 4, "say": "하나" }), json!({ "seq": 5 }), json!({ "seq": 6, "focus": { "title": "구현" } })]);
        // by number, whatever the case of the path; the rest keeps its order
        assert_eq!(drop_queued(&mut q, "C:/W/Api", &[4, 5]), 2);
        assert_eq!(q["c:/w/api"], vec![json!({ "seq": 6, "focus": { "title": "구현" } })]);
        // taken by the canvas meanwhile, or another agent's: nothing goes
        assert_eq!(drop_queued(&mut q, "c:/w/api", &[4]), 0);
        assert_eq!(drop_queued(&mut q, "c:/w/web", &[6]), 0);
        assert_eq!(q["c:/w/api"].len(), 1);
        // the agent is told what it was, in the words the chat showed
        assert_eq!(discard_note(" 설계 흐름이에요 "), "[maestro] 사용자가 캔버스에 올리려던 것을 올리지 않고 버렸어요: 설계 흐름이에요
그건 캔버스에 없어요.");
        assert_eq!(discard_note(""), "[maestro] 사용자가 캔버스에 올리려던 것을 올리지 않고 버렸어요. 그건 캔버스에 없어요.");
    }

    #[test]
    fn answer_note_says_what_was_picked() {
        assert_eq!(answer_note("어느 DB?", None, &["SQLite".into()], ""), "[maestro] 질문 '어느 DB?'에 사용자가 답했어요.\n고른 것: SQLite");
        assert_eq!(answer_note("어느 DB?", None, &["A".into(), "B".into()], " 둘 다 "), "[maestro] 질문 '어느 DB?'에 사용자가 답했어요.\n고른 것: A, B\n덧붙인 말: 둘 다");
        assert_eq!(answer_note("이름은?", None, &[], "maestro"), "[maestro] 질문 '이름은?'에 사용자가 답했어요.\n답: maestro");
        // changed by the user before answering: it isn't what the agent asked any more, so the note carries the
        // question as answered and every choice
        let choices = ["Postgres".to_string(), "SQLite".to_string()];
        assert_eq!(
            answer_note("어느 DB?", Some(Edited { body: " 로컬 개발용 ", choices: &choices }), &["SQLite".into()], ""),
            "[maestro] 사용자가 질문을 고쳐서 답했어요. 고친 질문: '어느 DB?'\n설명: 로컬 개발용\n보기: Postgres | SQLite\n고른 것: SQLite"
        );
        assert_eq!(answer_note("메모", Some(Edited { body: "", choices: &[] }), &[], "끝"), "[maestro] 사용자가 질문을 고쳐서 답했어요. 고친 질문: '메모'\n답: 끝");
    }

    #[test]
    fn question_sets_are_checked_and_answered_in_one_note() {
        let mut n = 0;
        let mut fresh = || {
            n += 1;
            n.to_string()
        };
        let set = question_set(&json!({ "title": " 시작 전에 ", "questions": [{ "title": "어느 DB?", "choices": ["Postgres", "SQLite"] }, { "title": "기능", "choices": "로그인|검색", "multi": true, "other": "true" }, { "title": "이름은?" }] }), &mut fresh).unwrap();
        assert_eq!((set["id"].clone(), set["title"].clone()), (json!("qs_4"), json!("시작 전에")));
        let qs = set["questions"].as_array().unwrap();
        assert_eq!(qs.iter().map(|q| q["id"].as_str().unwrap()).collect::<Vec<_>>(), vec!["q_1", "q_2", "q_3"]);
        assert_eq!((qs[0]["choices"].clone(), qs[1]["multi"].clone(), qs[1]["other"].clone(), qs[2]["choices"].clone()), (json!(["Postgres", "SQLite"]), json!(true), json!(true), json!([])));
        // just the list works too; one question isn't a set, and each is checked like a single question
        assert!(question_set(&json!([{ "title": "a" }, { "title": "b" }]), &mut fresh).is_ok());
        assert!(question_set(&json!({ "questions": [{ "title": "a" }] }), &mut fresh).unwrap_err().contains("둘 이상"));
        assert!(question_set(&json!({ "questions": [{ "title": "a" }, { "title": "b", "choices": ["하나"] }] }), &mut fresh).unwrap_err().starts_with("2번째 질문"));
        assert!(question_set(&json!({ "questions": [{ "title": "a" }, { "title": " a " }] }), &mut fresh).unwrap_err().contains("두 번"));
        assert!(question_set(&json!({ "questions": (0..11).map(|i| json!({ "title": format!("q{i}") })).collect::<Vec<_>>() }), &mut fresh).unwrap_err().contains("10개"));
        assert!(question_set(&json!({ "title": "x" }), &mut fresh).is_err());

        let answer = |title: &str, picked: &[&str], text: &str| SetAnswer { title: title.into(), edited: false, body: String::new(), choices: vec![], picked: picked.iter().map(|s| s.to_string()).collect(), text: text.into() };
        let mut items = vec![answer("어느 DB?", &["SQLite"], ""), answer("기능", &["로그인", "검색"], " 알림도 "), answer("이름은?", &[], "maestro")];
        assert_eq!(answers_note(&items), "[maestro] 질문 3개에 사용자가 답했어요.\n1. '어느 DB?' → 고른 것: SQLite\n2. '기능' → 고른 것: 로그인, 검색 · 덧붙인 말: 알림도\n3. '이름은?' → 답: maestro");
        // a question the user changed first carries what it became
        items[0].edited = true;
        items[0].body = "로컬 기준".into();
        items[0].choices = vec!["SQLite".into(), "DuckDB".into()];
        assert!(answers_note(&items).contains("1. '어느 DB?' (사용자가 고친 질문. 설명: 로컬 기준. 보기: SQLite | DuckDB) → 고른 것: SQLite"));
    }

    #[test]
    fn showing_checks_the_file() {
        let dir = std::env::temp_dir().join(format!("maestro-show-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("out")).unwrap();
        std::fs::write(dir.join("out/chart.png"), b"png").unwrap();
        std::fs::write(dir.join("notes.md"), b"# x").unwrap();
        let me = worktree::norm(&dir);
        let v = showing(&me, "out/chart.png", " 응답 시간 ", "view_1").unwrap();
        assert_eq!((v["id"].clone(), v["kind"].clone(), v["title"].clone()), (json!("view_1"), json!("image"), json!("응답 시간")));
        assert_eq!(v["path"], json!(format!("{me}/out/chart.png")), "a relative path is the caller's folder's");
        assert_eq!(showing(&me, &format!("{me}/out/chart.png"), "", "view_2").unwrap().get("title"), None, "an absolute path as it is; no title, none kept");
        assert!(showing(&me, "", "", "v").unwrap_err().contains("--file"));
        assert!(showing(&me, "notes.md", "", "v").unwrap_err().contains("이미지나 HTML"));
        assert!(showing(&me, "out/missing.png", "", "v").unwrap_err().contains("파일이 없어요"));
        std::fs::create_dir_all(dir.join("dir.png")).unwrap();
        assert!(showing(&me, "dir.png", "", "v").unwrap_err().contains("파일이 아니에요"));
        let _ = std::fs::remove_dir_all(&dir);

        // a view block is erased by its file (as given to show, or as shown) or its title
        let canvas = r#"{"nodes":[
            {"id":"view_1","type":"view_block","data":{"path":"C:/w/out/chart.png","kind":"image","title":"응답 시간"}},
            {"id":"view_2","type":"view_block","data":{"path":"C:/w/out/report.html","kind":"html"}}],"edges":[]}"#;
        let e = |v: Value| erasing("C:/w", &v, Some(canvas));
        assert_eq!(e(json!({"view":"out/chart.png"})).unwrap()["views"], json!(["view_1"]));
        assert_eq!(e(json!({"view":"c:/w/out/report.html"})).unwrap()["views"], json!(["view_2"]));
        assert_eq!(e(json!({"view":"응답 시간"})).unwrap()["views"], json!(["view_1"]));
        assert!(e(json!({"view":"out/none.png"})).unwrap_err().contains("보기 블록"));
        assert!(e(json!({"all":"true","view":"out/chart.png"})).is_err(), "--all alone");
        assert_eq!(erased(&e(json!({"all":"true"})).unwrap(), Some(canvas))["views"], json!(2), "--all takes what is shown too");
        assert_eq!(erased(&e(json!({"view":"응답 시간"})).unwrap(), Some(canvas)), json!({ "areas": 0, "blocks": 0, "arrows": 0, "questions": 0, "views": 1, "all": false }));
    }

    #[test]
    fn a_question_goes_to_the_canvas_or_the_chat() {
        assert_eq!(ask_in_chat(""), Ok(false));
        assert_eq!(ask_in_chat(" canvas "), Ok(false));
        assert_eq!(ask_in_chat("chat"), Ok(true));
        assert!(ask_in_chat("popup").unwrap_err().contains("chat 또는 canvas"));
        assert!(ROUTES.iter().any(|(r, d)| *r == "ask" && d.contains("--in chat")));
    }

    #[test]
    fn erasing_checks_names_against_the_canvas() {
        let canvas = r#"{"nodes":[
            {"id":"b1","type":"struct_block","data":{"title":"API"}},
            {"id":"b2","type":"struct_block","data":{"title":"DB"}},
            {"id":"z1","type":"zone","data":{"title":"백엔드"}},
            {"id":"c1","type":"worktree_card","data":{"worktreePath":"x"}}],
          "edges":[{"id":"e1","source":"b1","target":"b2","data":{"kind":"arrow"}}]}"#;
        let ok = |v: Value| erasing("C:/w", &v, Some(canvas));
        assert_eq!(ok(json!({"title":"API"})).unwrap()["blocks"], json!(["API"]));
        assert_eq!(ok(json!({"title":"백엔드"})).unwrap()["areas"], json!(["백엔드"]));
        assert!(ok(json!({"title":"없음"})).is_err());
        assert_eq!(ok(json!({"from":"API","to":"DB"})).unwrap()["arrows"], json!([{"from":"API","to":"DB"}]));
        assert!(ok(json!({"from":"DB","to":"API"})).is_err(), "direction matters");
        assert!(ok(json!({"from":"API"})).is_err(), "--to missing");
        assert!(ok(json!({})).is_err(), "nothing named");
        assert_eq!(ok(json!({"all":"true"})).unwrap()["all"], json!(true));
        // (the agents' cards aren't counted: they stay)
        assert_eq!(erased(&ok(json!({"all":"true"})).unwrap(), Some(canvas)), json!({ "areas": 1, "blocks": 2, "arrows": 1, "questions": 0, "unanswered": 0, "views": 0, "all": true }));
        assert!(ok(json!({"all":"true","title":"API"})).is_err(), "--all alone");
        assert!(erasing("C:/w", &json!({"title":"API"}), None).is_err(), "no canvas saved yet");
        // a question block goes by the id `ask` answered with, or by what it asks; --all takes every one of them
        let asked = r#"{"nodes":[
            {"id":"question_q_1","type":"question_block","data":{"qid":"q_1","title":"어느 DB?"}},
            {"id":"question_q_3","type":"question_block","data":{"qid":"q_3","title":"언제까지?","answer":{"picked":[],"text":"금요일","at":1}}},
            {"id":"question_q_2","type":"question_block","data":{"qid":"q_2","title":"이름은?","mine":true}}],"edges":[]}"#;
        let q = |v: Value| erasing("C:/w", &v, Some(asked));
        assert_eq!(q(json!({"question":"q_1"})).unwrap()["questions"], json!(["q_1"]));
        assert_eq!(q(json!({"question":"이름은?"})).unwrap()["questions"], json!(["q_2"]), "by the question itself");
        assert!(q(json!({"question":"q_9"})).unwrap_err().contains("질문 블록"));
        assert!(q(json!({"all":"true","question":"q_1"})).is_err(), "--all alone");
        let took = erased(&q(json!({"all":"true"})).unwrap(), Some(asked));
        assert_eq!((took["questions"].clone(), took["unanswered"].clone()), (json!(3), json!(1)), "--all takes the questions too: one of the agent's was still waiting (not the answered one, not the hand-drawn one)");
        assert!(q(json!({"title":"API"})).is_err(), "nothing drawn here, only questions");
        // a set's id takes its questions and the area they sit in
        let set = r#"{"nodes":[
            {"id":"zone_1","type":"zone","data":{"title":"질문 2개","qset":"qs_1","mode":"step"}},
            {"id":"question_q_1","type":"question_block","parentId":"zone_1","data":{"qid":"q_1","title":"어느 DB?","set":"qs_1"}},
            {"id":"question_q_2","type":"question_block","parentId":"zone_1","data":{"qid":"q_2","title":"이름은?","set":"qs_1"}},
            {"id":"question_q_3","type":"question_block","data":{"qid":"q_3","title":"따로 물은 것"}}],"edges":[]}"#;
        let s = erasing("C:/w", &json!({"question":"qs_1"}), Some(set)).unwrap();
        assert_eq!((s["questions"].clone(), s["sets"].clone()), (json!(["q_1", "q_2"]), json!(["qs_1"])));
        assert_eq!(erasing("C:/w", &json!({"question":"q_3"}), Some(set)).unwrap()["sets"], json!([]));
        // the title the set was asked under works too; one of its questions named alone goes alone
        let t = erasing("C:/w", &json!({"question":"질문 2개"}), Some(set)).unwrap();
        assert_eq!((t["questions"].clone(), t["sets"].clone()), (json!(["q_1", "q_2"]), json!(["qs_1"])));
        let one = erasing("C:/w", &json!({"question":"이름은?"}), Some(set)).unwrap();
        assert_eq!((one["questions"].clone(), one["sets"].clone()), (json!(["q_2"]), json!([])));
    }

    #[test]
    fn drawing_checks_names() {
        let dir = std::env::temp_dir().join(format!("maestro-draw-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let caller = worktree::norm(&dir);
        let put = |s: &str| std::fs::write(dir.join("s.json"), s).unwrap();
        put(r#"{"areas":[{"title":"서버"}],"blocks":[{"title":"API","area":"서버"},{"title":"화면"}],"arrows":[{"from":"화면","to":"API","label":"REST"}]}"#);
        let (ok, _) = drawing(&caller, "s.json").unwrap();
        assert_eq!(ok["blocks"].as_array().unwrap().len(), 2);
        assert!(ok.get("layout").is_none());
        put(r#"{"layout":"flow","areas":[{"title":"서버","layout":"tree"}],"blocks":[{"title":"API","area":"서버"}]}"#);
        assert_eq!(drawing(&caller, "s.json").unwrap().0["layout"], "flow", "the layout goes to the canvas");
        put(r#"{"layout":"spiral","blocks":[{"title":"API"}]}"#);
        assert!(drawing(&caller, "s.json").unwrap_err().contains("spiral"));
        // a border colour is one of the line colours, on a block or an area
        put(r#"{"areas":[{"title":"서버","color":"teal"}],"blocks":[{"title":"API","area":"서버","color":"blue"}]}"#);
        let (colored, _) = drawing(&caller, "s.json").unwrap();
        assert_eq!((colored["areas"][0]["color"].clone(), colored["blocks"][0]["color"].clone()), (json!("teal"), json!("blue")), "the colour goes to the canvas");
        put(r#"{"blocks":[{"title":"API","color":"pink"}]}"#);
        assert!(drawing(&caller, "s.json").unwrap_err().contains("pink"));
        put(r#"{"blocks":[{"title":"화면"}],"arrows":[{"from":"화면","to":"없음"}]}"#);
        assert!(drawing(&caller, "s.json").unwrap_err().contains("없음"));
        put(r#"{"blocks":[{"title":"API","area":"서버"}]}"#);
        assert!(drawing(&caller, "s.json").unwrap_err().contains("서버"));
        assert!(drawing(&caller, "").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn worker_phase_puts_a_question_first() {
        use crate::agent::AgentState::*;
        assert_eq!(worker_phase(&Running, true, false, false), "asking");
        assert_eq!(worker_phase(&Running, false, true, true), "running");
        assert_eq!(worker_phase(&Idle, false, true, true), "failed");
        assert_eq!(worker_phase(&Idle, false, false, true), "merge-pending");
        assert_eq!(worker_phase(&Idle, false, false, false), "idle");
        assert_eq!(worker_phase(&Terminated, false, false, false), "off");
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
    fn an_agent_stops_only_its_own_workers() {
        let lead = "C:/p";
        let mine = vec!["C:/p/.wt/api".to_string(), "c:\\p\\.wt\\web\\".to_string()];
        // its workers, whatever the case or the slashes
        assert!(check_stop(lead, &mine, "c:/P/.wt/API", "api").is_ok());
        assert!(check_stop(lead, &mine, "C:/p/.wt/web", "web").is_ok());
        // not itself
        assert!(check_stop(lead, &mine, "C:/p/", "Lead").unwrap_err().contains("자기 자신"));
        // not a sibling, its parent or a worker's worker (that one's own parent stops it)
        let err = check_stop("C:/p/.wt/api", &["C:/p/.wt/api-sub".to_string()], "C:/p/.wt/web", "web").unwrap_err();
        assert!(err.contains("'web'은(는) 내 워커가 아니에요"), "{err}");
        assert!(check_stop("C:/p/.wt/api", &[], lead, "Lead").is_err());
        assert!(check_stop(lead, &mine, "C:/p/.wt/api-sub", "api-sub").is_err());
    }

    #[test]
    fn a_stop_note_waits_for_the_next_instruction() {
        let dir = std::env::temp_dir().join(format!("maestro-stop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (stops, notes) = (dir.join("api.stop.json"), dir.join("api.notes.json"));
        let _ = std::fs::remove_file(&stops);
        let _ = std::fs::remove_file(&notes);
        // kept in its own file, not pasted: a paste would start a turn right after the stop
        assert!(keep_stop_note(&stops, "C:/P", &stop_note("Lead")));
        // a second stop by the same caller replaces the first; another caller's stays apart
        assert!(keep_stop_note(&stops, "c:/p", &stop_note("Lead")));
        assert!(keep_stop_note(&stops, "C:/q", &stop_note("Other")));
        assert_eq!(read_notes(&stops).iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(), ["stop:c:/p", "stop:c:/q"]);
        // the notes queue that a start, an opened view or a restart empties on its own has nothing to paste
        assert!(!notes.exists());
        assert!(!pass_notes(&notes, None, Some(&|_: &str, _: &[(String, String)]| -> Result<(), String> { panic!("pasted alone") })));
        // only the caller's next instruction takes it, and both go out as one text (one prompt, one turn)
        let note = take_stop_note(&stops, "c:/P").unwrap();
        assert_eq!(note, stop_note("Lead"));
        assert!(note.starts_with("[maestro] 상위 에이전트 'Lead'이(가) 하던 턴을 중단했어요"));
        assert!(!note.contains("이 메모만"), "{note}");
        let text = with_stop_note(Some(&note), "API 대신 CLI부터 고쳐 주세요");
        assert_eq!(text, format!("{}\n\nAPI 대신 CLI부터 고쳐 주세요", stop_note("Lead")));
        assert_eq!(take_stop_note(&stops, "C:/P"), None);
        assert_eq!(with_stop_note(None, "다음 지시"), "다음 지시");
        // the other caller's note waits for that one's instruction; the last one taken removes the file
        assert_eq!(take_stop_note(&stops, "c:/q"), Some(stop_note("Other")));
        assert!(!stops.exists());
        assert_eq!(take_stop_note(&stops, "c:/q"), None);
        let _ = std::fs::remove_dir_all(&dir);
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
        let (r, w) = prompt_result("api", Err("프로젝트 신뢰 확인이 먼저 필요해요".into()), true);
        assert_eq!((r["delivered"].clone(), r["kept"].clone()), (json!(false), json!(true)));
        assert!(r["error"].as_str().unwrap().contains("프로젝트 신뢰 확인"));
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
