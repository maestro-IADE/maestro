---
title: 에이전트 실행 방식 (런타임 인터페이스 + PTY 런타임)
area: architecture
status: done
files: [src-tauri/src/agent.rs, src-tauri/src/runtime.rs, src-tauri/src/pty.rs, src-tauri/src/transcript.rs, src-tauri/src/stream.rs, src-tauri/src/events.rs, src-tauri/src/claude.rs, src-tauri/src/antigravity.rs, src-tauri/src/approval.rs, src-tauri/src/catalog.rs, src-tauri/src/cli.rs, src-tauri/src/main.rs, src/chatFold.ts, scripts/chat-fold-parity.ts, src/nodes/XTerm.tsx, src/nodes/AgentNode.tsx, src/nodes/WorktreeCard.tsx]
related: [architecture/runtime-events.md, architecture/overview.md, architecture/local-server.md, features/agents/chat-view.md, features/agents/cli-view.md, features/agents/agent-adapters.md, features/agents/approvals.md, features/agents/cli-install.md, reference/events.md, reference/storage-layout.md]
updated: 2026-11-10
---

# 에이전트 실행 방식

## 요약

에이전트 하나는 **런타임** 하나가 돌려요(`Runtime` 트레이트, runtime.rs). 지금 런타임은 **PTY 런타임**(pty.rs) 하나예요. 실제 CLI(TUI)를 PTY에 띄우고, CLI 뷰는 그 화면을 그대로, 채팅 뷰는 CLI가 디스크에 쓰는 트랜스크립트를 **공통 이벤트**(events.rs)로 바꿔 보여 줘요. 채팅 입력은 PTY에 붙여넣기로 들어가서, 두 뷰는 같은 프로세스·같은 대화를 다르게 보여 줄 뿐이에요. 나머지 코드(main.rs, control.rs)는 `Agent` 파사드(agent.rs)만 써서 런타임이 무엇인지 몰라요. R4의 ACP 런타임은 같은 트레이트를 구현해요. → [공통 이벤트](runtime-events.md)

## 동작

- 에이전트 블록을 열면 CLI가 먼저 떠요. 채팅 입력은 CLI가 입력을 받을 준비가 된 뒤에야 보여요. 그 전에는 "준비 중…" 로딩이 보여요.
- 처음 여는 폴더면 Claude가 "이 폴더를 신뢰할까요?"를 물어요. 채팅 뷰에 "신뢰하고 시작" 카드가 떠요.
- 시작이 안 되면 이유를 보여 줘요: 실행 파일 없음, 로그인(또는 첫 실행 설정) 필요, 시작하다 종료(마지막 출력 한 줄), 시작 시간 초과. 채팅 뷰에는 "다시 시도"와 "CLI에서 보기" 버튼이 있어요. 카드와 에이전트 바에는 "다시 시도"가 있어요.
- 앱을 다시 켜도 같은 세션으로 이어서 실행돼요.
- 에이전트가 다시 시작되면(종료 뒤, 자리 비움 정지 뒤, 다시 시도) 열려 있는 채팅 뷰는 기록을 새로 받고, CLI 뷰는 화면을 지우고 새 프로세스 출력부터 그려요.
- Claude에서 `/clear`를 하거나 Antigravity가 더 최근 대화로 옮겨도 채팅 뷰는 처음 시작한 대화에 머물러요(R3 전과 같아요). 따라가는 코드는 있지만 `FOLLOW_SESSIONS`로 꺼 두었어요.

## 구현

### 계층

