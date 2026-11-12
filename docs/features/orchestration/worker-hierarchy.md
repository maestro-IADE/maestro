---
title: 워커 계층 (상위 블록 → 워커 연결)
area: orchestration
status: done
files:
  - src-tauri/src/main.rs
  - src-tauri/src/agent.rs
  - src-tauri/src/control.rs
  - src/Canvas.tsx
  - src/ProjectMap.tsx
  - src/Sidebar.tsx
  - src/NewAgentDialog.tsx
  - src/Workspace.tsx
  - src/store.ts
related:
  - features/orchestration/merge-request.md
  - features/orchestration/maestro-agent.md
  - features/orchestration/worker-done-notes.md
  - features/orchestration/maestro-cli.md
  - features/canvas/two-level-canvas.md
  - reference/tauri-commands.md
  - reference/storage-layout.md
updated: 2026-11-13
---

# 워커 계층 (상위 블록 → 워커 연결)

## 요약

에이전트끼리는 "상위 블록 → 워커" 관계로 이어져요. 관계는 상위 에이전트 메타의 `workers` 배열(워커 워크트리 경로 목록)에 저장되고, 항상 트리 모양을 유지해요(워커는 상위가 하나뿐, 고리 금지, 마에스트로는 워커가 될 수 없음). 캔버스의 `orchestrates` 선과 사이드바의 들여쓰기 트리는 모두 이 값에서 그려져요.

## 동작

- 마에스트로 캔버스에서 카드의 점을 다른 카드로 끌면, 끈 쪽이 상위, 놓은 쪽이 워커가 돼요.
- 카드에서 선을 끌어 빈 캔버스에 놓으면 새 워크트리 대화상자가 `새 워커 · <저장소>` 제목으로 열려요. 만들면 그 카드의 워커로 연결돼요.
- 도구 막대의 `+ 워커` 버튼도 지금 캔버스의 에이전트를 상위로 해서 같은 대화상자를 열어요.
- 선을 클릭해 선택하면 `×` 버튼이 나오고, 누르거나 `Delete` 키로 연결을 끊어요.
- 다른 상위로 연결하면 이전 상위에서는 빠져요.
- 사이드바는 마에스트로를 맨 위에, 그 워커를 들여쓰기로 아래에 보여요. 상위가 없는 워크트리는 마에스트로가 있을 때 `상위 없는 워크트리` 제목 아래에 모여요.
- 프로젝트 캔버스(`ProjectMap`)는 읽기 전용 트리예요. 연결은 마에스트로 캔버스에서만 바꿔요. 카드 아래쪽에 `워커 N`이 보여요.
- 워커가 한 턴을 끝내면 상위 에이전트 입력창에 완료/실패 메모가 붙어요(별도 문서).
- 병합 요청(`maestro merge-request`)은 프로젝트의 마에스트로만 할 수 있어요. 중간 상위(워커를 거느린 워커)는 요청할 수 없어요. 대상은 계층과 상관없이 이 프로젝트의 어떤 워크트리든(하위 워커, 상위 없는 워크트리, 외부 워크트리) 되고, 결과 메모(`[maestro] 병합 …`)는 요청한 마에스트로에게 가요. 대상 워커의 카드에는 `병합 요청` 칩이 붙어요. 워커 자신은 합치거나 푸시하지 않고 작업이 끝났다고 답해요. → [병합 요청](merge-request.md)
- PR 모드(R6)에서는 워커의 PR이 그 워커 카드·블록 머리·사이드바 줄에 `PR #N` 칩으로 보이고(CI 실패면 빨갛게), PR 카드는 요청한 마에스트로의 블록에 떠요. PR 메모(`[maestro] PR 생성 …`, `… CI 실패 …`, `… 병합 완료 …`)도 요청한 마에스트로에게 가요. 키는 요청 결과가 `merge:<워커 경로>`, PR 이후 상태가 `pr:<워커 경로>`예요(워커 완료 메모의 키 = 워커 이름과 겹치지 않아요). → [PR 모드](pr-mode.md)

## 구현

