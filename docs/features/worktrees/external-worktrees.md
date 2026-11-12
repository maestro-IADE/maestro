---
title: 외부 워크트리 (Maestro 밖에서 만든 워크트리)
area: worktrees
status: done
files:
  - src-tauri/src/main.rs
  - src-tauri/src/worktree.rs
  - src/nodes/AttachAgent.tsx
  - src/nodes/AgentNode.tsx
  - src/discard.tsx
  - src/Sidebar.tsx
  - src/sidebarModel.ts
  - src/status.tsx
  - src/nodes/WorktreeCard.tsx
  - src/ProjectMap.tsx
  - src/types.ts
related:
  - features/worktrees/create-worktree.md
  - features/worktrees/branch-selection.md
  - reference/storage-layout.md
  - reference/tauri-commands.md
  - features/worktrees/discard-worktree.md
  - features/ui/side-panels.md
updated: 2026-11-13
---

# 외부 워크트리 (Maestro 밖에서 만든 워크트리)

## 요약

git이나 다른 도구로 만든 워크트리도 `git worktree list`에 나오면 Maestro에 블록으로 보여요. 이런 워크트리는 `external: true`이고 `외부` 배지가 붙어요. 처음에는 에이전트가 없어서 `AttachAgent`로 에이전트를 붙여요. 설정은 `<project_dir>/agents/ext-<경로해시>.json`에 저장되고, 폐기해도 폴더와 브랜치는 그대로 남아요.

## 동작

- 저장소의 모든 연결된 워크트리가 목록에 나와요. 프로젝트 폴더 자체와 폴더가 지워진 항목(`prunable`)은 빠져요. 브랜치 없이 분리된 HEAD면 `(detached abc1234)`처럼 보여요.
- 카드와 프로젝트 캔버스에 `외부` 배지(`ext-badge`)가 붙어요. 툴팁은 "Maestro 밖에서 만든 워크트리 (폐기해도 폴더는 그대로)"예요.
- 에이전트가 없으면 블록에 "이 워크트리에는 아직 에이전트가 없어요" 안내와 에이전트·권한 선택, `에이전트 붙이기` 버튼이 나와요. 워커 카드에서는 권한 선택 없는 짧은 형태(`compact`)예요.
- 붙이면 그 폴더에서 바로 실행돼요.
- 사이드바에서 에이전트가 없는 행은 상태 레인이 빈 원("에이전트 없음"), 둘째 줄이 "에이전트 연결 필요"(흐린 글자)예요. 레인을 눌러도 아무 일이 없어요. 행 메뉴는 열기 · 에이전트 연결…(고정 탭으로 열어 위 붙이기 화면을 보여요) · 폴더 경로 복사 셋뿐이에요(폐기 없음). → [사이드바](../ui/side-panels.md)
- 폐기(에이전트 헤더 ⋯ 메뉴 또는 사이드바 행 메뉴의 "폐기…")하면 확인 창("에이전트 폐기 · <이름>", "폴더는 그대로 두고 에이전트만 떼어 내요", [폐기])으로 물은 뒤 에이전트 설정만 지워요. 에이전트가 아직 없는 외부 워크트리의 상태 글리프는 빈 원("에이전트 없음")이에요.

## 구현

- 목록: `worktree::list(root)`가 `git worktree list --porcelain`을 `parse_worktree_list()`로 읽어 프로젝트 폴더를 뺀 `(path, branch)`를 돌려줘요. Maestro가 만든 것과 외부 것을 같이 돌려줘요.
- 구분: `worktree::is_managed(path, project_dir)`는 경로가 `<project_dir>/worktrees` 아래인지 봐요(대소문자·구분자 무시). `worktree_infos()`에서 `w.external = !is_managed(...)`.
- 메타 위치: `files(app, root, path)`가 관리 워크트리가 아니면 `<project_dir>/agents/ext-<path_id(path)>.json`을 써요. `path_id`는 소문자·`/` 경로의 FNV-1a 8자리 16진수예요. 폴더 이름이 Maestro 워크트리와 겹칠 수 있어서 경로로 키를 만들어요.
- 에이전트 붙이기: `AttachAgent.tsx` → `ipc.attachAgent({ projectRoot, worktreePath, adapterId, permissionMode, model })` → `attach_agent` (`main.rs`).
  - 메타가 이미 있으면 `"이미 에이전트가 있는 워크트리예요"` 오류예요.
  - 쓰기 전에 에이전트(카탈로그, 실행 가능)와 권한(`catalog::validate_permission`)을 확인해요(`check_new_agent`, 2026-11-13).
  - `AgentMeta { adapter_id, permission_mode, model }`을 `create_with_rollback` 안에서 쓰고 `ensure()`로 실행해요. 시작이 실패하면 메타를 지워서 다시 `AttachAgent` 상태로 돌아가요.
  - 모델은 설정의 `agentModels[agent]`예요. 성공하면 `refreshWorktrees(root)`.
  - 에이전트 목록은 `agentUsable`로 걸러요. 기본값은 설정의 `defaultAgent`, `defaultPermission`. 권한 목록은 고른 에이전트가 받는 모드만(`permissionsFor`)이고, 기본 권한을 그 에이전트가 받지 않으면 `permissionFor`로 바꿔 보내요(백엔드 `attach_agent`가 받지 않는 권한을 거절해서예요).
- 표시: `AgentNode`와 `WorktreeCard`는 `wt.adapterId`가 없으면 `AttachAgent`를 보여 줘요. 사이드바는 `rowMeta`(`src/sidebarModel.ts`)가 `adapterId` 없음을 `line2: "noAgent"`로 가장 먼저 판정해요(승인·질문보다 앞).
- 폐기: 프론트(`useDiscard`, `src/discard.tsx`)는 외부 워크트리면 `check_discard` 없이 `ask()` 후 `discardWorktree(..., "clean")`을 불러요. `discard_worktree`는 `owns_folder`(마에스트로가 아니고 `is_managed`)가 거짓이면 에이전트를 끄고 `forget_agent`로 메타와 그 옆 파일(`.json.bak`, `.settings.json`, `.brief.md`, `.brief/`, `.notes.json`)만 지워요. `git worktree remove`나 브랜치 삭제를 하지 않아요. `check_discard`도 이런 워크트리에는 빈 결과를 돌려줘요.

## 제약과 알려진 문제

- `attach_agent`는 이름·설명을 받지 않아요. 이름은 브랜치로 보여요.
- 에이전트를 떼도 워크트리는 목록에 남고 다시 `AttachAgent` 상태로 돌아가요.

## 결정 기록

- 외부 워크트리는 Maestro가 만든 것이 아니라서 폴더와 브랜치를 절대 건드리지 않아요(`discard_worktree` 주석).
- 2026-11-13 (UI 구조 재구성 단계 4): 사이드바의 에이전트 없는 행은 둘째 줄로 "에이전트 연결 필요"를 알리고, 메뉴는 할 수 있는 것(열기, 연결, 경로 복사)만 남겼어요. 붙일 에이전트가 없는 행에 폐기·읽음 표시는 뜻이 없어요.
- 메타 파일 이름을 폴더 이름이 아니라 경로 해시로 정했어요. 외부 워크트리 폴더 이름이 Maestro 워크트리와 겹칠 수 있어서예요(`files()` 주석).
