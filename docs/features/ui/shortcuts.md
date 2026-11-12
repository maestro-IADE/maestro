---
title: 단축키
area: ui
status: partial
files: [src/shortcuts.ts, src/ui/useShortcuts.ts, src/QuickSwitch.tsx, src/settings/AboutSection.tsx, src/store.ts, src/Sidebar.tsx, src/sidebarModel.ts, src/attention.ts, src/nodes/Attention.tsx, src/App.tsx, src/Workspace.tsx, src/ime.ts, scripts/chrome-check.ts, scripts/sidebar-check.ts, scripts/attention-check.ts]
related: [features/ui/attention.md, features/ui/side-panels.md, features/ui/window-frame.md, features/ui/settings-dialog.md, features/ui/stage-tabs.md, features/ui/split-and-windows.md, features/ui/quick-switch.md]
updated: 2026-11-13
---

# 단축키

## 요약

앱 전체 단축키는 표 하나(`src/shortcuts.ts`)와 가드 하나로 정해요. 지금 동작하는 키는 Ctrl+, (설정), Alt+← / Alt+→ (뒤로 / 앞으로), Ctrl+J (다음 확인할 일), Ctrl+Shift+↑ / ↓ (이전 / 다음 에이전트), Ctrl+1…9 (N번째 탭), F2 (에이전트 설정), Ctrl+K (빠른 이동)예요. 표의 키는 모두 동작하고, 설정 → 정보에 같은 표가 보여요. 터미널, 코드 편집기, 입력칸에서 쓰는 키는 가로채지 않아요.

## 동작

| 키 | 하는 일 | 어디서 | 지금 |
| --- | --- | --- | --- |
| Ctrl+, | 설정 창 열기 | 어디서든(입력칸·편집기 안, 대화상자 위에서도). 터미널 안은 빼고 | 동작 |
| Alt+← / Alt+→ | 뒤로 / 앞으로(위쪽 막대 ← → 와 같아요) | 입력칸·편집기·터미널 밖, 대화상자가 없을 때 | 동작 |
| Ctrl+J | 다음 확인할 일을 고정 탭으로 열기. 지금 보는 에이전트의 항목 다음 것, 끝이면 처음으로 돌아요 | 같음 | 동작 |
| Ctrl+Shift+↑ / ↓ | 사이드바 순서로 이전 / 다음 에이전트를 미리보기 탭으로 열기. 접힌 프로젝트는 건너뛰고 끝에서 멈춰요 | 같음 | 동작 |
| Ctrl+1…9 | 포커스된 묶음의 N번째 탭(탭 줄에 보이는 순서, 프로젝트 탭이 1번). 탭이 그보다 적으면 아무 일도 없어요 | 같음 | 동작 |
| F2 | 에이전트 설정(이름 칸) | 사이드바 목록에 포커스가 있을 때(에이전트가 붙은 세션 행에서만) | 동작 |
| Ctrl+K | 빠른 이동 열기(열려 있으면 닫기) | 입력칸 안(채팅 입력창 등)에서도. 터미널·코드 편집기 안은 빼고(편집기의 Ctrl+K 묶음 키), 대화상자가 없을 때 | 동작 → [빠른 이동](quick-switch.md) |

- macOS에서는 Ctrl 대신 ⌘도 돼요.
- 한글 입력 중(IME 조합 중)에는 어떤 단축키도 동작하지 않아요. 글자를 조합하던 키가 단축키로 새지 않게 해요.
- Ctrl+B는 일부러 쓰지 않아요. 터미널(readline)이 커서를 한 칸 뒤로 옮기는 키예요.
- 단축키가 있는 버튼은 툴팁과 접근성 이름이 "이름 (키)"예요. 예: "설정 (Ctrl+,)", "뒤로 (Alt+←)".
- 메인 창과 분리 창 모두 같은 키가 동작하고, 그 창에서만 동작해요(분리 창의 Alt+←는 그 창의 기록을 따라가요).

## 구현

- `src/shortcuts.ts`(순수, React·스토어 없음)
  - `SHORTCUTS`: `{ id, label, keys, match(e), scope }` 표. `scope`는 `"anywhere"`(설정), `"outside-fields"`(대부분), `"outside-code"`(Ctrl+K), `"sidebar"`(F2).
  - 글자·숫자 키는 `e.code`(`KeyJ`, `Digit1`, `Comma`)로 맞춰요. 한글 입력 상태에서는 `e.key`가 자모("ㅓ")라서예요. 화살표와 F2는 `e.key`.
  - `matchShortcut(e)`: 눌린 키에 맞는 항목. `tip(id)`: "이름 (키)". `keysOf(id)`: 키 글만("Ctrl+J"), 확인할 일 버튼처럼 툴팁 문장을 따로 짤 때.
  - `guardAllows(s, target, dialogOpen)`: 포커스가 `.xterm` 안이면 늘 거절. `"anywhere"`면 허락. 대화상자가 떠 있거나(`dialogOpen`) 포커스가 `.monaco-editor`, `input`, `textarea`, `select`, `[contenteditable]` 안이면 거절. `"outside-code"`는 편집기 안만 거절하고 입력칸은 허락해요(편집기의 글 상자도 textarea라 편집기를 먼저 봐요). `"sidebar"`는 `.sb-body` 안일 때만.
