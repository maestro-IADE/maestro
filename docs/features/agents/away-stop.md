---
title: 자리 비움 정리, 슬립, 메모리 경고 (에이전트 끄기)
area: agents
status: done
files: [src-tauri/src/main.rs, src-tauri/src/memory.rs, src/MemoryNotice.tsx, src/agentMenu.tsx, src/agentActions.ts, src/nodes/AgentNode.tsx, src-tauri/src/agent.rs, src-tauri/src/acp_runtime.rs, src/App.tsx, src/settings.ts, src/settings/GeneralSection.tsx, src-tauri/src/runtime.rs, src-tauri/src/pty.rs]
related: [features/agents/chat-runtime.md, architecture/agent-runtime.md, features/ui/settings-dialog.md]
updated: 2026-11-22
---

# 자리 비움 정리

## 요약

에이전트 CLI를 꺼서 메모리를 아끼는 길이 세 가지예요. 모두 대화는 남고, 다시 필요해지면 이어서(`--resume`) 켜요.

- **슬립:** 에이전트 메뉴의 "슬립". 작업 중이면 먼저 물어요.
- **메모리 경고:** 컴퓨터 메모리가 모자라면 위쪽 막대에 "메모리" 버튼이 떠요. 거기서 쉬는 에이전트를 한 번에 꺼요.
- **자리 비움 정리:** 다른 프로젝트로 옮긴 뒤 정해 둔 시간이 지나면 그 프로젝트의 쉬는 에이전트를 꺼요. 기본은 꺼져 있어요(끄지 않음).

작업 중이거나, 승인을 기다리거나, 백그라운드 셸이 도는 에이전트는 메모리 경고와 자리 비움 정리가 남겨요.

## 동작

**슬립**
- 에이전트 메뉴(사이드바 행, 워커 카드, 지도 카드의 ⋯·우클릭)와 에이전트 헤더 ⋯에 "슬립"이 있어요. 아이콘은 달이에요. 에이전트가 켜져 있을 때만 보여요.
- 작업 중(`RUNNING`)이면 "작업 중인 에이전트예요"를 묻고, [슬립]을 눌러야 꺼요. 지금 하던 턴은 멈춰요.
- 꺼진 에이전트는 메시지를 보내거나, 그 보기를 다시 열거나, Lead가 `maestro send`로 일을 보내면 같은 대화로 다시 켜져요.

**메모리 경고**
- 10초마다 컴퓨터의 남은 메모리와 에이전트들이 쓰는 메모리(에이전트 프로세스와 그 아래 프로세스 전부)를 재요.
- 남은 메모리가 기준 아래로 내려가고 켜진 에이전트가 있으면, 메인 창 위쪽 막대에 경고색 "메모리" 버튼이 떠요.
  - 기준: 전체 메모리의 15%. 다만 1.5GB보다 작게, 4GB보다 크게 잡지 않아요. 8GB 컴퓨터면 1.5GB, 16GB면 2.4GB, 32GB 이상이면 4GB예요.
  - 기준의 1.25배를 넘어야 경고가 사라져요. 기준 근처에서 버튼이 깜빡이지 않게 하려는 거예요.
- 메모리가 모자라질 때마다 작은 창이 한 번 저절로 열려요. 닫으면 버튼만 남아요.
  - 창 내용: "메모리 부족", "남은 메모리 1.2 GB · 에이전트 6개 4.3 GB", [닫기], [쉬는 에이전트 슬립 N]
- [쉬는 에이전트 슬립 N]은 화면에 있든 없든 쉬는(`Idle`) 에이전트를 모두 꺼요. 승인 대기, 채팅↔터미널 전환 중, 백그라운드 셸이 도는 에이전트는 남겨요. 끌 것이 없으면 버튼이 눌리지 않아요.

**자리 비움 정리**
- **설정:** 일반 → 자리 비움 → "자리 비운 프로젝트 정리"에서 5분 / 15분 / 1시간 / 끄지 않음 중에 골라요. 기본은 끄지 않음이에요. 예전 기본값 15분으로 저장돼 있던 설정은 한 번 끄지 않음으로 바뀌어요(설정 v7).
- **끄는 조건:** 아래를 모두 만족해야 꺼요.
  - 지금 화면의 프로젝트가 아니에요.
  - 그 프로젝트를 떠난 지 정한 시간이 지났어요. 이번 실행에서 한 번도 안 연 프로젝트는 앱을 켤 때부터 센 것으로 쳐요.
  - 상태가 대기(`Idle`)예요.
  - CLI 화면 출력이 정한 시간 동안 없었어요.
  - 승인 대기가 없어요.
  - CLI 아래에 셸(`bash`, `sh`, `powershell`, `pwsh`)이 살아 있지 않아요.
- **다시 켜기:** 그 프로젝트로 돌아오거나 Lead가 `maestro send`로 일을 보내면 자동으로 이어서 켜져요. 켜지면 바로 `Idle` 상태가 보내져서 사이드바의 상태 글리프가 꺼짐(빈 원)에서 쉬는 중(채운 점)으로 바뀌어요.

