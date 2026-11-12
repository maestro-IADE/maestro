---
title: 에이전트 실행 방식 (런타임 인터페이스, PTY 런타임, ACP 런타임)
area: architecture
status: done
files: [src-tauri/src/agent.rs, src-tauri/src/runtime.rs, src-tauri/src/pty.rs, src-tauri/src/acp.rs, src-tauri/src/acp_runtime.rs, src-tauri/src/acp_setup.rs, src-tauri/src/proc.rs, src-tauri/src/control.rs, src-tauri/src/commands.rs, src-tauri/src/transcript.rs, src-tauri/src/stream.rs, src-tauri/src/events.rs, src-tauri/src/claude.rs, src-tauri/src/antigravity.rs, src-tauri/src/approval.rs, src-tauri/src/catalog.rs, src-tauri/src/cli.rs, src-tauri/src/main.rs, src/chatFold.ts, scripts/chat-fold-parity.ts, src/nodes/XTerm.tsx, src/nodes/AgentNode.tsx, src/nodes/WorktreeCard.tsx]
related: [architecture/runtime-events.md, architecture/acp-runtime-research.md, features/agents/chat-runtime.md, architecture/overview.md, architecture/local-server.md, features/agents/chat-view.md, features/agents/cli-view.md, features/agents/agent-adapters.md, features/agents/approvals.md, features/agents/cli-install.md, reference/events.md, reference/storage-layout.md]
updated: 2026-11-13
---

# 에이전트 실행 방식

## 요약

에이전트 하나는 **런타임** 하나가 돌려요(`Runtime` 트레이트, runtime.rs). 런타임은 둘이에요.
- **PTY 런타임**(pty.rs): 실제 CLI(TUI)를 PTY에 띄워요. CLI 뷰는 그 화면을 그대로 보여 주고, 채팅 입력은 PTY에 붙여넣어요.
- **ACP 런타임**(acp_runtime.rs, R4): Claude Code를 ACP 어댑터(`claude-agent-acp`, node)로 돌려요. 채팅 뷰 전용이고 터미널이 없어요.

어느 쪽이든 채팅 뷰는 CLI가 디스크에 쓰는 **트랜스크립트**를 **공통 이벤트**(events.rs)로 바꿔 보여 줘요. 그래서 채팅 항목, `maestro read`, 워커 완료 메모, 카드 요약은 어느 런타임이 턴을 돌렸든 같아요. ACP는 여기에 실시간 글(델타), 턴 끝, 취소, 권한 질문, 권한 모드를 더해요. 나머지 코드(main.rs, control.rs)는 `Agent` 파사드(agent.rs)만 써요. 파사드가 단일 쓰기 잠금 아래에서 런타임을 갈아 끼워요(채팅↔CLI 전환, ACP → PTY 폴백). → [공통 이벤트](runtime-events.md), [채팅 런타임](../features/agents/chat-runtime.md)

## 동작

- 에이전트 블록을 열면 CLI가 먼저 떠요. 채팅 입력은 CLI가 입력을 받을 준비가 된 뒤에야 보여요. 그 전에는 "준비 중…" 로딩이 보여요.
- 처음 여는 폴더면 Claude가 "이 폴더를 신뢰할까요?"를 물어요. 채팅 뷰에 "신뢰하고 시작" 카드가 떠요.
- 시작이 안 되면 이유를 보여 줘요: 실행 파일 없음, 로그인(또는 첫 실행 설정) 필요, 시작하다 종료(마지막 출력 한 줄), 시작 시간 초과. 채팅 뷰에는 "다시 시도"와 "터미널에서 보기" 버튼이 있어요. 카드와 에이전트 바에는 "다시 시도"가 있어요.
- 앱을 다시 켜도 같은 세션으로 이어서 실행돼요.
- 에이전트가 다시 시작되면(종료 뒤, 자리 비움 정지 뒤, 다시 시도) 열려 있는 채팅 뷰는 기록을 새로 받고, CLI 뷰는 화면을 지우고 새 프로세스 출력부터 그려요.
- Claude에서 `/clear`를 하거나 Antigravity가 더 최근 대화로 옮겨도 채팅 뷰는 처음 시작한 대화에 머물러요(R3 전과 같아요). 따라가는 코드는 있지만 `FOLLOW_SESSIONS`로 꺼 두었어요.
- Claude Code는 채팅 런타임이 **자동**이에요. Node.js 22 이상, 설정에서 설치한 어댑터, 실행 파일로 된 claude, 로그인이 모두 갖춰지면 채팅 뷰가 ACP로 돌아요. 하나라도 빠지면 지금처럼 PTY로 돌고, 이유를 작은 메모로 보여 줘요. Antigravity는 PTY 그대로예요.
- 채팅 뷰와 CLI 뷰를 오가면 **같은 대화**를 이어서 열어요(채팅 → CLI는 `claude --resume <id>`, CLI → 채팅은 ACP `session/load`). 한 번에 한쪽 프로세스만 대화를 써요.

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
| `agent.rs` | `AgentMeta`, `Files`, `AgentState`, `emit_state`, 시스템 브리프, `maestro_env`, 프로세스 목록, `summarize`, **`Agent` 파사드**(런타임 교체, `switch`, `fork`, `retire`), `switch_step` |
| `acp.rs` | JSON-RPC 2.0 stdio 클라이언트 `Client`(std 스레드 + mpsc), `classify` |
| `acp_runtime.rs` | `AcpRuntime: Runtime`, 실시간 글 `LiveText`, tail 훅 `AcpHook`, `gate`, `choose`, `Reply`, `settle_plan`, `trusted`, `session_meta`, `fork_session` |
| `acp_setup.rs` | Node·claude 실행 파일·어댑터 찾기, 관리형 설치, `plan`(ACP냐 아니냐), `FallbackReason`, 설정(`runtimes.json`), 캐시(`probe`, `health`), `acp_env` |
| `proc.rs` | `kill_tree`(두 런타임이 같이 써요), `contain`·`Tree`(어댑터 트리를 통째로: Windows 잡 오브젝트, Unix 프로세스 그룹) |

`antigravity::ID`로 나누는 곳은 `runtime::pty_agent()` 하나뿐이에요(그 밖에는 antigravity.rs와 카탈로그 테스트). 승인 훅 연결은 카탈로그 `features.approval_hook`을 보고 `ClaudeAgent::launch`가 해요.

### 트레이트

- `Runtime`(Send + Sync, 스레드 기반): `kind`, `ready() -> Result<Readiness, LaunchFailure>`, `accept_trust`, `prompt(&Prompt) -> Result<Sent, String>`, `cancel`, `kill`, `shutdown`(기본 = `kill`), `alive`, `process_alive`(기본 = `alive`. 런타임 상태와 상관없이 프로세스가 아직 있는지, 전환의 단일 쓰기 확인용), `settled`(기본 true), `commands`(기본 None), `quiet_for`, `pid`, `terminal() -> Option<&dyn Terminal>`, `sync`, `locate_tool(name, input, tool_call_id) -> ToolMatch { turn_id, tool_call_id, exact }`, `follow(conversation, transcript)`, `history() -> Option<(epoch, events)>`.
  - `Prompt { text, files, kept }`: `kept`는 이 글이 실어 나르는 워커 메모(워커, 메모)예요. 큐에 넣고 보내기 전에 꺼진 런타임이 다시 보관해요.
  - `Sent::Turn`이면 파사드가 상태를 Running으로 바꾸고, `Sent::Handled`(ACP의 `/model`)면 그대로 둬요.
  - `history()`는 자기 기록을 가진 런타임(ACP)만 `Some`이고, 그 기록이 번호를 매긴 epoch를 같이 줘요. 스트림의 지금 epoch와 다른 기록(재시작 중 아직 등록돼 있는 이전 실행의 것)은 쓰지 않아요.
  - 그 기록은 런타임이 따로 쌓지 않고 `Emitter::with_record`로 만든 Emitter가 쌓아요. seq를 매기는 같은 Stream 잠금 안에서 내보낸 durable 이벤트만 붙여서, 기록 길이와 live seq·epoch가 어긋날 수 없어요. ACP의 기록은 epoch가 시작될 때 트랜스크립트 전체로 채워요(`Emitter::reset_seeded`). 그 뒤 tail이 읽은 것과 런타임이 만든 이벤트(아래 "턴 마무리")가 붙어요.
- `Terminal`: `write`, `resize`, `snapshot() -> PtySnapshot { dataB64, seq, epoch, terminal }`. `terminal: false`는 ACP 런타임이 대화를 쥐고 있다는 뜻이에요(CLI 뷰가 "터미널로 넘기기" 띠를 보여요). 실행 중인 에이전트가 없으면 `true`예요.
- `PtyAgent`(CLI마다 다른 것): `id`, `launch(ctx, meta, files, exe) -> Launch { argv, transcript, resumed }`, `conversation_id`, `transcript`, `current_conversation`(agy만), `normalizer`, `preview`, `summarize`.
  - `pty_agent(id)`: `antigravity` → `AGY`, 나머지 → `CLAUDE`(예전 else 분기와 같아요).

