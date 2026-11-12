---
title: 채팅 런타임 (ACP, 자동 선택과 채팅↔CLI 전환)
area: agents
status: done
files: [src-tauri/src/acp.rs, src-tauri/src/acp_runtime.rs, src-tauri/src/proc.rs, src-tauri/src/acp_setup.rs, src-tauri/src/agent.rs, src-tauri/src/main.rs, src-tauri/src/catalog.rs, src-tauri/src/cli.rs, src-tauri/src/commands.rs, src-tauri/src/control.rs, src/nodes/AgentNode.tsx, src/nodes/XTerm.tsx, src/App.tsx, src/settingsMigrate.ts, src/nodes/ApprovalCards.tsx, src/settings/AgentsSection.tsx, src/settings/agentState.ts, src/store.ts, src/Workspace.tsx, src/ipc.ts, src/types.ts]
related: [architecture/agent-runtime.md, architecture/runtime-events.md, architecture/acp-runtime-research.md, features/agents/chat-view.md, features/agents/cli-view.md, features/agents/approvals.md, features/agents/cli-install.md, features/ui/settings-dialog.md]
updated: 2026-11-13
---

# 채팅 런타임

## 요약

Claude Code의 채팅 뷰는 터미널(TUI)에 글을 붙여넣는 대신 **ACP 어댑터**(`@agentclientprotocol/claude-agent-acp`)로 돌 수 있어요. 글이 실시간으로 흘러나오고, 턴 끝·취소·권한 질문을 정확히 알아요. 기본은 **자동**이에요: 조건이 갖춰지면 ACP, 아니면 지금처럼 터미널(PTY)로 돌고 이유를 작은 메모로 보여 줘요. 채팅 뷰와 CLI 뷰를 오가면 같은 대화를 이어서 열어요(PRD R4).

## 동작

- **자동이 ACP를 고르는 조건**(Claude Code만): Node.js 22 이상, 설정에서 설치한 어댑터, 실행 파일로 된 claude(2.1.226 이상), 로그인, 200자 이하의 폴더 경로. 하나라도 빠지면 터미널로 돌아요. 이유 예시:
  - "Node.js 22 이상이 없어 터미널 방식으로 실행 중이에요"
  - "폴더 경로가 너무 길어 대화 기록 위치를 찾을 수 없어 터미널 방식으로 실행 중이에요"(Claude가 200자를 넘는 폴더 이름을 해시로 줄여서 대화 기록 위치를 미리 알 수 없어요)
  - "채팅 런타임 어댑터가 설치되지 않았어요 · 설정 → 에이전트에서 설치"
  - "설정에서 채팅을 터미널 방식으로 고정했어요"
