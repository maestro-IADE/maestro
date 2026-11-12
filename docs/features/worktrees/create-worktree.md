---
title: 워크트리 만들기 (새 에이전트 블록)
area: worktrees
status: done
files:
  - src/NewAgentDialog.tsx
  - src/agents.tsx
  - src/Workspace.tsx
  - src/store.ts
  - src/Sidebar.tsx
  - src/ProjectMap.tsx
  - src/Canvas.tsx
  - src/ipc.ts
  - src-tauri/src/main.rs
  - src-tauri/src/worktree.rs
  - src-tauri/src/agent.rs
  - src-tauri/src/catalog.rs
related:
  - features/worktrees/branch-selection.md
  - features/worktrees/external-worktrees.md
  - features/worktrees/discard-worktree.md
  - features/orchestration/worker-hierarchy.md
  - features/orchestration/maestro-cli.md
  - architecture/agent-runtime.md
  - reference/storage-layout.md
  - reference/tauri-commands.md
updated: 2026-11-13
---

# 워크트리 만들기 (새 에이전트 블록)

## 요약

새 워크트리 대화상자(`NewAgentDialog`)에서 이름, 설명, 에이전트, 권한을 고르면 Maestro가 `~/.maestro/projects/<저장소이름>_<해시>/worktrees/<브랜치>`에 git 워크트리를 만들고 에이전트를 바로 실행해요. 백엔드는 `create_agent` 명령 → `create_worktree()` → `worktree::create()` 순서이고, 에이전트 설정은 `<project_dir>/agents/<폴더이름>.json`에 저장돼요. 원본 폴더에는 파일이 생기지 않아요.

## 동작

- 여는 곳: 사이드바 프로젝트 머리의 hover `+`, 프로젝트 메뉴 "새 에이전트"(마에스트로는 "마에스트로 만들기"), 에이전트가 없는 프로젝트의 "새 에이전트" 줄, 프로젝트 캔버스의 `+ 워크트리`, 에이전트 캔버스의 `+ 워커`(워커로 연결), 카드에서 빈 곳으로 선 끌기(워커로 연결). 모두 스토어 `openNewAgent({ root, mode, parentPath? })`를 부르고, 창마다 `WindowLayers`의 `NewAgentHost`가 대화상자를 그려요(메인 창, 분리 창).
- 제목: `새 워크트리 · <저장소>`, 워커로 만들 때는 `새 워커 · <저장소>`.
- 입력 항목:
  - `이름`: 비우면 브랜치 이름이 이름 대신 쓰여요.
  - `설명`: 이 워크트리에서 할 일(선택). 프로젝트 캔버스 카드와 워커 카드에 보여요.
  - `에이전트`: 로고(`AgentLogo`)와 함께 고르는 드롭다운. `Claude Code`, `Antigravity`, `Codex (준비 중)`이에요. 설정에서 끈 에이전트는 목록에서 빠지고, 설치되지 않은 것은 `· 설치 필요`로 비활성이에요.
  - `권한`: `acceptEdits`(파일 수정만 자동 허용), `auto`(자동 판단), `bypassPermissions`(모두 허용, 주의), `plan`(계획만). 고른 항목의 설명이 아래에 나와요. `acceptEdits`는 "파일 수정은 묻지 않고 허용해요. 명령 실행 등 다른 도구는 승인 카드로 물어봐요."예요. → [용어](../ui/terms.md)
  - 브랜치: 기본은 접혀 있고 `새 브랜치 agent-N · 기준 <base> — 변경`을 누르면 펼쳐져요(브랜치 선택 문서 참고).
- 모델은 대화상자에서 고르지 않아요. 설정의 에이전트별 모델(`agentModels[agent]`)을 쓰고, 비어 있으면 CLI 기본값이에요.
- `만들고 실행`을 누르면 만들어지고, 프로젝트 캔버스에 카드가 생기며 그쪽으로 화면이 이동해요. 실패하면 대화상자 안에 오류가 보여요.
- 쓸 수 있는 에이전트가 없으면 이유(예: 실행 파일 없음, 버전 낮음)와 "설정 → 에이전트에서 설치·업데이트하거나 켜 주세요" 안내가 나오고 버튼이 비활성이에요.
- 고른 에이전트가 "로그인 필요"면 막지 않고 안내만 보여 줘요. 에이전트 목록에는 상태가 붙어요(" · 버전 낮음", " (준비 중)" 등).

## 구현