| 파일 | 내용 |
|---|---|
| `events.rs` | 공통 이벤트 `RuntimeEvent`, `Envelope`, `Transcript`, 상태 규칙(`status_after`, `finishes_turn`), `last_answer`, `recent_turn_text`. 테스트 전용 `oracle` 모듈(R3 이전 코드 사본) |
| `runtime.rs` | `Runtime`·`Terminal` 트레이트, `Prompt`, `Readiness`, `LaunchFailure`, `PtyAgent` 트레이트·`Launch`·`LaunchCtx`, `pty_agent(id)`, `history()`(실행 중인 런타임의 기록, 없으면 `transcript_history()`), `snapshot()`, `PtySnapshot` |
| `pty.rs` | `PtyRuntime`: PTY 띄우기, 출력 배치, `Scrollback`, 준비 판정, 키 입력, 종료 감시, 트랜스크립트 tail 스레드 |
| `transcript.rs` | `Normalize` 트레이트, `TurnTracker`, `Tail`, `LiveTail`, `FOLLOW_SESSIONS` |
| `stream.rs` | 워크트리별 스트림(epoch·seq), `Emitter`(번호 매기기, 델타 묶기), PTY epoch |
| `claude.rs` | `ClaudeAgent: PtyAgent`, `ClaudeNormalizer`, `session_file`, argv, 요약 호출 |
| `antigravity.rs` | `AgyAgent: PtyAgent`, `AgyNormalizer`, argv, `conversation_for`, `ask` |
| `agent.rs` | `AgentMeta`, `Files`, `AgentState`, `emit_state`, 시스템 브리프, `maestro_env`, 프로세스 목록, `summarize`, **`Agent` 파사드** |

`antigravity::ID`로 나누는 곳은 `runtime::pty_agent()` 하나뿐이에요(그 밖에는 antigravity.rs와 카탈로그 테스트). 승인 훅 연결은 카탈로그 `features.approval_hook`을 보고 `ClaudeAgent::launch`가 해요.

### 트레이트

- `Runtime`(Send + Sync, 스레드 기반): `kind`, `ready() -> Result<Readiness, LaunchFailure>`, `accept_trust`, `prompt(&Prompt)`, `cancel`, `kill`, `alive`, `quiet_for`, `pid`, `terminal() -> Option<&dyn Terminal>`, `sync`, `locate_tool(name, input, tool_call_id) -> ToolMatch { turn_id, tool_call_id, exact }`, `follow(conversation, transcript)`, `history() -> Option<(epoch, events)>`.
  - `history()`는 자기 기록을 가진 런타임(ACP, R4)만 `Some`이고, 그 기록이 번호를 매긴 epoch를 같이 줘요. 스트림의 지금 epoch와 다른 기록(재시작 중 아직 등록돼 있는 이전 실행의 것)은 쓰지 않아요.
  - 그 기록은 런타임이 따로 쌓지 않고 `Emitter::with_record`로 만든 Emitter가 쌓아요. seq를 매기는 같은 Stream 잠금 안에서 내보낸 durable 이벤트만 붙이고 `reset` 때 비워서, 기록 길이와 live seq·epoch가 어긋날 수 없어요. ACP의 `history()`는 `Emitter::history()`를 그대로 돌려줘요(`a_record_kept_by_the_emitter_snapshots_at_the_live_seq`).
  - ACP(R4)에 대응: `ready` = initialize + session/new·load, `prompt` = session/prompt, `cancel` = session/cancel, 승인은 `Approvals::request`로.
- `Terminal`: `write`, `resize`, `snapshot() -> PtySnapshot { dataB64, seq, epoch }`.
- `PtyAgent`(CLI마다 다른 것): `id`, `launch(ctx, meta, files, exe) -> Launch { argv, transcript, resumed }`, `conversation_id`, `transcript`, `current_conversation`(agy만), `normalizer`, `preview`, `summarize`.
  - `pty_agent(id)`: `antigravity` → `AGY`, 나머지 → `CLAUDE`(예전 else 분기와 같아요).

### `Agent` 파사드(agent.rs)

main.rs와 control.rs가 부르는 API는 그대로예요: `spawn`, `run`(설정의 설치 터미널), `wait_ready() -> Result<bool, LaunchFailure>`, `accept_trust`, `send`, `interrupt`, `write`, `resize`, `snapshot`, `quiet_for`, `runs_shell`, `kill`, `alive`, `state`, `notes`, `deliver_kept_notes`. 새로 `sync`, `follow`, `locate_tool`이 있어요.

