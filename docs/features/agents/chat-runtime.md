---
title: 채팅 런타임 (ACP, 자동 선택과 채팅↔CLI 전환)
area: agents
status: done
files: [src-tauri/src/acp.rs, src-tauri/src/acp_runtime.rs, src-tauri/src/proc.rs, src-tauri/src/acp_setup.rs, src-tauri/src/agent.rs, src-tauri/src/main.rs, src-tauri/src/catalog.rs, src-tauri/src/cli.rs, src-tauri/src/commands.rs, src-tauri/src/control.rs, src/nodes/AgentNode.tsx, src/nodes/XTerm.tsx, src/nodes/ApprovalCards.tsx, src/SettingsDialog.tsx, src/store.ts, src/Workspace.tsx, src/ipc.ts, src/types.ts]
related: [architecture/agent-runtime.md, architecture/runtime-events.md, architecture/acp-runtime-research.md, features/agents/chat-view.md, features/agents/cli-view.md, features/agents/approvals.md, features/agents/cli-install.md, features/ui/settings-dialog.md]
updated: 2026-11-12
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
- **시작에 실패하면 터미널로 넘어가요**(같은 대화 `--resume`). 어댑터가 안 뜸·로그인 필요는 이번 실행 동안 그 에이전트 전체를 터미널로 묶어요. 어댑터가 10분 안에 두 번 꺼져도 그래요. 설정의 "다시 확인"(에이전트 목록 위의 "↻ 다시 확인"이나 채팅 런타임 줄의 "다시 확인"), 설치, 채팅 방식 변경만 풀어요. 설정 창을 열기만 해서는 풀리지 않고, 채팅 런타임 줄에 막힌 이유와 "다시 확인" 버튼이 보여요.
- **`CLAUDE_CONFIG_DIR`가 있어도 채팅 런타임을 써요.** 대화 기록은 그 폴더의 `projects/`에서 읽어요(비어 있으면 없는 것으로 봐요). 폴더를 신뢰했는지도 그 폴더의 `.claude.json`을 먼저 보고, 없으면 `~/.claude.json`을 봐요.
- **모델은 워크트리 것으로 맞춰요.** 사용자의 `~/.claude/settings.json` `model`이나 `ANTHROPIC_MODEL`이 있어도 채팅은 CLI 보기(`--model`)와 같은 모델로 돌아요. 그 모델을 채팅 런타임에 걸지 못하면 "모델(이름)을 채팅 런타임에 적용하지 못해 터미널 방식으로 실행 중이에요"로 터미널에서 돌아요.
- **설치는 설정에서만 해요.** 몰래 내려받지 않아요. 설정 → 에이전트의 Claude Code 칸에 채팅 런타임 상태, "채팅 방식: 자동 | 터미널(PTY)", "채팅 런타임 설치" 버튼이 있어요. 설치는 사용자 Node·npm으로 앱 데이터 폴더에 해요(약 30MB, SDK의 번들 CLI는 빼요). Node가 없으면 버튼 대신 "Node.js 22 이상 필요 · `winget install OpenJS.NodeJS.LTS`"를 보여 줘요.
- **개발용:** 환경 변수 `MAESTRO_ACP_CLAUDE_DIR=<node_modules/@agentclientprotocol/claude-agent-acp가 들어 있는 폴더>`가 있으면 그 설치를 써요(영수증·버전 고정 없음, "개발용 경로"로 표시).
- **채팅↔CLI 전환:** 채팅 뷰 = ACP 세션, CLI 뷰 = 같은 세션 id의 TUI(`claude --resume <id>`). 에이전트가 돌고 있을 때 헤더의 Chat | CLI를 누르면 먼저 대화를 옮기고(헤더에 "전환 중…") 끝나면 보기가 바뀌어요(터미널이 쥐고 있을 때 Chat을 눌러도 채팅 런타임으로 옮겨요). 에이전트가 꺼져 있으면 보기가 바로 바뀌고, 새 보기가 뜨면서 그 보기의 방식으로 시작해요(시작 오류도 그 보기에 떠요). 한쪽 프로세스를 끝내고 다른 쪽을 띄워서, 한 번에 한쪽만 대화를 써요. 채팅은 다시 읽은 기록으로 새로 그려져서 항목이 전과 같아요.
  - 턴이 도는 중이거나 승인 카드가 떠 있으면 바로 바꾸지 않아요. 헤더에 "턴이 끝나면 전환돼요 · 지금 중단하고 전환"을 보여 주고, 턴이 끝나면 바꿔요. "지금 중단하고 전환"은 턴을 멈춰요(중단 버튼과 같아요). 기다리는 동안 원래 보기를 다시 누르면 전환을 그만둬요.
  - 백그라운드 셸(Bash)이 돌고 있으면 "백그라운드 셸이 멈춰요. 그래도 전환할까요?"를 물어요.
  - 이전 프로세스가 5초 안에 끝나지 않으면 한 번 더 강제로 끝내고 기다려요. 그래도 어댑터나 그 아래 claude가 살아 있으면 바꾸지 않아요. "새 갈래로 열까요?"에 동의하면 대화를 복사해(`session/fork`) 새 갈래로 열어요. 동의하지 않으면 그대로 두고, 다시 전환을 누르면 처음부터 다시 확인해요.
  - 턴이 끝나기를 기다리는 동안 에이전트가 꺼지면(어댑터가 죽음) 기다림을 그만두고 새 보기의 방식으로 바로 다시 시작해요.
  - 마지막에 본 보기를 기억해요(`lastView`). CLI 뷰를 보다가 앱을 다시 켜면 터미널로 시작해요.
  - 보기가 화면에 뜨는 것만으로는 대화를 가져가지 않아요. 꺼져 있으면 그 보기의 방식으로 띄우고, 돌고 있으면 그대로 둬요(터미널이 쥐고 있으면 채팅 뷰도 터미널 기록으로 채팅해요). 같은 워크트리를 다른 곳(전체 보기, 팝아웃 창, 멀리서 다시 확대한 캔버스 블록)에서 다른 보기로 열어 둬도 서로의 프로세스를 끄지 않아요.
  - CLI 뷰인데 채팅이 대화를 쥐고 있으면(다른 창이나 탭이 채팅 뷰로 가져갔을 때) 터미널 위에 "지금은 채팅이 대화를 쓰고 있어요 · CLI로 전환" 띠가 떠요. 턴이 끝나기를 기다리는 동안은 "채팅의 턴이 끝나면 CLI로 전환돼요 · 지금 중단하고 전환"이에요. 이 기다림 띠는 백엔드가 정말 기다릴 때만 떠요(터미널이 늦게 뜨는 것만으로는 안 떠요). 기다림이 교체 없이 끝나면(원래 보기를 다시 누름, 시간 초과, 오류) 모든 창에서 내려가요.
