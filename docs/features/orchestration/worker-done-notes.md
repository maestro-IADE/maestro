---
title: 워커 완료 메모 (worker_done)
area: orchestration
status: done
files:
  - src-tauri/src/control.rs
  - src-tauri/src/agent.rs
  - src-tauri/src/tui_wait.rs
  - src-tauri/src/main.rs
  - src-tauri/src/events.rs
  - src-tauri/src/stream.rs
related:
  - features/orchestration/merge-request.md
  - features/orchestration/worker-hierarchy.md
  - features/orchestration/maestro-agent.md
  - features/orchestration/maestro-cli.md
  - architecture/agent-runtime.md
  - reference/storage-layout.md
  - architecture/runtime-events.md
updated: 2026-11-12
---

# 워커 완료 메모 (worker_done)

## 요약

워커 에이전트가 한 턴을 끝내면 Maestro가 그 턴의 답변을 상위 에이전트 입력창에 `[maestro] 워커 '<이름>' 완료. 마지막 답변:` 메모로 붙여 넣어요. 턴이 오류로 끝났으면(로그인 필요, API 오류, 어댑터 종료 등) `[maestro] 워커 '<이름>' 실패 (<오류>): <메시지>` 메모를 보내요. 실패한 턴을 완료로 알리지 않아요. 상위(보통 마에스트로)가 폴링하지 않아도 이어서 일할 수 있게 하는 기능이에요. 상위가 꺼져 있거나 폴더 신뢰 확인 화면에 있으면 메모를 `<meta>.notes.json`에 보관했다가, 상위가 다시 시작되거나 신뢰 확인을 마치면 붙여 넣어요. 구현은 `control::worker_done`·`pass_notes`와 `Agent::deliver_kept_notes`예요.

## 동작

- 워커가 답을 끝내고 대기 상태가 되면, 상위 에이전트의 프롬프트에 다음 형식의 메시지가 보내져요.
  - `[maestro] 워커 '<이름>' 완료. 마지막 답변:\n<답변>`
  - 답변은 **방금 끝난 턴의 글만** 써요. 그 턴에 글 답변이 없으면(도구만 쓰고 끝남 등) `[maestro] 워커 '<이름>' 완료. 이번 턴에는 글 답변이 없어요.`예요. 앞 턴의 답을 대신 붙이지 않아요.
  - 답변이 1500자를 넘으면 잘리고 `… (잘림, 전체는 \`maestro read --to <이름>\`)`이 붙어요.
- 턴이 실패로 끝나면(`turnEnded`의 결과가 `failed`) 다음 형식이에요. "완료"라는 말은 들어가지 않아요.
  - `[maestro] 워커 '<이름>' 실패 (<error>): <message>` (메시지는 앞뒤 공백을 빼고 300자까지)
  - 그 턴에 글이 있고 메시지와 같은 글이 아니면 `\n이번 턴 답변:\n<답변>`(1500자 자르기 같음). Claude API 오류는 오류 글이 그 턴의 답변이기도 해서, 같은 글을 두 번 붙이지 않아요.
  - 끝에 `\n작업이 끝나지 않았어요. 워커를 확인하고 다시 시키거나 사용자에게 알리세요.`
  - 앞 턴의 답은 절대 붙이지 않아요. 새 워커의 첫 턴이 실패하면 오류만 가요.
- 보관 키는 완료든 실패든 워커 이름이라, 보관 중인 실패 메모는 그 워커의 다음 완료 메모로 바뀌어요(반대도 같아요).
- 상위가 실행 중이 아니면(꺼짐) 메모를 보관해요. 메모 때문에 상위를 새로 띄우지는 않아요. 사용자가 상위를 열거나 메시지를 보내서 다시 시작되면(`ensure`), 입력창이 뜬 뒤 보관한 메모를 붙여 넣어요.
- 상위가 폴더 신뢰 확인 화면에 있으면 보관했다가, `신뢰하고 시작`(`accept_trust`) 뒤 입력창이 뜨면 붙여 넣어요.
- 붙여 넣기가 실패해도(터미널이 닫힘 등) 보관해요.
- 보관한 메모가 여럿이면 오래된 것부터 빈 줄로 이어 한 번에 붙여 넣어요. 새 메모가 올 때 상위가 준비돼 있으면 보관분 뒤에 새 메모를 붙여 같이 보내요.
- 보관은 워커마다 마지막 메모 하나예요. 같은 워커가 다시 끝나면 앞 메모를 새 메모로 바꾸고 맨 뒤로 옮겨요. 그래서 어느 워커가 끝났는지는 버리지 않고, 파일은 워커 수만큼만 커져요. 앞 턴의 답변은 `maestro read --to`로 볼 수 있어요.
- 상위가 입력창을 기다리는 사이에 꺼지거나, 다시 시작하다 바로 종료되면(로그인 만료, `--resume` 실패 등) 붙여 넣지 않고 보관해요.
- 상위가 없는 워크트리(또는 마에스트로 자신)는 메모를 보내지 않아요.

