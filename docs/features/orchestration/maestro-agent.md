---
title: 마에스트로 에이전트 (프로젝트 조율자)
area: orchestration
status: done
files:
  - src-tauri/src/main.rs
  - src-tauri/src/agent.rs
  - src-tauri/src/control.rs
  - src/NewAgentDialog.tsx
  - src/ProjectMap.tsx
  - src/Canvas.tsx
  - src/nodes/AgentNode.tsx
  - src/nodes/WorktreeCard.tsx
  - src-tauri/src/merge.rs
  - src-tauri/src/guard.rs
related:
  - features/orchestration/merge-request.md
  - features/orchestration/worker-hierarchy.md
  - features/orchestration/maestro-cli.md
  - features/orchestration/worker-done-notes.md
  - features/worktrees/branch-selection.md
  - features/canvas/two-level-canvas.md
  - architecture/agent-runtime.md
  - reference/storage-layout.md
updated: 2026-11-11
---

# 마에스트로 에이전트 (프로젝트 조율자)

## 요약

마에스트로는 프로젝트마다 하나만 만들 수 있는 조율 에이전트예요. 워크트리가 아니라 프로젝트 폴더 자체(지금 체크아웃된 브랜치)에서 실행되고, 일을 나눠 워커 워크트리에 맡긴 뒤 검증한 결과를 기준 브랜치(`base_branch`)에 합쳐 달라고 요청하는 역할을 해요(`maestro merge-request`). 실제 병합은 사용자가 카드에서 승인하면 앱이 해요. → [병합 요청](merge-request.md) 설정은 `~/.maestro/projects/<이름>_<해시>/agents/maestro.json`에 `role: "maestro"`로 저장돼요.

## 동작

- 프로젝트 캔버스(`ProjectMap.tsx`) 위쪽 도구 막대에 마에스트로가 없으면 `♛ 마에스트로 만들기`, 있으면 `♛ 마에스트로 캔버스` 버튼이 보여요.
- 만들기 대화상자(`NewAgentDialog`의 `maestro` 모드)에서는 이름·설명 입력이 없어요. 에이전트, 권한, 그리고 `워커 기준 브랜치`만 골라요. 원본 폴더에서 직접 실행되니 권한을 신중히 고르라는 안내가 나와요.
- 만들면 바로 실행돼요. 이름은 항상 `Maestro`예요.
- 사이드바와 카드에는 `♛`(`maestro-crown`) 표시가 붙고, 사이드바 트리와 캔버스 트리에서 항상 맨 위에 와요.
- 프로젝트 캔버스 카드 아래쪽에는 브랜치 대신 `기준 <base_branch>`가 보여요. 기준 브랜치가 없으면 저장소 기본 브랜치를 보여 줘요.
- 마에스트로의 캔버스(`Canvas.tsx`에서 `hub`)에서는 마에스트로가 전체 에이전트 블록으로, 다른 워크트리는 요약 카드로 보여요.
- 폐기(휴지통)하면 "프로젝트 폴더와 브랜치는 그대로 두고, 에이전트와 워커 연결만 지워요"라고 확인한 뒤 설정 파일만 지워요.

## 구현

- 브리프: 마에스트로(`role: "maestro"`)는 할 수 있는 일(명령) 안내와 플레이북 목록(`~/.maestro/playbooks`, 제목과 경로)을 받아요. **어떻게 지휘할지는 코드가 아니라 플레이북이 정해요**: 맡길지 직접 할지, 나누는 법, 워커에게 줄 페르소나(플레이북의 `구성` 표). 맞는 플레이북이 없으면 "기본 지휘"(`default.md`)를 따라요. → [문서와 플레이북](library-and-personas.md)


- 명령: `create_maestro` (`main.rs`). 요청 `CreateMaestroReq { project_root, adapter_id, permission_mode, model, base_branch }`.
  - `files(app, root, root)`로 메타 파일 위치를 정해요. `is_root(project_root, path)`가 참이면 `<project_dir>/agents/maestro.json`을 써요.
  - 이미 메타가 있으면 `"이 프로젝트에는 이미 마에스트로가 있어요"` 오류예요.
  - `AgentMeta { name: "Maestro", role: "maestro", base_branch, model, ... }`를 쓰고 `ensure()`로 바로 실행해요. `model`과 `base_branch`는 빈 문자열이면 `None`이에요.