- **어떤 방식으로 도는지:** 헤더의 에이전트 이름표에 마우스를 올리면 "채팅: Claude Agent(ACP)"나 터미널로 도는 이유가 보여요. 정말 넘어간 경우(설정에서 고른 것도, CLI 뷰도 아닐 때)는 이름표 옆에 작게 "· 터미널"이 붙어요.
- **실시간 글:** 답이 쓰이는 대로 보여요. 완성된 블록이 트랜스크립트에 쓰이면 같은 자리 근처에 완성본이 그려지고 실시간 글은 사라져요.
- **권한 질문**은 지금처럼 승인 카드로 와요. "CLI에서 답하기"는 없어요. 답하지 않은 채 끝나면 거부예요.
- **채팅에서 안 되는 것**(CLI 보기에서 하세요):
  - `!` 셸 명령: 입력창 안내가 "메시지 입력 · Enter 전송 · / 명령어 · 셸 명령(!)은 CLI 보기에서"로 바뀌고, `!`를 치면 "채팅에서는 셸 명령(!)을 쓸 수 없어요 · CLI 보기에서 실행"이 떠요. 보내면 "셸 명령(!)은 CLI 보기에서 실행해요" 오류예요.
  - 어댑터가 받지 않는 슬래시 명령: `/` 메뉴에 "채팅에서는 안 돼요 · CLI 보기에서" 표시가 붙고, 보내면 "/이름 명령은 채팅에서 쓸 수 없어요. CLI 보기에서 실행해 주세요." `/model x`는 Maestro가 직접 모델을 바꿔요. 메뉴는 런타임이 바뀔 때와 어댑터가 명령 목록을 보낼 때마다(세션이 열린 뒤에 와요) 다시 받아요.
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
  - `AgentPanel`의 `useRuntimeSwitch`(AgentNode.tsx): catalog `runtimes`에 `"acp"`가 있고 에이전트가 돌고 있으면(`Worktree.runtime`이 null 아님) Chat | CLI가 `switch_runtime`을 거쳐요(`ifIdle` → busy(turn·approval)면 `afterTurn`, busy(shell)면 확인 후 `interrupt`, stuck이면 확인 후 `fork_session`). 꺼져 있으면 보기만 바로 바꾸고, 새 보기가 `start_view`로 띄워요. 새 클릭이 이전 것을 이겨요(이전 것은 superseded. 백엔드도 `switching`을 잡은 뒤 `done`/교체를 정해서, 먼저 시작한 전환이나 폴백이 끝난 상태에서 마지막 클릭대로 다시 봐요). 패널이 언마운트되면 진행 중인 전환의 늦은 결과(done·갈래·확인 창)는 버려요(`last`를 올림). `start(view)` = `start_view` + 스토어 반영.
  - `useAgentBoot(root, path, enabled, view)`: 채팅 뷰(`ChatBody`)는 `start_view(.., "chat")` 뒤에 `agent_ready`를 불러요. 결과의 `runtime`/`runtimeNote`는 스토어로 가요.
  - `XTerm`의 `start` = `start_view(.., "cli")`, `takeOver` = 헤더 토글과 같은 전환. `pty_snapshot.terminal === false`나 `agent://runtime`의 `"acp"`면 "CLI로 전환" 띠, 스토어의 `switchPending === "cli"`면 기다림 띠를 보여요.
  - `Composer`: `agent://commands`가 오면 `/` 목록(`list_slash_commands`)을 다시 받아요.
  - `useTranscript`: `seq: null`인 `delta`를 `addDelta`(chatFold.ts)로 이벤트 목록에 붙여요. 글 델타만 남기고, 항목의 퇴장 델타가 오면 그 항목의 델타를 모두 빼요. durable `turnEnded`로는 빼지 않아요(생각 줄의 `end_turn`이 답 글 줄보다 먼저 읽힐 수 있어요). `fold`는 그대로예요. `npm test`가 "실시간 글 + 퇴장 델타 + 완성 블록 = 완성 블록만"과 "답 중간에 온 `turnEnded`는 흐르던 글을 지우지 않음"을 확인해요.
  - 스토어(store.ts): `Worktree.runtime`, `runtimeNote`, `switchPending`, `setRuntime`, `setSwitchPending`. 리스너는 Workspace.tsx `useAgentEvents`(`agent://runtime`, `agent://switch`).
  - 설정: `ChatRuntimeRow`(SettingsDialog.tsx). → [설정 창](../ui/settings-dialog.md)