**`Agent::spawn` 순서**
1. `catalog::require` → `launchable()` 확인 → `cli::command`(실행 파일 없으면 여기서 실패).
2. `pty_agent(adapter).launch(..)`: 세션 id(없으면 만들어 meta에 저장), 브리프 파일, Claude면 `--settings` 훅 파일과 승인 등록, argv.
   - Claude: 새 세션이면 `--session-id <uuid>`, 트랜스크립트가 있으면 `--resume <uuid>`.
   - Antigravity: `--conversation <id>`. → [에이전트 종류](../features/agents/agent-adapters.md)
3. `Emitter::new`(sink = `agent://event`, observer = 상태 점·워커 완료) → `emitter.reset(Restart)`: epoch를 올리고 `reset` 이벤트를 보내요.
4. `LiveTail::start`: 트랜스크립트를 지금 끝까지 읽어 개수만 세고(`prime`) 내보내지 않아요.
5. `PtyRuntime::start(.., Some(live), login_hints, resumed, on_exit)`: `on_exit` = `emit_state(Terminated)`.
6. `agent://state`로 `IDLE`을 직접 보내요(재시작한 에이전트가 "꺼짐"으로 남지 않게).

**observer**(모든 durable 이벤트, 보내기 전에): `status_after(ev)`가 상태를 주면 `finishes_turn(이전 상태, ev)`를 먼저 보고 `emit_state`, 끝난 턴이면 `control::worker_done`을 별도 스레드로 띄워요. 예전 tail 루프(상태 먼저, 그다음 이벤트)와 순서가 같아요.

**시작은 워크트리마다 하나씩:** `ensure()`와 `create_worktree`는 워크트리별 잠금(`AppState::start_lock`)을 쥐고, 살아 있는 에이전트를 잠금 안에서 다시 본 뒤 `Agent::spawn` → `agents.insert`까지 해요. 채팅 뷰(`agent_ready`), CLI 뷰(`ensure_agent`), `send_message`, `maestro send`가 동시에 불러도 하나만 떠요. 잠금이 없으면 둘 다 떠서 프로세스 하나가 남고, 두 번째 `Emitter::new`·`reset`이 epoch를 가져가 등록된 쪽의 이벤트·상태·워커 완료가 모두 버려질 수 있어요. 다른 워크트리는 따로 시작해요.

**`Agent::send`**: `sending` 잠금 → 꺼져 있으면 `"에이전트가 꺼져 있어요"` → `runtime.prompt` → `emit_state(Running)`.

### PTY 런타임(pty.rs)

- **환경 변수:** `TERM=xterm-256color`, `COLORTERM=truecolor`. `maestro_env()`가 `MAESTRO_URL`, `MAESTRO_TOKEN`, `MAESTRO_WORKTREE`를 넣고 `~/.maestro/bin`을 PATH 앞에 붙여요. `host_session_env()`가 Maestro를 Claude Code 안에서 실행했을 때 물려받는 `CLAUDECODE`, `CLAUDE_CODE_*`, `NO_COLOR` 등을 지워요.
- **출력:** 읽기 스레드가 블로킹으로 읽고, 배처가 16ms(`BATCH_WINDOW`) 단위로 묶어 `pty://data { worktreePath, dataB64, seq, epoch }`를 보내요. `seq`는 총 바이트 수, `epoch`는 이 키의 몇 번째 PTY인지예요(`stream::bump_pty`, 시작할 때마다 +1, 설치 터미널 포함). 시작하자마자 빈 배치(`dataB64: ""`, `seq: 0`)를 한 번 보내서 열린 터미널이 바로 초기화돼요.
- **스크롤백:** `Scrollback`(최대 1MB). `pty_snapshot`이 이걸 epoch와 함께 줘요.
- **ConPTY 커서 위치 질의:** ConPTY는 시작할 때 `ESC[6n`을 보내고 답을 기다려요. 백엔드가 `ESC[1;1R`로 대신 답하고, XTerm.tsx는 자기 답을 버려요.
- **준비 판정:** `ready()`가 `Scrollback::startup(timed_out, login_hints)`을 100ms마다 봐요. 출력이 800ms 조용하거나 45초(`STARTUP_LIMIT`)가 지났을 때만 판정해요.
  1. `mark` 뒤 출력(최대 8KB, `plain()`으로 이스케이프 제거)에 카탈로그 `login_hints`가 있으면 `LoginRequired`. 이어 띄운 세션(`resumed`)은 붙여넣기 모드가 켜진 뒤에는 보지 않아요(다시 그린 지난 대화와 로그인 화면을 구분할 수 없어서).
  2. 붙여넣기 모드(`ESC[?2004h`)를 봤으면 준비됨. 그 출력에 "trust"가 있으면 `Readiness::NeedsTrust`(파사드에서 `Ok(true)`).
  3. 45초가 지났는데 붙여넣기 모드를 못 봤으면 `Timeout`.
  - 기다리는 사이 프로세스가 끝나면 `classify`로 `LoginRequired` / `Exited`(마지막 출력 한 줄).
  - 준비됨만 기억해요. 실패는 기억하지 않아서 "다시 시도"가 다시 살펴봐요.
  - 테스트: `startup_decides_ready_login_or_timeout`, `classifies_failed_starts`, `strips_escapes_from_tui_output`.
