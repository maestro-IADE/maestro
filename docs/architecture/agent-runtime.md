---
title: 에이전트 실행 방식 (PTY + 트랜스크립트)
area: architecture
status: done
files: [src-tauri/src/agent.rs, src-tauri/src/antigravity.rs, src-tauri/src/catalog.rs, src-tauri/src/cli.rs, src-tauri/src/main.rs, src/nodes/XTerm.tsx, src/nodes/AgentNode.tsx, src/nodes/WorktreeCard.tsx]
related: [architecture/overview.md, features/agents/chat-view.md, features/agents/cli-view.md, features/agents/agent-adapters.md, features/agents/cli-install.md, reference/storage-layout.md]
updated: 2026-11-10
---

# 에이전트 실행 방식

## 요약

에이전트마다 실제 CLI(TUI)를 PTY 하나에 띄워요. CLI 뷰는 그 PTY 화면을 그대로 보여 주고, 채팅 뷰는 CLI가 디스크에 쓰는 대화 기록(트랜스크립트)을 따라 읽어서 말풍선으로 바꿔요. 채팅에서 보낸 메시지는 PTY에 붙여넣기로 입력돼요. 그래서 두 뷰는 같은 프로세스를 다르게 보여 줄 뿐이에요.

## 동작

- 에이전트 블록을 열면 CLI가 먼저 떠요. 채팅 입력은 CLI가 입력을 받을 준비가 된 뒤에야 보여요. 그 전에는 "준비 중…" 로딩이 보여요.
- 처음 여는 폴더면 Claude가 "이 폴더를 신뢰할까요?"를 물어요. 채팅 뷰에 "신뢰하고 시작" 카드가 떠요.
- 시작이 안 되면 이유를 보여 줘요: 실행 파일 없음, 로그인(또는 첫 실행 설정) 필요, 시작하다 종료(마지막 출력 한 줄), 시작 시간 초과. 채팅 뷰에는 "다시 시도"와 "CLI에서 보기" 버튼이 있어요. 카드와 에이전트 바에는 "다시 시도"가 있어요.
- 앱을 다시 켜도 같은 세션으로 이어서 실행돼요.

## 구현

**띄우기:** `Agent::spawn` → `Agent::run`(agent.rs)
- 세션 id를 메타 파일에 저장해요.
  - Claude: 새 세션이면 `--session-id <uuid>`, 트랜스크립트가 이미 있으면 `--resume <uuid>`로 띄워요.
  - Antigravity: `--conversation <id>`로 이어요. → [에이전트 종류](../features/agents/agent-adapters.md)
- 먼저 카탈로그(`catalog::require`)에서 에이전트를 찾고, 런타임이 없으면(Codex) 시작하지 않아요.
- 실행 파일은 `cli::command`가 찾은 경로로 실행해요. npm `.cmd` shim은 옆의 패키지 exe로 바꾸고, 그게 없을 때만 `cmd.exe /c call <shim>`이에요. 못 찾으면 "실행 파일을 찾지 못했어요"로 실패해요(실행 실패 분류의 "실행 파일 없음"). → [CLI 탐지](../features/agents/cli-install.md)
- `argv()`가 그 뒤에 인자를 붙여요. 권한 모드, 에이전트별 `--settings`(승인 훅), 시스템 브리프, 모델이 들어가요.
- 환경 변수:
  - `TERM=xterm-256color`, `COLORTERM=truecolor`
  - `maestro_env()`가 `MAESTRO_URL`, `MAESTRO_TOKEN`, `MAESTRO_WORKTREE`를 넣고 `~/.maestro/bin`을 PATH 앞에 붙여요.
  - `host_session_env()`가 Maestro를 Claude Code 안에서 실행했을 때 물려받는 `CLAUDECODE`, `CLAUDE_CODE_*`, `NO_COLOR` 등을 지워요. 이게 남으면 색이 안 나오거나 세션이 꼬여요.

