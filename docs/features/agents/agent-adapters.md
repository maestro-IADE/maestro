---
title: 에이전트 종류와 카탈로그 (Claude Code, Antigravity, Codex)
area: agents
status: partial
files: [src-tauri/src/catalog.rs, src-tauri/src/cli.rs, src-tauri/src/agent.rs, src-tauri/src/antigravity.rs, src/NewAgentDialog.tsx, src/main.tsx, src/assets/agents, src-tauri/src/runtime.rs, src-tauri/src/claude.rs]
related: [features/agents/chat-runtime.md, architecture/agent-runtime.md, architecture/agent-adapter-spec.md, features/agents/cli-install.md, features/agents/slash-commands.md, features/agents/answer-summaries.md, architecture/runtime-events.md]
updated: 2026-11-11
---

# 에이전트 종류와 카탈로그

## 요약

Maestro가 아는 에이전트는 Rust의 카탈로그(`catalog.rs` `AGENTS`) 한 곳에 정의돼요. 이름, 로고, 실행 파일, 설치 방법, 지원 런타임, ACP 어댑터, 최소 버전, 로그인 확인, 슬래시 명령, 모델, 기능 플래그가 다 여기 있고, 프런트는 `agent_catalog` 명령으로 받아 써요. 지금 워크트리 에이전트로 쓸 수 있는 건 Claude Code(`claude`)와 Antigravity CLI(`agy`)이고, Codex(`codex`)는 설치·로그인까지만 돼요.

## 동작

| 에이전트 | id | 실행 파일 | 런타임 | 최소 버전 | 상태 |
|---|---|---|---|---|---|
| Claude Code | `claude-code` | `claude` | PTY | 2.0.54 | 채팅, CLI, 승인, 슬래시 명령, 모델 선택 다 돼요 |
| Antigravity | `antigravity` | `agy` | PTY | 1.4.4 | 채팅, CLI 돼요. 승인 카드와 슬래시 명령 목록은 없어요. 끝까지 확인 안 됨 |
| Codex | `codex` | `codex` | 없음 | 없음 | 설치·로그인 터미널만 돼요. 새 워크트리에서는 "(준비 중)"으로 보이고 고를 수 없어요 |

- 에이전트 로고는 사이드바, 카드, 새 워크트리 창, 설정에 보여요(`src/assets/agents/`, 카탈로그의 `logo` 파일 이름).
- 설정에서 설치돼 있어도 끌 수 있어요(`disabledAgents`). 꺼진 에이전트는 새 워크트리에서 안 보여요.
- 고를 수 있는 조건(`agentUsable`): 런타임이 있고, 상태가 "준비됨" 또는 "로그인 필요"이고, 설정에서 끄지 않은 것. "설치 필요"와 "버전 낮음"은 막아요. "로그인 필요"는 파일로 추정한 거라 막지 않고 안내만 해요.

- **런타임:** Claude Code는 `runtimes: ["pty", "acp"]`예요. 채팅은 조건이 맞으면 ACP 어댑터(`claude-agent-acp@0.102.0`, `entry: dist/index.js`, `nodeMin: 22.0.0`, `devDirEnv: MAESTRO_ACP_CLAUDE_DIR`)로, 아니면 PTY로 돌아요. Antigravity는 `acp` 정보를 갖고 있지만 `runtimes`가 `["pty"]`라 PTY만 써요(`agy acp`는 R4 이후). Codex는 아직 실행할 수 없어요. `AgentDef::acp_adapter()`가 `runtimes`에 `acp`가 있을 때만 어댑터를 줘요. → [채팅 런타임](chat-runtime.md)

## 구현

**카탈로그** (`catalog.rs`)
- `AgentDef` 필드:
  - `id`, `name`, `logo`, `bin`(실행 파일 이름), `dirs`(PATH 다음에 찾아볼 설치 폴더, 홈 기준)
  - `install`: `Npm { package }` 또는 `Guide { url, text }`(Antigravity: 앱 설치 안내)
  - `login_cmd`: 설치 터미널이 설치 뒤 실행하는 명령(`claude`, `agy`, `codex login`)
  - `runtimes`: 지금 실행할 수 있는 런타임. Claude Code는 `[Pty, Acp]`, Antigravity는 `[Pty]`. 비어 있으면 실행 불가(Codex). `Pty`는 늘 폴백이라 `launchable()`은 `Pty`를 봐요
  - `acp`: ACP 어댑터 정보(패키지, 고정 버전, 인자, 실행 파일 경로를 넘기는 env, CLI 최소 버전, `entry`, `node_min`, `dev_dir_env`). 근거는 `docs/architecture/acp-runtime-research.md`
  - `version_args`(`--version`), `min_version`
  - `login`: 로그인 확인 방법(`Login::Claude` / `Codex` / `Antigravity`), `login_hints`: 시작 화면에서 로그인이 필요하다는 뜻의 글자(소문자)
  - `slash`(`Slash::Claude` / `None`), `slash_builtins`(이름, 설명, 안내)
  - `models`: `Fixed`(Claude 별칭 목록) 또는 `Command`(`agy models`)
  - `features`: `approval_hook`, `image_input`, `session_resume`, `model_command`(채팅 `/model` 목록), `assistant`(보조 모델로 쓸 수 있으면 기본 모델, `""`는 CLI 기본값)
