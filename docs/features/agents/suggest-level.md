---
title: 캔버스 추천 (추천 수준, 끔 · 보통 · 적극 · 알아서)
area: agents
status: done
files: [src-tauri/src/agent.rs, src-tauri/src/main.rs, src-tauri/src/control.rs, src-tauri/seed/playbooks/default.md, src/agents.tsx, src/nodes/RunPicker.tsx, src/nodes/AgentNode.tsx, src/NewAgentDialog.tsx, src/AgentSettingsDialog.tsx, src/agentForm.ts, src/agentActions.ts, src/store.ts, src/types.ts, src/ipc.ts, src/chatAsks.ts, src/nodes/ChoiceCard.tsx, src/nodes/VisualCard.tsx, src/styles.css, scripts/agent-form-check.ts]
related: [features/agents/agent-settings.md, features/agents/chat-view.md, features/agents/chat-choices.md, features/agents/chat-visuals.md, features/agents/approvals.md, features/orchestration/maestro-cli.md, features/orchestration/playbook-pick.md, features/ui/terms.md, reference/storage-layout.md]
updated: 2026-11-24
---

# 캔버스 추천 (추천 수준)

## 요약

에이전트마다 Maestro만의 방식(캔버스에 그리기, 캔버스에 묻기, 보기 블록, 채팅 선택지, 런북 권하기)을 얼마나 먼저 쓰는지 정하는 설정이에요: 끔 · 보통(기본) · 적극 · 알아서. 에이전트 CLI 자체의 권한 모드(권한)와는 따로예요. 값은 에이전트의 브리프에 들어가서, 켜져 있는 에이전트는 바꾸면 같은 대화로 다시 시작해요.

## 동작

**수준**

| id | 이름 | 에이전트가 하는 것 |
|---|---|---|
| `off` | 끔 | 보통의 코딩 에이전트예요. 그리기, 캔버스에 놓기·묻기, 채팅 선택지, Maestro 방식 권하기를 스스로 하지 않고, 사용자가 청할 때만 해요(그려 달라, 캔버스에 올려 달라, 런북 이름). 필요한 것은 답 글에서 물어요 |
| `normal` | 보통 (기본) | 브리프 그대로예요: 큰 단계(계획 세우기, 설계 설명, 여러 결정 모으기, 일 나누기)에서 Maestro 방식을 채팅 선택지로 권해요 → [채팅 선택지](chat-choices.md) |
| `eager` | 적극 | 큰 단계에 더해, 설명할 것(구조, 흐름, 비교, 시간에 따른 상태)이 읽기보다 보는 게 나으면 먼저 그려 줄지 권해요. 짧은 대화나 작은 수정에는 권하지 않고, 그냥 진행하라고 하면 그 대화에서 다시 권하지 않아요 |
| `auto` | 알아서 | 권하지 않고 바로 해요: 구조·흐름·비교·상태를 설명할 때 캔버스에 그리고(`--say`로 한 걸음씩), 봐야 하는 결과는 보기 블록으로 띄우고, 결정이 여럿 모이면 캔버스에 물어요. 원하지 않는 것은 사용자가 버려요(버렸다는 메모가 가요). 비용이 드는 것(워커 만들기, 사용자가 말한 방식 대신 런북 따르기)은 여전히 먼저 물어요(`maestro ask --in chat --suggest true`). 짧은 대화와 작은 수정은 그대로예요 |

**고르는 곳**

- **입력창 칩**: `<권한 짧은 이름> 추천 <수준>` 칩 하나예요(예: "수정 자동 추천 보통"). "추천 …"은 흐린 글자(모델 칩의 추론 수준처럼)이고, 권한 이름은 모두 허용일 때만 경고색이에요. 아이콘과 빗금은 없어요. 누르면 두 칸 팝업 **권한 | 캔버스 추천**이 떠요(모델 칩의 모델 | 추론 수준과 같은 팝업). 왼쪽을 누르면 ✓만 옮겨 가고 팝업은 남고, 오른쪽을 누르면 닫혀요. 고른 것은 닫힐 때 한 번에 저장돼요(둘 다 바꿔도 한 번만 다시 시작). 입력창 폭이 340px보다 좁으면 "추천 …"은 숨고 권한 이름만 남아요. → [채팅 뷰](chat-view.md) "입력창"
- **터미널 보기의 헤더**: 권한 칩에는 권한 이름만 보이고, 누르면 같은 두 칸 팝업이 떠요.
- **새 에이전트 창**: 에이전트 · 권한 · 캔버스 추천 · 모델 한 줄. 처음 값은 보통이고, 워커로 여는 창은 부모의 수준이에요. → [에이전트 만들기](../worktrees/create-worktree.md)
- **에이전트 설정 창**: 권한 · 캔버스 추천 · 모델 · 추론 수준 한 줄. → [에이전트 설정](agent-settings.md)
- **바꾸면**: 켜져 있는 에이전트는 권한을 바꿀 때처럼 같은 대화로 다시 시작해요(`applied: "restart"`. 쉬는 중이면 바로, 일하는 중이면 턴이 끝난 뒤). 꺼져 있으면 다음 시작부터예요.
- **워커**: `maestro worktree create`로 만든 워커는 만든 에이전트의 수준을 물려받아요(권한처럼). 에이전트 붙이기와 프로젝트 가져오기로 붙인 에이전트는 보통이에요.

