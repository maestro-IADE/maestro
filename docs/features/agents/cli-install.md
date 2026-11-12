---
title: 에이전트 CLI 탐지, 설치와 로그인
area: agents
status: partial
files: [src-tauri/src/cli.rs, src-tauri/src/catalog.rs, src-tauri/src/main.rs, src-tauri/src/exec.rs, src-tauri/src/pty.rs, src/settings/AgentsSection.tsx, src/settings/agentState.ts, src/NewAgentDialog.tsx, src/App.tsx, src/styles.css]
related: [features/agents/chat-runtime.md, features/agents/agent-adapters.md, features/ui/settings-dialog.md, features/agents/cli-view.md, architecture/agent-runtime.md]
updated: 2026-11-13
---

# 에이전트 CLI 탐지, 설치와 로그인

## 요약

카탈로그의 에이전트마다 실행 파일 경로를 찾고, 버전과 로그인 상태를 확인해서 미설치 / 버전 낮음 / 로그인 필요 / 준비됨 중 하나로 알려 줘요(`cli_status`). 네트워크를 쓰지 않고 사용자 설정도 바꾸지 않아요. 설정 → 에이전트 섹션에서 설치되지 않았거나 오래된 CLI를 앱 안의 터미널로 설치·업데이트하고 로그인까지 마칠 수 있어요. 에이전트 CLI가 하나도 없으면 앱을 켤 때 설정 창이 자동으로 열려요.

## 동작

- 에이전트마다 한 행이 있고 "관리"로 펼쳐요. → [설정 창](../ui/settings-dialog.md)
  - 상태 표시(CLI와 채팅 런타임을 합침): 확인 중 / 준비 중(런타임 없음, Codex) / 설치 필요 / 버전 낮음 / 로그인 필요 / 채팅 런타임 필요 / 사용 중 / 꺼짐
  - 준비되지 않은 이유(예: "Claude Code 2.0.54 이상이 필요해요. 지금은 1.9.0이에요.")는 상태 글의 툴팁과 "관리 → 상태·로그인" 첫 줄에 있어요.
  - 실행 명령 줄에 마우스를 올리면 찾은 실행 파일 경로가 보여요.
  - 행 오른쪽: "기본" 배지, 켜기/끄기 스위치(설치 안 됐거나 버전이 낮으면 켤 수 없음), "관리". 기본 에이전트는 기본값 카드에서만 골라요.
  - 관리 → 상태·로그인: 설치됐으면 "터미널에서 로그인", 안 됐거나 버전이 낮으면 설치 명령과 "설치하고 로그인"(업데이트도 같은 명령).
- 설치·로그인 버튼을 누르면 행 안에 터미널(340px)이 열려요.
  - npm 설치(`npm install -g …`)가 끝나면 바로 CLI를 실행해요. 버전이 낮을 때도 같은 명령으로 최신 버전을 깔아요.
  - 테마, 로그인, 신뢰 같은 첫 실행 화면을 그 자리에서 진행할 수 있어요.
  - Codex는 `codex login`을 실행해요.
  - 프로세스가 종료 코드 0으로 끝나면(설치·로그인을 마치고 CLI를 끝냄) 터미널이 저절로 닫히고 "끝났어요"가 남아요. 0이 아니면(설치 실패 등) 열어 둔 채 "끝났지만 오류가 있었어요 · 종료 코드 N"을 보여요. 어느 쪽이든 설치 상태를 다시 읽어요.
- 이미 설치된 CLI도 같은 터미널로 로그인할 수 있어요.
- Antigravity는 npm 패키지가 없어서 앱(antigravity.google)을 먼저 설치하라고 안내해요(카탈로그 `Install::Guide`).
- "↻ 다시 확인"으로 상태를 다시 읽어요(`cli_status { recheck: true }`: 이번 실행에서 막힌 채팅 런타임도 풀어요). 설정 창을 열 때와 앱이 시작할 때도 다시 읽지만(recheck 없음), 그때는 막힘을 풀지 않아요.
- 새 에이전트 창: 모든 에이전트가 목록에 남고 못 쓰는 것은 이유(" · 설치 필요", " · 버전 낮음", " · 꺼 둠", " · 준비 중")가 붙어요. 고르면 바닥 위 오류 상자에 이유와 "설정에서 설치"(꺼 둔 것은 "설정에서 켜기")가 나오고 "만들고 열기"가 꺼져요. "로그인 필요"는 고를 수 있고 안내 문구가 떠요. → [에이전트 만들기](../worktrees/create-worktree.md)

