---
title: 에이전트 만들기 (새 에이전트 창)
area: worktrees
status: done
files:
  - src/NewAgentDialog.tsx
  - src/agentForm.ts
  - src/agents.tsx
  - src/Workspace.tsx
  - src/store.ts
  - src/Sidebar.tsx
  - src/ProjectMap.tsx
  - src/Canvas.tsx
  - src/nodes/AgentNode.tsx
  - src/ipc.ts
  - scripts/agent-form-check.ts
  - src-tauri/src/main.rs
  - src-tauri/src/worktree.rs
  - src-tauri/src/agent.rs
  - src-tauri/src/catalog.rs
related:
  - features/worktrees/branch-selection.md
  - features/worktrees/external-worktrees.md
  - features/worktrees/discard-worktree.md
  - features/agents/agent-settings.md
  - features/agents/chat-view.md
  - features/orchestration/maestro-agent.md
  - features/orchestration/worker-hierarchy.md
  - features/orchestration/maestro-cli.md
  - architecture/agent-runtime.md
  - reference/storage-layout.md
  - reference/tauri-commands.md
updated: 2026-11-13
---

# 에이전트 만들기 (새 에이전트 창)

## 요약

새 에이전트 창(`NewAgentDialog`)에서 할 일, 에이전트, 권한, 모델을 고르고 **만들고 열기**(Ctrl+Enter)를 누르면 Maestro가 `~/.maestro/projects/<저장소이름>_<해시>/worktrees/<브랜치>`에 git 워크트리를 만들고 에이전트를 실행한 뒤, 그 에이전트를 고정 탭으로 열어요. 할 일은 **보내지 않고** 열린 채팅 입력창에 넣어 두기만 해요(Enter 한 번이면 가요). 창은 하나이고, 무엇을 만드는지(에이전트 / 워커 / 마에스트로)는 여는 곳이 정해요. 백엔드는 `create_agent` 명령 → `create_worktree()` → `worktree::create()` 순서이고, 에이전트 설정은 `<project_dir>/agents/<폴더이름>.json`에 저장돼요. 원본 폴더에는 파일이 생기지 않아요.

## 동작

- **모드와 여는 곳**(모두 스토어 `openNewAgent({ root, mode, parentPath? })`, 창마다 `WindowLayers`의 `NewAgentHost`가 그려요. 메인 창과 분리 창 둘 다):

  | 모드 | 제목 | 여는 곳 |
  | --- | --- | --- |
  | agent | `Plus` + "새 에이전트 · <프로젝트>" | 프로젝트 머리 hover `+`, 프로젝트 메뉴 "새 에이전트", 에이전트 없는 프로젝트의 "새 에이전트" 줄, 프로젝트 지도 도구 막대와 빈 상태의 "새 에이전트" |
  | worker | `Plus` + "새 워커 · <부모 이름>" | 사이드바 행 메뉴·에이전트 헤더 ⋯의 "워커 추가", 마에스트로 캔버스 도구 막대의 "워커", 카드나 에이전트 블록에서 빈 곳으로 선 끌기 |
  | maestro | `Crown` + "마에스트로 만들기 · <프로젝트>" | 프로젝트 메뉴 "마에스트로 만들기", 프로젝트 지도 도구 막대와 빈 상태, 설정 → 프로젝트 → [마에스트로 문서](../orchestration/maestro-agent.md) |

- **필드 순서**(창 폭 480, 안쪽 24, 필드 사이 16):
  1. `할 일 선택`: 3줄 입력칸, 처음 포커스. 아래에 "열린 입력창에 넣어 둬요. Enter로 보내요".
  2. 한 줄에 `에이전트` · `권한` · `모델`:
     - 에이전트: 로고 + 선택 상자. 모든 에이전트가 목록에 남고, 못 쓰는 것은 이유가 붙어요(" · 꺼 둠", " · 설치 필요", " · 버전 낮음", " · 준비 중"). 로그인이 안 된 것 같으면 " · 로그인 필요"를 붙이지만 고를 수 있어요. 못 쓰는 것을 고르면 바닥 위 오류 상자에 이유와 "설정에서 설치"(꺼 둔 것은 "설정에서 켜기") 글 버튼이 나오고 주 버튼이 꺼져요. 그 버튼은 설정을 에이전트 섹션으로 열고, 설정 창은 새 에이전트 창 **위에** 떠서 닫으면 쓰던 창으로 돌아와요.
     - 권한: 그 에이전트가 받는 모드만(`permissionsFor`, 카탈로그 `permissionModes` 순서). Antigravity에는 "매번 묻기"가 없어요. 아래 12px 설명은 [용어](../ui/terms.md) 표 그대로. 에이전트를 바꾸면 고른 권한이 그 에이전트에 없을 때 "파일 수정 자동 허용"(없으면 첫 항목)으로 바뀌어요.
     - 모델: "기본 (<설정의 이 에이전트 모델 또는 CLI 기본값>)" + `ipc.agentModels(id)` 목록. "기본"이면 설정의 에이전트별 모델(`agentModels[agent]`)을, 그것도 비면 CLI 기본값을 써요.
  3. `고급`(접힘, 에이전트·워커 모드): 28px 한 줄 "고급 · 새 브랜치 agent-5 · 기준 main"(기존 브랜치면 "고급 · 기존 브랜치 x"). 펼치면 이름 [선택], 설명 [선택], 새 브랜치 / 기존 브랜치, 브랜치 이름, 기준 브랜치 → [브랜치 고르기](branch-selection.md).
