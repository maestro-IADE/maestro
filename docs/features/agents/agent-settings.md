---
title: 에이전트 설정 (만든 뒤 이름·설명·권한·모델 바꾸기)
area: agents
status: done
files:
  - src/AgentSettingsDialog.tsx
  - src/agentForm.ts
  - src/agents.tsx
  - src/store.ts
  - src/Workspace.tsx
  - src/nodes/AgentNode.tsx
  - src/Sidebar.tsx
  - src/StageTabs.tsx
  - src/ui/useShortcuts.ts
  - src/ipc.ts
  - src/types.ts
  - scripts/agent-form-check.ts
  - src-tauri/src/main.rs
  - src-tauri/src/agent.rs
  - src-tauri/src/catalog.rs
  - src-tauri/src/control.rs
related:
  - features/worktrees/create-worktree.md
  - features/worktrees/branch-selection.md
  - features/agents/approvals.md
  - features/agents/chat-view.md
  - features/orchestration/maestro-agent.md
  - features/ui/side-panels.md
  - features/ui/stage-tabs.md
  - features/ui/shortcuts.md
  - architecture/agent-runtime.md
  - reference/tauri-commands.md
updated: 2026-11-13
---

# 에이전트 설정 (만든 뒤 이름·설명·권한·모델 바꾸기)

## 요약

이미 만든 에이전트의 이름, 설명, 권한, 모델(마에스트로는 이름 대신 워커 기준 브랜치)을 에이전트 설정 창(`AgentSettingsDialog`)에서 바꿔요. 저장은 바로 메타 파일에 써요(`update_agent`). 켜져 있는 에이전트는 시작할 때의 권한·모델로 계속 돌기 때문에, 쉬는 중이면 **저장하고 다시 시작**으로 같은 대화를 이어 가며 다시 띄워요(`restart_agent`). 작업 중이면 저장만 하고, 작업이 끝난 뒤 다시 시작할 수 있어요.

## 동작

- **여는 곳**(모두 스토어 `openAgentSettings({ path, focus? })`, 창마다 `WindowLayers`의 `AgentSettingsHost`가 그려요. 메인 창과 분리 창 둘 다):
  - 에이전트 헤더의 권한 칩(권한 칸에 포커스), 모델 칩(모델 칸에 포커스). 칩은 이제 버튼이에요.
  - 에이전트 헤더 ⋯ 메뉴 첫 항목 "에이전트 설정…".
  - 사이드바 행 메뉴(⋯, 우클릭) "에이전트 설정…"(오른쪽에 `F2`), 그리고 사이드바 행에 포커스가 있을 때 **F2**(이름 칸에 포커스, 글 전체 선택). 프로젝트 머리와 에이전트 없는 행에서는 F2가 아무것도 하지 않아요.
  - 탭 우클릭 "에이전트 설정…"(에이전트가 붙은 탭만).
  - (단계 9 예정) 설정 → 프로젝트의 "마에스트로 설정…".
- **창**(폭 480, 안쪽 24, 필드 사이 16):
  - 제목: `Settings2` + "<이름> 설정". 마에스트로는 "마에스트로 설정".
  - 바꿀 수 있는 것: 이름 [선택](마에스트로는 숨김, 최대 60자, 비우면 브랜치 이름), 설명 [선택], 권한(그 에이전트가 받는 모드만, `permissionsFor`) · 모델("기본 (CLI 기본값)" + 그 에이전트의 모델 목록) 한 줄, 권한 설명 한 줄(모두 허용은 warn 색), (마에스트로만) 워커 기준 브랜치.
  - 읽기 전용 두 칸 표(12px): 에이전트 종류(로고 + 이름), 브랜치, 폴더(복사 버튼), 상위(워커면 부모 이름).
  - 바닥 위 안내 12px muted: "권한과 모델은 에이전트를 다시 시작하면 적용돼요". 마에스트로의 권한을 바꾸고 있으면 "이 뒤에 만드는 워커가 새 권한을 물려받아요. 이미 있는 워커는 그대로예요".
  - 버튼: [취소] [저장]. 쉬는 중(IDLE)이고 다시 시작이 필요한 것(권한·모델·마에스트로 기준 브랜치)을 바꿨으면 [저장하고 다시 시작](주)이 더해지고 [저장]은 보조가 돼요. 작업 중(RUNNING)이고 그런 것을 바꿨으면 바닥 왼쪽에 "작업이 끝나면 다시 시작할 수 있어요". 바뀐 것이 없으면 [저장]이 꺼져 있어요.
  - 키: Ctrl+Enter = 주 버튼, Esc = 닫기(저장 중 제외), 한 줄 입력칸의 그냥 Enter는 아무것도 하지 않아요. 한글 조합 중에는 키를 보지 않아요.
