---
title: 브랜치 선택 (새 브랜치 / 기존 브랜치, Lead 기준 브랜치)
area: worktrees
status: done
files:
  - src/NewAgentDialog.tsx
  - src/AgentSettingsDialog.tsx
  - src/agentForm.ts
  - src/agents.tsx
  - src/types.ts
  - src/ipc.ts
  - src-tauri/src/worktree.rs
  - src-tauri/src/main.rs
  - src-tauri/src/control.rs
  - src-tauri/src/agent.rs
related:
  - features/worktrees/create-worktree.md
  - features/agents/agent-settings.md
  - features/orchestration/maestro-agent.md
  - features/orchestration/maestro-cli.md
  - reference/tauri-commands.md
updated: 2026-11-13
---

# 브랜치 선택 (새 브랜치 / 기존 브랜치, Lead 기준 브랜치)

## 요약

워크트리를 만들 때 새 브랜치를 기준 브랜치에서 만들지, 이미 있는 브랜치(로컬 또는 원격에만 있는 것)를 체크아웃할지 고를 수 있어요. 이미 다른 곳에 체크아웃된 브랜치는 git이 거부하므로 `inUse`로 표시하고 고를 수 없게 해요. 기준 브랜치의 기본값은 Lead의 `base_branch`, 없으면 저장소 기본 브랜치예요.

## 동작

- 새 에이전트 창의 접힌 `고급` 줄("고급 · 새 브랜치 agent-N · 기준 <base>", 기존 브랜치면 "고급 · 기존 브랜치 <이름>")을 누르면 펼쳐져요. 펼친 안에 이름·설명, `새 브랜치` / `기존 브랜치` 전환 버튼이 있어요.
- 새 브랜치: `새 브랜치 이름`과 `기준 브랜치`(드롭다운)를 정해요. 이름의 기본값은 아직 없는 다음 `agent-N`이에요(로컬·원격 이름을 모두 피함). 직접 고치면 그 값을 써요.
- 이미 있는 브랜치 이름(로컬이든 원격에만 있든)을 입력하면 그 칸 아래에 12px 빨간 글 "이미 있는 브랜치예요 · 다른 이름을 쓰거나 기존 브랜치를 골라요"가 나오고 입력칸 테두리가 빨개지며 "만들고 열기"가 꺼져요. 고급을 접어도 그 아래에 "이미 있는 브랜치 이름이에요 · 고급에서 바꿔요"가 남아요.
- 기존 브랜치: `체크아웃할 브랜치`를 골라요. 이 브랜치에서 이어서 작업하고, 워크트리를 폐기해도 브랜치는 남는다고 안내해요. 쓸 수 있는 브랜치가 하나도 없으면 `기존 브랜치` 버튼이 비활성이에요.
- 드롭다운 항목은 `<이름> · 원격 · 사용 중`처럼 표시돼요. 체크아웃용 목록에서는 `사용 중` 항목이 비활성이에요. 목록을 읽는 동안은 `브랜치 읽는 중…`이에요.
- Lead를 만들 때는 `워커 기준 브랜치`만 골라요(고급 줄 없음). 새 에이전트 창의 기준 브랜치 기본값이 이 값이에요.
- Lead의 기준 브랜치는 만든 뒤에도 에이전트 설정 창의 `워커 기준 브랜치`에서 바꿔요(`update_agent`의 `baseBranch`, 있는 로컬 브랜치만). 켜져 있는 Lead는 다시 시작해야 브리프에 새 값이 들어가요 → [에이전트 설정](../agents/agent-settings.md).

## 구현

- 목록: `list_branches(project_root)` → `worktree::branches(root)`.
  - 로컬: `git for-each-ref refs/heads`. 원격: `refs/remotes`에서 `<remote>/` 뒤 이름만 쓰고, 같은 이름의 로컬이 있으면 빼요. `origin/HEAD`(새 git에서는 `origin`)는 건너뛰어요.
  - `in_use`: `git worktree list --porcelain`의 브랜치들(프로젝트 폴더 포함)에 들어 있으면 참.
  - 결과 타입 `Branch { name, remote, in_use }`, 프론트에서는 `{ name, remote, inUse }`. 테스트: `merges_local_and_remote_branches`.
- 프론트: `BranchSelect({ branches, value, onChange, pickable, disabled })`(`agents.tsx`). 목록에 없는 값도 맨 위 옵션으로 보여 줘요. `existing` 상태가 `""`면 새 브랜치 모드예요. 다음 이름과 중복 판정은 순수 `agentForm.ts`의 `nextAgentBranch(branches, from)`, `branchTaken(branches, name)`(`scripts/agent-form-check.ts`).
- 생성 요청: `CreateAgentReq.existing_branch`(프론트 `existingBranch`)가 참이면 `branch_name`을 그대로 체크아웃해요.
  - `worktree::create(..., existing = true)`: `git worktree add <path> <branch>`. 원격에만 있는 브랜치는 git 기본 동작대로 로컬 추적 브랜치가 생겨요. 새 브랜치일 때만 이름 충돌 시 `-2`, `-3`을 붙여요.
  - `AgentMeta.keep_branch = existing_branch`. `discard_worktree`는 `keep_branch`면 폴더만 지우고 브랜치는 지우지 않아요. 이 브랜치의 커밋은 남으니 폐기 점검(`check_discard`)도 커밋하지 않은 파일만 봐요(→ [워크트리 폐기](discard-worktree.md)).
- 기준 브랜치:
  - `AgentMeta.base_branch`(Lead만). `create_maestro`가 빈 값이면 `None`으로 저장해요.
  - `base_branch(app, root)`: Lead 메타 값 → `worktree::default_branch(root)`(`origin/HEAD` → 현재 브랜치 → `HEAD`).
  - 프론트 기본값: `maestroBase ?? repo.defaultBranch`.
  - `WorktreeInfo.base_branch` / `baseBranch`로 나가고 프로젝트 캔버스 Lead 카드에 `기준 …`으로 보여요.
  - Lead 브리프에 "Base branch: …. 새 워커는 여기서 갈라지고, 끝난 작업은 `maestro merge-request`로만 여기에 들어간다"가 들어가요(Claude Code와 Antigravity 둘 다). → [병합 요청](../orchestration/merge-request.md)
- CLI: `maestro worktree create --branch <새> --base <기준>` 또는 `--checkout <기존>`. `maestro branch list`는 `{ base, branches }`.

## 제약과 알려진 문제

- 중복 경고와 `in_use`는 창을 열 때 한 번 읽은 목록 기준이에요. 그 사이 다른 곳에서 체크아웃하면 git 오류로 실패해요.
- CLI `--checkout`은 `in_use`를 미리 확인하지 않아요. git이 거부하면 오류가 돌아와요.
- 원격 브랜치 목록은 마지막 fetch 기준이에요. Maestro가 fetch를 하지는 않아요.

## 결정 기록

- 기존 브랜치로 만든 워크트리는 폐기해도 브랜치를 지우지 않아요. 원래 있던 브랜치의 커밋을 잃지 않게 하려는 것이에요(`AgentMeta.keep_branch` 주석).
- 2026-11-13 (UI 구조 재구성 단계 8): 브랜치 칸은 고급 아래로 접어요. 대부분은 기본값(다음 `agent-N`, Lead의 기준 브랜치)으로 만들고, 할 일과 에이전트·권한·모델이 먼저 보이게 하려는 것이에요.