- `src/ui/useShortcuts.ts`: `useShortcuts()`가 `window`에 `keydown`(버블 단계, 캡처 아님)을 하나 달아요. 이미 `defaultPrevented`거나 `isImeComposing(e)`(ime.ts)면 넘어가요. 대화상자 여부는 `.modal-backdrop`이 있는지로 봐요(확인 창도 이 클래스를 써요). 허락되면 `preventDefault()` 후 `ACTIONS[id]`를 불러요. 지금 `ACTIONS`에는 `settings`(`openSettings()`, 이미 열려 있으면 그대로), `back`/`forward`(`goNav(-1 | 1)`), `prevAgent`/`nextAgent`(`stepAgent(-1 | 1)`, Sidebar.tsx: `flatAgentOrder` + `stepPath`, 순수 함수는 `src/sidebarModel.ts`), `nextAttention`(`nextItem(selectAttention(s), selectedPath)` → `select(path, true)`, 순수 함수는 `src/attention.ts`), `tabN`(`tabNumber(e)`로 1~9를 읽고 `barTabs(포커스된 묶음)`의 그 자리를 `openTab`, store.ts), `rename`(포커스된 요소에서 `closest("[data-agent-path]")`의 경로로 `openAgentSettings({ path, focus: "name" })`), `quickSwitch`(확인할 일 팝오버를 닫고 `setQuickOpen(true)`)가 있어요. 표의 모든 id에 동작이 있어서 `ACTIONS`는 `Record<ShortcutId, …>`예요. 동작 함수는 키 이벤트를 받아요(`tabN`, `rename`이 써요). → [에이전트 설정](../agents/agent-settings.md) → [확인할 일](attention.md), [가운데 화면 탭](stage-tabs.md)
- 창마다 한 번: 메인 창 `App`(App.tsx), 분리 창 `WorkspaceWindow`(Workspace.tsx).
- 설정 → 정보의 "단축키" 묶음(`AboutSection.tsx`)이 `SHORTCUTS`를 그대로 행으로 그려요(이름, 키캡, F2만 "사이드바 목록에서" 설명). 표에 키를 더하면 거기에도 저절로 보여요.
- 시험: `scripts/chrome-check.ts`(`npm test`)가 키 맞추기, 툴팁, 가드(터미널·편집기·입력칸·대화상자·사이드바, Ctrl+K의 입력칸 허락과 편집기 거절)를 확인해요. 에이전트 순서와 한 걸음은 `scripts/sidebar-check.ts`.
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
- 2026-11-13 (UI 구조 재구성 단계 6): Ctrl+1…9를 연결했어요. 숫자는 위쪽 숫자 줄(`Digit1`~`Digit9`)만 받고 숫자 패드는 넘겨요. 탭 줄에 보이는 순서대로 세서 프로젝트 탭이 1번이에요. 브라우저처럼 Ctrl+9를 "마지막 탭"으로 하지 않은 것은 표에 적힌 뜻(N번째 탭)을 그대로 지키려는 것이에요.
- 2026-11-13 (UI 구조 재구성 단계 5): Ctrl+J를 연결했어요. Ctrl+Shift+↑/↓와 달리 끝에서 처음으로 돌아요. 확인할 일은 처리하면 목록에서 빠지니 돌아도 같은 곳을 맴돌지 않아요. 고정 탭으로 여는 것은 승인하러 간 탭이 다음 미리보기에 밀려 사라지지 않게 하려는 것이에요.
- 2026-11-13 (UI 구조 재구성 단계 8): F2를 묶었어요. 포커스된 요소에서 `[data-agent-path]`(에이전트가 붙은 세션 행만 가짐)를 찾아 `openAgentSettings({ path, focus: "name" })`을 불러요. 프로젝트 머리와 에이전트 없는 행에서는 아무것도 하지 않아요.
- 2026-11-13 (UI 구조 재구성 단계 O1): Ctrl+K를 연결했어요. 계획의 가드는 다른 키와 같았지만(입력칸이면 넘김) 입력칸 안에서도 받게 했어요. 전체 화면에서는 채팅 입력창에 포커스가 있는 때가 많아서 입력칸을 빼면 거의 쓸 수 없고, 입력칸에서 Ctrl+K가 하는 일이 없어요. 코드 편집기(Monaco의 Ctrl+K 묶음 키)와 터미널(readline의 줄 끝까지 지우기)에서는 넘겨요. 이를 위해 범위 `outside-code`를 더했어요. 팔레트가 열려 있으면 대화상자로 쳐서 이 가드는 막고, 팔레트 입력칸이 Ctrl+K를 받아 닫아요.