## 구현

- 호출 지점: `Agent::spawn`이 만든 스트림 observer(`agent.rs`). durable 공통 이벤트마다 `events::status_after(ev)`로 다음 상태를 구하고, `events::finishes_turn(직전 상태, ev)`면 "끝난 턴"으로 봐요.
  - 이벤트가 `turnEnded`이고 결과가 `done`(모델이 끝냄) 또는 `failed`(API 오류로 끝남)
  - 채팅 런타임이 실패한 턴 앞에 넣는 `notice(failed)`(채팅의 빨간 이유 줄)는 상태를 바꾸지 않아요(`status_after` = 없음). 그래서 뒤따르는 `turnEnded(failed)`가 아직 Running을 보고, 실패 메모가 가요.
  - 직전 상태가 `AgentState::Running`
  - Esc 중단(`cancelled`)이나 슬래시 명령(`command`), 새 프롬프트로 바뀐 턴(`replaced`)은 빠져요. R3 이전 규칙(최종 `stop_reason`이 있는 assistant 이벤트)과 같아요.
  - 조건이 맞으면 그 `turnEnded`의 `turn_id`와 결과(`TurnOutcome`)를 들고 `thread::spawn(move || control::worker_done(&app, &path, &turn, &outcome))`. 전달이 상위 프롬프트가 뜰 때까지 막히므로 별도 스레드예요.
  - observer는 live로 나가는 이벤트만 봐요. 시작할 때나 트랜스크립트가 짧아져 처음부터 다시 읽을 때(`reset(Truncated)`)는 이미 있던 줄을 세기만 해서(`prime`), 지난 턴마다 메모가 다시 가지 않아요.
  - 이미 닫힌 턴의 메시지에서 늦게 온 블록(straggler)은 턴을 다시 닫지 않아서, 다른 tail 묶음에 읽혀도 메모가 두 번 가지 않아요. → [에이전트 실행 방식](../../architecture/agent-runtime.md)
- `worker_done(app, path, turn_id, outcome)` (`control.rs`):
  1. `locate()`로 프로젝트와 자기 `WorktreeInfo`를 찾아요.
  2. `worktree_infos()`에서 `parent_of()`로 상위를 찾아요(`workers`에 내 경로가 있는 에이전트).
  3. `history_of()`(`runtime::history()`, 실행 중이면 그 런타임의 기록)로 대화를 공통 이벤트로 읽고 `worker_note(이름, turn_id, outcome, 이벤트)`로 메모를 만들어요.
  4. 상위의 `files()`로 보관 파일 경로(`Files::notes()` = `<meta>.notes.json`)를 정해요.
  5. `state.get(parent)`의 `wait_ready()`가 `Ok(false)`(입력창이 뜸)이고 그 뒤에도 `alive()`면 `pass_notes(file, Some((이름, note)), Some(send))`, 아니면 `pass_notes(file, Some((이름, note)), None)`(보관만). 신뢰 확인 중(`Ok(true)`)이나 시작 실패(`Err`, 로그인 필요·종료·시간 초과)면 보관만 해요.
  - 모든 오류는 버려요(`let Ok(..) else return`).