- **프롬프트:** 브래킷 붙여넣기(`ESC[200~ … ESC[201~`) 뒤 60ms 쉬고 Enter. `!`로 시작하면 `!`를 먼저 키로 치고 80ms 쉬어요(bash 모드). `/명령`만 있으면 공백을 붙여요(`/model/model` 방지). 첨부는 ` @경로`.
- **취소:** Esc. **종료:** 0.5초마다 `try_wait()`(ConPTY는 종료 뒤에도 파이프가 열려 있을 수 있어서). 끝나면 `exited` 표시 → `on_exit`(`TERMINATED`) → `LiveTail::exited()`가 `exited { turnId: 열려 있던 턴 }`을 보내요.
- **kill:** Windows에서 `taskkill /T /F`로 프로세스 트리 전체를 끝내요. 앱이 끝나면(`RunEvent::Exit`) 모든 에이전트를 끝내요.

### 트랜스크립트 → 공통 이벤트

- 위치:
  - Claude: `~/.claude/projects/<경로 슬러그>/<세션>.jsonl`
  - Antigravity: `~/.gemini/antigravity-cli/brain/<대화>/.system_generated/logs/transcript.jsonl`
- `Normalize::line(v, line_no, out)`: 한 줄 → 이벤트들. 안에 `TurnTracker`가 있어 모든 이벤트에 `turn_id`를 붙이고, 도구 호출의 주인 턴을 기억해요. 결정적이에요: id는 파일 내용과 줄 번호에서만 나와서, 다시 읽어도 같은 이벤트가 나와요. → [공통 이벤트](runtime-events.md)
- `Tail`: 바이트 오프셋부터 **완전한 줄만** 읽어요. `lines`(파싱 여부와 상관없이 모든 줄)와 `count`(이벤트 수)를 세요. `Tail::read_all`이 기록(history) 읽기예요.
- `LiveTail`(실행 중인 에이전트, `PtyRuntime`이 `Arc<Mutex<..>>`로 들고 250ms(`TAIL_EVERY`)마다 `tick`):
  1. 트랜스크립트가 아직 없으면(agy 새 대화) `pa.current_conversation()`을 봐요. 찾으면 meta의 `cliSessionId`를 읽고-고쳐-쓰고, `reset(Source)`, 처음부터 읽는 Tail로 바꿔요.
  2. 파일이 읽은 곳보다 짧아졌으면 `reset(Truncated)` 뒤 새 Tail로 지금 내용을 읽어 개수만 세요(`prime`, `LiveTail::start`와 같아요). 지난 턴들은 화면이 `reset`을 받고 다시 받아 가요. live로 다시 보내면 observer가 지난 턴마다 상태를 바꾸고 워커 완료 메모를 보내기 때문이에요. 그 뒤에 쓰인 줄만 live로 나가요.
  3. 새 줄의 이벤트를 `emitter.push`.
  4. `FOLLOW_SESSIONS`면(R3에서는 꺼짐) 2초마다 agy의 최근 대화를 봐요. 다른 대화이고 그 트랜스크립트가 지금 것보다 최근에 쓰였으면 따라가요(`follow`).
  - `follow(id, path)`: meta 먼저 쓰고, `reset(Source)` 뒤 새 파일을 2번처럼 `prime`해요. 같은 대화면 아무것도 안 해요. 1번(막 만든 agy 대화 채택)만 R3 전처럼 처음부터 live로 읽어요: 그 안에 있는 것은 지금 진행 중인 첫 턴이에요.
  - `Runtime::sync()`는 같은 잠금으로 `tick` 한 번. `locate_tool`은 `tick` 뒤 `LiveTail::locate`: 프롬프트에 id가 있으면 그 호출의 주인 턴(`TurnTracker::owner`, 아직 안 읽혔으면 열린 턴), 없으면 `TurnTracker::locate`(이름·입력이 같으면 `exact`, 이름만 같으면 짐작). 짐작이면 턴만 주고 `tool_call_id`는 `None`이에요.
