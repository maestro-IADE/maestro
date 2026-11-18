---
title: 마크다운 블록과 참조 선
area: canvas
status: partial
files:
  - src/nodes/MarkdownBlock.tsx
  - src/Canvas.tsx
  - src/LibraryPanel.tsx
  - src/RightPanel.tsx
  - src/types.ts
  - src-tauri/src/main.rs
  - src-tauri/src/library.rs
related:
  - features/orchestration/library-and-personas.md
  - features/canvas/two-level-canvas.md
  - features/orchestration/maestro-cli.md
  - reference/tauri-commands.md
updated: 2026-11-02
---

# 마크다운 블록과 참조 선

## 요약

마크다운 블록(`markdown_block`)은 캔버스 위에 놓인 `.md` 파일 하나예요. 지금은 파일의 첫 `#` 제목과 파일 이름만 보여요. 블록을 에이전트에 선으로 이으면 그 에이전트의 참조(`refs`)가 되고, 같은 파일의 마지막 블록을 캔버스에서 지우면 그 캔버스 에이전트들의 참조도 빠져요.

## 동작

- 문서(시스템의 카드나 캔버스의 문서 패널)의 파일, 또는 오른쪽 파일 트리의 `.md` 파일을 캔버스로 끌어 놓으면 놓은 자리에 블록이 생겨요. 끄는 동안 캔버스가 강조돼요(`data-dragging="md"`).
- 블록은 `MD` 아이콘, 제목, 파일 이름을 보여 줘요. 크기는 240×72(격자 10×3칸)로 고정이에요.
- 블록 네 변 어디서나 선을 끌 수 있어요. 블록 ↔ 에이전트(카드나 에이전트 블록)를 이으면 참조가 연결되고, 선은 블록에서 에이전트로 가는 화살표(`ref` 스타일)로 그려져요.
- 선을 선택해 `×` 또는 `Delete`로 지우면 그 에이전트의 참조만 빠져요.
- **문서 묶음** (2026-11-18): 문서 블록을 다른 문서 블록 위에 놓으면 묶음이 돼요(→ [영역](zones.md) "묶음"). 묶음의 틀에서 에이전트로 선을 하나 그으면 묶음의 문서 전부가 그 에이전트의 참조가 되고, 선은 묶음에서 하나만 그려져요. 그 선을 지우면 묶음의 문서 전부가 참조에서 빠져요. 이어진 묶음에 문서를 더 넣으면 그 문서도 참조가 돼요. 페르소나 문서에 문서를 더 붙여 한 묶음으로 주는 쓰임이에요.
- 블록을 지울 때, 같은 파일의 다른 블록이 캔버스에 남아 있지 않으면 이 캔버스의 모든 에이전트에서 그 파일 참조가 빠져요.
- 다른 곳(프로젝트 캔버스의 `RefPicker`, `maestro ref add`)에서 참조를 추가하면 이 캔버스 에이전트 블록 왼쪽에 블록이 자동으로 쌓여요. 다른 곳에서 해제하면, 이 캔버스의 다른 에이전트가 참조하지 않는 한 블록도 사라져요.

## 구현

- 컴포넌트: `MarkdownBlock` (`src/nodes/MarkdownBlock.tsx`). `MD_SIZE = { width: 240, height: 72 }`. 네 변에 `type="source"` `Handle`이 있고, 캔버스는 `ConnectionMode.Loose`라 어느 쪽에서든 연결돼요.
- 타입: `MarkdownNode = Node<{ path; title }, "markdown_block">` (`types.ts`). 엣지는 `CanvasEdge`의 `data.kind = "reference"`.
- 드래그 데이터: `MD_MIME = "application/x-maestro-md"` (`LibraryPanel.tsx`), 값은 JSON `{ path, title }`.
  - `LibraryPanel`은 제목을 채워 보내요.
  - `RightPanel`의 파일 트리는 `title: ""`로 보내고, 캔버스가 `ipc.mdTitle(path)`(`md_title` → `library::title_of`)로 채워요. 실패하면 파일 이름이에요.