- 프론트(`NewAgentDialog.tsx`, 에이전트 목록·로고·권한·브랜치 선택은 `agents.tsx`):
  - `AGENTS`: 백엔드 카탈로그(`agent_catalog`)를 `loadAgents()`로 받은 목록. 로고는 카탈로그의 파일 이름으로 `src/assets/agents/`에서 찾아요. → [에이전트 종류와 카탈로그](../agents/agent-adapters.md)
  - `agentUsable(id, cli, disabled)`: 런타임이 있고, `cli_status` 상태가 `ready` 또는 `loginRequired`이고, 설정에서 끄지 않은 것.
  - 기본 에이전트는 설정의 `defaultAgent`, 쓸 수 없으면 첫 번째 쓸 수 있는 것. 기본 권한은 `defaultPermission`.
  - 기본 브랜치 이름은 `agent-<이 저장소 워크트리 수 + 1>`.
  - `ipc.createAgent({ projectRoot, adapterId, permissionMode, branchName, baseBranch, existingBranch, name, description, model })`. `baseBranch`가 비면 `"HEAD"`.
  - 성공하면 `upsert(root, info)`. `parentPath`가 있으면 `ipc.linkWorktrees` 후 `refreshWorktrees`.
- 백엔드(`main.rs`):
  - `create_agent(req: CreateAgentReq)` → `check_new_agent`(카탈로그에 있음, 실행할 수 있음, `catalog::validate_permission`으로 그 에이전트가 받는 권한) → `create_worktree(app, state, req)`. 확인은 워크트리를 만들기 **전**이라, 받지 않는 권한(예: Antigravity의 `default`)은 아무것도 만들지 않고 "Antigravity에서는 쓸 수 없는 권한이에요: default"로 거절돼요. `maestro worktree create`도 `create_worktree`를 쓰지만 이 확인은 거치지 않아요(마에스트로의 권한을 그대로 물려줘요, `control.rs`). CLI 쪽은 워크트리를 만든 뒤 `--prompt` 첫 지시를 보내지 못해도 카드를 돌려줘요(`warning`, 지시는 보관). → [maestro CLI](../orchestration/maestro-cli.md)
  - `worktree::project_dir(home, root)` = `~/.maestro/projects/<sanitize(폴더이름)>_<fnv1a(root) 8자리 16진수>`. FNV-1a는 Rust 버전이 바뀌어도 값이 같아서 썼어요.
  - `worktree::create(root, project_dir, branch, base, existing)`: 경로는 `<project_dir>/worktrees/<sanitize(브랜치)>`(영숫자·`-` 외는 `_`). 새 브랜치일 때 이름이나 폴더가 이미 있으면 `-2`, `-3` … 을 붙여 99까지 시도해요. `git worktree add -b <branch> <path> <base>`.
  - 메타: `Files::new(pdir, path)` = `<project_dir>/agents/<워크트리 폴더 이름>.json`. `AgentMeta { adapter_id, permission_mode, name, description, model, keep_branch }`를 쓰고(공백만 있으면 `None`) `Agent::spawn`으로 실행해요.
  - 실행이 실패하면 방금 만든 워크트리를 `worktree::discard(..., DiscardChoice::Clean)`으로 지우고 메타 파일도 지워요(→ [워크트리 폐기](discard-worktree.md)).
  - 결과는 `WorktreeInfo`예요.
- 실행 때 생기는 파일: Claude Code는 `<meta>.settings.json`(승인 훅, `Bash(maestro:*)` 허용)과 `<meta>.brief.md`(시스템 브리프), Antigravity는 `<meta>.brief/AGENTS.md`(같은 브리프). 대화 id는 메타의 `cli_session_id`에 저장돼요.
- git 호출은 Windows에서 `CREATE_NO_WINDOW`로 콘솔 창이 뜨지 않게 해요.

## 제약과 알려진 문제

- `Codex`는 목록에만 있고 선택할 수 없어요(`disabled: true`).
- 워크트리 폴더 이름은 브랜치 이름에서 나와요. 이름이 다른 두 브랜치가 같은 폴더 이름으로 바뀌면(예: `a/b`와 `a_b`) 뒤의 것은 `-2`가 붙어요.
- 실패 정리에서 기존 브랜치를 체크아웃한 경우(`keep_branch`)는 브랜치를 남겨요.
- 대화상자의 권한 목록은 아직 에이전트별로 거르지 않아요(단계 8에서 카탈로그 `permissionModes`로 걸러요). 지금 목록 네 개는 Claude Code와 Antigravity가 모두 받아서 거절되는 경우는 없어요.

## 결정 기록

- 워크트리를 원본 저장소 안이 아니라 `~/.maestro/projects/` 아래에 둬요. 원본 폴더에 파일이 생기지 않게 하려는 것이에요(`App.tsx` 안내 문구).
- 실행에 실패한 워크트리는 남기지 않아요. 고아 워크트리를 막으려는 것이에요(`create_worktree` 주석).
- 2026-11-13 (UI 구조 재구성 단계 7): 권한은 백엔드가 에이전트마다 확인해요. agy는 모르는 모드를 조용히 accept-edits로 돌려서(`antigravity::argv`), 화면에서 고른 것과 다른 권한으로 도는 일을 막으려는 것이에요.