**추천 카드**

- Maestro가 자기 방식을 권하는 카드는 배지가 "추천"이에요: 채팅 창구의 그림 카드(→ [채팅 창구](chat-visuals.md)), `--suggest true`로 띄운 채팅 선택지.
- 모양: 색 없는 가는 테두리에 왼쪽 3px 흐린 막대, 흐린 배지, 패널 바탕. 사방이 경고색 테두리인 승인 카드(권한)와 구분돼요. 에이전트의 보통 선택지는 그대로예요(배지 "선택", 강조색 테두리).

## 구현

- 저장: meta `suggest`(`AgentMeta.suggest`: `"off"` / `"eager"` / `"auto"`, 없으면 보통). `agent::SUGGEST_LEVELS` = `["off", "normal", "eager", "auto"]`. `suggest_level(s)`: `""`와 `"normal"`은 `None`(저장하지 않음), 목록 밖은 "없는 추천 수준이에요: …" 오류.
- 브리프: `system_brief`가 `MAESTRO_HINT` 바로 뒤에 `suggest_hint(level)`을 붙여요. `SUGGEST_OFF` / `SUGGEST_EAGER` / `SUGGEST_AUTO`가 각각 `## 추천 수준: 끔` / `적극` / `알아서` 절이에요. 보통은 아무것도 붙이지 않아요(`MAESTRO_HINT`가 보통 기준으로 쓰여 있어요). 끔 절은 위의 권하기 지침보다 앞선다고 말해요.
- 바꾸기: `update_agent` 패치의 `suggest`(`AgentPatch.suggest`, `""` = 보통) → `AgentMeta::patched`가 `suggest_level`로 확인. `agent::applied`가 보는 시작 값에 `suggest`가 들어 있어서 켜진 에이전트면 `restart`예요.
- 만들기: `create_agent`·`create_maestro`·`attach_agent` 요청의 `suggest`(선택, `""` = 보통)를 `suggest_level`로 확인해 meta에 써요(`create_worktree`). 에이전트 붙이기(`AttachAgent.tsx`)와 가져오기(`Sidebar.tsx`)는 보내지 않아요.
- 워커: control.rs `worktree/create`가 호출자 meta의 `suggest`를 `CreateAgentReq.suggest`로 넘겨요.
- 목록: `WorktreeInfo.suggest`(`null` = 보통) → 스토어 `Worktree.suggest` → `suggestOf(id)`(agents.tsx: 없거나 모르는 값은 `"normal"`). 화면 이름은 `SUGGEST_NAMES`(`label`: 끔 · 보통 · 적극 · 알아서, `short`: 칩의 "추천 …"), 순서는 `SUGGEST_LEVELS`.
- 칩과 팝업: `useRunMenus`(nodes/RunPicker.tsx)의 `perm(el)`이 `RunPanel`을 `kind: "perm"`으로 열어요(왼쪽 "권한" = `permissionsFor`, 오른쪽 "캔버스 추천" = `SUGGEST_LEVELS`). 닫힐 때 `closed("perm", [mode, level])`가 바뀐 것만 `changeAgent(wt, { permissionMode?, suggest? })`로 저장해요. `Composer`의 칩(`.composer-chip[data-kind="perm"]`)은 `.composer-chip-label`(권한, `data-warn`)과 `.composer-chip-sub`(`suggestNow`의 `short`)예요. `@container (max-width: 339px)`가 `.composer-chip-sub`를 숨겨요.
- 대화상자: `NewAgentDialog`의 `suggest`(처음 값 `suggestOf(부모의 suggest)`) → `ipc.createAgent` / `createMaestro`. `AgentSettingsDialog`는 `agentPatch`(agentForm.ts)의 `suggest`. 바뀌면 `restart: true`.
- 추천 카드: control.rs `ask`가 `--in chat`이고 `--suggest true`(`true`·`1`·`yes`)면 질문에 `"suggest": true`를 넣어요. 캔버스 질문에서는 무시돼요. `ChatAsk.suggest`(chatAsks.ts) → `ChoiceCard`가 `suggest-card` 클래스와 배지 "추천". `VisualCards`는 늘 `suggest-card`, 배지 "추천". CSS: `.approval.suggest-card`(`--mae-border` 테두리, 왼쪽 3px `--mae-line-strong`, `--mae-panel` 바탕), `.suggest-card .approval-badge`(`--mae-muted`).
- 지침: `MAESTRO_HINT`의 권하기 명령과 기본 런북(`seed/playbooks/default.md` "흐름을 권하고 고르게 한다")이 `--suggest true`를 써요. 런북은 권하는 정도가 추천 수준을 따른다고(끔이면 권하지 않는다) 말해요.
- 시험: agent.rs `a_suggest_level_is_saved_changed_and_told_in_the_brief`, `scripts/agent-form-check.ts`(`npm test`).