- **채팅 런타임 어댑터(Claude Code):** 같은 칸에 채팅 런타임 상태(`cli_status`의 `acp`), "채팅 방식: 자동 | 터미널(PTY)", "채팅 런타임 설치" 버튼이 있어요. 설치는 사용자 Node·npm으로 `<app data>/acp/claude-agent-acp@0.102.0/`에 하고(`--omit=optional`, 버전 고정, npx 없음), 진행은 `acp://install`로 와요. Node 22 이상이 없으면 설치 대신 `winget install OpenJS.NodeJS.LTS` 안내를 보여요. "다시 확인"은 채팅 런타임의 캐시와 막힌 상태도 풀어요. → [채팅 런타임](chat-runtime.md)

## 구현

**경로 찾기** (`cli::find`, `find_in`)
1. PATH의 폴더를 순서대로 봐요(터미널이 실행할 것과 같게).
2. 그다음 카탈로그 `dirs`(홈 기준)를 봐요. PATH가 오래됐거나(앱을 켠 뒤 설치) 비어 있을 때(Finder에서 실행)를 위해서예요.
   - Claude Code: `.local/bin`, `.claude/local`, `.claude/local/node_modules/.bin`, `AppData/Roaming/npm`
   - Antigravity: `AppData/Local/agy/bin`, `.local/bin`
   - Codex: `AppData/Roaming/npm`, `.local/bin`
- Windows는 `.exe`, `.cmd`, `.bat` 순으로 찾아요. npm이 함께 두는 확장자 없는 sh shim과 `.ps1`은 실행할 수 없어서 안 봐요(Nimbalyst `claudeExecutableResolver.ts`와 같은 방식).
- macOS/Linux는 시작할 때 로그인 셸(`$SHELL -ilc`, 없으면 `/bin/zsh`)의 PATH를 읽어요(`main.rs` `use_shell_path` → `login_shell_path`). `exec::run`으로 10초까지만 기다리고, 넘거나 표시(`__MAESTRO_PATH__`) 사이 값이 온전히 오지 않으면 원래 PATH를 그대로 써요. rc 파일에는 `MAESTRO_SHELL_PROBE=1`이 보여서 무거운 부분을 건너뛸 수 있어요. 창이 뜨기 전에 동기로 돌아서 rc가 느리면 시작이 그만큼(최악 약 12초) 늦어요. Windows는 레지스트리 PATH를 다시 읽지 않아요.

**실행 명령** (`cli::launcher`, `cli::command`)
- `.cmd`/`.bat` shim이면 옆의 `node_modules/<패키지>/bin/<bin>.exe`를 찾아 바로 실행해요(T3 Code `ClaudeExecutable.ts`와 같은 방식). 없으면 `cmd.exe /c call <shim 경로>`예요. `call`을 앞에 두는 건 shim 경로에 공백이 있어 따옴표로 시작하면, 뒤 인자에도 따옴표가 있을 때 `cmd /c`가 처음과 마지막 따옴표를 지워 실행이 깨지기 때문이에요.
- `command(def)`가 실패하면 "실행 파일을 찾지 못했어요"예요. 실행 실패 분류의 "실행 파일 없음"이 이거예요(`architecture/agent-runtime.md`).

**버전** (`probe_version`, `parse_version`, `older`)
- `<실행 파일> --version`을 창 없이 실행하고 8초가 지나면 끊어요. 출력에서 처음 나오는 `x.y.z`를 읽어요.
- 버전을 못 읽으면 막지 않아요.
- 이 PC 기준 걸린 시간: `claude.exe --version` 약 0.7초, `agy --version` 약 1초. `claude.cmd`를 거치면 약 4.5초였어요. 세 에이전트는 스레드로 동시에 확인해요(`status()`).