- 잠금 순서는 항상 LiveTail → Stream → AgentState예요. 승인 쪽은 자기 잠금을 쥔 채 `sync`를 부르지 않아요.

### 스트림: epoch와 seq(stream.rs)

- `agent://event`는 `Envelope { worktreePath, epoch, seq, event }`예요.
- **seq:** durable 이벤트가 지금 소스의 `history()`에서 몇 번째인지(1부터). 소스는 런타임마다 달라요: PTY는 CLI 트랜스크립트를 처음부터 읽은 것(`Runtime::history()`가 `None`이라 `transcript_history()`), ACP(R4)는 런타임이 이번 epoch에 직접 쌓은 목록(`Runtime::history()`가 `Some`). `get_transcript`, `maestro read`, 카드 요약이 모두 `runtime::history(live, …)`로 같은 소스를 읽어요. 시작할 때 `prime(n)`으로 이미 있던 개수 뒤부터 이어요. live-only 이벤트는 `seq: null`이고 번호를 쓰지 않아요.
- **epoch:** 워크트리마다 1에서 시작해 늘기만 해요. 스트림을 처음부터 다시 보낼 때 올려요. 올릴 때는 쌓인 델타를 버리고 `seq = 0`으로 하고 `reset { reason }`을 `seq: null`, 새 epoch로 보내요.
  - `restart`: 런타임이 (다시) 시작할 때마다(spawn, 자리 비움 뒤 재개, 다시 시도). PRD의 "재시작"이에요.
  - `source`: 다른 대화를 읽게 됐을 때(agy 대화 채택, `/clear`, 더 최근 agy 대화).
  - `truncated`: 파일이 짧아졌을 때.
  - `switch`(채팅↔CLI), `fallback`(ACP→PTY), `replay`(session/load): R4가 같은 API로 써요.
  - 재연결·다시 마운트는 올리지 않아요. 마운트하는 화면은 늘 기록을 새로 받기 때문이에요.
- **스냅샷 원자성:** `runtime::snapshot`이 epoch를 읽고(e1) → meta에서 소스를 정하고 → 그 워크트리의 런타임을 `AppState`에서 **매번 새로** 찾아 `history()` → epoch를 다시 읽어요(e2). 다르면 한 번 더 해요. `{ epoch: e2, seq: events.len(), events }`. 아직 내보내지 않은 줄의 이벤트는 나중에 `seq ≤ 스냅샷 seq`로 와서 버려져요.
  - `Agent::spawn`은 새 런타임을 `AppState`에 넣기 전에 `reset(Restart)`를 보내요. 그 `reset`을 받고 다시 읽으면 아직 이전 실행이 보여요. 그래서 런타임의 기록은 자기 epoch가 e1과 같을 때만 쓰고, 아니면 한 번 더 찾고, 그래도 다르면 트랜스크립트를 읽어요. e1 ≠ e2로 끝났을 때도 런타임 기록은 e2로 찍지 않고 트랜스크립트로 바꿔요(`a_record_of_another_epoch_is_never_used`). PTY는 기록이 `None`이라 바뀌는 것이 없어요.