## 제약과 알려진 문제

- 화면은 확인 안 됨(2026-11-24).
- 수준은 브리프의 지침이에요. 앱이 명령을 막지 않아서, 끔이어도 에이전트가 `maestro draw`를 부르면 그려져요.
- 알아서로 그린 것도 설정 "추천 그림"을 따라요: 눌러서면 캔버스의 안내나 채팅의 '캔버스에 올리기'를 기다려요.
- 새 에이전트의 기본값을 정하는 앱 설정은 없어요(늘 보통, 워커만 부모 수준).

## 결정 기록

- 2026-11-24 (사용자: "시각화라던가 우리 마에스트로의 독점적인 캔버스 사용 기능은 허용이 아니라 추천이라는 것으로 흐름을 좀 나누자. 뭐 허용은 퓨어 에이전트의 기능인데"): 에이전트 CLI의 권한(허용)과 Maestro만의 방식을 나눴어요. Maestro 방식을 얼마나 먼저 쓰는지를 에이전트마다 추천 수준으로 두고, 설정의 "시각화 허용 [자동 | 눌러서]"는 "추천 그림 [바로 | 눌러서]"로 이름을 바꿨어요(키 `guideMode`와 값은 그대로).
  - "에이전트 처음 만들 때도 있어야": 새 에이전트 창에도 칸을 뒀어요. 워커는 권한처럼 만든 에이전트의 수준을 물려받아요.
  - "적극 다음 단계까지 넣는 건 어때?": 알아서를 더했어요. 더 자주 권하는 것이 아니라 묻지 않고 하는 단계예요. 그린 것은 사용자가 버릴 수 있어서 묻지 않고, 비용이 드는 것(워커 만들기, 사용자가 말한 방식 대신 런북 따르기)은 계속 물어요.
  - 보통은 브리프에 아무것도 더하지 않아요. 기존 브리프가 보통 기준으로 쓰여 있어서, 지금까지 쓰던 에이전트가 그대로 돌아요.
  - "추천을 선택하는 시각적으로 보여주는 방식도 허용과 조금의 차별점을 주고": 추천 카드를 승인 카드(경고색 테두리)와 다르게 했어요. 브리프와 기본 런북의 권하기 명령에 `--suggest true`를 붙여, 에이전트 자신의 질문과 Maestro의 추천이 채팅에서 달라 보여요.
  - "이상한 민트색 말고 추론 수준처럼 연하게": 추천에 청록색을 써 봤다가 뺐어요. 칩의 "추천 …"은 추론 수준처럼 흐린 글자이고, 추천 카드는 색 없이 흐린 테두리·왼쪽 막대·흐린 배지예요.
  - "판단 수준의 방패 아이콘도 빼자", "왼쪽에 있는 모델 선택과 비슷한 ui로 한번에 팝업이 하나로", "슬래쉬 없애자": 권한 칩의 방패를 빼고, 권한과 추천을 칩 하나와 두 칸 팝업 하나로 합쳤어요(모델 칩과 같은 팝업). 칩 글은 빗금 없이 "<권한> 추천 <수준>"이에요. 좁은 입력창에서는 "추천 …"을 숨기고 권한 이름을 남겨요.
  - "추천이라고만 쓰니까 조금 불친절하다. 캔버스라는 단어를 넣을까? 팝업에만?": 팝업의 칸 이름과 두 대화상자의 칸 이름을 "캔버스 추천"으로 했어요. 칩은 자리가 없어 "추천 …" 그대로예요.