- `pass_notes(file, new, send)` (`control.rs`): 보관 파일(`[워커 이름, 메모]` 쌍의 JSON 배열)을 읽고, `new`의 워커와 같은 이름의 항목을 빼고 `new`를 뒤에 붙여요. `send`가 있고 성공하면(메모만 `"\n\n"`로 이어 한 번에) 파일을 지우고 `true`. `send`가 없거나 실패하면 전부 파일에 다시 써요. 전역 `Mutex`(`NOTES`)로 한 번에 하나만 처리해서, 시작 시 전달과 새 워커 메모가 겹쳐도 같은 메모가 두 번 붙지 않아요. 테스트: `notes_are_kept_until_the_parent_can_take_them`.
- `Agent::deliver_kept_notes(self: Arc<Self>)` (`agent.rs`): 보관 파일이 있으면 스레드에서 `wait_ready()`를 기다렸다가 `Ok(false)`이고 아직 `alive()`면 `pass_notes(file, None, Some(send))`. 부르는 곳:
  - `ensure()`(`main.rs`): 에이전트를 새로 띄웠을 때. `send_message`, `ensure_agent`, `agent_ready`, `create_maestro`, `attach_agent`, `maestro send` 등이 모두 이 길로 시작해요.
  - `accept_trust` 명령: 신뢰 확인을 수락한 뒤.
  - 상위의 터미널이 자기 선택지로 답을 기다리던 상태가 풀렸을 때(`Agent::observer`, `Agent::write`, `Agent::interrupt`). 그 상태 동안 `send_prompt`가 오류를 돌려서 메모는 보관 파일에 남아요. → [승인](../agents/approvals.md)
- `Agent::send`는 `sending` 잠금으로 붙여 넣기와 Enter를 한 번에 하나씩만 해요. 보관 메모 전달과 사용자 채팅 입력이 동시에 와도 글이 섞이지 않아요. 잠금을 잡은 뒤 `alive()`가 아니면 `Err("에이전트가 꺼져 있어요")`예요. ConPTY는 CLI가 끝난 뒤에도 쓰기를 받아서, 이 확인이 없으면 꺼진 상위에 붙인 메모가 보낸 것으로 처리되고 보관 파일이 지워져요.
- 보관 파일은 `forget_agent`(폐기)가 meta와 함께 지워요.
- `worker_note(name, turn_id, outcome, evs)` (`control.rs`, 순수 함수): 실패면 `events::turn_answer(evs, turn_id)`만 써요. 완료면 같은 함수를 쓰되, 그 `turn_id`가 기록에 아예 없으면(다시 읽기가 재시작과 겹친 경우) 안전망으로 `events::last_answer(evs)`를 써요. 턴은 찾았는데 글이 없으면 없음 그대로예요.
- `events::turn_answer(events, turn_id)`: `turn_id()`가 그 턴인 이벤트만 골라 `last_answer`를 돌려요. 그 턴에 글이 없으면 `None`이에요. 같은 `itemId`로 다시 온 ACP 글은 마지막 글 하나예요. 테스트: `turn_answer_reads_only_that_turn`.
- `done_note(name, outcome, answer)`: 완료/실패 형식을 만들어요. 답변은 `1500`자(문자 단위, `char_indices`)에서 자르고 잘렸으면 안내를 붙여요. 실패 메시지는 300자까지예요. 완료 + 글 있음은 R3 이전과 바이트가 같아요. 이름은 `name`이 없으면 브랜치예요.
- 테스트: `done_note_truncates_long_answers`, `failed_turn_note_has_no_earlier_answer`, `failed_claude_api_turn_keeps_its_own_text_once`, `done_note_reads_only_the_ended_turn`, `done_note_falls_back_when_turn_unknown`.
- 메모는 스트림에서 받은 이벤트 대신 `history_of()`로 다시 읽은 기록에서 만들어요. ACP는 observer가 기록에 넣기 전에 불리지만, `worker_done`은 다른 스레드에서 같은 잠금을 잡고 읽어서 그때는 `turnEnded`까지 들어 있어요. PTY는 `turnEnded`가 파일에서 왔으니 다시 읽어도 있어요. `control::last_answer()`(대화 전체의 마지막 답)는 이제 `maestro read`만 써요.
- 마에스트로 브리프(`COORDINATOR_HINT`)에도 이 메모가 온다고 적혀 있어요.
- 전달 함수는 `control::tell(app, root, to_path, key, note)`예요(`files()` → `notes()`, 입력창이 뜨고 살아 있으면 보내고 아니면 보관). `worker_done`은 `tell(…, 상위 경로, 워커 이름, 메모)`로 불러요.
- `maestro worktree create --prompt`가 워커를 만든 뒤 첫 지시를 보내지 못하면, 그 지시를 **새 워커 자신의** `.notes.json`에 키 `prompt:<만든 에이전트 경로 소문자>`로 보관해요(`control::keep_prompt`). `[maestro]` 메모가 아니라 지시 글 그대로예요. 워커가 다음에 시작되거나 신뢰 확인을 마치면 `deliver_kept_notes`가 붙여 넣어요. 시작이 늦거나(입력창 제한 시간 초과) 로그인 화면에서 멈춰 워커가 켜진 채로 남으면, `Agent::deliver_kept_notes_when_ready`가 그 워커를 지켜보다가 입력창이 뜨면 붙여 넣어요(워커가 꺼지거나 새로 시작되거나 신뢰 확인 창이 뜨면 그만 지켜봐요). 채팅 뷰가 `agent_ready`로 준비를 확인했을 때와 `maestro send`가 보낼 때도 남은 메모를 먼저 보내요(`send`의 글보다 앞). 보관은 오래된 순이라 그 뒤에 온 메모보다 먼저 가요. 테스트: `kept_prompt_is_delivered_on_next_start`. → [maestro CLI](maestro-cli.md)
- 병합 결과 메모(`[maestro] 병합 …`)도 같은 길로 가요. 키는 `merge:<워커 경로 소문자>`라서, 같은 워커의 완료 메모(키 = 워커 이름)와 서로 지우지 않고 나란히 보관돼요. 테스트: `merge_notes_keep_worker_done`. → [병합 요청](merge-request.md)