### `Agent` 파사드(agent.rs)

main.rs와 control.rs가 부르는 API: `spawn`, `run`(설정의 설치 터미널), `wait_ready() -> Result<bool, LaunchFailure>`, `accept_trust`, `send`, `send_note`, `interrupt`, `write`, `resize`, `snapshot`, `quiet_for`, `runs_shell`, `kill`, `retire`, `alive`, `state`, `notes`, `deliver_kept_notes`, `sync`, `follow`, `locate_tool`, `shared_runtime`, `commands`, `owner`, `switch`, `fork`, `switching_now`. 예전 `runtime() -> &dyn Runtime`은 없어졌어요(교체될 수 있어서 `shared_runtime()`으로 `Arc`를 받아요).

**필드:** `runtime: RwLock<Arc<dyn Runtime>>`(모든 메서드가 읽기 잠금으로 복제해 써요), `gen`(세대), `exited`(마지막으로 끝난 런타임의 세대), `switching`(단일 쓰기 잠금), `owner { kind, note }`, `switch_seq`, `retired`, 다시 시작할 때 쓰는 `{ home, files }`.

**세대(`gen`):** 런타임을 교체하거나 일부러 멈출 때마다 1씩 올려요. 런타임의 종료 콜백은 자기가 시작된 세대가 아직 지금 세대일 때만 `TERMINATED`를 내요. 그래서 전환·폴백으로 죽인 프로세스가 에이전트를 "꺼짐"으로 만들지 않아요. 콜백은 세대가 맞든 아니든 먼저 `exited`에 자기 세대를 적어요. 전환이 세대를 올린 뒤 교체 없이 되돌릴 때(`restore_gen`) `exited`가 되돌린 세대면 그동안 가려진 종료를 지금 `TERMINATED`로 내요. 되돌린 뒤의 종료는 콜백이 스스로 세요.

**`Agent::spawn(app, home, path, files, meta, want)` 순서**
1. `catalog::require` → `launchable()` 확인.
2. `acp_setup::plan_for(def, home, path, want)`: `Ok(plan)`이면 ACP, `Err(이유)`면 PTY와 그 이유(`owner.note`). → 아래 "ACP를 쓸지 정하기"
3. `switching`을 쥐고 `start(target, Restart)` → 교체 → ACP면 `begin()`(부팅 스레드). ACP 부팅은 기다리지 않아서 `ensure()`는 바로 돌아와요. 채팅 뷰는 `agent_ready`가 기다리는 동안 로딩을 보여요.
4. `agent://state`로 `IDLE`, `agent://runtime { worktreePath, runtime, note }`를 보내요.

**`start(Pty, 이유)`**(예전 spawn 본문 그대로):
1. `cli::command`(실행 파일 없으면 여기서 실패) → `pty_agent(adapter).launch(..)`: 세션 id(없으면 만들어 meta에 저장), 브리프 파일, Claude면 `--settings` 훅 파일과 승인 등록, argv.
   - Claude: 새 세션이면 `--session-id <uuid>`, 트랜스크립트가 있으면 `--resume <uuid>`.
   - Antigravity: `--conversation <id>`. → [에이전트 종류](../features/agents/agent-adapters.md)
2. `Emitter::new`(sink = `agent://event`, observer = 상태 점·워커 완료) → `emitter.reset(이유)`: epoch를 올리고 `reset` 이벤트를 보내요.
3. `LiveTail::start`: 트랜스크립트를 지금 끝까지 읽어 개수만 세고(`prime`) 내보내지 않아요.
4. `PtyRuntime::start(.., Some(live), login_hints, resumed, on_exit)`: `on_exit` = 세대를 본 `emit_state(Terminated)`.

**`start(Acp(plan), 이유)`:** `Emitter::with_record`(같은 sink·observer) → `AcpRuntime::new(ctx, emitter, 폴백, on_exit)`. ctx에는 `acp_env`, 브리프, 승인 질문 함수(`Approvals::request`)가 들어가요.

**observer**(모든 durable 이벤트, 보내기 전에): `status_after(ev)`가 상태를 주면 `finishes_turn(이전 상태, ev)`를 먼저 보고 `emit_state`, 끝난 턴이면 그 `turnEnded`의 `turn_id`와 결과를 넘겨 `control::worker_done`을 별도 스레드로 띄워요. 예전 tail 루프(상태 먼저, 그다음 이벤트)와 순서가 같아요.

**시작은 워크트리마다 하나씩:** `ensure()`와 `create_worktree`는 워크트리별 잠금(`AppState::start_lock`)을 쥐고, 살아 있는 에이전트를 잠금 안에서 다시 본 뒤 `Agent::spawn` → `agents.insert`까지 해요. 채팅 뷰(`agent_ready`), CLI 뷰(`ensure_agent`), `send_message`, `maestro send`가 동시에 불러도 하나만 떠요. 잠금이 없으면 둘 다 떠서 프로세스 하나가 남고, 두 번째 `Emitter::new`·`reset`이 epoch를 가져가 등록된 쪽의 이벤트·상태·워커 완료가 모두 버려질 수 있어요. 다른 워크트리는 따로 시작해요.

**`wait_ready`**: 지금 런타임의 `ready()`. 오류가 났는데 그 사이 폴백·전환이 런타임을 바꾸는 중이면(둘 다 `switching`을 쥔 채 옛 런타임을 먼저 멈춰요) `switching`이 풀릴 때까지 기다렸다가, 바뀌었으면 새 런타임을 다시 기다려요. 그래서 ACP→PTY 폴백 중에 `agent_ready`·워커 메모 전달이 옛 런타임의 "에이전트가 꺼졌어요"를 받지 않아요. `switching`이나 `sending`을 쥔 채로는 부르지 않아요(전환이 `switching` → `sending` 순서로 잡아요).

**`Agent::send` / `send_note`**: 꺼져 있으면 `"에이전트가 꺼져 있어요"` → 지금 런타임이 입력을 받을 때까지 기다림(`wait_ready`와 같음, 신뢰 확인 중이면 `"폴더 신뢰 확인이 먼저 필요해요"`, 시작 실패면 그 이유) → `sending` 잠금 → 기다린 런타임이 아직 지금 런타임인지 확인(아니면 처음부터 다시) → `runtime.prompt` → `Sent::Turn`이면 `emit_state(Running)`. 부르는 쪽이 먼저 `wait_ready`를 했어도, 그 뒤 `sending`을 얻기 전에 전환이 아직 뜨는 중인 새 런타임을 넣을 수 있어서예요(뜨지 않은 TUI에 붙여넣은 글과 그 워커 메모는 사라져요). `send_note`는 `control::pass_notes`가 메모 항목을 같이 넘길 때 써요.

**`interrupt`**: `runtime.cancel()`. PTY에서만 "트랜스크립트에 아무것도 안 생긴 프롬프트면 1.8초 뒤 Idle" 보정을 해요. ACP는 필요 없어요: 프롬프트의 턴은 응답이 끝내고, Claude Code가 스스로 시작한 턴은 트랜스크립트가 끝내요.

### 채팅↔CLI 전환(`Agent::switch(view, when)`)

`view`: `Chat`(ACP를 쓸 수 있으면 ACP) / `Cli`(PTY). `when`: `IfIdle`(바쁘면 바로 busy), `AfterTurn`(턴이 끝날 때까지 기다림, 30분), `Interrupt`(한 번 중단하고 15초까지 기다림).

