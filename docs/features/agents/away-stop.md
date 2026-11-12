---
title: 자리 비움 정리 (안 보는 프로젝트의 에이전트 끄기)
area: agents
status: done
files: [src-tauri/src/main.rs, src-tauri/src/agent.rs, src-tauri/src/acp_runtime.rs, src/App.tsx, src/settings.ts, src/SettingsDialog.tsx, src-tauri/src/runtime.rs, src-tauri/src/pty.rs]
related: [features/agents/chat-runtime.md, architecture/agent-runtime.md, features/ui/settings-dialog.md]
updated: 2026-11-13
---

# 자리 비움 정리

## 요약

다른 프로젝트로 옮긴 뒤 정해 둔 시간(기본 15분)이 지나면, 그 프로젝트에서 쉬고 있는 에이전트 CLI를 꺼서 메모리를 아껴요. 작업 중이거나, 승인을 기다리거나, 백그라운드 셸이 도는 에이전트는 남겨요. 다시 필요해지면 대화를 이어서(`--resume`) 켜요.

## 동작

- **설정:** 실행 탭 → "자리 비운 프로젝트 정리"에서 5분 / 15분 / 1시간 / 끄지 않음 중에 골라요.
- **끄는 조건:** 아래를 모두 만족해야 꺼요.
  - 지금 화면의 프로젝트가 아니에요.
  - 그 프로젝트를 떠난 지 정한 시간이 지났어요. 이번 실행에서 한 번도 안 연 프로젝트는 앱을 켤 때부터 센 것으로 쳐요.
  - 상태가 대기(`Idle`)예요.
  - CLI 화면 출력이 정한 시간 동안 없었어요.
  - 승인 대기가 없어요.
  - CLI 아래에 셸(`bash`, `sh`, `powershell`, `pwsh`)이 살아 있지 않아요.
- **다시 켜기:** 그 프로젝트로 돌아오거나 마에스트로가 `maestro send`로 일을 보내면 자동으로 이어서 켜져요. 켜지면 바로 `Idle` 상태가 보내져서 사이드바의 상태 글리프가 꺼짐(빈 원)에서 쉬는 중(채운 점)으로 바뀌어요.

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

- **채팅 런타임:** ACP로 도는 에이전트도 같은 규칙으로 멈춰요(`quiet_for`는 어댑터와 마지막으로 주고받은 때, `runs_shell`은 node 아래 셸). 채팅↔CLI 전환이나 폴백 중(`Agent::switching_now`)인 에이전트는 건너뛰어요. 다시 필요해지면 마지막에 본 보기의 런타임으로 이어서 시작해요. 아직 다 뜨지 않은 ACP 에이전트(예: Maestro의 폴더 신뢰 질문에서 기다리는 중)를 멈춰도 꺼짐(`TERMINATED`)이 되어 다음에 새로 시작해요. Claude Code가 백그라운드 작업 뒤 스스로 시작한 턴(Running)은 원래대로 건너뛰고, Stop으로 멈출 수 있어요(`session/cancel`).

## 제약과 알려진 문제

- 꺼졌다 켜지면 사라지는 것: 세션 중에 `/model`로 바꾼 모델, "이번 세션만" 허용한 권한, 보내지 않은 입력, 프롬프트 캐시. 대화 내용은 그대로예요.
- Windows에서 프로세스 목록을 읽는 데 약 4초가 걸려요. 정리 스레드 안에서만 읽어요.
- CLI가 셸을 계속 살려 두는 방식이면 그 에이전트는 꺼지지 않아요. 안전한 쪽으로 실패하는 거예요.
- 실제로 꺼졌다 다시 켜지는 흐름은 확인하지 않았어요.
- `create_worktree`(새 워크트리 대화상자, `maestro worktree create`)로 띄운 에이전트는 `roots`에 등록되지 않아서, 다시 시작(`ensure`)되기 전까지는 대상이 아니에요.

## 결정 기록

- **처음에 다 켜기 / 보일 때 켜기 / 수동** 중에서 "보일 때 켜기 + 자리 비움 정리"를 골랐어요. CLI 하나가 수백 MB라 다 켜 두면 쌓이고, 수동은 캔버스 요약과 빠른 입력과 맞지 않아요.
- **카드 단위 대신 프로젝트 단위:** 사용자가 생각하는 단위와 같고, 캔버스를 조금 스크롤했다고 꺼지는 일이 없어요.
- 설정은 에이전트 탭이 아니라 새 "실행" 탭에 드롭다운으로 두었어요.