- `get(id)`, `require(id)`, `default_id()`(첫 항목, 다른 정보가 없을 때의 기본 에이전트), `launchable()`, `npm_package()`
- 프런트로 넘기지 않는 필드(`dirs`, `login`, `login_hints`, `slash*`, `models`, `version_args`)는 `#[serde(skip)]`이에요.

**최소 버전 근거**
- Claude Code 2.0.54: `PermissionRequest` 훅이 2.0.45에 생겼고, 그 훅의 "항상 허용"(권한 갱신)이 2.0.54에 됐어요(Claude Code changelog, `~/.claude/cache/changelog.md`). `approval.rs`가 둘 다 써요.
- Antigravity 1.4.4: Windows에서 `--mode auto`가 이 버전에 생겼어요(`agy changelog`). `--add-dir`·`--conversation`이 언제 생겼는지는 확인 안 됨.
- 이 PC에서 확인한 버전: `claude` 2.1.302, `agy` 1.5.4.

**프런트** (`NewAgentDialog.tsx`)
- `AGENTS`: `loadAgents()`가 `agent_catalog`로 채워요. `main.tsx`가 첫 렌더 전에 기다려서, 이름·로고·선택 목록이 동기적으로 읽을 수 있어요.
- 화면은 `agentDef(id).runtimes.includes("acp")`로 채팅 런타임이 있는 에이전트를 알아요: Chat/CLI 전환이 `switch_runtime`을 거치고(AgentNode.tsx `useRuntimeSwitch`), 설정에 채팅 런타임 행이 생겨요(`ChatRuntimeRow`).
- `agentDef(id)`, `agentLabel(id)`, `agentLaunchable(id)`, `AgentLogo`(로고는 `import.meta.glob`으로 `src/assets/agents/*`를 읽어 카탈로그의 파일 이름으로 찾음), `STATUS_LABEL`, `agentUsable(id, cli, disabled)`
- 프런트에는 에이전트 id·이름·명령을 하드코딩하지 않아요. 예외는 설정 기본값 `assistantAgent: "claude-code"`(settings.ts) 하나예요.

**실행 파일과 실행** (`cli::command`)
- 실행할 때 이름(`claude`)이 아니라 찾은 경로로 실행해요. 찾는 방법은 `cli-install.md`에 있어요.
- npm `.cmd` shim 옆에 패키지의 `bin/<bin>.exe`가 있으면 그 exe를 바로 실행해요(Claude Code). 없으면 `cmd.exe /c call <shim>`이에요.
- 못 찾으면 시작하지 않고 "실행 파일을 찾지 못했어요"로 실패해요.
- `Agent::spawn`: 카탈로그에 없는 id → "알 수 없는 에이전트", 런타임이 없는 에이전트 → "아직 워크트리 에이전트로 실행할 수 없어요".

**PTY 에이전트:** CLI마다 다른 부분은 `PtyAgent` 트레이트(runtime.rs)예요: `launch`(argv, 세션, 브리프), `transcript`, `current_conversation`, `normalizer`(트랜스크립트 → 공통 이벤트), `preview`, `summarize`. `runtime::pty_agent(id)`가 고르고, `antigravity::ID`로 나누는 곳은 여기뿐이에요. → [에이전트 실행 방식](../../architecture/agent-runtime.md)

**Claude Code** (claude.rs `ClaudeAgent`, `argv`)
- `--permission-mode`, `--settings <승인 훅 파일>`, 시스템 브리프, `--model`을 넘겨요.
- 세션은 `--session-id` / `--resume`으로 정해요.
- 승인 훅 파일은 카탈로그 `features.approval_hook`(Claude만 `true`)을 Rust가 읽어서 써요.
- 트랜스크립트는 `ClaudeNormalizer`가 공통 이벤트로 바꿔요. `/clear` 뒤 새 세션을 따라가는 코드(SessionStart 훅)는 있지만 R3에서는 꺼 두었어요(`FOLLOW_SESSIONS`). 채팅은 처음 대화에 머물러요.