1. `my = switch_seq += 1`. 목표를 정해요. Cli → PTY. Chat → `plan(Auto)`가 Ok면 ACP, 아니면 PTY(이유와 함께).
2. 보기를 meta에 적지 않아요. 다음 `ensure()`는 앱의 에이전트 보기(`set_view_default` → `AppState.view_default`, main.rs `start_want`)를 따라요. 예전에는 `meta.lastView`를 써서 터미널을 잠깐 본 것이 다음 시작을 터미널로 만들었어요(UI 구조 재구성 단계 12에서 끊음).
3. 지금 런타임 종류와 목표가 같으면 끝(`done`). 다만 이 답도 `switching`을 잡고 정해요: 주인은 `switching` 아래에서만 바뀌어요(교체, 폴백). 그래서 먼저 시작한 전환이나 폴백이 아직 돌고 있으면 끝날 때까지 기다린 뒤 다시 봐요. 그 사이 더 새 전환이 왔으면 `superseded`, 주인이 목표와 달라졌으면 4부터 다시 해요(예: Chat 클릭 A가 PTY→ACP 교체 중에 CLI 클릭 B가 오면, B는 "이미 PTY"로 바로 `done`하지 않고 A가 ACP를 넣은 뒤 다시 PTY로 바꿔요). 채팅이 PTY로 정해졌으면 아무것도 죽이지 않고 이유만 바꿔요(채팅은 PTY 트랜스크립트로도 돼요).
4. 조용할 때까지 기다려요. 기다리는 동안 에이전트가 꺼지면(턴 중에 어댑터가 죽거나 터미널이 끝남) **`off`**로 돌아가요. 그러면 `switch_runtime`이 그 보기의 런타임으로 새로 띄운 뒤(`start_on_view`) `done`을 돌려줘요(화면은 `off`를 보지 않아요). 예전에는 `TERMINATED`가 되돌아오지 않아 30분 동안 기다렸어요. 조용함 = 상태 Idle + `runtime.settled()` + 이 워크트리에 열린 승인 카드 없음. 조용하지만 1초(`SWITCH_SILENCE`) 안에 출력이 있었으면(`quiet_for`: PTY는 TUI가 마지막으로 그린 뒤, ACP는 마지막 stdio 뒤) `when`과 상관없이 200ms 뒤 다시 봐요(`wait_limit`까지, 넘으면 busy(turn)). TUI에 바로 친 Enter나 막 시작한 턴은 상태 점이 Running이 되기 전(tail은 250ms마다 읽어요)에도 TUI 다시 그리기나 어댑터 메시지로 보여서, 전환이 그 턴을 죽이지 않아요. 처음 기다릴 때 `agent://switch { worktreePath, pending: "chat"|"cli" }`를 한 번 보내요. 더 새 전환 요청이 오면 `superseded`. 기다림을 알린 전환은 어떻게 끝나든(교체 없이 끝난 superseded·busy·stuck·오류 포함) 돌아갈 때 `agent://switch { pending: null }`을 보내요(`Announced`의 drop). 그래서 다른 창의 "턴이 끝나면 전환돼요"도 내려가요.
5. `switching` → `sending` 순서로 잠그고 **곧바로 `gen += 1`**. 잠금을 기다리는 사이 더 새 전환이 왔으면 `restore_gen` 후 `superseded`(아무것도 바꾸지 않아요. 새 전환이 이 상태에서 정해요). 여기부터 옛 런타임의 종료는 전환이 읽어요: 아래 확인(느린 `processes()`, 다른 창의 TUI에 친 `/exit`, 어댑터 충돌) 사이에 끝난 종료가 콜백으로 `TERMINATED`를 내면, 새 런타임이 들어가도 에이전트가 꺼짐으로 남고 다음 마운트가 같은 세션에 두 번째 에이전트를 띄워요. 이미 꺼져 있으면 세대를 되돌리고 4로(거기서 `off`). 다시 조용하고 1초 동안 출력이 없었는지 봐요(그 사이 들어온 프롬프트). 아니면 `restore_gen` 후 잠금을 풀고 4로.
6. `processes()`로 셸이 돌고 있으면 `Interrupt`가 아닌 한 `restore_gen` 후 `busy("shell")`. 폐기(`retired`)됐어도 `restore_gen` 후 오류. `processes()`(Windows PowerShell `Get-CimInstance`, 0.5~3초)가 도는 동안에도 턴이 시작될 수 있어서(`sending`은 Maestro가 보내는 프롬프트만 막아요. TUI에 직접 친 입력, 백그라운드 작업·서브에이전트가 끝나 스스로 시작한 턴은 못 막아요) `shutdown` 바로 앞에서 5의 확인(조용함 + 1초 무출력 + 주인 그대로)을 한 번 더 해요. 아니면 `restore_gen` 후 4로.
7. `old.shutdown()`(PTY: kill, ACP: `session/close` 5초 → stdin 닫기 → 0.5초 → 트리 kill).
8. 5초 안에 옛 프로세스가 끝났는지 확인해요(`confirm_dead`): `old.process_alive()`가 false(런타임 상태가 아니라 프로세스예요. ACP의 `alive()`는 `shutdown`이 세션을 닫자마자 false라 node가 아직 살아 있어도 지나가요), 그다음 트리 전체: 잡이 있으면(ACP의 `proc::Tree`) 잡의 `ActiveProcesses`가 0(`Runtime::tree_gone`, 정확해요). 잡이 없으면(PTY) 그 pid가 프로세스 목록에 없고, 그 pid를 부모로 둔 프로세스도 없음(`tree_listed`: node가 죽고 남은 claude.exe 고아가 대화를 계속 써요). 목록에서는 옛 런타임이 시작된 뒤(`Runtime::started`, spawn 직전의 `now_ms`) 생긴 프로세스만 세요. Windows는 `ParentProcessId`를 고치지 않고 pid를 금방 재사용해서, 예전에 같은 pid였던 다른 부모의 자식(먼저 생긴 무관한 프로세스)이 영원히 걸려 `stuck`이 되풀이됐어요. 아니면 `old.kill()`로 한 번 더 죽이고 다시 5초 확인해요. 그래도 남아 있으면 `gen`을 되돌리고(가려진 종료를 세지 않아요: 루트가 죽었어도 고아가 남았어요) **`stuck`**이에요. 옛 런타임이 계속 주인이고, 사용자가 동의하면 `fork`로 사본을 열어요. 다시 전환을 누르면 처음부터 다시 확인해요. 에이전트를 `TERMINATED`로 두지 않아요: 그러면 다음 `ensure()`가 남은 프로세스와 같은 세션에 두 번째 쓰기 주체를 띄워요.
9. `start(target, Switch)` → 교체 → ACP면 `begin()`. `agent://runtime`을 보내요.

**순서 보장:** 옛 프로세스가 죽은 뒤에 새 프로세스가 떠요. 새 Emitter의 `reset`은 그 뒤예요. 옛 tail이 늦게 한 번 더 돌아도 옛 epoch에만 쓰여 버려져요. 전환 전후의 기록은 같은 트랜스크립트라 채팅 항목이 같아요.

**판정표(`switch_step`, 순수 함수, 테스트 `the_switch_waits_for_quiet_and_asks_before_killing_a_shell`):**

| 지금 = 목표 | 조용함 | 셸 | when | 결과 |
|---|---|---|---|---|
| 예 | - | - | - | Done |
| 아니요 | 아니요 | - | IfIdle | Busy(turn 또는 approval) |
| 아니요 | 아니요 | - | AfterTurn | Wait |
| 아니요 | 아니요 | - | Interrupt | 처음 Interrupt, 그다음 Wait |
| 아니요 | 예 | 예 | Interrupt 아님 | Busy(shell) |
| 아니요 | 예 | - | - | Proceed |

**`fork(view)`**(사용자가 `stuck` 뒤 "새 갈래로 열기"에 동의했을 때만): 새 어댑터로 `initialize` → `session/fork { sessionId, cwd, mcpServers: [] }` → 새 id → `session/close` → 종료. 복사는 몇 초 걸려요(node, initialize, 사본 쓰기). 그다음 `switching`을 잡고 세대를 올린 뒤 에이전트가 아직 살아 있는지 봐요. 그사이 stuck이던 프로세스가 끝나 꺼졌으면(`TERMINATED`는 되돌아오지 않아요) 세대를 되돌리고 `meta.cliSessionId = 새 id`만 쓴 뒤 아무것도 띄우지 않고 **`off`**를 돌려줘요. `fork_session` 명령이 그걸 받아 `switch_runtime`처럼 `start_on_view`로 사본 위에 에이전트를 하나만 띄워요(여기서 띄우면 꺼진 에이전트 안의 런타임이라 다음 마운트가 두 번째를 띄우고 첫째는 죽지 않았어요). 살아 있으면 옛 런타임을 kill, `meta.cliSessionId = 새 id`, 목표 런타임을 새 id로 시작해요. 시작이 실패하면 `TERMINATED`(다음 마운트가 사본으로 띄워요). 폐기됐으면 오류. ACP를 쓸 수 없으면 "갈래를 만들 수 없어요".

**화면 쪽(AgentNode.tsx `useRuntimeSwitch`, XTerm.tsx):** 채팅 런타임이 있는 에이전트(카탈로그 `runtimes`에 `acp`)만 이 경로를 써요.