- **이름과 설명**: 이름이 비면 할 일의 첫 줄(최대 40자)을, 할 일도 비면 브랜치 이름을 써요. 설명이 비면 할 일 전체를 설명으로 저장해요(프로젝트 지도 카드·워커 카드에 보여요).
- **만들고 열기**: 주 버튼 또는 Ctrl+Enter(입력칸 어디서든, 한글 조합 중에는 아무것도 하지 않아요). 한 줄 입력칸의 그냥 Enter로는 만들지 않아요. 만드는 중에는 "만드는 중…"이고 필드가 잠겨요. Esc와 바깥 클릭은 닫기(만드는 중 제외).
- **만든 뒤**: `upsert` → (워커면 `linkWorktrees` + `refreshWorktrees`) → 할 일이 있으면 스토어 `seedComposer(path, 할 일)` → `select(path, true)`로 고정 탭을 열어요(역할별 첫 화면: 에이전트는 설정 `agentLayout`, 마에스트로는 `maestroLayout`). 채팅 입력창이 처음 열릴 때 할 일이 들어 있고 포커스가 가요 → [채팅 보기 "첫 글"](../agents/chat-view.md). 터미널 보기로 열리면 할 일은 채팅 보기를 처음 열 때까지 기다려요. 체크박스("만든 뒤 열기")는 없어요. 늘 열려요.
- 실패하면 바닥 위 오류 상자(`--mae-err` 글, 같은 색 8% 면)에 백엔드 오류가 보이고 창은 그대로예요.

## 구현

- 프런트:
  - `NewAgentDialog.tsx`: 창 하나. 모드는 `mode`, 워커의 부모는 `parentPath`. 마에스트로 모드 세부(기준 브랜치, 병합 방식, 이미 있을 때)는 [마에스트로 문서](../orchestration/maestro-agent.md).
  - `agents.tsx`: `AGENTS`(카탈로그), `defaultAgentOf(defaultAgent, cli, disabled)`(설정의 기본, 못 쓰면 쓸 수 있는 첫 에이전트), `agentBlocker(id, cli, disabled)`("" / "준비 중" / "꺼 둠" / "설치 필요" / "버전 낮음"), `permissionsFor(id)`, `permissionFor(id, wanted)`(받지 않으면 acceptEdits, 그것도 없으면 첫 모드), `useModels(id)`(설정 창과 같이 씀), `BranchSelect`.
  - `agentForm.ts`(순수, `scripts/agent-form-check.ts`가 시험): `nextAgentBranch(branches, from)`(아직 없는 다음 `agent-N`, 로컬·원격 모두 피함), `branchTaken(branches, name)`, `nameFromTask(task)`.
  - 기본 브랜치 이름은 `agent-<이 프로젝트의 마에스트로 아닌 에이전트 수 + 1>`부터 비어 있는 첫 번호. 사용자가 브랜치 이름을 고치면 그 값을 써요.
  - `ipc.createAgent({ projectRoot, adapterId, permissionMode, branchName, baseBranch, existingBranch, name, description, model })`. `baseBranch`가 비면 `"HEAD"`.
  - 할 일은 `ipc.send` 계열로 보내지 않아요. 스토어 `composerSeed`(경로를 대소문자·슬래시 무시 키로)에만 넣어요.
