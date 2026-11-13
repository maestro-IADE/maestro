---
title: 구조 그리기 (블록, 영역, 화살표)
area: canvas
status: in-progress
files: [src/nodes/StructNodes.tsx, src/drawing.ts, src/Canvas.tsx, src/types.ts, src/nodes/AgentNode.tsx, src/styles.css, src-tauri/src/agent.rs, src-tauri/src/control.rs]
related: [features/canvas/two-level-canvas.md, features/canvas/markdown-blocks.md, features/canvas/canvas-controls.md, features/orchestration/maestro-cli.md]
updated: 2026-11-13
---

# 구조 그리기

## 요약

에이전트 캔버스에 FigJam처럼 아키텍처나 구조를 그려요. **블록**(이름 + 마크다운 내용), 블록을 묶는 **영역**, 둘을 잇는 **화살표**가 있어요. 그린 구조는 캔버스와 함께 저장되고, 선택하면 그 캔버스의 에이전트가 `maestro selection`이나 입력창 칩으로 내용까지 받아요.

## 동작

- **만들기:** 캔버스 왼쪽 위 도구 막대의 `블록 ▾` 메뉴에서 "구조 블록" 또는 "영역". 보이는 화면 가운데에 생기고, 바로 이름 창이 열려요.
- **블록**
  - 겉에는 이름과 내용 첫 줄이 보여요. 내용이 있으면 오른쪽 위 모서리가 접혀 보여요.
  - 더블클릭: 이름과 마크다운 내용 창. `보기`(렌더링) / `편집`(마크다운). 내용이 없으면 편집으로 열려요. 입력은 바로 반영돼요. Esc나 바깥을 누르면 닫혀요.
  - 선택하면 모서리를 끌어 크기를 바꿔요.
- **영역**
  - 점선 테두리에 왼쪽 위 이름. 더블클릭하면 이름을 바꿔요. 선택하면 크기를 바꿔요.
  - 블록, 마크다운 블록, 다른 영역을 끌어서 **가운데가 영역 안에 오게 놓으면** 그 영역에 들어가요(겹치면 가장 안쪽 영역). 영역을 옮기면 안의 것도 함께 움직여요. 밖으로 끌어 놓으면 빠져나와요.
  - 영역을 지우면 안에 있던 것은 남고 한 단계 바깥으로 나와요(같이 선택해서 지우면 함께 지워져요).
  - 영역은 늘 다른 블록 아래에 깔려요.
- **화살표**
  - 블록이나 영역에 마우스를 올리면 네 변에 점이 나와요. 점을 끌어 다른 블록·영역(또는 마크다운 블록, 에이전트 블록)에 놓으면 화살표가 생겨요. 한쪽이 블록이나 영역이면 화살표예요.
  - 꺾인 직각 선이에요. 더블클릭하면 설명(라벨)을 적어요. 선택하면 ×로 지워요(Delete 키도 돼요).
- **에이전트에게 넘기기:** 블록·영역을 선택한 채 아래 입력창을 쓰면 선택 칩에 같이 붙어요.
  - 블록: 이름, 속한 영역, 화살표가 가는 곳(라벨), 마크다운 내용.
  - 영역: 이름, 안에 든 것, 화살표가 가는 곳.
  - 에이전트는 `maestro selection`으로도 같은 내용을 읽어요.
- 되돌리기(Ctrl+Z)는 다른 블록과 같아요.
- **에이전트가 그리기:** 에이전트는 `maestro draw --file 구조.json`으로 자기 캔버스에 영역·블록·화살표를 그려요. 새 것은 지금 있는 것 오른쪽에 자동으로 배치되고(영역은 안의 것을 세 개씩 줄지어 담는 크기), 같은 이름이 이미 있으면 내용만 바뀌어요. 그린 뒤 화면이 그 구조에 맞춰져요. 캔버스가 닫혀 있었으면 열 때 그려져요.
- "아키텍처 먼저" 플레이북이 이걸 써요: 프롬프트 하나("○○ 만들어 줘")로 마에스트로가 설계 → 캔버스에 그리기 → 블록마다 워커 → 합치기까지 해요.