- **시작에 실패하면 터미널로 넘어가요**(같은 대화 `--resume`). 어댑터가 안 뜸·로그인 필요는 이번 실행 동안 그 에이전트 전체를 터미널로 묶어요. 어댑터가 10분 안에 두 번 꺼져도 그래요. 설정의 "다시 확인"(에이전트 목록 위의 "↻ 다시 확인"이나 채팅 런타임 줄의 "다시 확인"), 설치, 채팅 연결 변경만 풀어요. 설정 창을 열기만 해서는 풀리지 않고, 채팅 런타임 줄에 막힌 이유와 "다시 확인" 버튼이 보여요.
- **`CLAUDE_CONFIG_DIR`가 있어도 채팅 런타임을 써요.** 대화 기록은 그 폴더의 `projects/`에서 읽어요(비어 있으면 없는 것으로 봐요). 폴더를 신뢰했는지도 그 폴더의 `.claude.json`을 먼저 보고, 없으면 `~/.claude.json`을 봐요.
- **모델은 워크트리 것으로 맞춰요.** 사용자의 `~/.claude/settings.json` `model`이나 `ANTHROPIC_MODEL`이 있어도 채팅은 CLI 보기(`--model`)와 같은 모델로 돌아요. 그 모델을 채팅 런타임에 걸지 못하면 "모델(이름)을 채팅 런타임에 적용하지 못해 터미널 방식으로 실행 중이에요"로 터미널에서 돌아요.
- **설치는 설정에서만 해요.** 몰래 내려받지 않아요. 설정 → 에이전트의 Claude Code 행에 채팅 런타임 줄이 늘 보여요("채팅 런타임 · 설치되지 않았어요 [설치]", 펼치지 않아도 돼요). 행의 상태도 채팅 런타임을 합쳐서, 채팅 연결이 자동인데 런타임이 준비되지 않았으면 "채팅 런타임 필요"(경고색)예요. "관리"를 펼치면 채팅 런타임의 자세한 상태와 npm 출력, 그 아래 "채팅 연결: 자동 | 터미널 경유"가 있어요. 헤더의 "터미널로 실행 중" 칩을 누르면 이 행이 펼쳐진 채로 열려요. 설치는 사용자 Node·npm으로 앱 데이터 폴더에 해요(약 30MB, SDK의 번들 CLI는 빼요). Node가 없으면 버튼 대신 "Node.js 22 이상 필요 · `winget install OpenJS.NodeJS.LTS`"를 보여 줘요.
- **개발용:** 환경 변수 `MAESTRO_ACP_CLAUDE_DIR=<node_modules/@agentclientprotocol/claude-agent-acp가 들어 있는 폴더>`가 있으면 그 설치를 써요(영수증·버전 고정 없음, "개발용 경로"로 표시).
- **채팅↔CLI 전환:** 채팅 뷰 = ACP 세션, CLI 뷰 = 같은 세션 id의 TUI(`claude --resume <id>`). 에이전트가 돌고 있을 때 헤더의 보기 전환 아이콘(터미널 `SquareTerminal` / 채팅 `MessageSquare`)을 누르면 먼저 대화를 옮기고(헤더에 "전환 중…") 끝나면 그 에이전트만 그 보기가 돼요(터미널이 쥐고 있을 때 채팅 아이콘을 눌러도 채팅 런타임으로 옮겨요). 어느 보기를 고르는지는 [채팅 뷰](chat-view.md)의 "보기"에 있어요(설정의 에이전트 보기 하나 + 이 에이전트만 잠시). 에이전트가 꺼져 있으면 보기가 바로 바뀌고, 새 보기가 뜨면서 그 보기의 방식으로 시작해요(시작 오류도 그 보기에 떠요). 한쪽 프로세스를 끝내고 다른 쪽을 띄워서, 한 번에 한쪽만 대화를 써요. 채팅은 다시 읽은 기록으로 새로 그려져서 항목이 전과 같아요.
  - 턴이 도는 중이거나 승인 카드가 떠 있으면 바로 바꾸지 않아요. 헤더에 "턴이 끝나면 전환돼요 · 지금 중단하고 전환"을 보여 주고, 턴이 끝나면 바꿔요. "지금 중단하고 전환"은 턴을 멈춰요(중단 버튼과 같아요). 기다리는 동안 원래 보기를 다시 누르면 전환을 그만둬요.
  - 백그라운드 셸(Bash)이 돌고 있으면 확인 창("백그라운드 셸이 멈춰요", 빨간 [그래도 전환])으로 물어요.
  - 이전 프로세스가 5초 안에 끝나지 않으면 한 번 더 강제로 끝내고 기다려요. 그래도 어댑터나 그 아래 claude가 살아 있으면 바꾸지 않아요. 확인 창("전환하지 못했어요", [새 갈래로 열기])에서 동의하면 대화를 복사해(`session/fork`) 새 갈래로 열어요. 동의하지 않으면 그대로 두고, 다시 전환을 누르면 처음부터 다시 확인해요. 확인 창이 떠 있는 동안 더 새로운 전환이 시작됐으면 동의해도 이 전환은 진행하지 않아요. → [메뉴와 확인 창](../ui/menus-and-dialogs.md)
  - 턴이 끝나기를 기다리는 동안 에이전트가 꺼지면(어댑터가 죽음) 기다림을 그만두고 새 보기의 방식으로 바로 다시 시작해요.
  - **시작 런타임은 앱의 에이전트 보기를 따라요.** 화면 없이 시작하는 에이전트(마에스트로의 위임, `maestro send`, 자리 비움 뒤 재개, 빠른 프롬프트, 카드 요약 등 `ensure`를 거치는 곳)는 설정 → 일반의 에이전트 보기가 채팅이면 자동(ACP가 되면 ACP), 터미널이면 터미널로 떠요. 메인 창이 시작할 때와 값이 바뀔 때 `set_view_default`로 알려 줘요(백엔드 메모리, 기본 채팅). 에이전트마다 마지막 보기를 기억하지 않아요: 한 에이전트를 잠깐 터미널로 본 것이 그 에이전트의 다음 시작을 터미널로 만들지 않아요(예전 meta의 `lastView`는 읽지 않아요).
  - 보기가 화면에 뜨는 것만으로는 대화를 가져가지 않아요. 꺼져 있으면 그 보기의 방식으로 띄우고, 돌고 있으면 그대로 둬요(터미널이 쥐고 있으면 채팅 뷰도 터미널 기록으로 채팅해요). 같은 워크트리를 다른 곳(전체 보기, 팝아웃 창, 멀리서 다시 확대한 캔버스 블록)에서 다른 보기로 열어 둬도 서로의 프로세스를 끄지 않아요.
  - CLI 뷰인데 채팅이 대화를 쥐고 있으면(다른 창이나 탭이 채팅 뷰로 가져갔을 때) 터미널 위에 "지금은 채팅이 대화를 쓰고 있어요 · 터미널로 넘기기" 띠가 떠요. 설정의 에이전트 보기를 터미널로 바꿔 열린 패널이 터미널이 됐을 때도 이 띠예요(설정을 바꾼다고 돌고 있는 대화를 옮기지 않아요). 턴이 끝나기를 기다리는 동안은 "채팅의 턴이 끝나면 터미널로 넘겨요 · 지금 중단하고 전환"이에요. 이 기다림 띠는 백엔드가 정말 기다릴 때만 떠요(터미널이 늦게 뜨는 것만으로는 안 떠요). 기다림이 교체 없이 끝나면(원래 보기를 다시 누름, 시간 초과, 오류) 모든 창에서 내려가요.