| 누가 | 부르는 것 | 결과 처리 |
|---|---|---|
| 헤더 보기 아이콘과 ⋯ "터미널로 보기 / 채팅으로 돌아가기", 채팅 뷰의 "터미널에서 보기", CLI 뷰 띠의 "터미널로 넘기기" | 에이전트가 꺼져 있으면(`Worktree.runtime` null) 부르지 않고 보기만 바로 바꿔요. 새로 뜬 보기가 아래 두 줄처럼 띄우고, 시작 오류도 그 자리에서 보여요. 돌고 있으면 ACP든 PTY든 `switch_runtime(v, "ifIdle")` | `done` → 그 에이전트만 보기를 바꿈(스토어 `setViewOverride`). busy(turn·approval) → "턴이 끝나면 전환돼요" + `afterTurn`으로 다시. busy(shell) → 확인 후 `interrupt`. `stuck` → 확인 후 `fork_session`. `superseded`·늦은 결과 → 무시(새 클릭이 이겨요) |
| 채팅 뷰가 뜰 때(`useAgentBoot`의 `view`) | `start_view("chat")` 뒤 `agent_ready` | 결과·오류 무시. 꺼져 있으면 채팅의 런타임으로 띄우고, 돌고 있으면 그대로 둬요(터미널이 쥐고 있으면 터미널로 채팅) |
| CLI 뷰가 뜰 때(XTerm `start`) | `start_view("cli")` 뒤 `pty_snapshot` | 꺼져 있으면 터미널로 띄워요. 채팅 런타임이 쥐고 있으면(`terminal: false`) "터미널로 넘기기" 띠. 헤더 토글이 기다리는 중이면(`switchPending === "cli"`, `agent://switch`) "채팅의 턴이 끝나면 터미널로 넘겨요" 띠 |

**뜨는 것만으로는 대화를 가져가지 않아요(`start_view`):** 같은 워크트리를 한쪽은 채팅, 다른 쪽은 CLI로 볼 수 있어요(캔버스 블록과 전체 보기, 팝아웃 창). LOD로 본문이 내려갔다 다시 뜨거나 다른 창이 마운트될 때마다 대화를 빼앗으면, 반대편 프로세스가 매번 죽고 `--resume`/`session/load`로 다시 떠서 기록을 다시 받고 TUI에 치던 입력도 사라져요. 그래서 마운트는 꺼진 에이전트를 띄우기만 하고, 옮기는 건 사용자가 누른 토글과 "터미널로 넘기기"뿐이에요. 설정의 에이전트 보기를 바꿔 열린 패널의 보기가 바뀌어도, 사이드바 메뉴·"터미널에서 답하기"로 그 에이전트의 보기를 바꿔도 마찬가지예요(보기만 바뀌고 옮기기는 띠에서).

`done`의 `runtime`/`note`와 `agent://runtime`은 스토어(`setRuntime`)로, `agent://switch`는 `setSwitchPending`으로 가요. 채팅 목록은 새 epoch(`reset switch`)를 보고 다시 받아요. "지금 중단하고 전환"은 `stop_agent`예요(기다리던 `afterTurn`이 이어서 전환해요).

### 다시 시작(`Agent::restart`, 2026-11-13)

에이전트 설정을 바꾼 뒤(`update_agent`) 권한·모델을 적용하려고 같은 대화로 다시 띄워요. `restart_agent` 명령이 먼저 순수 판정 `can_restart(alive, state, waiting, switching)`을 보고(꺼짐 → "꺼져 있어요. 다음 시작부터 적용돼요", 승인 카드나 터미널 질문 대기 → 거부, 전환·폴백 중 → 거부, Idle이 아님 → "작업이 끝나면 다시 시작할 수 있어요") 통과하면 이 메서드를 불러요.

1. meta를 읽고 목표를 정해요(`restart_want(owner)`, 순수): 지금 주인이 PTY이고 이유가 없거나 `cliView`면 CLI 보기라 PTY 그대로, 아니면 채팅이라 `plan_for(Auto)`를 다시 봐요(바뀐 권한·모델 때문에 났던 폴백 `modeRejected`·`modelRejected`가 이번에는 안 날 수 있어요). 잠그기 전에 해요(프로브가 몇 초 걸려요).
2. 방금 끝난 턴이 아직 그리는 중이면 3초까지 `still()`을 기다려요. `processes()`로 백그라운드 셸을 봐요.
3. `switching` → `sending`을 잡고 다시 봐요: 폐기·꺼짐, 승인·터미널 질문, `still()` 아님, 셸, 그사이 전환으로 주인 종류가 바뀜이면 이유와 함께 거부(`RESTART_*` 상수).
4. `gen += 1` → `old.shutdown()` → `confirm_dead`(안 되면 `kill` 후 한 번 더). 끝나지 않으면 세대를 되돌리고 오류(두 번째 쓰기 주체를 띄우지 않아요).
5. `start(target, Restart)` → 교체 → ACP면 `begin()`. `start`가 meta를 새로 읽어서 바뀐 권한·모델·브리프(마에스트로의 기준 브랜치)가 들어가요. 실패하면 `TERMINATED`(다음 `ensure()`가 새로 띄워요). 성공하면 보관해 둔 워커 메모를 보내요.

`kill()` 뒤 `ensure()`로 새 `Agent`를 만들지 않고 전환처럼 같은 `Agent` 안에서 바꿔요. `kill`은 비동기라 `ensure`가 아직 살아 있는 옛 에이전트를 돌려받을 수 있고, 그 사이 화면이 "꺼짐"을 보거나 마운트가 두 번째 에이전트를 띄울 수 있어서예요. 같은 보기를 지키는 것도 이 방식이 쉬워요. 다시 시작하는 동안 채팅은 새 epoch(`reset restart`)를 받아 잠깐 부팅 화면으로 바뀌었다가 기록을 다시 받아요. 정상이에요.

**`retire()`**: `switching`을 쥐고 `retired = true`, kill. `discard_worktree`가 `kill` 대신 불러요. 그 뒤로는 전환·폴백이 아무것도 띄우지 않아요.

**자리 비움 정지:** `switching_now()`(전환·폴백 중)인 에이전트는 건너뛰어요.

### ACP를 쓸지 정하기(acp_setup.rs `plan`)

`plan(PlanInput { acp, want, pref, probe, cwd, health })`는 순수 함수예요. 먼저 걸리는 이유가 이겨요.

| 순서 | 이유(`FallbackReason`) | 조건 |
|---|---|---|
| 1 | `forced` | 설정에서 "터미널(PTY)"로 고정 |
| 2 | `notSupported` | 카탈로그 `runtimes`에 `acp`가 없음(Antigravity, Codex) |
| 3 | `cliView` | CLI 뷰가 요청(메모 없음) |
| 4 | `cliNotExecutable` | claude를 찾지 못했거나, `.exe`도 npm 패키지의 `cli.js`도 아님 |
| 5 | `cliTooOld` | claude가 카탈로그 `acp.min_version`(2.1.226)보다 낮음. 버전을 모르면 막지 않아요 |
| 6 | `transcriptUnreachable` | 폴더 경로가 200자를 넘음(Claude가 폴더 이름을 해시로 줄여 트랜스크립트 위치를 미리 알 수 없음, `claude::slug_hashed`) |
| 7 | `nodeMissing` | `node.exe` 없음(`.cmd` 심은 안 봐요) |
| 8 | `nodeTooOld` | Node가 `acp.node_min`(22.0.0)보다 낮음 |
| 9 | `adapterMissing` / `adapterBroken` | 어댑터가 없거나 설치가 깨짐 |
| 10 | `loggedOut` | 파일로 본 로그인 상태가 확실히 아님(`cli::logged_in`) |
| 11 | 이번 실행에서 막힌 이유 | `startFailed`, `loginRequired`, `crashed`(`health`) |

시작한 뒤에 나오는 이유: `startFailed`, `loginRequired`, `loadFailed`, `modeRejected`, `modelRejected`. 앞의 둘과 `crashed`(준비된 어댑터가 10분 안에 두 번 꺼짐)는 에이전트 전체를 막아요(`blocks_agent`). 사용자가 직접 한 것만 풀어요: "다시 확인"(`cli_status { recheck: true }`), 설치, 설정 변경이 `invalidate()`(probe 캐시 + 막힘)를 불러요. 설정 창이 열리거나 앱이 시작할 때의 `cli_status`(recheck 없음)는 probe 캐시만 지워서(`invalidate_probe`) 막힘이 남고, 설정의 채팅 런타임 줄이 `blocked`와 "다시 확인" 버튼을 보여 줘요. 예전에는 설정을 열기만 해도 막힘이 풀려 크래시 반복을 막지 못했어요. `loadFailed`·`modeRejected`·`modelRejected`는 그 워크트리만의 일이라 막지 않아요.

**probe 캐시:** Node 경로·버전, claude 실행 파일, claude 버전(어댑터가 있을 때만 `--version`을 돌려요), 로그인 상태, 어댑터 위치를 60초 기억해요. 어댑터가 없는 보통 사용자는 프로세스를 하나도 띄우지 않아요.

**어댑터 위치(`locate_adapter`):**
- `MAESTRO_ACP_CLAUDE_DIR`(개발용): `<폴더>/node_modules/@agentclientprotocol/claude-agent-acp`. 영수증이 필요 없고 다른 버전도 돼요(`dev: true`).
- 관리형: `<app_local_data_dir>/acp/claude-agent-acp@0.102.0/`. `.install-complete.json` 영수증, package.json 버전 = 고정 버전, `dist/index.js`가 모두 있어야 설치된 걸로 쳐요. 아니면 `Broken(이유)`.

