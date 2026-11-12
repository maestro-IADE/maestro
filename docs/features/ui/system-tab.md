---
title: 시스템 (문서·플레이북 대시보드)
area: ui
status: done
files: [src/LibraryPanel.tsx, src/StageTabs.tsx, src/Workspace.tsx, src/Sidebar.tsx, src/store.ts, src/styles.css, src-tauri/src/library.rs]
related: [features/orchestration/library-and-personas.md, features/ui/stage-tabs.md, features/ui/side-panels.md, features/ui/split-and-windows.md]
updated: 2026-11-13
---

# 시스템

## 요약

Maestro가 관리하는 공용 문서(페르소나, 지침, PRD, 디자인, RAG)와 플레이북을 카드로 보여 주는 대시보드예요. 사이드바 바닥의 `문서`·`플레이북`을 누르면 **다른 프로젝트를 여는 것과 똑같이** 열려요. 묶음의 첫 탭이 `시스템` 프로젝트 탭이 되고, 거기서 연 문서는 그 뒤 탭으로 붙어요.

## 동작

- **여는 법**
  - 사이드바 `문서` 또는 `플레이북`: 포커스된 묶음이 시스템으로 바뀌어요. 원래 프로젝트의 탭은 기억해 뒀다가 그 프로젝트로 돌아가면 되살려요(프로젝트 전환과 같아요). 이미 시스템을 보고 있는 묶음이 있으면 그 묶음으로 가요.
  - 시스템을 보는 중에는 사이드바의 해당 줄이 강조돼요.
- **탭**
  - 첫 탭: 격자 아이콘과 `시스템`. 프로젝트 탭처럼 누르면 대시보드로 돌아오고, ×로 닫으면 묶음이 닫혀요(탭은 기억해요). 끌어서 분할·새 창으로 옮길 수 있어요.
  - 카드를 누르면 그 문서가 시스템 묶음의 파일 탭으로 열려요(한 번 클릭은 미리보기 탭).
- **화면**
  - 위쪽 막대: `문서 | 플레이북` 전환, (문서일 때) `+ 새 분류`, ↻(다시 읽기). 설명 글은 두지 않아요.
  - 분류마다 접고 펴는 머리줄: ▸, 이름, 개수 배지. 머리줄을 누르면 접혀요(접힌 분류는 localStorage `maestro.systemShut`에 기억). 마우스를 올리면 `+`(새 문서)와 `⤓`(바깥 마크다운을 복사해서 가져오기)가 나와요.
  - 카드: 제목과 본문 첫 줄(두 줄까지). 마우스를 올리면 `×`(확인 후 지우기).
  - 카드를 캔버스로 끌면 그 파일을 **참조**하는 마크다운 블록이 생겨요(복사하지 않아요). 다른 묶음의 캔버스로 끌면 돼요.
  - 여백: 본문 좌우 32px, 내용 폭 최대 1200px, 카드 칸은 최소 240px에 12px 간격, 카드는 분류 이름보다 한 단 들여써요.

## 구현

- `SYSTEM_ROOT = "maestro:system"`(store.ts): 묶음의 `root`로 쓰는 가짜 프로젝트예요. 저장소 목록에 없으니 오른쪽 패널·프로젝트 맵 등 저장소를 찾는 곳은 저절로 비어요.
- `openSystem(view)`: `systemView`를 정하고 `setActive(SYSTEM_ROOT)`. 묶음 전환·탭 기억(`parked`)·분할·새 창은 프로젝트와 같은 코드를 그대로 써요.
- 문서 열기: `openEditor`의 `rootOf`가 저장소·워크트리에 없는 경로를 지금 프로젝트(`activeRoot`)로 보내서, 시스템 묶음에서 연 문서는 그 묶음 탭이 돼요.
- `StageTabs`: `root === SYSTEM_ROOT`이면 프로젝트 탭 자리에 `시스템`(격자 아이콘)을 그려요.
- `GroupView`(Workspace.tsx): `root === SYSTEM_ROOT`이면 프로젝트 맵 대신 `SystemDashboard`(LibraryPanel.tsx). 화면은 `Board`가 머리 막대까지 그려요. 목록은 `useDocs`가 `library_list`/`playbook_list`로 읽고, 카드 첫 줄은 백엔드 `MdFile.summary`.
- 뒤로/앞으로 기록(`navValid`)은 저장소에 있는 프로젝트만 받아서 시스템은 기록에 남지 않아요.

## 결정 기록

- 사이드바의 `프로젝트 | 문서 | 플레이북` 탭을 없애고, 문서·플레이북은 넓은 가운데 화면의 카드 대시보드로 옮겼어요.
- 처음엔 모든 묶음 맨 왼쪽의 닫히지 않는 "시스템 탭"(묶음마다 켜짐/꺼짐 상태)으로 만들었는데, 탭 줄이 엉켰어요. 사용자 의견대로 다른 프로젝트처럼 여는 방식으로 바꿨어요. 상태가 따로 없고 프로젝트 전환 코드를 그대로 써서 단순해요.
