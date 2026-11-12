---
title: 단축키
area: ui
status: partial
files: [src/shortcuts.ts, src/ui/useShortcuts.ts, src/Sidebar.tsx, src/sidebarModel.ts, src/App.tsx, src/Workspace.tsx, src/ime.ts, scripts/chrome-check.ts, scripts/sidebar-check.ts]
related: [features/ui/side-panels.md, features/ui/window-frame.md, features/ui/settings-dialog.md, features/ui/stage-tabs.md, features/ui/split-and-windows.md]
updated: 2026-11-13
---

# 단축키

## 요약

앱 전체 단축키는 표 하나(`src/shortcuts.ts`)와 가드 하나로 정해요. 지금 동작하는 키는 Ctrl+, (설정), Alt+← / Alt+→ (뒤로 / 앞으로), Ctrl+Shift+↑ / ↓ (이전 / 다음 에이전트)예요. 나머지 키는 표에 이름만 있고 뒤 단계에서 연결해요. 터미널, 코드 편집기, 입력칸에서 쓰는 키는 가로채지 않아요.

## 동작

| 키 | 하는 일 | 어디서 | 지금 |
| --- | --- | --- | --- |
| Ctrl+, | 설정 창 열기 | 어디서든(입력칸·편집기 안, 대화상자 위에서도). 터미널 안은 빼고 | 동작 |
| Alt+← / Alt+→ | 뒤로 / 앞으로(위쪽 막대 ← → 와 같아요) | 입력칸·편집기·터미널 밖, 대화상자가 없을 때 | 동작 |
| Ctrl+J | 다음 확인할 일 | 같음 | 표에만 있음(단계 5) |
| Ctrl+Shift+↑ / ↓ | 사이드바 순서로 이전 / 다음 에이전트를 미리보기 탭으로 열기. 접힌 프로젝트는 건너뛰고 끝에서 멈춰요 | 같음 | 동작 |
| Ctrl+1…9 | 포커스된 묶음의 N번째 탭 | 같음 | 표에만 있음(단계 6) |
| F2 | 에이전트 설정(이름 칸) | 사이드바 목록에 포커스가 있을 때 | 표에만 있음(단계 8) |
| Ctrl+K | 빠른 이동 | 같음 | 표에만 있음(선택 단계) |

- macOS에서는 Ctrl 대신 ⌘도 돼요.
- 한글 입력 중(IME 조합 중)에는 어떤 단축키도 동작하지 않아요. 글자를 조합하던 키가 단축키로 새지 않게 해요.
- Ctrl+B는 일부러 쓰지 않아요. 터미널(readline)이 커서를 한 칸 뒤로 옮기는 키예요.
- 단축키가 있는 버튼은 툴팁과 접근성 이름이 "이름 (키)"예요. 예: "설정 (Ctrl+,)", "뒤로 (Alt+←)".
- 메인 창과 분리 창 모두 같은 키가 동작하고, 그 창에서만 동작해요(분리 창의 Alt+←는 그 창의 기록을 따라가요).

## 구현

- `src/shortcuts.ts`(순수, React·스토어 없음)
  - `SHORTCUTS`: `{ id, label, keys, match(e), scope }` 표. `scope`는 `"anywhere"`(설정), `"outside-fields"`(대부분), `"sidebar"`(F2).
  - 글자·숫자 키는 `e.code`(`KeyJ`, `Digit1`, `Comma`)로 맞춰요. 한글 입력 상태에서는 `e.key`가 자모("ㅓ")라서예요. 화살표와 F2는 `e.key`.
  - `matchShortcut(e)`: 눌린 키에 맞는 항목. `tip(id)`: "이름 (키)".
  - `guardAllows(s, target, dialogOpen)`: 포커스가 `.xterm` 안이면 늘 거절. `"anywhere"`면 허락. 대화상자가 떠 있거나(`dialogOpen`) 포커스가 `.monaco-editor`, `input`, `textarea`, `select`, `[contenteditable]` 안이면 거절. `"sidebar"`는 `.sb-body` 안일 때만.
- `src/ui/useShortcuts.ts`: `useShortcuts()`가 `window`에 `keydown`(버블 단계, 캡처 아님)을 하나 달아요. 이미 `defaultPrevented`거나 `isImeComposing(e)`(ime.ts)면 넘어가요. 대화상자 여부는 `.modal-backdrop`이 있는지로 봐요(확인 창도 이 클래스를 써요). 허락되면 `preventDefault()` 후 `ACTIONS[id]`를 불러요. 지금 `ACTIONS`에는 `settings`(`openSettings()`, 이미 열려 있으면 그대로), `back`/`forward`(`goNav(-1 | 1)`), `prevAgent`/`nextAgent`(`stepAgent(-1 | 1)`, Sidebar.tsx: `flatAgentOrder` + `stepPath`, 순수 함수는 `src/sidebarModel.ts`)가 있어요.
- 창마다 한 번: 메인 창 `App`(App.tsx), 분리 창 `WorkspaceWindow`(Workspace.tsx).
- 시험: `scripts/chrome-check.ts`(`npm test`)가 키 맞추기, 툴팁, 가드(터미널·편집기·입력칸·대화상자·사이드바)를 확인해요. 에이전트 순서와 한 걸음은 `scripts/sidebar-check.ts`.
- 사이드바 목록 안의 ↑/↓·Enter·Shift+F10은 이 표 밖에서 `Sidebar.tsx`가 처리해요(포커스 이동, 열기, 메뉴). → [사이드바](side-panels.md)

## 제약과 알려진 문제

- 키를 바꾸는 설정은 없어요.
- 캔버스의 Ctrl+Z / Ctrl+Shift+Z와 코드 편집기의 Ctrl+S는 이 표 밖에서 각자 처리해요(`Canvas.tsx`, `CodeView.tsx`).
- 확인 창이 떠 있는 동안에는 확인 창이 캡처 단계에서 키를 먼저 받아 끊기 때문에 Ctrl+,도 동작하지 않아요.
- 실제 앱에서 키를 눌러 보지 않았어요(가드는 코드와 순수 함수 시험으로만 확인).

## 결정 기록

- 2026-11-13 (UI 구조 재구성 단계 2): 전역 단축키가 하나도 없던 상태에서 표 하나와 가드 하나로 시작했어요. 단축키를 컴포넌트마다 흩어 달면 터미널·편집기와 겹치는 키를 막는 규칙도 흩어져요. Ctrl+,만 입력칸 안에서도 받는 것은 VS Code와 같아요(입력칸에서 쓰는 키가 아니에요). 뒤 단계 키를 미리 표에 적어 둔 것은 툴팁 글(`tip`)과 키 겹침을 한곳에서 보려는 것이에요.
- 표와 가드를 React 훅과 다른 파일로 나눴어요. 훅은 스토어와 `ime.ts`(창 리스너를 다는 모듈)를 가져와서 Node 시험에서 불러올 수 없어요.
- 2026-11-13 (UI 구조 재구성 단계 4): Ctrl+Shift+↑/↓를 연결했어요. 순서는 사이드바에 보이는 순서(프로젝트 목록 순, 그 안은 계층 순)이고 접힌 프로젝트는 건너뛰어요. 보이지 않는 에이전트로 건너가면 어디 있는지 놓치기 쉬워서예요. 끝에서는 돌지 않고 멈춰요. 미리보기 탭으로 열어 넘기는 동안 탭이 쌓이지 않게 했어요.