**관리형 설치(`install`, 설정에서만):** Node ≥22 확인 → `<node 폴더>/node_modules/npm/bin/npm-cli.js`를 node로(없으면 `cmd.exe /c call npm.cmd`) → staging(`<app data>/acp/.staging-<pid>-<nanos>`)에 `npm install --prefix <staging> --omit=optional --no-audit --no-fund --save-exact @agentclientprotocol/claude-agent-acp@0.102.0`(창 없음, 10분 제한, 줄마다 진행 이벤트) → 버전 확인 → `node <entry> --version` → 진짜 `initialize` 핸드셰이크 → 영수증 → rename. 이미 맞게 설치돼 있으면 staging을 지우고 "이미 설치돼 있어요". 깨진 설치는 지우고 바꿔요(지우지 못하면 "실행 중인 채팅 에이전트를 끄고 다시 시도해 주세요"). 실패하면 staging을 지워요. 진행: `acp://install { id, kind: "line"|"done"|"failed", text }`. 사용자의 전역 npm·Node 설정은 건드리지 않아요.

**설정 파일:** `~/.maestro/runtimes.json` = `{"chat":{"claude-code":"auto"|"pty"}}`. 없으면 auto예요. UI 없이 시작하는 경로(`maestro send`, 자리 비움 재개, 워커 메모)도 같은 값을 봐야 해서 백엔드에 둬요.

### ACP 런타임(acp_runtime.rs)

**JSON-RPC 클라이언트(acp.rs):** 한 줄 = JSON 객체 하나. `method`+`id` = 에이전트의 요청, `method`만 = 알림, `id`만 = 응답(숫자 id로 기다리던 요청을 찾고, 모르는 id는 버려요). 알림과 요청은 읽기 스레드에서 순서대로 `on_incoming`을 불러요(처리기는 다른 응답을 기다리면 안 돼요). 그래서 `session/load`의 리플레이가 응답보다 먼저 처리돼요. 읽기 EOF나 자식 종료(1초 여유)가 종료예요: 기다리던 요청은 모두 `Exited` + stderr 마지막 줄로 실패하고, 트리의 나머지를 죽인 뒤(`kill_rest`) `on_exit`이 한 번 불려요. `close(grace)` = 트리 kill → stdin 닫기 → grace 동안 종료를 기다림.
- **트리(`proc::Tree`):** `spawn`이 node를 띄우자마자 Windows 잡 오브젝트(`KILL_ON_JOB_CLOSE`만, 이탈 허용 없음)에 넣어요(Unix는 `contain`의 프로세스 그룹). node가 claude를 띄우는 데 걸리는 시간보다 훨씬 빨라서 claude와 그 아래가 모두 잡 안에서 태어나요. 이탈(`BREAKAWAY_OK`)을 허용하지 않는 이유: libuv(node, Bun으로 만든 claude)는 자식마다 `SILENT_BREAKAWAY_OK` 잡을 따로 씌우는데, 조용한 이탈은 이탈을 허용하는 부모 잡을 모두 타고 올라가요. 허용하면 claude가 띄운 셸·데몬이 우리 잡 밖에서 태어나 살아남아요. libuv는 `CREATE_BREAKAWAY_FROM_JOB`을 쓰지 않아서 막아도 node·claude 실행에는 문제가 없어요. `kill()`은 루트가 살아 있으면 먼저 `kill_tree`(부모 pid로 아래를 찾아 내려가요. node·claude가 먼저 죽으면 고아를 찾을 수 없어요), 그다음 `TerminateJobObject`라 루트가 이미 죽었어도 고아까지 끝내요. 어댑터가 스스로 죽으면 `finish`가 잡을 끝내서, 그 claude가 같은 `<id>.jsonl`에 계속 쓰는 동안 다음 `ensure()`의 `session/load`·`claude --resume`이 두 번째 쓰기 주체로 뜨는 일이 없어요. 핸들이 닫히면(Maestro가 죽어도) 남은 프로세스도 끝나요. 잡을 못 만들었으면 죽은 루트를 부모로 둔 프로세스 중 루트가 시작된 뒤 생긴 것만 `processes()`로 찾아 `kill_tree`해요(먼저 생긴 무관한 프로세스 트리를 죽이지 않게). `Tree::empty`는 잡의 `JobObjectBasicAccountingInformation.ActiveProcesses == 0`(Unix: 그룹에 `kill -0`)이에요. `processes()`는 `Proc { pid, ppid, name, started }`를 돌려줘요(`started`: Windows `CreationDate`, Unix `ps`의 `etime`으로 계산, ms).
  - 테스트: `the_layout_matches_win32`, `the_tree_takes_orphans_along`(ignored, 실제 프로세스, 잡의 개수도 확인), `an_orphan_keeps_the_old_tree_listed`(먼저 생긴 프로세스는 안 셈), `finds_shells_below_the_cli_only`, `reads_ps_elapsed_times`, `an_exit_counts_only_in_its_own_generation`(가려진 종료는 되돌릴 때 셈).

| 상수 | 값 | 대상 |
|---|---|---|
| `INIT_LIMIT` | 30초 | initialize |
| `SESSION_LIMIT` | 120초 | session/new, session/load, session/fork |
| `CALL_LIMIT` | 30초 | set_mode, set_config_option |
| prompt | 없음 | 응답이나 종료로 끝나요 |

**부팅(`begin` → 별도 스레드):**
1. `node <entry>`를 워크트리 cwd, 아래 환경으로 띄워요. 실패 → `startFailed`.
2. `initialize { protocolVersion: 1, clientCapabilities: { fs: {readTextFile: false, writeTextFile: false}, terminal: false }, clientInfo }`. v1이 아니거나 `loadSession`이 없으면 `startFailed`. `authenticate`는 부르지 않아요.
3. 신뢰: meta `acpTrusted`이거나 `~/.claude.json`의 `projects[<폴더나 그 위>].hasTrustDialogAccepted`면 통과(읽기만 해요). 아니면 `NeedsTrust`로 기다려요. 채팅 뷰의 "신뢰하고 시작" → `accept_trust`(기다리지 않고 바로 돌아와요. 단계는 여기서 같은 잠금 안에 `NeedsTrust` → `Booting`으로 바꿔요: 바로 뒤의 `ready()`(`deliver_kept_notes`, 채팅의 `agent_ready`)가 신뢰 질문을 다시 읽지 않고 세션을 기다려요. 부트 스레드가 깨어나 바꾸면 그 사이에 보관한 워커 메모가 안 붙고 신뢰 카드가 한 번 더 떴어요. 부트 스레드는 `trust_ok`가 아직 없을 때만 같은 잠금 안에서 `NeedsTrust`로 가요) → meta `acpTrusted = true`. `~/.claude.json`은 쓰지 않아서 나중에 CLI 뷰의 TUI가 한 번 더 물을 수 있어요.
4. 세션. 모든 요청에 `_meta = { systemPrompt: { append: <브리프> }, claudeCode: { options: { allowedTools: ["Bash(maestro:*)"], model: <meta.model, 없으면 뺌>, settings: <AcpCtx.settings, 없으면 뺌> } } }`. 문자열 systemPrompt는 Claude Code의 시스템 프롬프트를 통째로 바꿔서 쓰지 않아요. `settings`는 명령 가드(`Approvals::acp_settings`: `PreToolUse` 훅 `/pretool/acp-<path_id>`와 `permissions.deny`)로, 어댑터가 CLI의 `--settings` 층으로 넘겨 모든 권한 모드에서 훅이 돌아요(카탈로그 `approval_hook`이 켜진 에이전트만, `session/fork`에는 안 넘김). → [병합 요청](../features/orchestration/merge-request.md)
   - meta에 세션 id가 있고 트랜스크립트 파일이 있으면 `session/load`. 그동안 오는 `session/update`는 버려요(트랜스크립트가 이미 기록이에요). 실패하면 `loadFailed` → PTY가 같은 파일을 `--resume`해요. **`session/new`로 바꾸지 않아요**(대화가 몰래 갈라지지 않게).
   - 아니면 `session/new` → `sessionId`를 meta `cliSessionId`에 읽고-고쳐-써요.
   - 오류 -32000이나 응답 전 `_auth/status_update { authStatus: { kind: "none" } }` → `loginRequired`(PTY TUI가 로그인 화면을 보여 줘요).