## 구현

- 프런트엔드: App.tsx가 `activeRoot`나 `awayMinutes`가 바뀔 때마다 `ipc.setAway(activeRoot, minutes)`를 불러요.
- `set_away`(main.rs): `AppState.away`에 지금 프로젝트, 분, 프로젝트별로 떠난 시각(`left`)을 기록해요.
- `AppState.roots`: `ensure()`가 에이전트를 띄울 때 워크트리 경로와 프로젝트 루트를 짝지어 둬요. 설치용 터미널(`setup:*`)은 여기 없어서 대상이 아니에요.
- `reap_away`: setup에서 띄운 스레드가 30초마다 불러요. 조건에 맞는 후보가 있을 때만 프로세스 목록을 읽어요.
- 프로세스 확인(agent.rs)
  - `processes()`: Windows는 PowerShell `Get-CimInstance Win32_Process`, 그 밖은 `ps -A -o pid=,ppid=,etime=,comm=`로 읽어요. 목록을 못 읽으면 아무것도 끄지 않아요.
  - `shell_below()`: CLI 프로세스 아래 자손 중에 셸이 있는지 봐요(`is_shell`). 셸로 보는 이름은 bash, sh, zsh(macOS 기본 셸), fish, dash, ash, ksh, mksh, tcsh, csh, nu, powershell, pwsh예요. 경로를 뗀 이름을 대소문자 없이, 로그인 셸의 앞 `-`(`-zsh`)와 `.exe`를 빼고 비교해요. MCP 서버(node, python, cmd 래퍼)는 셸이 아니라서 무시돼요. cmd.exe는 일부러 빼요.
  - `Agent::quiet_for()`: 런타임의 `Runtime::quiet_for()`. PTY 런타임은 마지막 PTY 출력 뒤로 지난 시간이에요(ACP는 마지막 stdio 통신 뒤).
  - `Agent::runs_shell()`: `Runtime::pid()` 아래에서 `shell_below()`를 봐요.
  - 다시 켜질 때는 `reset restart`로 스트림 epoch가 올라가서, 열린 채팅 뷰와 CLI 뷰가 처음부터 다시 그려요.
- 테스트: `finds_shells_below_the_cli_only`, `knows_unix_and_windows_shells`, `lists_processes_live`(ignored, 실제 프로세스 목록)
- 끌 수 있는지(main.rs)
  - `agents_on`: 프로세스가 도는 에이전트. 설정의 설치 터미널(`setup:*`)은 빼요.
  - `resting(key, a, waiting)`: `Idle`이고, 승인 대기가 없고(`approvals_waiting`), 전환 중이 아니에요. 자리 비움 정리와 메모리 경고가 같이 써요.
  - `idle_now`: `resting`이면서 아래에 셸이 없는 것(`runs_shell`).
- 슬립
  - `turn_off_agent(worktreePath)`(`ipc.turnOffAgent`): `Agent::kill`. `retire`가 아니라서 다음 시작(`ensure`, 메시지, `maestro send`)에 이어서 켜져요.
  - 화면: `turnOffAgent(w)`(agentActions.ts)가 `RUNNING`이면 `ask`(위험)로 묻고 불러요. 메뉴는 `useAgentMenu`(agentMenu.tsx)와 헤더 ⋯(nodes/AgentNode.tsx) 두 곳이에요. `TERMINATED`면 항목을 빼요.
- 메모리 경고
  - memory.rs: `Watch`가 `sysinfo`(기능 `system`만)의 `System`을 들고 있다가 `look()`마다 메모리와 프로세스 목록(메모리만)을 새로 읽어요. 프로세스는 `agent::Proc` 모양으로 바꿔서 `runs_shell`에 그대로 써요(시작 시각은 초 → ms).
  - `line(total)` = 15%를 1.5~4GB로 자른 값. `low_now(was_low, total, available)`가 기준 아래면 낮음, 낮았으면 기준의 1.25배까지 낮음으로 둬요.
  - `tree_bytes(procs, mem, root, since)`: 에이전트 프로세스와 그 자손의 메모리 합. `since`(런타임 시작) 전에 생긴 프로세스는 pid 재사용이라 빼요(`born_since`).
  - main.rs `memory_tick`: setup에서 띄운 스레드가 10초마다 불러요. `Report { low, totalMb, availableMb, agentsMb, agentsOn, idle }`를 `MemoryNow`에 두고, 바뀌었을 때만(낮음, 개수, 크기는 100MB 단위) `maestro://memory`로 보내요.
  - `memory_report`(`ipc.memoryReport`): 마지막 보고. `turn_off_idle`(`ipc.turnOffIdle`): `idle_now`를 모두 끄고 몇 개인지 돌려줘요. 셸 확인은 `agent::processes()`로 다시 읽어서 `spawn_blocking`에서 해요.
  - 화면: `MemoryNotice`(src/MemoryNotice.tsx)가 `App` 위쪽 막대의 업데이트 버튼 왼쪽에 있어요. `low && agentsOn > 0`일 때만 보이고, 낮아질 때마다 한 번 창을 열어요. 모양은 업데이트 창(`.tb-update`, `.update-pop`)을 그대로 쓰고 버튼만 `.tb-warn`(`--mae-warn` 글자색)이에요.
  - 테스트: `the_line_is_a_share_kept_between_bounds`, `low_comes_and_goes_with_a_margin`, `a_tree_counts_its_children_not_older_strangers`(memory.rs), 설정 v7은 `scripts/view-check.ts`