- 놓기: `Canvas.tsx`의 `onDragOver`는 `MD_MIME`이 있을 때만 받고, `onDrop`이 `md_<시간>` id로 노드를 추가해요. 위치는 격자에 맞춰요(`snap`).
- 연결: `linkFor()`가 카드 하나와 마크다운 하나면 `{ kind: "ref" }`(이미 참조 중이면 `null`). `onConnect` → `ipc.setRef(root, card, md, true)`.
- 선 그리기: `worktrees[].refs`와 캔버스의 마크다운 블록 경로가 맞으면 `ref_<블록id>><경로>` 선을 만들어요. 계층 선과 함께 메타에서 다시 그려요.
- 선 지우기: `onEdgesDelete`에서 `reference`면 `ipc.setRef(root, card, md, false)`. 문서가 묶음 안에 있으면(`stacked`) 그 묶음의 문서 가운데 그 에이전트가 참조하는 것 전부를 끊어요.
- 묶음: `linkFor`가 문서가 든 묶음(`stackOf`)과 카드면 `{ kind: "refs", docs }`를 돌려주고 `onConnect`가 문서마다 `ipc.setRef`를 불러요. 그리기는 `shownEdges`에서 묶음 속 문서의 `reference` 선의 시작을 묶음 틀로 옮기고 같은 묶음·같은 에이전트의 선은 첫 번째만 보여요(백엔드의 참조는 여전히 문서마다 하나). 문서가 묶음에 들어올 때는 `stackDocs` 효과가 "묶음의 다른 문서를 모두 참조하는 에이전트"에 새 문서의 참조를 더해요.
- 블록 지우기: `onNodesDelete`에서 지운 블록과 같은 경로의 블록이 남았는지 보고(`left`), 없으면 `worktrees.filter(w => w.refs에 path 포함)`마다 `ipc.setRef(root, w.path, path, false)`.
- 양방향 동기화: `selfRefs` 효과가 이 캔버스 에이전트(`self`)의 `refs` 변화를 보고 없는 블록은 추가(`md_<시간>_<i>`, 에이전트 블록 왼쪽 `x0`에 세로로 쌓기), 사라진 참조의 블록은 제거해요. 링크되지 않고 놓기만 한 블록은 건드리지 않아요.
- 저장: 마크다운 블록과 `reference` 선은 캔버스 상태 파일(`<project_dir>/canvases/<key>.json`)에 함께 저장돼요. `orchestrates` 선만 빼고 저장해요. 다만 `reference` 선도 불러온 뒤 메타 기준으로 다시 그려져요.
- 백엔드: `set_ref` 명령 설명은 "worktree가 비면 모든 에이전트에서 제거"라고 되어 있고 `ipc.setRef` 주석도 같아요. 하지만 `onNodesDelete`는 빈 `worktree`를 쓰지 않고 에이전트마다 따로 불러요.

## 제약과 알려진 문제

- 블록은 제목만 보여 줘요. 내용 보기·편집은 아직 없어요(`MarkdownBlock` 주석 "viewer/editor later").
- 블록을 지울 때 참조를 빼는 대상은 "이 캔버스에 보이는 에이전트"(`worktrees`)뿐이에요. 워크트리 캔버스에서는 그 워크트리 하나만이에요.
- 마크다운 블록끼리, 또는 블록과 프롬프트 블록은 연결할 수 없어요.
- 놓은 블록의 제목은 놓을 때 한 번 읽어요. 파일 제목이 바뀌어도 블록 제목은 갱신되지 않는 것으로 보여요(확인 안 됨).

## 결정 기록

- 참조의 진실은 에이전트 메타(`refs`)예요. 캔버스 블록은 그 표현이라, CLI나 프로젝트 캔버스에서 바꾼 참조도 블록으로 나타나게 했어요(`Canvas.tsx` 주석 "References stay two-way").