- **어떤 방식으로 도는지:** 헤더의 에이전트 이름표에 마우스를 올리면 "채팅: Claude Agent(ACP)"나 터미널로 도는 이유가 보여요. 정말 넘어간 경우(설정에서 고른 것도, CLI 뷰도 아닐 때)는 헤더와 캔버스 아래 입력창 머리에 "터미널로 실행 중" 칩이 붙어요(툴팁 = 이유, 누르면 설정 → 에이전트의 그 행).
- **실시간 글:** 답이 쓰이는 대로 보여요. 완성된 블록이 트랜스크립트에 쓰이면 같은 자리 근처에 완성본이 그려지고 실시간 글은 사라져요.
- **권한 질문**은 지금처럼 승인 카드로 와요. "터미널에서 답하기"는 없어요(예전 이름 "CLI에서 답하기"). 답하지 않은 채 끝나면 거부예요.
- **채팅에서 안 되는 것**(터미널에서 하세요):
  - `!` 셸 명령: 입력창 안내가 "메시지 입력 · Enter 보내기 · / 명령어 · 셸 명령(!)은 터미널에서"로 바뀌고, `!`를 치면 "채팅에서는 셸 명령(!)을 쓸 수 없어요 · 터미널에서 실행"이 떠요. 보내면 "셸 명령(!)은 CLI 보기에서 실행해요" 오류예요(백엔드 오류 글은 이번 정리 범위 밖).
  - 어댑터가 받지 않는 슬래시 명령: `/` 메뉴에 "채팅에서는 안 돼요 · 터미널에서" 표시가 붙고, 보내면 "/이름 명령은 채팅에서 쓸 수 없어요. CLI 보기에서 실행해 주세요." `/model x`는 Maestro가 직접 모델을 바꿔요. 메뉴는 런타임이 바뀔 때와 어댑터가 명령 목록을 보낼 때마다(세션이 열린 뒤에 와요) 다시 받아요.
  - AskUserQuestion(에이전트가 객관식으로 묻는 도구)은 꺼져요.