5. 권한 모드: 응답의 `modes.currentModeId`가 meta `permissionMode`와 다르면 `session/set_mode`. 그 모드가 `availableModes`에 없거나 실패하면 `session/close` 뒤 `modeRejected`. 어댑터 기본값으로 돌리지 않아요.
   - 모델: meta `model`이 있고 응답의 `models.currentModelId`(없으면 `configOptions`의 id `model`의 `currentValue`, `current_model`)와 다르면 `session/set_config_option { configId: "model", value }`. 어댑터 0.102.0은 `ANTHROPIC_MODEL`, 그다음 `~/.claude/settings.json`의 `model`을 `options.model`보다 앞에 두고, 이어 붙인 세션(`session/load`)에는 그 모델을 `setModel`로 다시 걸어요. 그대로 두면 CLI 보기(`--model`)와 채팅이 같은 대화에서 다른 모델을 써요. `set_config_option`은 `opus`·`haiku` 같은 별칭도 받아요. 실패하면 `session/close` 뒤 `modelRejected`(터미널로, `--model`이 맞는 모델을 써요). `options.model`도 그대로 넣어서 CLI가 처음부터 그 모델로 떠요.
6. tail: `LiveTail::start_seeded(.., Switch 또는 Restart)` → 파일 전체가 새 epoch의 기록(`reset_seeded`, `reset` 한 번만 나가요) → `AcpHook` 연결 → tick 스레드(250ms 또는 깨움 신호마다 `tick`, 클라이언트가 죽으면 끝).
7. `Ready` → 보내기 스레드.

부팅 중 실패는 파사드의 폴백(`fallback`)을 불러요. 폴백은 `switching`을 쥐고, 이 ACP 런타임이 아직 지금 런타임이면 세대를 올리고 kill → `start(Pty, Fallback)` → 교체(`owner.note = 이유`) → `agent://runtime`. PTY도 못 뜨면 `TERMINATED`이고 기다리던 `ready()`가 그 오류를 받아요. `kill`/`shutdown`으로 멈춘 런타임은 폴백하지 않아요. 그 대신 그 멈춤이 에이전트의 종료예요: 부팅이 `Err(None)`으로 끝나면 `fail`이 `on_exit`을 한 번 불러요(`exit_done`). 예를 들어 신뢰 질문(NeedsTrust)에서 기다리던 에이전트를 자리 비움 정리가 kill하면 `TERMINATED`가 되어 다음 `ensure()`가 새로 띄워요(예전에는 `alive()`인 채 남아 모든 보내기가 실패했어요). 전환·폴백이 멈춘 것은 세대가 이미 올라가 있어서 파사드가 무시해요.

**트레이트 구현:**

| 메서드 | ACP |
|---|---|
| `ready` | 부팅 중이면 기다림. Ready → Ready, NeedsTrust → NeedsTrust, Failed → 그 오류 |
| `prompt` | Ready일 때만(부팅·신뢰 중이면 "아직 준비 중이에요": 부팅 중 큐에 든 프롬프트는 폴백과 함께 사라지니까요. 파사드가 먼저 기다려요) `gate` → 큐에 넣고 바로 `Sent::Turn` |
| `cancel` | 열린 턴이 있으면 `session/cancel`: 응답을 기다리는 프롬프트의 턴이거나, 트랜스크립트에 열린 턴(백그라운드 작업이 끝나 Claude Code가 스스로 시작한 턴, 어댑터의 autonomous cycle — 기다리는 프롬프트가 없어요). 보냈을 때만 열린 권한 질문을 `cancelled`로 답해요. 열린 턴이 없으면 아무것도 보내지 않아요(끝난 턴의 빈 "중단됨"이 생기지 않게). 큐는 남겨요(워커 메모가 사라지지 않게) |
| `shutdown` | `session/close`(5초) → `close(500ms)` |
| `kill` | 트리 kill, Closed |
| `alive` | 클라이언트가 살아 있고 Failed·Closed가 아님 |
| `process_alive` | 클라이언트가 살아 있음(node의 종료를 아직 못 봄). `shutdown`·`kill`이 Closed로 바꿔도 프로세스가 끝날 때까지 true |
| `settled` | 큐가 비고 보내는 중인 것이 없음 |
| `commands` | 마지막 `available_commands_update`의 (이름, 설명). 올 때마다 `agent://commands { worktreePath }`를 보내서 입력창이 `/` 목록을 다시 받아요(목록은 세션 응답 뒤에 와요) |
| `pid` | node. claude.exe와 그 셸은 그 아래라 `runs_shell`이 그대로 돼요 |
| `terminal` | 없음 |
| `sync` | tail 깨우기 |
| `locate_tool` | `tick` 뒤 `LiveTail::locate` |
| `history` | `emitter.history()` |

**보내기(`gate` → 큐 → 보내기 스레드):**

| 입력 | 결과 |
|---|---|
| `!…` | 거부: "셸 명령(!)은 CLI 보기에서 실행해요" |
| `/이름 …`, 명령 목록이 아직 없음 | 2초 기다린 뒤 판단, 그래도 없으면 통과 |
| `/이름 …`, 어댑터 목록에 있음 | 통과(CLI가 실행하고 트랜스크립트에 남아요) |
| `/model x`, 목록에 `model` 없음 | `session/set_config_option { configId: "model", value: x }` → `Sent::Handled` |
| 그 밖의 `/이름 …` | 거부: "/이름 명령은 채팅에서 쓸 수 없어요. CLI 보기에서 실행해 주세요." |
| 명령 이름 모양이 아닌 `/…`(경로 등) | 통과 |

첨부는 PTY처럼 글 뒤에 ` @경로`로 붙여 텍스트 블록 하나로 보내요(트랜스크립트의 말풍선이 같아요). `resource_link`는 쓰지 않아요. 큐는 클라이언트 쪽에 있고(Paradev와 같아요), 어댑터의 promptQueueing은 쓰지 않아요. 보내기 스레드: Ready이고 보내는 중인 것이 없으면 꺼내서 `session/prompt { sessionId, prompt: [{type: "text", text}] }` → `LiveText::begin_prompt` → tail 깨우기 → 응답 대기 → 턴 마무리.

**업데이트 처리(읽기 스레드):** 우리 sessionId가 아닌 것과 load 리플레이는 버려요.

| `sessionUpdate` | 처리 |
|---|---|
| `agent_message_chunk`(text) | `LiveText::chunk(Text, messageId, text)` |
| `agent_thought_chunk` | `LiveText::chunk(Thinking, …)` |
| `tool_call` | `tools[toolCallId] = (_meta.claudeCode.toolName ‖ title, rawInput)`, 블록 경계, tail 깨우기 |
| `tool_call_update` | 이름·입력 갱신, completed/failed면 tail 깨우기. `toolUpdated`는 보내지 않아요(기록 = 트랜스크립트) |
| `available_commands_update` | 목록 저장(세션 id를 알기 전에 와도 받아요) → `agent://commands` |
| 그 밖(`current_mode_update`, `plan`, `usage_update`, …) | 무시(`MAESTRO_ACP_LOGS=1`이면 stderr에 남겨요) |

그 밖의 알림: `_auth/status_update` → 로그아웃 표시, `$/cancel_request { requestId }` → 그 권한 질문을 철회. 요청: `session/request_permission` → 아래, 나머지는 -32601.