- **결과**(`update_agent`의 `applied`):
  - `saved`(꺼져 있거나 이름·설명만 바뀜): 토스트 없이 닫고, 돌려받은 `info`로 사이드바·탭·헤더가 바로 바뀌어요.
  - `restart`(켜져 있는데 권한·모델·기준 브랜치가 바뀜), [저장]을 눌렀을 때: 창을 닫지 않고 바닥에 "저장했어요. 다시 시작하면 적용돼요" + [닫기] + (쉬는 중이면) [지금 다시 시작]. 필드는 저장한 값으로 잠겨요. 에이전트가 그 사이 쉬는 중이 되면 [지금 다시 시작]이 나타나요.
  - [저장하고 다시 시작] 또는 [지금 다시 시작]: 다시 시작이 성공하면 "다시 시작했어요"를 잠깐(0.9초) 보이고 닫아요. 저장은 됐는데 다시 시작이 거절되면(그 사이 작업을 시작함, 승인 대기, 보기 전환 중, 백그라운드 명령) 오류 상자에 백엔드 이유를 보이고 "저장했어요" 상태로 바뀌어요.
  - 다시 시작하는 동안 그 에이전트의 화면이 잠깐 부팅 화면으로 바뀌는 것은 정상이에요 → [에이전트 런타임 "다시 시작"](../../architecture/agent-runtime.md).
- **마에스트로 권한 상속**: 마에스트로가 `maestro worktree create`로 만드는 워커는 마에스트로의 권한을 물려받아요(`control.rs`). 워커의 에이전트가 그 권한을 받지 않으면 그보다 느슨하지 않은 가장 가까운 권한이에요(예: "매번 묻기" 마에스트로의 Antigravity 워커는 "계획만"). 이 창에서 마에스트로 권한을 바꾸면 그 뒤에 만드는 워커부터 새 권한이고, 이미 있는 워커는 그대로예요(각 워커의 권한은 그 워커의 설정 창에서 바꿔요).
- **모델 이름**: 영문자·숫자와 `. _ : [ ] / @ -`만 되고 `-`로 시작할 수 없어요. 다른 글자가 있으면 저장하지 않고 "모델 이름에 쓸 수 없는 글자가 있어요: …"가 떠요(`agent::check_model`).
- 창이 열린 사이 에이전트가 없어지면(다른 곳에서 폐기) 창이 저절로 닫혀요.

## 구현

- `AgentSettingsDialog.tsx`: `AgentSettingsDialog({ target, onClose })`가 `useWorktree`로 에이전트를 찾고 `Form`을 그려요. 처음 값(`start`)은 열 때 한 번 잡아요(`Worktree.name`이 브랜치와 같으면 이름이 없던 것으로 보고 빈 칸 + 브랜치 안내).
- 보낼 것: 순수 `agentPatch(was, now, maestro)`(`src/agentForm.ts`) → `{ patch, changed, restart }`. 바뀐 필드만 `patch`에 넣어요(공백 정리, 마에스트로 이름 제외, 기준 브랜치는 마에스트로만). 빈 문자열은 지우기(이름 → 브랜치 이름, 모델 → CLI 기본값). 시험은 `scripts/agent-form-check.ts`(`npm test`).
- `ipc.updateAgent({ projectRoot, worktreePath, ...patch })` → `{ info, applied }` → `upsert(root, info)`. `ipc.restartAgent(projectRoot, worktreePath)`.
- 다시 시작 버튼은 `wt.status === "IDLE"`일 때만(스토어 상태라 창이 열린 동안 바뀌면 따라가요). 실제 거절 판단은 백엔드 `can_restart`와 `Agent::restart`가 해요.
- 스토어: `agentSettings: { path, focus?: "name" | "permission" | "model" } | null`, `openAgentSettings`, `closeAgentSettings`. 호스트는 `Workspace.tsx` `AgentSettingsHost`(키 = 경로 + 포커스라 다른 입구에서 다시 열면 새로 그려요).
- F2: 단축키 표의 `rename`(`shortcuts.ts`, 범위 `sidebar` = 사이드바 목록 안에서만). `ui/useShortcuts.ts`가 포커스된 요소에서 `[data-agent-path]`(에이전트가 붙은 세션 행만 가짐)를 찾아 `openAgentSettings({ path, focus: "name" })`.
- 헤더 칩: `nodes/AgentNode.tsx` `HeadChips`. 권한 칩(`permissionOf`의 짧은 이름, 모두 허용은 warn), 모델 칩(모노, 값 없으면 "기본 모델", 최대 160px 말줄임), 마에스트로 병합 칩(누르면 `openSettings({ section: "projects", root })`), 폴백 칩. 블록 폭 420px 미만이면 모두 숨겨요(컨테이너 쿼리).
- 창 순서: `WindowLayers`는 새 에이전트 창 → 에이전트 설정 창 → 설정 창 순으로 그려서, 대화상자 안의 "설정에서 설치" 같은 링크로 연 설정 창이 위에 떠요.
- 백엔드: `update_agent`(시작 잠금 안에서 `Files::update_meta` 하나로: 읽기 → `AgentMeta::patched`(모델은 `check_model`) → 바뀐 권한만 `validate_permission`, 바뀐 기준 브랜치만 존재 확인 → 쓰기 → `applied`), `restart_agent`(`can_restart` → `Agent::restart`) → [Tauri 명령](../../reference/tauri-commands.md).
- meta 잠금: meta를 읽고-고쳐-쓰는 곳(턴의 대화 id·트랜스크립트 위치, 요약, 워커 연결, 참조, 폐기 정리, 이 창)은 모두 `Files::update_meta`로 파일마다 하나인 잠금 안에서 해요. 저장한 권한이 돌던 턴의 쓰기에 덮여 되돌아가거나, 이 창의 저장이 그 사이 바뀐 대화 id를 지우지 않아요. → [저장 위치](../../reference/storage-layout.md)

