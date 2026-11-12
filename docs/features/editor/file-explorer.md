---
title: 오른쪽 패널 (파일 탐색기, 변경사항)
area: editor
status: done
files: [src/RightPanel.tsx, src-tauri/src/main.rs, src-tauri/src/worktree.rs, src/styles.css]
related: [features/editor/code-editor.md, features/editor/drag-attach.md, features/ui/side-panels.md]
updated: 2026-11-12
---

# 오른쪽 패널

## 요약

오른쪽 패널에는 "파일"과 "변경사항" 두 탭이 있어요. 지금 고른 워크트리(없으면 원본 저장소)의 파일 트리와 git 변경 목록을 보여 줘요. 파일을 누르면 편집기가, 변경된 파일을 누르면 diff 뷰가 열려요.

## 동작

- **머리글:** 워크트리 브랜치 이름이 보여요. 워크트리를 고르지 않았으면 "<저장소> (원본)"이 보여요.
- **파일 탭**
  - 폴더를 누르면 펼쳐져요. 파일을 누르면 편집기가 열려요.
  - 아이콘으로 파일, 닫힌 폴더, 열린 폴더를 구분해요. 폴더 아이콘은 강조색이에요.
  - 지금 편집기에 열린 파일은 선택 배경이 깔려요.
  - 긴 이름은 "…"로 자르고, 마우스를 올리면 전체 경로가 보여요.
  - `.git`은 숨겨요. 폴더가 먼저 오고, 이름은 대소문자를 무시하고 정렬해요.
- **변경사항 탭**
  - `git status` 결과를 "N개 파일 변경"과 목록으로 보여 줘요.
  - 상태 글자 색: M(수정)은 경고색, A와 ?(새 파일)는 초록, D(삭제)는 빨강이에요.
  - 에이전트 상태가 바뀔 때(작업이 끝났을 가능성이 클 때) 다시 읽어요. ↻로 직접 새로고침도 돼요.
  - 파일을 누르면 HEAD와 비교하는 diff 뷰가 열려요.
- 파일과 폴더를 끌어서 첨부할 수 있어요. → [드래그 첨부](drag-attach.md)

## 구현

- `RightPanel`, `Tree`, `TreeItem`, `TreeIcon`, `Changes`(RightPanel.tsx)
- `list_dir(path)`: 폴더 항목을 정렬해서 돌려줘요.
- `git_status(worktree_path)`: `worktree::status` → `git status --porcelain=v1 -z -uall`. `-z`라 한글·공백·따옴표가 든 이름도 따옴표나 `í…` 같은 이스케이프 없이 그대로 와요(`core.quotePath` 설정과 상관없음). 이름 바뀐 파일은 새 이름을 써요.
- 아이콘 경로는 `TREE_ICONS`(Lucide file / folder / folder-open)예요. 드래그 칩도 이 경로를 써요.

## 제약과 알려진 문제

- 파일 트리는 자동으로 새로고침되지 않아요. 에이전트가 새 파일을 만들면 폴더를 다시 접었다 펴야 보여요.
- 이름 바뀐 파일의 diff는 HEAD 쪽을 새 이름으로 찾아서 새 파일처럼 보여요.