- **그 밖의 차이:** 첨부는 글 뒤의 `@경로`로 가요(지금과 같아요). Stop을 눌러도 큐에 쌓인 글(워커 메모 포함)은 남아서 취소된 턴 뒤에 가요. 백그라운드 작업이 끝나 Claude Code가 스스로 시작한 턴(보낸 글 없이 도는 턴)도 Stop·자리 비움 정지·"지금 중단하고 전환"으로 멈춰요. 채팅 런타임이 아직 뜨는 중이면 보내는 글은 준비될 때까지 기다렸다가 가요(전환 직후 보낸 글도 새 쪽이 입력을 받을 때 가요). 폴더 신뢰는 Maestro가 따로 물어요. 다만 TUI에서 이미 신뢰한 폴더는 다시 묻지 않아요. Claude Code는 워크트리의 신뢰를 원래 저장소 경로에 적어 두므로(`~/.claude.json`의 `projects`), 워크트리의 `.git` 파일에서 원래 저장소를 찾아 그 경로도 봐요. 채팅에서 신뢰한 뒤 CLI 보기로 가면 TUI가 한 번 더 물을 수 있어요.
- **턴이 실패하면 채팅에 이유가 빨간 줄로 떠요.** 로그인이 필요할 때("로그인이 필요해요. CLI 보기에서 /login 해 주세요"), 어댑터 오류("에이전트 오류: <어댑터가 준 글>"), 큐에 있던 글이 어댑터가 꺼져서 못 간 때("에이전트가 꺼져 전달되지 않았어요")예요. 보낸 글은 말풍선으로 남고 그 아래에 줄이 붙어요. 이 줄은 런타임이 만든 것이라 트랜스크립트에 없어서, 앱이나 에이전트를 다시 시작하면 사라져요.
- `maestro send`/`read`, 워커 완료 메모, 답 요약, 자리 비움 정지, 다시 시작, 모델·권한 모드는 그대로 돼요. 어댑터가 꺼질 때 큐에 남은 워커 메모는 다시 보관했다가 다음 시작 때 보내요. 어댑터가 스스로 죽으면 그 아래 claude와 셸도 같이 끝내요(Windows 잡 오브젝트). 그래서 다음 시작이 같은 대화에 두 번째로 쓰는 일이 없어요. Maestro가 갑자기 꺼져도 어댑터 트리가 남지 않아요.

## 구현