- `push`는 durable 이벤트가 받은 seq를 돌려줘요(live-only이거나 버렸으면 `None`). `reset`은 새 epoch를 돌려주고(거절되면 `None`), `Emitter::epoch()`로 자기 epoch를 읽어요.
- 지난 실행의 `Emitter`는 아무것도 못 써요. `push`·`prime`은 epoch가 다르면 버리고, `reset`은 그 워크트리의 지금 Emitter(`Stream.emitter`)이면서 epoch도 같을 때만 해요. 그래서 이전 실행의 꼬리 루프가 새 실행의 `Emitter::new` 뒤에 한 번 더 돌아(최대 250ms) `truncated`·agy 채택·`/clear` 따라가기로 `reset`을 불러도, 새 실행의 스트림을 빼앗지 못해요(`an_older_emitter_cannot_reset`).

### 델타 묶기(~50ms)

- `Emitter::push`가 Stream 잠금 안에서 다 해요.
  - durable: 쌓인 델타를 먼저 보내고 → `seq += 1` → observer → sink.
  - `delta`: 마지막으로 쌓인 것과 `(itemId, channel)`이 같고 새 `offset`이 쌓인 글의 끝(`offset + UTF-16 길이`)과 같을 때만 글을 이어 붙이고 첫 `offset`을 유지해요. 아니면 새로 쌓아요. 같은 조각이 다시 오거나 겹쳐 와도(재시도, 재연결) 따로 나가서, offset에 맞춰 접으면 묶지 않았을 때와 결과가 같아요(`only_contiguous_deltas_merge_so_folding_stays_idempotent`). 64KB를 넘으면 바로 보내요.
  - 그 밖의 live-only: 쌓인 델타를 먼저 보내고 `seq: null`로 보내요.
- 타이머 스레드는 첫 델타가 올 때 하나만 생겨요(`DELTA_WINDOW` = 50ms). PTY 런타임은 델타를 보내지 않으므로 스레드도 지연도 없어요. Emitter가 사라지면 스레드도 끝나요. 그래서 `Drop for Emitter`가 마지막 창(50ms 이내)에 쌓인 델타를 그 자리에서 보내요(자기 epoch일 때만). 안 그러면 다음 실행의 `reset`이 버려 꼬리가 사라져요(`a_dropped_emitter_sends_its_last_deltas`). sink·observer는 Stream 잠금을 쥔 채 마지막 `Arc<Emitter>`를 들지 않아서 같은 잠금을 다시 잡는 일이 없어요.
- 생각은 Paradev의 mergeDeltas·flush-before-durable에서 왔어요. 코드는 베끼지 않았어요.

### 상태 규칙

| 이벤트 | 새 상태 |
|---|---|
| `userMessage` | Running |
| `notice` | Idle |
| `turnEnded` done/failed/cancelled/command | Idle |
| `turnEnded` replaced, `turnStarted`, 그 밖 | 그대로 |

- `finishes_turn(prev, ev)` = 직전이 Running이고 `turnEnded` done/failed. Esc와 슬래시 명령은 빠져요. → [워커 완료 메모](../features/orchestration/worker-done-notes.md)
- 프로세스 종료는 `TERMINATED`(종료 감시).

### 훅은 신호만(PRD R3-3)

- Claude 설정 파일(`--settings`)에 `PermissionRequest` 옆으로 `Stop`과 `SessionStart`(matcher `*`)를 넣어요. 둘 다 `curl -s --max-time 5 … "http://127.0.0.1:<port>/hook/<우리 세션 id>?t=<토큰>" || exit 0`, timeout 10초예요.
  - `|| exit 0`: Stop 훅이 종료 코드 2를 내면 Claude가 멈추지 않고 계속해요. curl이 실패해도 그러지 않게 막아요.
  - 서버는 곧바로 빈 본문 200으로 답해요. SessionStart의 출력은 Claude 컨텍스트에 들어가기 때문이에요.
  - URL에 원래 세션 id가 있어서 `/clear` 뒤에도 같은 워크트리로 찾아가요.