- 로그: `MAESTRO_ACP_LOGS=1`이면 어댑터 로그를 `<app data>/acp/logs/<경로 id>/agent.log`에, 무시한 업데이트를 stderr에 남겨요.

## 제약과 알려진 문제

- 같은 워크트리를 두 곳에서 채팅 뷰와 CLI 뷰로 동시에 열면, 먼저 띄운(또는 마지막으로 토글한) 쪽이 대화를 쥐어요. 채팅이 쥐고 있으면 CLI 뷰에 "CLI로 전환" 띠가 뜨고, 터미널이 쥐고 있으면 채팅 뷰는 터미널 기록으로 채팅해요(헤더의 Chat을 누르면 채팅 런타임으로 옮겨요).
- 설치 중에 설정 창을 닫았다 열면 진행 줄은 다시 보이지 않아요(설치는 계속돼요). 끝나면 "↻ 다시 확인"으로 상태를 볼 수 있어요.
- 바깥에서 같은 대화를 쓰는 프로세스(사용자가 터미널에서 직접 `claude --resume`)는 알아챌 수 없어요.
- 긴 대화의 `session/load`는 리플레이를 다 받은 뒤에 끝나서 느릴 수 있어요(측정 안 됨). 그러면 `session/resume`으로 바꿀 수 있어요.
- 실시간 글을 지우는 기준이 글 앞부분 일치라, 어댑터가 글을 바꿔 보내면 턴이 끝날 때까지 둘 다 보일 수 있어요.

## 결정 기록

- **자동 + 이유 표시:** 조건이 안 맞는 PC에서도 지금처럼 동작해야 해요(PTY 폴백). 왜 터미널인지 몰라 헷갈리지 않게 이유를 남겨요.
- **설치를 설정에서만:** 약 30MB를 사용자 모르게 받지 않아요. 개발과 테스트는 `MAESTRO_ACP_CLAUDE_DIR`로 해요.
- **같은 세션 재개 + 단일 쓰기 잠금:** R0 결정이에요. 포크는 잠금을 못 걸었을 때 사용자가 동의해야만 해요. → [ACP 런타임 사전 조사](../../architecture/acp-runtime-research.md)