**실시간 글(`LiveText`)과 tail 훅(`AcpHook`):** → [공통 이벤트](runtime-events.md#acp-런타임-매핑)
- 조각은 `live:<n>:<k>` 항목의 `delta`로 나가요(UTF-16 offset). 같은 채널·messageId 동안 한 항목이고, `tool_call`, 채널 바뀜, messageId 바뀜에서 끊어요.
- 이 프롬프트의 `userMessage`가 트랜스크립트에서 나오기 전에는 쌓아만 둬요(말풍선 위에 글이 그려지지 않게). `userMessage`가 지나간 직후 한꺼번에 보내요.
- 트랜스크립트의 `agentText`/`thinking`이 나가기 **직전**, 같은 채널에서 아직 살아 있는 가장 오래된 항목 중 한쪽 글(앞뒤 공백 제외)이 다른 쪽의 앞부분인 것을 `delta { offset: 0, text: "" }`로 지워요. 채팅은 빈 글을 그리지 않아요. 지운 항목에 늦게 온 조각은 버려요.
- 프롬프트의 턴이 끝나면(응답 뒤 트랜스크립트를 다시 읽고 나서, `finish_prompt`) 남은 항목을 모두 지워요. **트랜스크립트의 `turnEnded`에서 지우지 않는 이유:** Claude는 멈춤 이유(`end_turn`)를 생각 줄에 먼저 달고 글 줄을 그 뒤에 써요(실측). 이 프롬프트에 트랜스크립트 글이 하나도 없었는데 남은 글이 있으면(어댑터만 보여 준 출력) 지우면서 `notice`(localCommand)로 남겨요.
- 잠금 순서: tick 스레드는 tail → live → stream, 읽기 스레드는 live → stream이에요.

**턴 마무리(`settle`, 응답이 온 뒤):** tail을 바로, +300ms, +1s, +2s에 다시 읽어요. 트랜스크립트가 이 프롬프트를 받았고 턴을 닫았으면 끝. 아니면 `settle_plan`대로 LiveTail에서 이벤트를 만들어요(`TurnTracker`와 기록이 맞게).

| 응답 | 트랜스크립트에 프롬프트 있음 | 만드는 것 |
|---|---|---|
| `stopReason: cancelled` | 예 | `notice { interrupted, "중단됨" }` + `turnEnded cancelled`(Esc와 같아요) |
| 그 밖의 stopReason | 예 | `turnEnded done { stopReason }` |
| 오류 -32000 | 예 | `turnEnded failed { error: "auth", message: "로그인이 필요해요. CLI 보기에서 /login 해 주세요" }` |
| 오류 -32603, 종료 | 예 | `turnEnded failed { error: "acp", message: data.message ‖ message ‖ stderr 마지막 줄 }` |
| 무엇이든 | 아니요 | `userMessage`(`p:<n>:u`) + 위의 끝(보낸 글이 화면에서 사라지지 않게) |

**권한 질문(fail-closed):** `session/request_permission { sessionId, toolCall, options }`가 오면 철회 표시를 등록하고 별도 스레드에서:
1. `Reply` 가드를 만들어요. 답을 보내지 않고 사라지면(패닉 포함) `reject_once`(없으면 `reject_always`, 그것도 없으면 `cancelled`)로 답해요.
2. `ApprovalReq { toolName: tools[id] ‖ _meta.claudeCode.toolName ‖ title, toolInput: rawInput ‖ tools[id], canAlways, turnId: locate, toolCallId: Some(id), cliAnswer: false, waitMax: None }` → `Approvals::request(…, still_waiting = 클라이언트 살아 있음 && 철회 안 됨 && 취소 중 아님)`.
3. `choose(답, 취소됨, options)`로 답해요.

| 답 | 응답 |
|---|---|
| 허용 | 첫 `allow_once`. 없으면 거부(묻은 것보다 더 주지 않아요) |
| 항상 허용 | `allow-with-updates`, `allow-skill-exact`, `exit-plan-accept-edits` 순, 그다음 `exit-plan-`이 아닌 첫 `allow_always`, 그다음 `allow_once`. 컨텍스트 지우기나 auto/bypass로 올리는 선택지는 고르지 않아요 |
| 거부 / CLI | `reject_once`, 없으면 `reject_always` |
| 답 없음, 취소나 `$/cancel_request` 뒤 | `{ outcome: { outcome: "cancelled" } }` |
| 답 없음, 그 밖(끝남, 죽음, 앱 종료) | `reject_once`. `cancelled`는 쓰지 않아요: v1 포그라운드 턴에서 어댑터가 도구를 중단으로 받아요(acp-agent.js 8280-8290) |

내놓은 optionId만 돌려줘요. `canAlways`는 "항상 허용"이 "허용"과 다른 선택지를 고를 때만 true예요. 카드는 지금처럼 observer의 `settle`(트랜스크립트의 `toolFinished`·`turnEnded`)로 치워요. 카드의 도구 id는 `tool_use.id`와 같아요.

**종료:** 준비된 런타임의 어댑터가 꺼지면 `exited { turnId }`, 큐에 남은 채팅 글은 `userMessage` + `turnEnded failed { error: "exited", message: "에이전트가 꺼져 전달되지 않았어요" }`로 보여 주고, 워커 메모는 `control::keep_notes`로 다시 보관해요(다음 시작 때 `deliver_kept_notes`가 보내요). 보내는 중이던 프롬프트가 트랜스크립트에 닿기 전이었으면 그 메모도 다시 보관해요. `prompt()`는 큐에 넣기 직전 `send` 잠금 아래에서 단계(Ready)를 다시 봐요(`exited`는 Closed로 바꾼 뒤 같은 잠금 아래에서 큐를 비워요). 그래서 앞의 확인(슬래시 명령 목록을 2초까지 기다림) 뒤 어댑터가 죽어도, 글은 비우기 전에 들어가 "전달되지 않았어요"로 보이고 메모가 남거나, 오류("에이전트가 꺼져 있어요")로 돌아가 `pass_notes`가 메모 파일을 지우지 않아요. 죽은 런타임 큐에 들어가 메모가 사라지는 일은 없어요. 일부러 멈춘 게 아니면 `health`에 꺼짐을 기록하고, 세대를 본 `on_exit`이 `TERMINATED`를 내요.

### ACP 어댑터의 환경 변수(`acp_env`)

- 지워요: `CLAUDECODE`, `CLAUDE_PID`, `CLAUDE_CODE_ENTRYPOINT`(SDK는 비어 있을 때만 채워요. 남아 있으면 호스트 세션의 것이 찍혀요), `CLAUDE_CODE_EXECUTABLE`, `CLAUDE_CODE_SSE_PORT`, `CLAUDE_AGENT_ACP_PROTOCOL`, `CLAUDE_AGENT_LOGS`, `NODE_OPTIONS`, `ELECTRON_RUN_AS_NODE`, 그리고 `CLAUDECODE`가 있을 때 `host_session_env`가 지우는 것.
- 남겨요: `ANTHROPIC_*`, `CLAUDE_CODE_USE_BEDROCK`/`_VERTEX`, `CLAUDE_CODE_GIT_BASH_PATH`, 호스트 세션이 없을 때 사용자가 둔 `CLAUDE_CODE_*`.
- 지운 뒤 넣어요: `CLAUDE_CODE_EXECUTABLE=<사용자 claude>`, `CLAUDE_AGENT_ACP_PROTOCOL=v1`, `maestro_env`(MAESTRO_URL·TOKEN·WORKTREE, PATH 앞에 `~/.maestro/bin`. 어댑터가 `process.env`를 CLI에 넘겨서 Bash에서 `maestro`가 돼요), `MAESTRO_ACP_LOGS=1`일 때만 `CLAUDE_AGENT_LOGS=<app data>/acp/logs/<경로 id>`.
- PTY 환경은 그대로예요.

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
- **kill:** Windows에서 `taskkill /T /F`(`proc::kill_tree`)로 프로세스 트리 전체를 끝내요. 앱이 끝나면(`RunEvent::Exit`) 모든 에이전트를 병렬로 끝내요(`thread::scope`).

### 트랜스크립트 → 공통 이벤트

- 위치:
  - Claude: `CLAUDE_CONFIG_DIR`(없으면 `~/.claude`)`/projects/<경로 슬러그>/<세션>.jsonl`. 훅이 알려 준 위치(meta `cliTranscript`)가 먼저예요(`runtime::transcript_path`).
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
- **seq:** durable 이벤트가 지금 소스의 `history()`에서 몇 번째인지(1부터). 소스는 런타임마다 달라요: PTY는 CLI 트랜스크립트를 처음부터 읽은 것(`Runtime::history()`가 `None`이라 `transcript_history()`), ACP는 epoch 시작 때 트랜스크립트로 채운 기록에 그 뒤 이벤트를 붙인 것(`Runtime::history()`가 `Some`). `get_transcript`, `maestro read`, 카드 요약이 모두 `runtime::history(live, …)`로 같은 소스를 읽어요. 시작할 때 `prime(n)`으로 이미 있던 개수 뒤부터 이어요. live-only 이벤트는 `seq: null`이고 번호를 쓰지 않아요.
- **epoch:** 워크트리마다 1에서 시작해 늘기만 해요. 스트림을 처음부터 다시 보낼 때 올려요. 올릴 때는 쌓인 델타를 버리고 `seq = 0`으로 하고 `reset { reason }`을 `seq: null`, 새 epoch로 보내요.
  - `restart`: 런타임이 (다시) 시작할 때마다(spawn, 자리 비움 뒤 재개, 다시 시도). PRD의 "재시작"이에요.
  - `source`: 다른 대화를 읽게 됐을 때(agy 대화 채택, `/clear`, 더 최근 agy 대화).
  - `truncated`: 파일이 짧아졌을 때.
  - `switch`(채팅↔CLI 전환), `fallback`(ACP → PTY). ACP의 load 리플레이는 버리므로 따로 이유가 없어요(`replay`는 없어졌어요).
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
- 모든 신호 훅 뒤 `agent.note_transcript(session_id, transcript_path)`: meta의 세션 id와 같으면 위치를 `cliTranscript`에 남기고(바뀔 때만), 실행 중 tail을 그 파일로 옮겨요(`Runtime::retarget`, PTY만).
- 훅은 이벤트를 만들지 않아요. 넣지 않은 훅: `StopFailure`(최소 버전 2.0.54보다 새롭고, 모르는 키가 있으면 설정 파일 전체가 거부돼 `PermissionRequest`까지 잃을 수 있어요), `Notification`(`idle_prompt`가 60초 늦고, 권한 요청은 이미 와요). Antigravity는 훅이 없어요.
- 예전보다 정확해진 판정: API 오류 → `turnEnded failed`, 재시도 알림 → `error`, 턴 도중 종료 → `exited { turnId }`.

### 부르는 곳

| 쓰는 곳 | 경로 |
|---|---|
| 채팅 뷰 | `get_transcript` → `runtime::snapshot`, 그다음 `agent://event`(ACP의 실시간 `delta` 포함) |
| `maestro read` | `control::last_answer` → `control::history_of`(`runtime::history(실행 중인 런타임, …)`) + `events::last_answer` |
| 워커 완료/실패 메모 | `control::worker_done` → `control::history_of` + `events::turn_answer`(끝난 턴의 글만) |
| 카드 요약 | `summarize_agent` → `runtime::history(실행 중인 런타임, …)` + `events::recent_turn_text` |
| 설정 에이전트 행의 실행 명령 | `agent::launch_preview` → `pty_agent(id).preview` (실행 불가 에이전트는 실행 파일 이름만) |

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
- **ACP 채팅의 차이:** `!` 셸 명령과 어댑터 목록에 없는 슬래시 명령은 CLI 보기에서 해요. AskUserQuestion은 쓸 수 없어요(elicitation을 광고하지 않아 어댑터가 그 도구를 꺼요). Stop을 눌러도 큐에 쌓인 글은 남아요(취소된 턴 뒤에 보내요). 폴더 신뢰는 Maestro가 따로 물어서, CLI 보기로 넘어가면 TUI가 한 번 더 물을 수 있어요.
- **ACP 턴 끝이 트랜스크립트보다 빠를 때:** 응답 뒤 2초 동안 파일에 턴 끝이 없으면 런타임이 만들어요. 그 뒤에 파일에 마지막 줄이 쓰이면 닫힌 턴의 줄로 붙어요(straggler 규칙). 취소한 턴은 "[Request interrupted" 줄이 2초 뒤에 오면 "중단됨"이 두 번 보일 수 있어요(라이브 테스트에서는 안 봤어요).
- **바깥에서 같은 대화를 쓰는 프로세스**(사용자가 터미널에서 직접 `claude --resume`)는 알아챌 수 없어요.
- **ACP 실시간 글을 지우는 기준은 글 앞부분 일치예요.** 어댑터가 글을 바꿔 보내면 턴이 끝날 때까지 실시간 글과 완성된 블록이 같이 보일 수 있어요.

## 결정 기록

- 2026-11-13 (UI 구조 재구성 단계 12): 백엔드가 보기를 에이전트마다 기억하지 않아요(`remember_view` 삭제, `lastView`는 옛 meta를 읽으려고 필드만 남김). 화면 없는 시작(`ensure`)은 앱이 알려 준 에이전트 보기를 따라요(`set_view_default`, `start_want`). 터미널을 잠깐 본 것이 위임·재개 같은 다음 시작을 모두 터미널로 만들었어요.
- **stream-json 방식을 버린 이유:** 처음에는 `claude -p --output-format stream-json`으로 채팅만 만들었어요(커밋 48c3399). 실제 CLI와 슬래시 명령, 권한, 화면이 달라서 지금 구조로 바꿨어요.
- **채팅 데이터를 트랜스크립트에서 읽는 이유:** 화면 출력(ANSI)을 파싱하는 것보다 안정적이에요.
- **파사드를 둔 이유(R3):** `AppState.agents`를 `Arc<dyn Runtime>`으로 바꾸면 main.rs·control.rs의 부르는 곳이 모두 바뀌어요. `Agent`가 런타임을 감싸면 그대로 두고 런타임만 갈아 끼울 수 있어요(R4: `RwLock<Arc<dyn Runtime>>`).
- **ACP에서도 트랜스크립트가 기록인 이유(R4):** 어댑터가 띄운 claude가 같은 jsonl을 써요(R0 실험 1). 같은 tail·normalizer를 쓰면 id, 말풍선, 도구 출력, 로컬 명령 알림, `last_answer`, 요약 키가 PTY와 바이트 단위로 같아요. ACP 업데이트로 기록을 따로 만들면(AcpNormalizer) 실시간과 다시 읽은 기록의 항목이 달라져요. AcpNormalizer는 트랜스크립트가 없는 런타임(agy acp, Codex) 때 만들어요. 폴더 이름이 해시로 줄어 트랜스크립트 위치를 모르면 ACP를 쓰지 않아요(`transcriptUnreachable`). `CLAUDE_CONFIG_DIR`는 그 폴더에서 읽어서 막지 않아요.
- **CLI → 채팅에 `session/resume`이 아니라 `session/load`를 쓰는 이유:** R0 결정이에요. 리플레이는 읽기 스레드에서 바로 버려서 비용이 작아요. 긴 대화에서 load가 느리면 `session/resume`으로 한 줄만 바꾸면 돼요.
- **큐를 클라이언트에 둔 이유:** 한 번에 하나만 보내면 각 프롬프트의 응답이 그 턴의 끝이에요. 실시간 글을 프롬프트마다 나누고, 꺼질 때 보내지 못한 워커 메모를 정확히 다시 보관할 수 있어요.
- **권한 모드를 늘 직접 정하는 이유:** 어댑터의 기본 모드가 사용자가 고른 것보다 많이 허용할 수 있어요. 정하지 못하면 ACP를 쓰지 않아요(`modeRejected`).
- **실시간 글을 따로 두고 지우는 이유:** 델타는 기록에 없어서(live-only) 트랜스크립트의 블록과 id를 맞출 수 없어요. 따로 그리고, 완성된 블록이 오면 빈 델타로 지워요. chatFold.ts는 바꾸지 않아요.
- **훅이 이벤트를 만들지 않는 이유:** 훅은 기록에 남지 않아 `history()`로 다시 만들 수 없어요. 그러면 실시간과 기록의 번호가 어긋나요. 그래서 훅은 "지금 읽어라"는 신호만 줘요.
- **seq를 durable 이벤트에만 주는 이유:** ACP에서는 텍스트 조각의 경계를 `history()`가 다시 만들 수 없어요. 조각(`delta`)은 live-only이고, 완성된 블록(`agentText`)이 같은 `itemId`로 덮어써요.
- **`StopFailure` 훅을 넣지 않은 이유:** 위 "훅은 신호만" 참고.
- **straggler 규칙:** 메시지를 닫은 `message.id`를 기억해 둬요(`closing_msg`). 이 규칙이 없으면 위의 1112건마다 가짜 에이전트 턴이 하나씩 생겨요.
- **ACP 동일성 검사(R4):** 단위 테스트는 가짜 에이전트(`acp::tests::pair`)로 클라이언트를, `fold`(델타 포함 chatFold 사본)로 "실시간 글 + 완성된 블록 = 완성된 블록만"을 확인해요(`durable_blocks_retire_their_live_text`, `the_hook_swaps_live_for_durable_in_one_order`). 라이브 테스트(`acp_live_*`, `#[ignore]`)는 실제 어댑터로 채팅 항목 = 트랜스크립트 다시 읽기 항목을 확인해요. → [R0 조사의 R4 확인](acp-runtime-research.md#r4에서-확인한-것)
- **동작 동일성 검사:** `events::oracle`에 R3 이전 코드(`to_events`, `state_after`, `recent_turn_text`, `last_answer`, 채팅 fold)를 그대로 두고, 새 경로와 화면 항목·상태 변화·`worker_done` 시점·요약 글·`maestro read` 답을 비교해요(`claude_parity_with_legacy`, `antigravity_parity_with_legacy`, 모든 접두 구간). 여기서 새 fold는 chatFold.ts의 Rust 사본(`oracle::items`)이에요. 화면이 실제로 돌리는 chatFold.ts `fold`는 따로 확인해요: `chat_fold_fixture_is_current`가 두 샘플의 공통 이벤트와 R3 이전 fold의 항목을 `src-tauri/tests/fixtures/chat-parity.json`에 적고(바뀌면 `MAESTRO_WRITE_FIXTURES=1`로 다시 써야 테스트가 통과해요), `npm test`(`scripts/chat-fold-parity.ts`, Node의 타입 제거 실행)가 그 이벤트를 `fold`에 넣어 항목이 같은지 봐요. `useTranscript`와 XTerm.tsx의 epoch·seq 처리는 테스트가 없어요. 실제 기록 비교는 `MAESTRO_PARITY_DIR=… cargo test -- --ignored parity_over_local`(2026-11-10: Claude 156개, agy 125개 파일, 불일치 0).
- **다시 시작은 같은 `Agent` 안에서(2026-11-13, UI 구조 재구성 단계 7):** PRD는 "kill 후 `ensure`"를 적었지만 `kill`이 비동기라 `ensure`가 아직 살아 있는 옛 에이전트를 돌려받거나, 그 사이 마운트가 두 번째 에이전트를 띄울 수 있었어요. 전환과 같은 단일 쓰기 잠금 안에서 옛 프로세스가 끝난 것을 확인하고 새 런타임을 넣어요. 판정(`can_restart`, `restart_want`)만 순수 함수로 테스트하고, 프로세스 교체는 전환과 같은 코드(`confirm_dead`, `start`, `swap`)를 써요.