**Antigravity** (antigravity.rs `AgyAgent`)
- 데이터 폴더: `~/.gemini/antigravity-cli/`
- 권한 모드 매핑(`argv`):
  - `acceptEdits` → `--mode accept-edits`
  - `plan` → `--mode plan`
  - `auto` → `--mode auto`
  - `bypassPermissions` → `--dangerously-skip-permissions`
- 이어 하기는 `--conversation <id>`예요. agy는 대화 id를 미리 정할 수 없어요. 그래서 `cache/last_conversations.json`(작업 폴더 → 최근 대화)에서 워크트리 경로로 찾아요(`conversation_for`).
- `AgyNormalizer`가 `USER_INPUT` / `PLANNER_RESPONSE` / `GENERIC` / `SYSTEM_MESSAGE`를 Claude와 같은 공통 이벤트로 바꿔요. 생각만 있는 응답은 `thinking`으로 남기고, 모르는 단계는 `unknown`으로 담아요.
- 대화가 아직 없이 시작했으면 agy가 `last_conversations.json`에 기록하는 대로 그 대화를 가져와요(R3 전과 같아요). 그 뒤 더 최근 대화로 옮겨 가는 것은 따라가지 않아요(`FOLLOW_SESSIONS`, R3에서는 꺼짐).
- `GENERIC` 단계인데 기다리는 도구가 없으면 버리지 않고 `unknown`으로 담아요.
- 모델 목록은 카탈로그의 `Models::Command(["models"])`로 `agy models`에서 읽어요(`cli::models` → `agent_models`).
- 시스템 브리프: agy에는 시스템 프롬프트 옵션이 없어요. 그래서 Claude와 같은 브리프(`system_brief`)를 `<meta>.brief/AGENTS.md`로 쓰고, 그 폴더를 `--add-dir`로 넘겨요. agy는 작업 공간 폴더의 `AGENTS.md`/`GEMINI.md`를 규칙으로 읽어요(`agy -p`로 확인, 2026-11-01). 워크트리 안에는 아무 파일도 안 생겨요.
- 요약용 호출(`ask`)은 임시 폴더에서 실행해요. 그래야 그 대화가 워크트리 대화로 잡히지 않아요.

**보조 모델** (agent.rs `summarize` → `PtyAgent::summarize`): 카탈로그 `features.assistant`가 있는 에이전트만 돼요. 모델이 비어 있으면 그 기본 모델(Claude는 `haiku`)을 써요.

## 제약과 알려진 문제

- Codex 어댑터(트랜스크립트 읽기, argv)는 아직 없어요.
- Antigravity는 승인 훅이 없어서 권한 확인을 CLI 뷰에서 답해야 해요. `maestro` 명령 자동 허용 규칙도 넘기지 못해요.
- 브리프 규칙 폴더를 대화형 실행(TUI)에서 읽는지, 그 폴더에 신뢰 확인을 묻는지는 실제로 확인하지 않았어요. 확인한 건 `-p` 실행뿐이에요.
- Antigravity의 `login_hints`는 실제로 로그아웃된 상태에서 본 문구가 아니에요. 확인 안 됨.
- `maestro worktree create`의 도움말(`control.rs` `ROUTES`)에는 `--agent claude-code|antigravity`가 글자로 적혀 있어요. 기본값은 `catalog::default_id()`를 써요.
- `features.image_input`은 모두 `false`예요. 채팅에 이미지 입력이 아직 없어요.

## 결정 기록

- Gemini CLI를 넣으려다 서비스 종료를 확인하고 Antigravity CLI로 바꿨어요.
- 2026-11-10 (PRD R2): 정의가 `cli.rs`, `agent.rs`, `NewAgentDialog.tsx`, `SettingsDialog.tsx`, `slashCommands.ts`에 흩어져 있던 걸 `catalog.rs` 하나로 모았어요. 카탈로그는 정적 데이터만 두고, 탐지(`cli.rs`)와 실행(`agent.rs`, `antigravity.rs`)이 읽어 써요. trait로 런타임을 추상화하는 건 R3에서 해요.
- 로고 파일은 프런트에 두고(번들 자산), 카탈로그에는 파일 이름만 둬요.
- 최소 버전은 Maestro가 넘기는 플래그·훅이 생긴 버전으로 정했어요. 근거를 못 찾은 에이전트(Codex)는 막지 않아요(Paradev `acpCatalogue.ts`의 "추측으로 막지 않는다"와 같은 방침).