## 구현

- 노드 종류: `struct_block`(`StructNode`, data `{ title, body? }`), `zone`(`ZoneNode`, data `{ title }`). 컴포넌트는 `nodes/StructNodes.tsx`의 `StructBlock`, `ZoneFrame`. 크기 `BLOCK_SIZE` 8×3칸, `ZONE_SIZE` 18×12칸. `NodeResizer`로 크기 조절.
- 영역 = React Flow 부모 노드(`parentId`). `Canvas.nest()`가 `onNodeDragStop`에서 놓인 노드의 가운데를 절대 좌표(`getInternalNode().internals.positionAbsolute`)로 재서 안에 든 가장 작은 영역을 부모로 정하고 위치를 그 영역 기준으로 바꿔요. 자기 자신·자기 안의 영역은 후보에서 빼요. `parentsFirst()`로 부모가 배열 앞에 오게 정렬해요(React Flow 요구사항).
- 영역은 `zIndex: ZONE_Z`(-1000)로 늘 아래에 그려요(선택해도). `normalize`가 불러올 때마다 다시 붙여요.
- 영역 지우기: `onBeforeDelete`가 React Flow가 자동으로 끼운 자식(선택되지 않은 것)을 빼고, 그 자식들을 남은 가장 가까운 조상으로 옮겨요.
- 화살표: edge `type: "arrow"`, `data.kind: "arrow"`, `data.label`. `ArrowEdge`(smoothstep). 라벨 편집은 `onEdgeDoubleClick` → `updateEdgeData({ editing: true })`, 저장할 때 `editing`은 빼요. 연결 규칙은 `isArrow`(한쪽이 `struct_block`/`zone`, 자기 자신 제외).
- 저장: 캔버스 JSON의 노드에 `parentId`, 엣지에 `sourceHandle`/`targetHandle`이 새로 들어가요.
- 에이전트 그리기: CLI `draw` → `control::drawing()`이 JSON을 읽고 이름을 검사 → `AppState.drawings`(워크트리별 대기열)에 넣고 `maestro://draw` 이벤트. 그 워크트리의 캔버스가 `take_canvas_drawings`로 꺼내 `applyDrawing()`(src/drawing.ts)으로 노드·엣지를 만들어요. 화살표 방향(변)은 영역 위치를 더한 절대 좌표로 골라요. 테스트: `drawing_checks_names`.
- 선택: `Picked`에 `block`(`title, body, area, to`)과 `area`(`title, holds, to`). `pickedLine`이 입력창 앞머리 글로 바꿔요. 백엔드는 그대로 JSON으로 저장·전달(`set_canvas_selection` → `maestro selection`).

## 결정 기록

- 2026-11-13 (UI 구조 재구성 단계 11): `+ 블록`과 `+ 영역` 두 버튼을 `블록 ▾` 메뉴 하나로 합쳤어요. 도구 막대 버튼이 8개에서 6개(워커 캔버스는 4개)로 줄고, 블록·영역 추가는 한 번 더 눌러요.

- 블록 내용은 파일이 아니라 캔버스 JSON에 둬요. 이름만 있는 블록과 내용 있는 블록을 같은 종류로 다루고, 파일이 쌓이지 않게 하려고요. 파일로 관리할 문서는 기존 마크다운 블록(파일 참조)을 쓰면 돼요.
- 영역에 넣는 기준은 "놓은 블록의 가운데"예요. 살짝 겹친 것까지 들어가 버리지 않게요.
- 에이전트 그리기는 캔버스 저장 파일을 직접 고치지 않고 이벤트로 열린 캔버스에 넘겨요. 열린 캔버스가 자동 저장하면서 덮어쓰는 경합을 피하려고요.
- 아직 없는 것: 색, 모양 종류, 복사·붙여넣기, 정렬 보조선, 에이전트가 그림을 지우는 명령.