- `Stop` → `agent.sync()`(`STOP_HOOK` 상수로 끌 수 있어요): 턴 끝이 최대 250ms 대신 바로 보여요. 상태와 `worker_done`은 그래도 트랜스크립트에서만 나와요.
- `SessionStart`이고 `source == "clear"` → `agent.follow(session_id, transcript_path)`(`FOLLOW_SESSIONS`로 끌 수 있어요).
- 훅은 이벤트를 만들지 않아요. 넣지 않은 훅: `StopFailure`(최소 버전 2.0.54보다 새롭고, 모르는 키가 있으면 설정 파일 전체가 거부돼 `PermissionRequest`까지 잃을 수 있어요), `Notification`(`idle_prompt`가 60초 늦고, 권한 요청은 이미 와요). Antigravity는 훅이 없어요.
- 예전보다 정확해진 판정: API 오류 → `turnEnded failed`, 재시도 알림 → `error`, 턴 도중 종료 → `exited { turnId }`.

### 부르는 곳

| 쓰는 곳 | 경로 |
|---|---|
| 채팅 뷰 | `get_transcript` → `runtime::snapshot`, 그다음 `agent://event` |
| `maestro read`, 워커 완료 메모 | `control::last_answer` → `runtime::history(실행 중인 런타임, …)` + `events::last_answer` |
| 카드 요약 | `summarize_agent` → `runtime::history(실행 중인 런타임, …)` + `events::recent_turn_text` |
| 설정 타일의 실행 명령 | `agent::launch_preview` → `pty_agent(id).preview` (실행 불가 에이전트는 실행 파일 이름만) |

모두 `PtyAgent::normalizer()`와 `Tail`을 지나요. 화면용과 다른 변환 경로가 없어요.

## 제약과 알려진 문제

- 준비 판정은 TUI 출력 모양에 기대요. CLI가 화면 구성을 바꾸면 45초 뒤 "시작 시간 초과"가 될 수 있어요.
- 로그인 필요 판정은 화면 글자(`login_hints`)로 해요. 이어 띄운 세션은 입력창이 뜨면 이 판정을 건너뛰어요. Antigravity 문구는 확인 안 됨.
- 트랜스크립트 형식은 각 CLI의 내부 형식이라 버전이 바뀌면 깨질 수 있어요.
- **늦게 온 블록(straggler):** Claude는 한 메시지(`message.id`)를 블록마다 한 줄로 쓰고, 생각 줄과 글 줄이 둘 다 `end_turn`을 달고 오기도 해요(로컬 기록 40개에서 1112번). 닫힌 턴의 메시지에서 온 줄은 그 턴에 붙고 턴을 다시 열거나 두 번 닫지 않아요. 그래서 그런 줄이 **다음 tail 묶음**에 따로 읽히면, 예전처럼 상태를 Idle로 바꾸거나 `worker_done`을 다시 부르지 않아요(예전에는 낙관적 전송 직후 Idle로 돌아갈 수 있었어요). 의도한 유일한 상태 차이예요.
- **대화 따라가기는 꺼져 있어요(`FOLLOW_SESSIONS = false`).** 켜면 바뀌는 것: 채팅·카드 요약·`maestro read`·다음 `--resume`이 새 대화를 봐요. 켜기 전에 할 일:
  - `/clear` 따라가기는 Claude가 `SessionStart`(`source: "clear"`)를 보낸다는 문서에 기대요. 이 PC에서 끝까지 확인하고 사용자 동의를 받아요. 카탈로그의 `/clear` 안내 문구("새 세션이 시작돼 Chat 보기가 이어지지 않아요")도 같이 바꿔요.
  - agy 따라가기는 `last_conversations.json`을 봐요. 사용자가 같은 폴더(특히 마에스트로가 도는 프로젝트 루트)에서 agy를 직접 실행해 만든 대화도 가져오므로 막아야 해요.
  - 따라간 대화는 `prime`으로 읽어서, 알아채기 전(agy는 최대 2초)에 끝난 턴은 워커 완료 메모를 보내지 않아요.
