---
title: 슬래시 명령, bash 모드, 모델 선택
area: agents
status: done
files: [src-tauri/src/catalog.rs, src-tauri/src/commands.rs, src-tauri/src/cli.rs, src/slashCommands.ts, src/nodes/AgentNode.tsx, src-tauri/src/agent.rs]
related: [features/agents/chat-runtime.md, features/agents/chat-view.md, features/agents/agent-adapters.md, architecture/agent-runtime.md]
updated: 2026-11-18
---

# 슬래시 명령, bash 모드, 모델 선택

## 요약

채팅 입력창에서 `/`를 치면 그 워크트리 에이전트의 명령 목록이 떠요. 목록은 에이전트마다 카탈로그(`catalog.rs`)에서 와요. Claude Code는 기본 명령과 프로젝트·사용자의 사용자 정의 명령, 스킬이 보이고, Antigravity에는 Claude 목록이 보이지 않아요. `!`로 시작하면 bash 모드가 돼요. `/model`은 TUI에만 있는 선택 화면 대신 채팅용 모델 목록을 보여 줘요(그 기능이 있는 에이전트만).

## 동작

- **`/` 메뉴**
  - 이름으로 걸러지고, 방향키와 Enter로 골라요.
  - 터미널에서만 열리는 명령(`/config`, `/mcp`, `/permissions` 등)에는 "터미널에서 열려요" 안내가 붙어요.
  - `/clear`는 새 세션이 시작돼서 채팅 뷰가 이어지지 않는다고 알려 줘요.
- **에이전트별 목록**
  - Claude Code: 기본 명령 + `.claude/commands/**` 명령 + `.claude/skills/*/SKILL.md` 스킬. 프로젝트(워크트리)와 사용자(`~/.claude`) 둘 다 읽어요.
  - Antigravity: 비어 있어요. agy 자체 명령·스킬은 아직 읽지 않아요.
  - Codex: 워크트리 에이전트로 실행할 수 없어서 해당 없음.
- **`/model`** (카탈로그 `features.model_command`가 켜진 에이전트, 지금은 Claude Code만)
  - 고를 수 있는 모델: Claude Code에 직접 물어 온 목록이에요(아래 "모델 목록"). 계열마다 가장 새 버전 하나씩, CLI가 부르는 이름으로 나와요. 예(Claude Code 2.1.302, 2026-11-18): `opus` Opus 5.5 · `claude-fable-5-2` Fable 5.2 · `sonnet` Sonnet 5.5 · `haiku` Haiku 5.5. 줄마다 보낼 값(굵게)과 그 이름이 나와요. 목록에 없는 것(`default`, `opusplan`, `opus[1m]`, 예전 버전)은 `/model <값>`으로 직접 치면 돼요
  - 고르면 `/model <이름>`으로 보내요. `/model`만 치고 보내면 목록을 열어요.
- **bash 모드:** `!`로 시작하면 입력창이 bash 모양(테두리 색, 고정폭 글꼴)으로 바뀌고, 명령이 셸에서 실행돼요.

- **채팅 런타임(ACP)에서:** 어댑터가 받는 명령 목록(`available_commands_update`)에 없는 기본·사용자 명령에는 "채팅에서는 안 돼요 · 터미널에서" 메모가 붙고, 보내면 거절돼요(오류 글은 아직 "CLI 보기에서 실행해 주세요"예요). `/model 이름`은 Maestro가 `session/set_config_option`으로 바꿔요(턴이 생기지 않아요). 어댑터만 아는 명령(MCP 프롬프트 등)은 목록 끝에 "에이전트" 출처로 붙어요. 입력창은 런타임이 바뀔 때마다(`Worktree.runtime`, 전환·폴백 뒤), 그리고 어댑터가 목록을 보낼 때마다(`agent://commands`: 목록은 세션이 열린 뒤에 와서, 첫 요청에는 아직 없을 때가 많아요) 목록을 다시 받아요. → [채팅 런타임](chat-runtime.md)

## 구현

- 정의: 카탈로그 `AgentDef`의 `slash`(`Slash::Claude` / `Slash::None`), `slash_builtins`(이름, 설명, 안내), `models`(`Models::Claude`: CLI에 묻고, 못 물으면 쓰는 별칭 목록 / `Models::Command`: CLI 명령의 출력)
- 목록: `commands::list(def, home, worktree)` → `list_slash_commands(worktreePath, adapterId)` → `ipc.slashCommands(path, adapterId)`
  - 기본 명령이 먼저(카탈로그 순서, 출처 "기본"), 그다음 `Slash::Claude`면 `claude_custom`(이름순, 같은 이름은 프로젝트가 이김)
  - `SlashCommand { name, description, source, note? }`