- 목록: `worktree_infos()`는 `maestro.json`이 있으면 프로젝트 폴더를 첫 항목으로 넣어요. 브랜치는 `worktree::current_branch(root)`예요. `WorktreeInfo.is_maestro`는 `meta.role == "maestro"`일 때 참이에요.
- 기준 브랜치: `base_branch(app, root)`는 마에스트로 메타의 `base_branch`, 없으면 `worktree::default_branch(root)`(origin 기본 브랜치 → 현재 브랜치 → `HEAD`)를 돌려줘요. 새 워크트리 대화상자의 기본 기준값과 `maestro worktree create`의 기본 `--base`가 이 값을 써요.
- 시스템 브리프: `agent::system_brief()`는 `role == "maestro"`이면 `MAESTRO_HINT` 뒤에 `COORDINATOR_HINT`를 붙여요. 내용은 기능 설명이에요: 프로젝트 폴더에서 돌고, 워커는 연결된 워크트리이고, `worktree create`/`send`/`ref add`/`read` 명령이 있다. 그리고 병합 흐름: 직접 합치거나 푸시하지 않는다, 워커가 자기 워크트리에서 최신 기준 브랜치를 합치거나 rebase하고 충돌을 풀게 한 뒤 검증(빌드·테스트)하고 `maestro merge-request --to <워커> --verified "…"`로 요청한다, 사용자가 카드에서 승인하면 앱이 요청한 커밋 그대로 `git merge --no-ff`로 합친다(프로젝트 폴더에 커밋하지 않은 변경이 없어야 함), 결과는 `[maestro] 병합 …` 메모로 온다, 플레이북의 "합친다"는 이 요청을 뜻하고 한 번에 하나씩, 합칠 때마다 프로젝트 폴더에서 빌드·테스트한다. 그 뒤에 플레이북 목록과 "맞는 플레이북을 읽고 따르라, 없으면 기본 지휘, `구성` 표의 페르소나를 워커에 이어라"는 문장과 페르소나 폴더의 실제 경로가 붙어요(플레이북이 하나도 없으면 "스스로 정하라"). `base_branch`가 있으면 "새 워커는 여기서 갈라지고, 끝난 작업은 `maestro merge-request`로만 여기에 들어간다. 요청 전에 프로젝트 폴더를 깨끗하게 둬라"는 문장도 붙어요. 모든 에이전트가 받는 `MAESTRO_HINT`에도 git 규칙이 있어요: 자기 워크트리 브랜치에만 커밋, 푸시·기준 브랜치 병합·프로젝트 폴더 기록 변경 금지, 파괴적 git 명령 금지. Claude Code와 Antigravity 둘 다 받아요(Antigravity는 `--add-dir` 규칙 폴더로).
- 계층: `link()`는 `is_root(root, child)`면 `"마에스트로는 다른 블록의 워커가 될 수 없어요"`로 거부해요. 캔버스의 `linkFor()`도 대상이 `isMaestro`면 선을 만들지 않아요.
- 폐기: `discard_worktree`는 `is_root`이면 프로세스를 끄고 `forget_agent`로 `maestro.json`과 그 옆 파일(`.settings.json`, `.brief.md`, `.brief/`, `.notes.json`)을 지워요. 폐기 점검(`check_discard`)은 하지 않아요. 폴더와 브랜치는 건드리지 않아요.
- CLI: `maestro worktree list`는 마에스트로 항목에 `role: "maestro"`를 넣어요.

## 제약과 알려진 문제

- 프로젝트당 마에스트로는 하나예요.
- 마에스트로는 원본 프로젝트 폴더에서 실행되므로 권한 모드에 따라 원본 파일을 바로 바꿀 수 있어요.
- 마에스트로는 기준 브랜치에서 일반 `git commit`(작은 수정)은 할 수 있지만, 병합·푸시·`commit --amend`·브랜치 전환 등은 명령 가드가 막아요(Claude Code만. Antigravity는 브리프로만). → [병합 요청](merge-request.md)
- 기준 브랜치를 만든 뒤 바꾸는 명령은 현재 코드에 없어요. 커밋 0f01eac 메시지에는 "카드에서 바꿀 수 있다"고 적혀 있지만 지금 소스에서는 찾지 못했어요(확인 안 됨).

## 결정 기록

- 지휘 정책("구현은 맡기고 직접 고치지 마라" 등)을 코드(`COORDINATOR_HINT`)에서 빼서 "기본 지휘" 플레이북으로 옮겼어요. 코드에 박혀 있으면 사용자가 바꿀 수 없고 모든 프로젝트에 강제돼서요. 코드는 무엇을 할 수 있는지만 말해요.
- 예외: 병합·푸시 규칙은 플레이북이 아니라 코드가 정하고 막아요(R5, D4). 되돌리기 어려운 git 동작은 프롬프트만으로 막으면 지켜지지 않은 사례가 있어서예요. 그래서 브리프도 "직접 합치지 말고 요청하라"를 코드에서 말하고, 실제 병합은 앱이 프로젝트 폴더에서 해요.

- 마에스트로를 워크트리가 아닌 프로젝트 폴더에 둔 이유: 워커 브랜치가 기준 브랜치에 들어가는 자리가 필요해서예요(`AgentMeta.role` 주석). R5부터 병합은 앱이 같은 프로젝트 폴더에서 하고, 마에스트로는 빌드·테스트로 확인해요.
- 마에스트로 폐기는 폴더와 브랜치를 남겨요. 원본 저장소를 지우면 안 되기 때문이에요.