- **채팅 런타임(ACP)에서 꺼질 때:** 메모를 붙인 글은 ACP 큐에서 차례를 기다려요. 보내기 전에 어댑터가 꺼지면(또는 보낸 글이 트랜스크립트에 닿기 전에 꺼지면) 그 메모를 `control::keep_notes`로 메모 파일에 다시 넣고, 다음 시작 때 `deliver_kept_notes`가 보내요. 그사이 같은 워커의 더 새 메모가 있으면 그게 이겨요. `pass_notes`는 보낸 메모 항목을 `Agent::send_note`에 같이 넘겨요.

## 제약과 알려진 문제

- 사용자가 직접 워커에게 시킨 턴이 끝나도 메모가 가요. 누가 시킨 턴인지 구분하지 않아요.
- 상위가 작업 중일 때 붙여 넣은 메모가 어떻게 처리되는지(대기열에 들어가는지)는 확인 안 됨이에요. `send()`는 상위 상태를 따로 확인하지 않아요.
- 보관 메모는 상위가 다시 시작될 때 전달돼요. 상위가 계속 꺼져 있으면 쌓이기만 하고(워커마다 마지막 하나), 상위를 깨우지는 않아요.
- 상위가 Maestro 밖에서 다시 시작되면(터미널에서 직접 `claude --resume` 등) 보관 메모는 전달되지 않아요. Maestro가 띄운 에이전트만 대상이에요.
- Antigravity 워커도 같은 스트림을 쓰므로 조건이 맞으면 메모가 가요(도구 호출 없는 응답 = `turnEnded done`). 실제 agy 실행에서 확인하지는 않았어요.

## 결정 기록

- 상위가 꺼져 있으면 깨우지 않아요. 메모 하나 때문에 에이전트를 다시 띄우지 않으려는 것이에요(`worker_done` 주석).
- 예전에는 꺼져 있거나 신뢰 확인 중인 상위에게 가는 메모를 버렸어요. 지휘자가 워커 결과를 놓치는 문제라서(PRD R1) 보관 후 전달로 바꿨어요.
- 보관 위치를 meta 필드가 아니라 별도 파일(`.notes.json`)로 정했어요. meta는 여러 곳에서 통째로 다시 쓰고(요약 생성은 모델 호출 뒤 다시 읽어 쓰지만, 읽기와 쓰기가 원자적이지 않아요) 그 사이에 붙인 메모가 지워질 수 있어서예요.
- 답변 전체 대신 1500자만 붙여요. 전체는 `maestro read --to`로 읽게 했어요.
- 메모를 대화 전체의 마지막 답이 아니라 끝난 턴의 글로만 만들어요. 예전에는 실패한 턴도 `완료`로 알리고 앞 턴의 답을 붙여서, 지휘자가 끝나지 않은 일을 끝난 것으로 읽었어요. 그래서 실패는 `실패` 메모로 따로 보내고, 그 턴에 글이 없으면 없다고 적어요. 보관 키는 그대로 워커 이름이라, 실패 메모와 완료 메모는 서로 바꿔 써요.
- 보관 개수를 최근 N개로 자르지 않고 워커마다 하나로 정했어요. 개수로 자르면 워커가 많을 때 앞서 끝난 워커의 완료 소식이 사라져서, R1이 고치려던 문제(완료를 놓침)가 다시 생겨요.