- 모델 목록: `cli::models(def)` → `agent_models(id)` → `ipc.agentModels(id)`의 `{ id, label, listed }`. `id`는 `--model`과 `/model`이 받는 값, `label`은 이름, `listed`는 고르는 곳에 내놓는지예요(내놓지 않는 것도 이름은 줘서, 그 값이 저장된 에이전트의 칩이 이름으로 보여요).
  - **Claude Code**(`Models::Claude`): `claude::models(exe)`가 CLI를 머리 없이 띄워(`-p --output-format stream-json --input-format stream-json --verbose --no-session-persistence --setting-sources local --strict-mcp-config --disable-slash-commands`, 임시 폴더에서) 제어 채널의 첫 메시지 `{"type":"control_request","request":{"subtype":"initialize"}}`를 보내요. CLI는 프롬프트를 받기 전에 `control_response`로 답하고(모델에게 묻는 것이 없어서 비용이 없어요) 입력이 끝나면 나가요. 답의 `response.response.models[]`에서 `value`와 `displayName`을 읽어요(`models_in`). 12초 안에 답이 없으면 그만둬요. 설정·훅·MCP·슬래시 명령을 읽지 않게 해서 사용자의 훅이 돌지 않아요.
  - `cli::offered(list)`가 그 목록에서 내놓을 것을 골라요: 이름이 "<계열> <버전>" 꼴인 것 중 계열마다 가장 높은 버전 하나(같은 버전이 둘이면 앞의 것 = 별칭). "Default (recommended)", "(1M context)", plan mode처럼 그 꼴이 아닌 것은 내놓지 않아요.
  - 답은 10분 동안 기억해요(`claude_models`). 못 물었으면(미설치, 로그인 안 됨, 질문을 모르는 옛 CLI) 1분 뒤에 다시 묻고, 그동안은 카탈로그의 별칭 목록을 버전 없이 써요(Opus · Sonnet · Haiku · Fable).
  - 화면: 설정과 대화상자의 모델 선택(`useModels`)은 `listed`만, 입력창의 모델 칩(`modelsOnce`, 앱이 켜진 동안 에이전트마다 한 번)은 전부를 받아 이름을 찾고 메뉴에는 `listed`만 내요. 채팅의 `/model` 목록도 `listed`만이에요.
- `Composer`(AgentNode.tsx): 워크트리의 `adapterId`로 두 목록을 받고, `agentDef(adapterId).features.modelCommand`일 때만 `/model ` 목록을 띄워요.
- `matchCommands`(slashCommands.ts): 이름 앞부분이 맞는 것 먼저, 그다음 이름·설명에 들어 있는 것
- 보낼 때의 처리는 `Agent::send`가 해요.
  - `/명령`만 있으면 뒤에 공백을 붙여요. TUI 자동완성 때문에 `/model/model`이 되던 문제를 막아요.
  - `!`는 먼저 키로 입력해요. 붙여넣은 `!`로는 bash 모드가 켜지지 않아요.
- 명령 결과(`<local-command-stdout>`)와 bash 입출력(`<bash-input>`, `<bash-stdout>`)은 `to_events()`가 `local_command` 이벤트로 바꿔서 채팅에 보여 줘요.
- 테스트: `lists_namespaced_commands_and_skills`(Antigravity 목록이 비는지 포함), `slash_lists_are_per_agent`, `lists_models_per_agent`

## 제약과 알려진 문제

- 선택 화면이나 대화상자를 여는 명령은 채팅에서 결과가 안 보여요. CLI 보기로 가야 해요.
- Antigravity 명령 목록은 비어 있어요. agy는 `~/.gemini/antigravity-cli/skills`, `~/.gemini/config/skills`의 스킬과 자체 `/` 명령이 있지만(Orca `antigravity-chat-user-skills.ts`, agy `--disable-slash-commands` 도움말), 어떤 것이 `/`로 불리는지 확인하지 않았어요.

## 결정 기록

- 2026-11-18 (사용자: "모델명이 좀 이상한데?" → 고른 답: "버전이 없음", "Best · Opus Plan · 1M 항목"): 모델 목록을 카탈로그에 적어 둔 별칭 대신 Claude Code에 직접 물어 와요. 별칭이 어느 버전을 뜻하는지는 설치된 CLI마다 달라서(이 컴퓨터의 2.1.302에서 `fable`은 Fable 5.1, 기본값은 Fable 5.2), 앱에 버전을 적어 두면 틀린 이름이 나오기 때문이에요. 그날 앞서 제가 지어 붙였던 이름(Best, Opus Plan, Opus 1M, Sonnet 1M)은 없앴어요. 계열마다 가장 새 버전 하나만 내놓는 것은 제 판단이에요(사용자가 고른 답의 "Opus · Sonnet · Haiku · Fable만 남겨요"를 버전이 여럿인 목록에 맞춘 것).

- 2026-11-13 (UI 구조 재구성 단계 12): 메뉴 메모의 보기 이름을 용어 표대로 "터미널"로 바꿨어요(`OPENS_IN_CLI`, `CHAT_CANT`). 백엔드 오류 글은 그대로예요.
- 2026-11-10 (PRD R2): 기본 명령과 모델 목록을 `slashCommands.ts`에서 카탈로그로 옮기고, 목록을 에이전트별로 받게 했어요. 전에는 Antigravity 채팅에도 Claude의 `.claude/commands`·스킬이 보였어요.