- 결정: `acp_setup::plan_for(def, home, cwd, want)` → `plan(PlanInput)`(순수 함수). 이유는 `FallbackReason`이고 `view()`가 `{ kind, …필드, message }`로 화면에 가요. → [에이전트 실행 방식](../../architecture/agent-runtime.md#acp를-쓸지-정하기acp_setuprs-plan)
- 시작: `Agent::spawn(.., want)` → ACP면 `AcpRuntime::new` + `begin()`(비동기 부팅). 부팅 실패 → 파사드 `fallback` → PTY(`reset(fallback)`).
- 전환: `switch_runtime(projectRoot, worktreePath, view, when)` → `Agent::switch`. 결과 `SwitchResult { status: "done"|"busy"|"stuck"|"superseded", runtime, note, busy: "turn"|"approval"|"shell" }`(기다리는 동안 꺼지면 `Agent::switch`의 `off`를 명령이 받아 새로 띄우고 `done`). 보기가 뜰 때: `start_view(projectRoot, worktreePath, view)`(꺼져 있을 때만 띄움). 갈래: `fork_session(projectRoot, worktreePath, view)`(복사하는 사이 옛 프로세스가 끝나 꺼졌으면 `Agent::fork`의 `off`를 받아 사본 위에 하나만 새로 띄우고 `done`).
- 설치: `acp_install(id)`(진행은 `acp://install`), `acp_install_cancel(id)`. npm의 stdout·stderr는 바이트로 끝까지 읽어 줄마다 `from_utf8_lossy`로 바꿔요(`run_logged`): UTF-8이 아닌 줄(한국어 Windows의 cmd.exe·lifecycle 스크립트가 내는 CP949)에서 읽기를 멈추면 npm이 아무도 안 읽는 파이프에 쓰다 EPIPE로 죽어요. npm이 끝나면 읽기 스레드가 다 읽을 때까지(최대 2초) 남은 줄을 받은 뒤 결과를 정해서, 실패 문구 "npm이 실패했어요: …"의 마지막 줄이 정말 마지막 줄이에요. 방식: `set_chat_runtime(id, "auto"|"pty")`(다음 시작부터). 상태: `cli_status(recheck?)`의 `agents[].acp`, `node`(`recheck: true` = "다시 확인", 막힘도 풀어요).
- 화면에 알리는 이벤트: `agent://runtime { worktreePath, runtime, note }`(시작·폴백·전환), `agent://switch { worktreePath, pending }`(턴 끝을 기다리는 중, `null` = 교체 없이 끝남), `agent://commands { worktreePath }`(어댑터의 명령 목록이 왔어요). `agent_ready`·`list_worktrees`에 `runtime`, `runtimeNote`.
- 실패한 턴: `settle_plan`(응답 오류)과 `exited()`(큐에 남은 프롬프트)가 `close_turn(Failed, None)`을 부르면 `TurnTracker::close_with_notice`가 `turnEnded(failed)` 앞에 `notice(failed, transcript::failure_note(error, message))`(항목 `<turnId>:acp`)를 넣어요. `failure_note`는 `acp` 오류에만 "에이전트 오류: " 앞말을 붙여요. `TurnOutcome.message`는 그대로라 워커 실패 메모는 원래 글을 읽어요.
- 실시간 글: `delta`(항목 `live:<n>:<k>`, `seq: null`). → [공통 이벤트](../../architecture/runtime-events.md#acp-런타임-매핑)
- 환경: `acp_env`가 호스트 세션 변수와 `CLAUDE_CODE_ENTRYPOINT`를 지우고 `CLAUDE_CODE_EXECUTABLE=<사용자 claude>`를 넣어요. `ANTHROPIC_*`는 남겨요.
- 화면(프런트엔드):
  - `AgentPanel`의 `useRuntimeSwitch`(AgentNode.tsx): catalog `runtimes`에 `"acp"`가 있고 에이전트가 돌고 있으면(`Worktree.runtime`이 null 아님) 헤더 보기 전환(아이콘, ⋯ 항목)이 `switch_runtime`을 거쳐요(`ifIdle` → busy(turn·approval)면 `afterTurn`, busy(shell)면 확인 후 `interrupt`, stuck이면 확인 후 `fork_session`). 꺼져 있으면 보기만 바로 바꾸고, 새 보기가 `start_view`로 띄워요. 새 클릭이 이전 것을 이겨요(이전 것은 superseded. 백엔드도 `switching`을 잡은 뒤 `done`/교체를 정해서, 먼저 시작한 전환이나 폴백이 끝난 상태에서 마지막 클릭대로 다시 봐요). 패널이 언마운트되면 진행 중인 전환의 늦은 결과(done·갈래·확인 창)는 버려요(`last`를 올림). `start(view)` = `start_view` + 스토어 반영. 전환이 끝나면 `onView(v)` = 스토어 `setViewOverride(path, v)`(그 에이전트만, 전역과 같으면 예외를 지움).
  - 시작 런타임: `App`의 효과가 `ipc.setViewDefault(agentView)`(시작 + 바뀔 때, 메인 창만. 분리 창에서 바꾼 값은 storage 이벤트로 메인 창에 와요) → `set_view_default` → `AppState.view_default`. `ensure_with`가 `start_want(want, view_default)`(main.rs, 순수: 명시한 `want`가 이기고, 없으면 채팅 = `Auto`, 터미널 = `Cli`)로 정해요. `start_on_view`·`Agent::switch`·`Agent::fork`는 meta에 보기를 쓰지 않아요(`remember_view` 삭제).
  - `useAgentBoot(root, path, enabled, view)`: 채팅 뷰(`ChatBody`)는 `start_view(.., "chat")` 뒤에 `agent_ready`를 불러요. 결과의 `runtime`/`runtimeNote`는 스토어로 가요.
  - `XTerm`의 `start` = `start_view(.., "cli")`, `takeOver` = 헤더 토글과 같은 전환. `pty_snapshot.terminal === false`나 `agent://runtime`의 `"acp"`면 "터미널로 넘기기" 띠, 스토어의 `switchPending === "cli"`면 기다림 띠를 보여요.
  - `Composer`: `agent://commands`가 오면 `/` 목록(`list_slash_commands`)을 다시 받아요.
  - `useChat`: `seq: null`인 `delta`를 리듀서(chatFold.ts `reduceChat`)가 실시간 항목으로 그리고, 항목의 퇴장 델타가 오면 그 항목을 빼요. durable `turnEnded`로는 빼지 않아요(생각 줄의 `end_turn`이 답 글 줄보다 먼저 읽힐 수 있어요). `npm test`가 "실시간 글 + 퇴장 델타 + 완성 블록 = 완성 블록만"과 "답 중간에 온 `turnEnded`는 흐르던 글을 지우지 않음"을 확인해요.
  - 스토어(store.ts): `Worktree.runtime`, `runtimeNote`, `switchPending`, `setRuntime`, `setSwitchPending`. 리스너는 Workspace.tsx `useAgentEvents`(`agent://runtime`, `agent://switch`).
  - 설정: `ChatRuntimeLine`(행의 한 줄)·`ChatRuntimeRow`(관리 안)·`ChatConnectionRow`·`useAcpInstall`(settings/AgentsSection.tsx), 합친 상태 `agentRowState`·`chatRuntimeNeeded`(settings/agentState.ts). → [설정 창](../ui/settings-dialog.md)
- 로그: `MAESTRO_ACP_LOGS=1`이면 어댑터 로그를 `<app data>/acp/logs/<경로 id>/agent.log`에, 무시한 업데이트를 stderr에 남겨요.

## 제약과 알려진 문제

- 같은 워크트리를 두 곳에서 채팅 뷰와 CLI 뷰로 동시에 열면, 먼저 띄운(또는 마지막으로 토글한) 쪽이 대화를 쥐어요. 채팅이 쥐고 있으면 CLI 뷰에 "터미널로 넘기기" 띠가 뜨고, 터미널이 쥐고 있으면 채팅 뷰는 터미널 기록으로 채팅해요(헤더의 채팅 아이콘을 누르면 채팅 런타임으로 옮겨요).
- 메인 창이 `set_view_default`를 부르기 전에 들어온 화면 없는 시작(앱이 막 켜졌을 때의 마에스트로 CLI 호출 등)은 기본값(채팅)으로 시작해요. 에이전트 보기를 터미널로 둔 사람은 그런 시작이 채팅으로 뜰 수 있어요.
- 예전에 터미널로 보던 에이전트(meta `lastView: "cli"`)는 이 버전부터 다음 시작에 에이전트 보기(기본 채팅)로 떠요. 의도한 변화예요.
- 설치 중에 설정 창을 닫았다 열면 진행 줄은 다시 보이지 않아요(설치는 계속돼요). 끝나면 "↻ 다시 확인"으로 상태를 볼 수 있어요.
- 바깥에서 같은 대화를 쓰는 프로세스(사용자가 터미널에서 직접 `claude --resume`)는 알아챌 수 없어요.
- 긴 대화의 `session/load`는 리플레이를 다 받은 뒤에 끝나서 느릴 수 있어요(측정 안 됨). 그러면 `session/resume`으로 바꿀 수 있어요.
- 실시간 글을 지우는 기준이 글 앞부분 일치라, 어댑터가 글을 바꿔 보내면 턴이 끝날 때까지 둘 다 보일 수 있어요.

## 결정 기록

- 2026-11-13 (UI 구조 재구성 단계 12): 백엔드가 에이전트마다 마지막 보기(`lastView`)를 기억하던 것을 끊었어요. 터미널을 잠깐 본 것이 `lastView: "cli"`로 남아 마에스트로의 위임·자리 비움 재개·빠른 프롬프트 같은 화면 없는 시작(`ensure`를 거치는 곳 전부)이 모두 터미널로 떴어요. 이제 시작은 앱이 알려 준 에이전트 보기 하나를 따르고, "이 에이전트만" 예외는 화면(스토어 메모리)에만 있어요. `lastView`를 예외로 쓰고 전역 변경 때 모두 덮어쓰는 안은 에이전트별 저장 상태를 남겨 버렸어요. 필드는 옛 meta를 읽으려고 남겼어요(읽지 않음, 쓰지 않음).
- 2026-11-13 (UI 구조 재구성 단계 9): "채팅 방식: 자동 | 터미널(PTY)"을 "채팅 연결: 자동 | 터미널 경유"로 바꿨어요. 설정 → 일반의 "에이전트 보기: 채팅 | 터미널"과 말이 겹쳐 헷갈렸어요. 채팅 런타임 상태를 에이전트 행에 늘 보이게 하고 행 상태에 합쳤어요(타일에서는 "모델 · 로그인"을 펼쳐야 보였고, 런타임이 없는 Claude도 "사용 중"이었어요).

- **자동 + 이유 표시:** 조건이 안 맞는 PC에서도 지금처럼 동작해야 해요(PTY 폴백). 왜 터미널인지 몰라 헷갈리지 않게 이유를 남겨요.
- **설치를 설정에서만:** 약 30MB를 사용자 모르게 받지 않아요. 개발과 테스트는 `MAESTRO_ACP_CLAUDE_DIR`로 해요.
- **같은 세션 재개 + 단일 쓰기 잠금:** R0 결정이에요. 포크는 잠금을 못 걸었을 때 사용자가 동의해야만 해요. → [ACP 런타임 사전 조사](../../architecture/acp-runtime-research.md)
