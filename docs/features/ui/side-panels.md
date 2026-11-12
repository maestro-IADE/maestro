---
title: 사이드바와 양쪽 패널 (숨기기, 너비 조절)
area: ui
status: done
files: [src/App.tsx, src/Sidebar.tsx, src/RightPanel.tsx, src/styles.css, src/store.ts, src/status.tsx, src/ui/Confirm.tsx]
related: [features/ui/window-frame.md, features/editor/file-explorer.md, features/canvas/canvas-controls.md]
updated: 2026-11-13
---

# 사이드바와 양쪽 패널

## 요약

왼쪽 사이드바(저장소·워크트리)와 오른쪽 패널(파일·변경사항)은 위쪽 막대 버튼으로 숨기거나 보일 수 있고, 가운데 화면과의 경계를 끌어 너비를 바꿀 수 있어요. 보임 상태와 너비는 저장돼요.

## 동작

**숨기기**
- 위쪽 막대의 패널 아이콘으로 숨기고 보여요. 접히는 움직임은 모션 설정을 따라요.
- 숨긴 패널의 내용은 그대로 살아 있어서, 다시 열면 펼친 폴더 같은 상태가 남아 있어요.

**너비 조절**
- 경계(10px 틈)에 마우스를 올리면 커서가 바뀌고 얇은 강조색 선이 보여요.
- 끌어서 바꿔요. 왼쪽은 180~480px(기본 260), 오른쪽은 220~560px(기본 300)이에요.
- 경계를 더블클릭하면 기본 너비로 돌아가요.
- 패널을 숨기면 그쪽 손잡이도 사라져요.

**왼쪽 사이드바 내용**
- 맨 위 `문서`·`플레이북` 줄: 누르면 포커스된 묶음이 다른 프로젝트로 바뀌듯 [시스템](system-tab.md)으로 바뀌고 그 보드를 보여 줘요. 지금 보이는 보드의 줄이 강조돼요.
- 그 아래 `프로젝트` 머리줄: 누르면 프로젝트 목록을 접고 펴요(▸는 마우스를 올렸을 때나 접혀 있을 때만 보여요)(localStorage `maestro.projectsShut`). 오른쪽 `+`(`Plus` 아이콘)는 저장소 추가. 고른 폴더가 Git 저장소가 아니면 확인 창("Git 저장소가 아니에요", [Git 저장소로 만들고 추가])으로 묻고, 저장소지만 커밋이 없으면 첫 커밋만 만들지 물어요("아직 커밋이 없어요", [첫 커밋 만들고 추가]). 워커 워크트리는 커밋에서 갈라져 나와야 해서예요. 승낙하면 `init_repo` 후 추가해요. → [메뉴와 확인 창](menus-and-dialogs.md)
- 저장소마다 이름과 기본 브랜치, 그 아래 워크트리가 계층 순서로 나와요. 마에스트로가 먼저, 워커는 들여쓰기돼요. → [워커 계층](../orchestration/worker-hierarchy.md)
- 워크트리 줄에 보이는 것:
  - 상태 글리프와 에이전트 로고 → [테마와 모션](themes-and-motion.md)
  - 마에스트로면 ♛와 굵은 이름(배경 강조는 없어요. 선택처럼 보여서 뺐어요)
  - 상태 글자는 "승인 대기"만 보여요. 작업 중·새 응답·쉬는 중·꺼짐은 상태 글리프의 모양으로 보여요.
- 아래쪽에 설정 버튼이 있어요.

## 구현

- 보임: `usePanelPref("maestro.showLeft" | "maestro.showRight")`, `.app[data-hide-left|right]`
- 너비: `usePanelWidth(key, def, min, max)`(localStorage `maestro.leftWidth`, `maestro.rightWidth`)
  - `.app`의 인라인 스타일로 `--mae-sidebar-w`, `--mae-right-w`를 바꿔요.
- 손잡이: `Resizer`(App.tsx)
  - pointer capture로 끌기를 처리해요.
  - 끄는 동안 `.app[data-resizing]`이 켜져서 다음이 바뀌어요.
    - 너비 애니메이션을 꺼서 패널이 늦게 따라오지 않게 해요.
    - 스테이지의 포인터를 막아서 캔버스나 터미널이 움직임을 가로채지 않게 해요.
- 손잡이는 shell 틈(10px)에 놓인 flex 항목이에요. 음수 margin으로 늘어나는 틈을 상쇄해요.
- 사이드바 상태 글자: `STATUS_LABEL`(Sidebar.tsx). 상태 글자는 일부러 비워 뒀어요(승인 대기만 따로 표시).

## 제약과 알려진 문제

- 창이 아주 좁을 때 양쪽을 최대로 늘리면 가운데가 매우 좁아질 수 있어요. 창 크기에 맞춘 제한은 없어요.