- **채팅 런타임:** ACP로 도는 에이전트도 같은 규칙으로 멈춰요(`quiet_for`는 어댑터와 마지막으로 주고받은 때, `runs_shell`은 node 아래 셸). 채팅↔CLI 전환이나 폴백 중(`Agent::switching_now`)인 에이전트는 건너뛰어요. 다시 필요해지면 마지막에 본 보기의 런타임으로 이어서 시작해요. 아직 다 뜨지 않은 ACP 에이전트(예: Maestro의 폴더 신뢰 질문에서 기다리는 중)를 멈춰도 꺼짐(`TERMINATED`)이 되어 다음에 새로 시작해요. Claude Code가 백그라운드 작업 뒤 스스로 시작한 턴(Running)은 원래대로 건너뛰고, Stop으로 멈출 수 있어요(`session/cancel`).

## 제약과 알려진 문제

- 꺼졌다 켜지면 사라지는 것: 세션 중에 `/model`로 바꾼 모델, "이번 세션만" 허용한 권한, 보내지 않은 입력, 프롬프트 캐시. 대화 내용은 그대로예요.
- Windows에서 프로세스 목록을 읽는 데 약 4초가 걸려요. 정리 스레드 안에서만 읽어요.
- CLI가 셸을 계속 살려 두는 방식이면 그 에이전트는 꺼지지 않아요. 안전한 쪽으로 실패하는 거예요.
- 실제로 꺼졌다 다시 켜지는 흐름은 확인하지 않았어요. 슬립은 사용자가 dev 빌드에서 확인했어요. 메모리 경고 버튼과 창은 화면에서 확인하지 않았어요.
- 메모리 경고는 메인 창에만 떠요. 분리 창에는 없어요.
- 에이전트 메모리는 프로세스의 실제 사용량(작업 집합/RSS)을 더한 값이라, 공유 메모리가 겹쳐 세어져 실제보다 조금 크게 보일 수 있어요.
- [쉬는 에이전트 슬립]은 화면에 보이는 에이전트도 꺼요. 그 보기는 꺼짐으로 바뀌고, 메시지를 보내면 다시 켜져요.
- `create_worktree`(새 워크트리 대화상자, `maestro worktree create`)로 띄운 에이전트는 `roots`에 등록되지 않아서, 다시 시작(`ensure`)되기 전까지는 대상이 아니에요.

## 결정 기록

- **처음에 다 켜기 / 보일 때 켜기 / 수동** 중에서 "보일 때 켜기 + 자리 비움 정리"를 골랐어요. CLI 하나가 수백 MB라 다 켜 두면 쌓이고, 수동은 캔버스 요약과 빠른 입력과 맞지 않아요.
- **카드 단위 대신 프로젝트 단위:** 사용자가 생각하는 단위와 같고, 캔버스를 조금 스크롤했다고 꺼지는 일이 없어요.
- 설정은 에이전트 탭이 아니라 새 "실행" 탭에 드롭다운으로 두었어요.
- 2026-11-13 (UI 구조 재구성 단계 9): 항목 하나짜리 "실행" 섹션을 없애고 일반 섹션의 "자리 비움" 묶음으로 옮겼어요.
- 2026-11-22 (사용자: "끄지 않음으로 바꾸고 메모리 경고 기능을 좀 넣어주자. 그리고 수동으로 끌 수 있는 기능도."): 자리 비움 정리의 기본을 끄지 않음으로 바꿨어요. 꺼졌다 켜지면 세션 중 바꾼 모델과 "이번 세션만" 허용한 권한이 조용히 사라지고, 프로젝트를 오가는 게 기본 사용이라 15분은 너무 짧았어요. 사용자가 모르게 끄는 대신, 메모리가 실제로 모자랄 때 알리고 끄는 건 사용자가 고르게 했어요(메모리 경고, 슬립). 메모리는 PowerShell 프로세스 목록(4초)이 아니라 `sysinfo`로 읽어요: 10초마다 재려면 가벼워야 해서예요.
- 2026-11-22 (사용자: "끄기라는 표현 대신 슬립이라고 하자"): 메뉴 항목, 확인 버튼, 메모리 경고 버튼의 "끄기"를 "슬립"으로, 아이콘을 전원에서 달로 바꿨어요. 명령 이름(`turn_off_*`)과 꺼진 상태의 이름(꺼짐)은 그대로예요.