**로그인** (`logged_in`, 파일만 읽어요)
- Claude Code(`claude_logged_in`):
  - env `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `CLAUDE_CODE_OAUTH_TOKEN`, `CLAUDE_CODE_USE_BEDROCK`, `CLAUDE_CODE_USE_VERTEX` 중 하나가 있으면 로그인됨
  - `CLAUDE_CONFIG_DIR`(없거나 비어 있으면 `~/.claude`, `claude::config_dir`)의 `.credentials.json`에 `claudeAiOauth`가 있으면 로그인됨(Windows, Linux)
  - `~/.claude.json`(또는 설정 폴더 안)에 `oauthAccount`나 `primaryApiKey`가 있으면 로그인됨(macOS는 토큰이 키체인에 있어서 이걸로 봐요)
  - 아니면 로그인 필요
- Codex(`codex_logged_in`): `OPENAI_API_KEY`, 또는 `CODEX_HOME`(`~/.codex`)의 `auth.json`에 `tokens`/`OPENAI_API_KEY`. `config.toml`이 keyring 저장을 쓰면 "알 수 없음"
- Antigravity(`antigravity_logged_in`): Windows는 자격 증명 관리자의 `gemini:antigravity` 항목이 있는지 `CredReadW`로 봐요(비밀값은 읽자마자 해제, 들여다보지 않음). 다른 OS는 `~/.gemini/antigravity-cli/antigravity-oauth-token` 파일이 있으면 로그인됨, 없으면 "알 수 없음"(키링은 확인하려면 권한을 물어서). 위치는 Orca `antigravity-host-acp.ts`, `native-windows-credentials.ts`에서 확인했어요.

**상태 판정** (`decide(found, version, min, logged_in)`)
1. 실행 파일이 없으면 `missing`(미설치)
2. 버전과 최소 버전이 둘 다 있고 버전이 낮으면 `tooOld`(버전 낮음)
3. 로그인이 확실히 안 돼 있으면 `loginRequired`(로그인 필요)
4. 나머지는 `ready`(준비됨). 알 수 없는 버전·로그인은 막지 않아요.
- 테스트: `decides_status`, `parses_and_compares_versions`, `finds_windows_executables_before_shims`, `skips_npm_shim_for_package_exe`, `runs_shim_with_spaces_and_quoted_args`(Windows), `reads_login_state_from_files`. 이 PC에서 실제 결과를 보려면 `cargo test -- --ignored cli_status_live --nocapture`.

**명령과 화면**
- `cli::status(recheck)` → `cli_status(recheck?)`: 에이전트별 `{ id, status, installed, path, version, minVersion, loggedIn, installCmd, message }`와 npm 유무를 돌려줘요.
- `cli::setup_argv(id)`: 쓸 수 있게 설치돼 있으면 카탈로그 `login_cmd`만, 없거나 버전이 낮으면 `npm install -g <pkg> && <login_cmd>`를 만들어요. npm 패키지가 아니면 오류예요. Windows는 `cmd.exe /c`로 감싸요.
- `setup_start(id)`: `Agent::run`으로 PTY를 띄워요. 키는 `setup:<id>`예요. `setup_stop`으로 끝내요.
- `settings/AgentsSection.tsx`: `AgentRow`(상태 글자는 `settings/agentState.ts` `agentRowState`), `SetupTerminal`(`XTerm`을 그 키로 붙이고 `pty://exit`를 들음), `InstalledDetails`(설치된 에이전트의 관리), `InstallHelp`(npm 명령 또는 카탈로그 `install.text`). 버전이 낮으면 관리가 `InstallHelp`를 보여요.
- 종료 코드: pty.rs의 프로세스 감시 스레드가 끝날 때 `pty://exit { worktreePath, code }`를 보내요(`agent://state` TERMINATED보다 먼저). → [이벤트](../../reference/events.md)
- 상태 글자 색: `.agent-state[data-tone=…]`(styles.css). "설치 필요", "버전 낮음", "로그인 필요", "채팅 런타임 필요"는 경고색, "사용 중"은 초록이에요.
- App.tsx: 시작할 때 `loadCli()` 결과에 설치된 게 하나도 없으면 설정 창을 열어요.

## 제약과 알려진 문제

- 설치 터미널로 실제 설치와 로그인을 끝까지 해 보지 않았어요.
- npm(Node.js)이 없으면 설치 버튼이 동작하지 않아요.
- 로그인 판정은 파일 존재로 추정해요. 토큰이 만료·폐기됐는지는 몰라요(네트워크를 안 쓰기 때문). 그래서 "로그인 필요"여도 실행은 막지 않고, 실제로 로그인 화면이 뜨면 시작 실패로 알려 줘요(`architecture/agent-runtime.md`).
- Windows에서 앱을 켠 뒤 PATH에 추가된 폴더는 카탈로그 `dirs`에 있을 때만 찾아요.
- Antigravity는 npm이 아니라서 버전이 낮아도 앱 안 터미널로 업데이트하지 못해요(`agy update`가 있지만 연결하지 않았어요).

## 결정 기록

- 2026-11-13 (UI 구조 재구성 단계 9): 설치 터미널을 끝난 뒤 직접 닫던 것을 종료 코드 0이면 저절로 닫게 했어요(닫기 한 번 줄임). 실패한 설치는 출력을 읽어야 해서 열어 둬요. 타일을 행으로 바꾸고 "기본으로"를 없앴어요.

- 2026-11-10 (PRD R2): 설치 여부(`where`/`which`)만 보던 것을 경로·버전·로그인 판정으로 바꿨어요. Orca의 ready/tooOld/unknown/missing처럼 "모르는 건 막지 않는다"를 따랐어요.
- 로그인 확인에 `claude auth status` 같은 명령을 쓰지 않았어요. 토큰 갱신이나 네트워크 호출이 일어날 수 있어서, 파일과 env만 봐요.
