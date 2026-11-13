---
title: 슬래시 명령, bash 모드, 모델 선택
area: agents
status: done
files: [src-tauri/src/catalog.rs, src-tauri/src/commands.rs, src-tauri/src/cli.rs, src/slashCommands.ts, src/nodes/AgentNode.tsx, src-tauri/src/agent.rs]
related: [features/agents/chat-runtime.md, features/agents/chat-view.md, features/agents/agent-adapters.md, architecture/agent-runtime.md]
updated: 2026-11-13
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
  - 고를 수 있는 모델: `default`, `best`, `opus`, `sonnet`, `haiku`, `fable`, `opusplan`, `opus[1m]`, `sonnet[1m]`
  - 고르면 `/model <이름>`으로 보내요. `/model`만 치고 보내면 목록을 열어요.
- **bash 모드:** `!`로 시작하면 입력창이 bash 모양(테두리 색, 고정폭 글꼴)으로 바뀌고, 명령이 셸에서 실행돼요.

- **채팅 런타임(ACP)에서:** 어댑터가 받는 명령 목록(`available_commands_update`)에 없는 기본·사용자 명령에는 "채팅에서는 안 돼요 · 터미널에서" 메모가 붙고, 보내면 거절돼요(오류 글은 아직 "CLI 보기에서 실행해 주세요"예요). `/model 이름`은 Maestro가 `session/set_config_option`으로 바꿔요(턴이 생기지 않아요). 어댑터만 아는 명령(MCP 프롬프트 등)은 목록 끝에 "에이전트" 출처로 붙어요. 입력창은 런타임이 바뀔 때마다(`Worktree.runtime`, 전환·폴백 뒤), 그리고 어댑터가 목록을 보낼 때마다(`agent://commands`: 목록은 세션이 열린 뒤에 와서, 첫 요청에는 아직 없을 때가 많아요) 목록을 다시 받아요. → [채팅 런타임](chat-runtime.md)

## 구현

- 정의: 카탈로그 `AgentDef`의 `slash`(`Slash::Claude` / `Slash::None`), `slash_builtins`(이름, 설명, 안내), `models`(`Models::Fixed` 별칭 목록)
- 목록: `commands::list(def, home, worktree)` → `list_slash_commands(worktreePath, adapterId)` → `ipc.slashCommands(path, adapterId)`
  - 기본 명령이 먼저(카탈로그 순서, 출처 "기본"), 그다음 `Slash::Claude`면 `claude_custom`(이름순, 같은 이름은 프로젝트가 이김)
  - `SlashCommand { name, description, source, note? }`
- 모델: `cli::models(def)` → `agent_models(id)` → `ipc.agentModels(id)`의 `{ id, label, description }`. 채팅은 `description`을, 설정의 기본 모델 선택은 `label`을 써요.
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

- 2026-11-13 (UI 구조 재구성 단계 12): 메뉴 메모의 보기 이름을 용어 표대로 "터미널"로 바꿨어요(`OPENS_IN_CLI`, `CHAT_CANT`). 백엔드 오류 글은 그대로예요.
- 2026-11-10 (PRD R2): 기본 명령과 모델 목록을 `slashCommands.ts`에서 카탈로그로 옮기고, 목록을 에이전트별로 받게 했어요. 전에는 Antigravity 채팅에도 Claude의 `.claude/commands`·스킬이 보였어요.