- 승인 카드의 `toolCallId`는 이름·입력으로 찾아요. 이름·입력이 같은 호출이 보일 때까지 100ms씩 3번 다시 봐요. 그래도 없으면 이름만 같은 가장 최근 호출의 **턴만** 쓰고 `toolCallId`는 null이에요. 다른 입력의 같은 도구(병렬 Bash)가 열려 있으면 그 호출일 수 있어서, 화면이 이 id로 도구 카드를 찾을 때 엉뚱한 카드에 붙지 않게 해요.
- `/clear`·agy 따라가기는 meta의 `cliSessionId`를 바꿔요. meta를 통째로 쓰는 곳 중 오래 쥐는 곳(카드 요약: 모델 호출 수 초)은 쓰기 직전에 meta를 다시 읽고 자기 필드만 바꿔요. 그러지 않으면 옛 세션 id가 되살아나 채팅·`maestro read`·다음 `--resume`이 옛 대화를 읽어요.
- `create_worktree`는 `roots`에 등록하지 않아서, 그 경로로 만든 에이전트는 자리 비움 정지 대상에서 빠져요(R3 범위 밖).

## 결정 기록

- **stream-json 방식을 버린 이유:** 처음에는 `claude -p --output-format stream-json`으로 채팅만 만들었어요(커밋 48c3399). 실제 CLI와 슬래시 명령, 권한, 화면이 달라서 지금 구조로 바꿨어요.
- **채팅 데이터를 트랜스크립트에서 읽는 이유:** 화면 출력(ANSI)을 파싱하는 것보다 안정적이에요.
- **파사드를 둔 이유(R3):** `AppState.agents`를 `Arc<dyn Runtime>`으로 바꾸면 main.rs·control.rs의 부르는 곳이 모두 바뀌어요. `Agent`가 런타임을 감싸면 그대로 두고 런타임만 갈아 끼울 수 있어요(R4에서 `RwLock<Arc<dyn Runtime>>`로 채팅↔CLI 전환).
- **훅이 이벤트를 만들지 않는 이유:** 훅은 기록에 남지 않아 `history()`로 다시 만들 수 없어요. 그러면 실시간과 기록의 번호가 어긋나요. 그래서 훅은 "지금 읽어라"는 신호만 줘요.
- **seq를 durable 이벤트에만 주는 이유:** ACP에서는 텍스트 조각의 경계를 `history()`가 다시 만들 수 없어요. 조각(`delta`)은 live-only이고, 완성된 블록(`agentText`)이 같은 `itemId`로 덮어써요.
- **`StopFailure` 훅을 넣지 않은 이유:** 위 "훅은 신호만" 참고.
- **straggler 규칙:** 메시지를 닫은 `message.id`를 기억해 둬요(`closing_msg`). 이 규칙이 없으면 위의 1112건마다 가짜 에이전트 턴이 하나씩 생겨요.
- **동작 동일성 검사:** `events::oracle`에 R3 이전 코드(`to_events`, `state_after`, `recent_turn_text`, `last_answer`, 채팅 fold)를 그대로 두고, 새 경로와 화면 항목·상태 변화·`worker_done` 시점·요약 글·`maestro read` 답을 비교해요(`claude_parity_with_legacy`, `antigravity_parity_with_legacy`, 모든 접두 구간). 여기서 새 fold는 chatFold.ts의 Rust 사본(`oracle::items`)이에요. 화면이 실제로 돌리는 chatFold.ts `fold`는 따로 확인해요: `chat_fold_fixture_is_current`가 두 샘플의 공통 이벤트와 R3 이전 fold의 항목을 `src-tauri/tests/fixtures/chat-parity.json`에 적고(바뀌면 `MAESTRO_WRITE_FIXTURES=1`로 다시 써야 테스트가 통과해요), `npm test`(`scripts/chat-fold-parity.ts`, Node의 타입 제거 실행)가 그 이벤트를 `fold`에 넣어 항목이 같은지 봐요. `useTranscript`와 XTerm.tsx의 epoch·seq 처리는 테스트가 없어요. 실제 기록 비교는 `MAESTRO_PARITY_DIR=… cargo test -- --ignored parity_over_local`(2026-11-10: Claude 156개, agy 125개 파일, 불일치 0).