- 저장: `AgentMeta.workers: Vec<String>` (`agent.rs`). `WorktreeInfo.workers`로 프론트에 나가요.
- 연결: `link_worktrees(project_root, parent, child)` → `link()` (`main.rs`).
  - `is_root(root, child)`면 거부해요.
  - `agent_files()`로 마에스트로와 모든 워크트리의 메타를 읽고, 순수 함수 `relink(tree, parent, child)`로 새 `workers`를 계산해요.
  - `relink`는 자기 자신 연결, 메타 없는 상위, 고리(child가 parent의 조상)를 거부해요. child는 모든 목록에서 빠지고 parent 목록 끝에 붙어요. 경로 비교는 대소문자와 `\`/`/`를 무시해요.
  - 바뀐 메타만 다시 써요. 테스트: `relink_keeps_a_tree`.
- 해제: `unlink_worktrees(project_root, parent, child)`는 parent 메타의 `workers`에서 child를 빼요.
- 폐기: `discard_worktree`는 관리 워크트리든 외부 워크트리든 `forget_agent`로 모든 메타의 `workers`에서 그 경로를 빼요(경로 표기는 `\`/`/`·대소문자 무시).
- CLI: `maestro worktree create`로 만든 워크트리는 `link(app, root, me, new)`로 호출한 에이전트의 워커가 돼요. `maestro whoami`는 `parent`와 `workers`를 보여 줘요(`control::parent_of`).
- 캔버스(`Canvas.tsx`):
  - `linkFor(a, b)`: 두 카드면 `{kind: "child"}`. 대상이 `isMaestro`이거나 이미 워커면 `null`. `isValidConnection`도 이 함수를 써요.
  - `onConnect` → `ipc.linkWorktrees` 후 `refreshWorktrees`.
  - `onConnectEnd`: 시작 노드가 카드이고 `toNode`가 없으면 `onNewAgent({ parentPath })`. `GroupView`(Workspace.tsx)가 스토어 `openNewAgent({ root, mode: "worker", parentPath })`로 새 에이전트 창을 열고, `NewAgentDialog`가 생성 후 `ipc.linkWorktrees(root, parentPath, info.path)`를 불러요.
  - 선은 `worktrees[].workers`에서 매번 다시 그려요. id는 `orch_<parent>><child>`, `data.kind = "orchestrates"`, `className: "orch"`, 애니메이션 있음. 마에스트로 캔버스(`hub`)에서만 그려요. `orchestrates` 선은 캔버스 파일에 저장하지 않아요.
  - `onEdgesDelete`: `orchestrates` 선이면 `ipc.unlinkWorktrees`.
  - `treeLayout()`: 마에스트로를 맨 위에 두고 워커를 아래에 배치해요. 상위 없는 워크트리는 마에스트로 아래에 선 없이 붙어요. 계층이 바뀌면 `arrange()`로 다시 배치해요.
- 사이드바(`Sidebar.tsx`의 `treeRows`): 마에스트로부터 DFS로 `depth`를 매기고, 그다음 누구의 워커도 아닌 워크트리를 `loose: true`로 이어 붙여요(마에스트로가 있을 때만). 마지막 루프는 깨진(순환) 링크 파일로 닿지 않는 항목을 위한 것이에요. 들여쓰기는 `22 + depth * 16`px예요.
- 프로젝트 캔버스(`ProjectMap.tsx`): `workers`에서 `bottom` → `top` 선을 그려요. `nodesConnectable={false}`라 여기서는 연결을 바꿀 수 없어요.

## 제약과 알려진 문제

- `unlink_worktrees`는 `relink`처럼 경로 표기를 정규화하지 않고 `eq_ignore_ascii_case`만 써요.
- 워크트리 캔버스(`hub`가 아님)에서는 워커 카드가 없어서 계층 선도 그려지지 않아요.
- `workers`는 상위 메타 파일에만 있어요. 상위가 메타 없이 사라지면 연결 정보도 같이 사라져요.

## 결정 기록

- 계층의 진실은 백엔드 메타예요. 캔버스는 선을 저장하지 않고 메타에서 다시 그려요. CLI로 바꾼 연결도 그대로 반영하려는 것이에요(`Canvas.tsx` 주석 "the backend is the truth").
- 끄는 방향이 곧 계층 방향이에요(상위에서 워커로).