**PTY 출력**
- 읽기 스레드가 블로킹으로 읽어요. 배처가 16ms(`BATCH_WINDOW`) 단위로 묶어서 `pty://data` 이벤트 하나로 보내요. `seq`는 지금까지의 총 바이트 수예요.
- 최근 출력은 `Scrollback`에 보관해요. 새로 열린 터미널은 `pty_snapshot`으로 이걸 받고, 이미 받은 범위의 실시간 조각은 `seq`로 걸러요.

**ConPTY 커서 위치 질의**
- ConPTY는 시작할 때 커서 위치 질의(`ESC[6n`)를 보내고 답이 올 때까지 멈춰요.
- 백엔드가 `ESC[1;1R`로 대신 답해요. CLI 뷰의 xterm도 답하므로, XTerm.tsx가 자기 답을 버려서 답이 하나만 가게 해요.

**준비 판정과 실행 실패 분류:** `wait_ready() -> Result<bool, LaunchFailure>`
- 한 번 살펴보는 건 `Scrollback::startup(timed_out, login_hints)`이에요. 출력이 800ms 조용하거나 45초(`STARTUP_LIMIT`)가 지났을 때만 판정해요.
  1. 출력(`mark` 뒤, 최대 8KB, `plain()`으로 이스케이프 제거)에 카탈로그 `login_hints`가 있으면 `LoginRequired`. 로그인 화면은 붙여넣기 모드를 켜지 않을 수 있어서, 이걸 먼저 봐요.
     - 이어 띄운 세션(`Scrollback::resumed`: Claude `--resume`, Antigravity `--conversation`)은 붙여넣기 모드가 켜진 뒤에는 보지 않아요. TUI가 다시 그린 지난 대화에 "Please run /login" 같은 글이 있으면 로그인 화면과 구분할 수 없어서예요. 로그인이 정말 풀렸으면 첫 턴에서 오류로 보여요.
  2. 붙여넣기 모드 켜짐(`ESC[?2004h`)을 봤으면 준비됨. `Ok(asks_trust)`: 그 출력에 "trust"가 있으면 신뢰 확인 화면이에요. `accept_trust`가 Enter를 보내요. 45초가 지났어도 붙여넣기 모드를 봤으면 준비됨이에요(무언가 계속 움직여서 조용해지지 않는 경우).
  3. 45초가 지났는데 붙여넣기 모드를 못 봤으면 `Timeout`. 전에는 이때 준비된 것으로 쳤어요.
- 기다리는 사이에 프로세스가 끝나면 출력으로 분류해요(`classify`): 로그인 문구면 `LoginRequired`, 아니면 `Exited`(마지막 출력 한 줄).
- 준비됨(`Ok`)만 기억해요(`ready`). 실패는 기억하지 않아서 "다시 시도"가 다시 살펴봐요.
- 로그인 필요를 알린 뒤에는 `mark`를 그 뒤로 옮기고 `login_shown`을 켜요. 새 출력이 없으면 계속 로그인 필요예요. 사용자가 CLI 보기에서 로그인을 마치면 그 뒤 출력으로 다시 판정해서, 같은 프로세스로 이어서 쓸 수 있어요.
- `LaunchFailure`는 `{ kind: "loginRequired" | "exited" | "timeout", message }`로 직렬화돼요. `agent_ready`가 `failure`로 돌려주고, 프런트 `useAgentBoot`가 `{ error: message }`로 보여 줘요.
- 부르는 쪽: `send_message`·`maestro send`(`control::deliver`)는 실패 메시지를 오류로 돌려줘요. 워커 완료 메모(`worker_done`, `deliver_kept_notes`)와 페르소나 알림(`set_reference`)은 `Ok(false)`일 때만 붙여 넣어요.
- 테스트: `startup_decides_ready_login_or_timeout`, `classifies_failed_starts`, `strips_escapes_from_tui_output`