- 백엔드(`main.rs`):
  - `create_agent(req: CreateAgentReq)` → `check_new_agent`(카탈로그에 있음, 실행할 수 있음, `catalog::validate_permission`으로 그 에이전트가 받는 권한) → `create_worktree(app, state, req)`. 확인은 워크트리를 만들기 **전**이라, 받지 않는 권한(예: Antigravity의 `default`)은 아무것도 만들지 않고 "Antigravity에서는 쓸 수 없는 권한이에요: default"로 거절돼요. `maestro worktree create`도 `create_worktree`를 쓰지만 이 확인은 거치지 않아요(마에스트로의 권한을 그대로 물려줘요, `control.rs`). CLI 쪽은 워크트리를 만든 뒤 `--prompt` 첫 지시를 보내지 못해도 카드를 돌려줘요(`warning`, 지시는 보관). → [maestro CLI](../orchestration/maestro-cli.md)
  - `worktree::project_dir(home, root)` = `~/.maestro/projects/<sanitize(폴더이름)>_<fnv1a(root) 8자리 16진수>`. FNV-1a는 Rust 버전이 바뀌어도 값이 같아서 썼어요.
  - `worktree::create(root, project_dir, branch, base, existing)`: 경로는 `<project_dir>/worktrees/<sanitize(브랜치)>`(영숫자·`-` 외는 `_`). 새 브랜치일 때 이름이나 폴더가 이미 있으면 `-2`, `-3` … 을 붙여 99까지 시도해요. `git worktree add -b <branch> <path> <base>`.
  - 메타: `Files::new(pdir, path)` = `<project_dir>/agents/<워크트리 폴더 이름>.json`. `AgentMeta { adapter_id, permission_mode, name, description, model, keep_branch }`를 쓰고(공백만 있으면 `None`) `Agent::spawn`으로 실행해요.
  - 실행이 실패하면 방금 만든 워크트리를 `worktree::discard(..., DiscardChoice::Clean)`으로 지우고 메타 파일도 지워요(→ [에이전트 폐기](discard-worktree.md)).
  - 결과는 `WorktreeInfo`예요.
- 실행 때 생기는 파일: Claude Code는 `<meta>.settings.json`(승인 훅, `Bash(maestro:*)` 허용)과 `<meta>.brief.md`(시스템 브리프), Antigravity는 `<meta>.brief/AGENTS.md`(같은 브리프). 대화 id는 메타의 `cli_session_id`에 저장돼요.
- git 호출은 Windows에서 `CREATE_NO_WINDOW`로 콘솔 창이 뜨지 않게 해요.

## 제약과 알려진 문제

- `Codex`는 목록에 " · 준비 중"으로 남고 고르면 주 버튼이 꺼져요(런타임 없음).
- 워크트리 폴더 이름은 브랜치 이름에서 나와요. 이름이 다른 두 브랜치가 같은 폴더 이름으로 바뀌면(예: `a/b`와 `a_b`) 뒤의 것은 `-2`가 붙어요.
- 실패 정리에서 기존 브랜치를 체크아웃한 경우(`keep_branch`)는 브랜치를 남겨요.
- 선택 상자의 항목은 브라우저 기본 `<option>`이라 못 쓰는 항목을 따로 흐리게 그리지 못해요. 이유 글자로 구분해요.
- 할 일은 그 창(메인 또는 분리 창)의 스토어에만 있어요. 다른 창에서 그 에이전트의 채팅을 먼저 열면 거기에는 들어가지 않아요. 앱을 다시 켜면 사라져요.

## 결정 기록

- 워크트리를 원본 저장소 안이 아니라 `~/.maestro/projects/` 아래에 둬요. 원본 폴더에 파일이 생기지 않게 하려는 것이에요(`App.tsx` 안내 문구).
- 실행에 실패한 워크트리는 남기지 않아요. 고아 워크트리를 막으려는 것이에요(`create_worktree` 주석).
- 2026-11-13 (UI 구조 재구성 단계 7): 권한은 백엔드가 에이전트마다 확인해요. agy는 모르는 모드를 조용히 accept-edits로 돌려서(`antigravity::argv`), 화면에서 고른 것과 다른 권한으로 도는 일을 막으려는 것이에요.
- 2026-11-13 (단계 8): 할 일은 만들자마자 보내지 않고 입력창에 넣어 둬요. 만들자마자 보내면 ACP 부팅·폴더 신뢰 확인과 경쟁해 메시지를 잃을 수 있어서예요. 클릭 수는 같아요(Enter 한 번).
- 2026-11-13 (단계 8): 창을 모드별로 나누지 않고(같은 필드에 파일만 늘어요) 여는 곳이 모드를 정해요. 종류 세그먼트는 두지 않아요.
- 2026-11-13 (단계 8): 워커 모드 제목에도 `Plus` 아이콘을 붙였어요(계획은 에이전트·마에스트로만 아이콘). 세 모드의 제목 줄 모양을 맞추려는 것이에요.
- 2026-11-13 (단계 8): 원격에만 있는 브랜치 이름도 "이미 있는 브랜치"로 막아요. 같은 이름의 로컬 브랜치를 새로 만들면 원격 브랜치와 헷갈리기 때문이에요.