## 제약과 알려진 문제

- 실행 중인 에이전트에 권한·모델을 바로 적용하지 않아요(ACP `session/set_mode`·`set_model` 없음). 저장 + 쉬는 중이면 다시 시작까지만 해요.
- 만든 뒤 에이전트 종류(Claude Code → Codex)와 일반 에이전트의 브랜치는 바꿀 수 없어요.
- 이름을 브랜치와 똑같이 지어 둔 에이전트는 창에서 이름 칸이 빈 것으로 보여요(이름 없음과 구분하지 못함). 그대로 저장하면 아무것도 바뀌지 않아요.
- 모델 목록은 `agent_models`가 주는 것만이에요. 목록에 없는 모델이 저장돼 있으면 그 값을 맨 위 항목으로 보여 줘요.
- 포커스 가두기는 없어요(확인 창만 가둬요). 입력칸에 포커스가 있는 동안 터미널이 키를 가져가지 않는 것은 다른 대화상자와 같아요.

## 결정 기록

- 2026-11-13 (UI 구조 재구성 단계 8): 라이브 적용 대신 "저장 + 쉬는 중이면 다시 시작"으로 했어요. ACP의 세션 중간 모드·모델 바꾸기가 검증되지 않았기 때문이에요(PRD S12).
- 2026-11-13 (리뷰 수정): meta 쓰기를 파일마다 잠그는 `update_meta`로 모았어요. 시작 잠금만 쥐면 잠금 없이 meta 전체를 다시 쓰는 곳(ACP의 대화 id 기록, 트랜스크립트 위치, 요약)과 겹쳐, "모두 허용"에서 "매번 묻기"로 바꾼 저장이 디스크에서 되돌아가고 다음 시작이 다시 모두 허용으로 뜰 수 있었어요. 비교 후 다시 시도 대신 잠금을 고른 것은 쓰는 곳이 모두 한 프로세스 안이라서예요.
- 2026-11-13 (리뷰 수정): 모델 이름을 엄격한 글자 집합으로 확인해요. 이 창에서 모델을 만든 뒤에도 고칠 수 있게 되면서, Windows의 `cmd.exe /c call <cli>.cmd` 실행에 `&`·`|` 같은 글자가 그대로 넘어가 명령으로 실행될 수 있었어요. 예전 meta에 남은 이상한 모델은 터미널로 시작할 때도 거절해요.
- 2026-11-13 (리뷰 수정): `maestro worktree create`의 워커 권한은 맞춰서 만들고(그보다 느슨하지 않은 가장 가까운 것), 거절하지 않아요. Antigravity에는 "매번 묻기"가 없어 거절하면 그런 마에스트로는 agy 워커를 아예 만들 수 없기 때문이에요. 바뀌면 결과에 `permission` 메모를 붙여 마에스트로가 알게 해요.
- 2026-11-13 (단계 8): [저장하고 다시 시작]은 쉬는 중이면서 다시 시작이 필요한 것을 바꿨을 때만 보여요. 이름·설명만 바꾸고 다시 시작하는 것은 대화를 끊기만 하기 때문이에요.
- 2026-11-13 (단계 8): 마에스트로의 제목은 이름("Maestro") 대신 "마에스트로 설정"이에요. 마에스트로 이름은 바꿀 수 없고, 용어 표의 이름이 더 분명해서예요.
- 2026-11-13 (단계 8): 바닥 안내 문구는 앱 문구 규칙대로 끝 마침표를 뺐어요(PRD 화면 8의 문장에는 마침표가 있음).
- 2026-11-13 (단계 8): 탭 더블클릭은 "탭 고정"이라 이름 바꾸기에 쓰지 않아요. 이름 바꾸기는 사이드바 F2와 이 창이에요(PRD S10).