**메시지 보내기:** `Agent::send`
- 브래킷 붙여넣기(`ESC[200~ … ESC[201~`) 후 Enter를 보내요. `sending` 잠금으로 한 번에 하나씩만 보내서, 동시에 온 메시지(채팅 입력과 워커 완료 메모)가 섞이지 않아요.
- 잠금을 잡은 뒤 에이전트가 꺼져 있으면(`alive()` 거짓) 오류를 돌려줘요. ConPTY는 CLI가 끝난 뒤에도 쓰기를 받아서, 확인하지 않으면 꺼진 에이전트에 보낸 글이 성공으로 처리돼요.
- `!`로 시작하면 `!`를 먼저 키로 입력해요. 그래야 TUI가 bash 모드로 바뀌어요.
- `/명령`만 있으면 뒤에 공백을 붙여요. 안 그러면 TUI 자동완성이 한 번 더 완성해서 `/model/model`처럼 돼요.
- 첨부 파일은 `@경로`로 붙여요.

**트랜스크립트 따라 읽기:** `Tail`
- 위치:
  - Claude: `~/.claude/projects/<경로 슬러그>/<세션>.jsonl`
  - Antigravity: `~/.gemini/antigravity-cli/brain/<대화>/.system_generated/logs/transcript.jsonl` (IDE 앱의 `antigravity/` 폴더가 아니에요)
- `to_events()`가 줄을 채팅 이벤트(`user_input`, `assistant`, `user`(도구 결과), `interrupted`, `local_command`)로 바꿔요.
- 이벤트는 `agent://event`로 보내요. 상태(`Running`/`Idle`/`Terminated`)는 `state_after()`로 계산해서 `agent://state`로 보내요.
- 턴이 끝나면 `control::worker_done`이 상위 에이전트에 알려요. 상위가 꺼져 있거나 신뢰 확인 중이면 메모를 보관했다가, `ensure()`로 다시 시작되거나 `accept_trust` 뒤 준비되면 `Agent::deliver_kept_notes`가 붙여 넣어요. → [워커 완료 알림](../features/orchestration/worker-done-notes.md)

**종료**
- 프로세스 감시 스레드가 0.5초마다 `try_wait()`로 종료를 확인해요. ConPTY는 종료 뒤에도 파이프가 열려 있을 수 있어서 이렇게 해요.
- `kill()`은 Windows에서 `taskkill /T /F`로 프로세스 트리 전체를 끝내요.
- `Esc`(`interrupt`)는 현재 턴만 멈추고 프로세스는 살려 둬요.
- 앱이 끝나면(`RunEvent::Exit`) 모든 에이전트를 끝내요.

## 제약과 알려진 문제

- 준비 판정은 TUI 출력 모양에 기대요. CLI가 화면 구성을 바꾸면 45초 뒤 "시작 시간 초과"가 될 수 있어요.
- 로그인 필요 판정은 화면 글자(`login_hints`)로 해요. 이어 띄운 세션은 입력창이 뜨면 이 판정을 건너뛰어서, 그 세션에서 로그인 화면이 떠도 준비됨으로 보고 첫 턴에서야 실패해요. Claude의 문구("Select login method", "Choose the text style" 등)는 CLI 출력에서 알려진 것이지만, 이 PC에서 로그아웃 상태로 띄워 확인하지는 않았어요. Antigravity 문구는 확인 안 됨.
- 트랜스크립트 형식은 각 CLI의 내부 형식이라 버전이 바뀌면 깨질 수 있어요.

## 결정 기록

- **stream-json 방식을 버린 이유:** 처음에는 `claude -p --output-format stream-json`으로 채팅만 만들었어요(커밋 48c3399). 실제 CLI와 슬래시 명령, 권한, 화면이 달라서 지금 구조로 바꿨어요.
- **채팅 데이터를 트랜스크립트에서 읽는 이유:** 화면 출력(ANSI)을 파싱하는 것보다 안정적이에요.
