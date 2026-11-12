---
title: 빠른 이동 (Ctrl+K)
area: ui
status: done
files: [src/QuickSwitch.tsx, src/quickModel.ts, src/shortcuts.ts, src/ui/useShortcuts.ts, src/store.ts, src/nodes/Attention.tsx, src/Workspace.tsx, src/App.tsx, src/settings/AboutSection.tsx, src/icons.tsx, src/styles.css, scripts/quick-check.ts, scripts/chrome-check.ts]
related: [features/ui/shortcuts.md, features/ui/attention.md, features/ui/side-panels.md, features/ui/settings-dialog.md, features/ui/window-frame.md, features/ui/split-and-windows.md]
updated: 2026-11-13
---

# 빠른 이동 (Ctrl+K)

## 요약

Ctrl+K를 누르면 창 위쪽에 검색 팔레트가 열려요. 에이전트, 프로젝트, 앱 동작(설정, 확인할 일, 새 에이전트, 마에스트로 만들기, 프로젝트 추가, 에이전트 보기 바꾸기, 패널 토글, 문서, 플레이북)을 이름으로 찾아 Enter로 열거나 실행해요. 목록은 스토어에 있는 것만으로 만들고 새 백엔드 명령은 없어요. 메인 창과 분리 창 모두에서 동작해요. 위쪽 막대에는 버튼이 없고, 키는 설정 → 정보의 단축키 목록에 있어요.

## 동작

**열고 닫기**
- Ctrl+K(macOS는 ⌘K)로 열어요. 채팅 입력창 같은 입력칸 안에서도 열리지만, 터미널과 코드 편집기 안에서는 그쪽 키라서 열리지 않아요. 다른 대화상자가 떠 있으면 열리지 않아요. → [단축키](shortcuts.md)
- 열리면 입력칸에 포커스가 가요. Esc, Ctrl+K, 바깥 누름으로 닫혀요. 그냥 닫으면 포커스가 열기 전 자리로 돌아가고, 항목을 고르면 그 항목이 연 화면에 남아요.
- 열린 채로 Ctrl+,를 누르면 팔레트가 닫히고 설정이 열려요. 열 때 확인할 일 팝오버가 열려 있었으면 닫아요.

**목록**
- 아무것도 입력하지 않으면 모든 항목을 이 순서로 보여요: 에이전트(사이드바 순서, 접힌 프로젝트의 에이전트도 포함) → 프로젝트(사이드바 순서) → 동작.
- 입력하면 맞는 것만, 잘 맞는 순서로 보여요. 이름 전체가 같음 > 이름이 그 글로 시작 > 이름 속 단어가 그 글로 시작 > 이름 속 어딘가에 있음. 점수가 같으면 위 순서를 지켜요. 대소문자는 가리지 않아요.
- 입력 전체가 이름에 없으면 띄어 쓴 단어마다 따로 찾아요. 단어는 이름이나 보조 낱말(에이전트의 프로젝트 이름·브랜치·에이전트 종류·"마에스트로", 프로젝트의 기본 브랜치 등)에 있어야 하고, 모든 단어가 어딘가에 있어야 보여요. 예: "maestro 로그인" = maestro 프로젝트의 "로그인 화면 고치기" 에이전트. 보조 낱말로만 맞은 항목은 이름으로 맞은 항목 뒤에 와요.
- 맞는 게 없으면 "맞는 항목이 없어요". 한 번에 최대 100줄까지 보여요.

**행 (높이 36)**

| 종류 | 모양 | 고르면 |
| --- | --- | --- |
| 에이전트 | 상태 글리프 + 이름 13 + 프로젝트 12 muted + 오른쪽 끝 에이전트 로고 12 | 사이드바 행을 누른 것과 같아요(미리보기 탭, 그 역할의 첫 화면) |
| 프로젝트 | `Folder` 14 + 이름 | 프로젝트 지도(사이드바 프로젝트 머리를 누른 것과 같아요) |
| 동작 | 아이콘 14 + 이름 + 오른쪽 키캡(단축키가 있을 때) | 아래 표 |

| 동작 | 하는 일 | 키캡 |
| --- | --- | --- |
| 설정 | 설정 창 열기 | Ctrl+, |
| 확인할 일 (뒤에 "N개") | 위쪽 막대의 확인할 일 팝오버 열기 | |
| 다음 확인할 일 | 확인할 일이 있을 때만. Ctrl+J와 같아요 | Ctrl+J |
| 새 에이전트 · <프로젝트> | 프로젝트마다 하나, 보고 있는 프로젝트가 먼저. 그 프로젝트 지도를 열고 새 에이전트 창 | |
| 마에스트로 만들기 · <프로젝트> | 마에스트로가 없는 프로젝트마다. 새 에이전트 창(마에스트로 모드) | |
| 프로젝트 추가 | 사이드바 머리의 `FolderPlus`와 같아요 | |
| 에이전트 보기를 터미널로 / 채팅으로 | 설정 → 일반의 에이전트 보기(전역 값)를 다른 쪽으로. 다음에 여는 대화부터 적용돼요 | |
| 왼쪽 / 오른쪽 패널 숨기기·보이기 | 메인 창에서만. 위쪽 막대 패널 버튼과 같아요 | |
| 문서 / 플레이북 | 사이드바 바닥의 문서·플레이북과 같아요 | |

**키**: ↑/↓ 강조 이동(끝에서 반대쪽 끝으로), Enter 고르기, Esc 닫기, Tab은 아무 일도 하지 않아요(포커스가 입력칸에 남아요). 한글 조합 중인 키는 무시해요. 마우스는 움직일 때만 강조를 옮기고 누르면 골라요. 바닥 줄 "Enter 열기 · Esc 닫기 · ↑↓ 이동".

**크기**: 폭 600(좁은 창에서는 좌우 16을 남기고 줄어듦), 창 높이의 20% 아래에서 시작. 입력 줄 44(`Search` 16), 목록 안쪽 4, 행 36(안쪽 0 12, 반경 6), 바닥 28. 반경 10, 테두리 `--mae-line-strong`, 그림자 `--mae-shadow`. 뒤는 대화상자와 같은 어두운 막.

## 구현

- `src/quickModel.ts`(순수, React·스토어 없음): `QuickItem { id, kind: "agent" | "project" | "action", label, keywords? }`, `normQuery`(소문자, 공백 하나로), `matchTier(text, term)`(4 전체 / 3 시작 / 2 단어 시작 / 1 안쪽 / 0 없음. 단어 시작은 공백과 `-_/.·:#()[]` 뒤), `quickScore(item, q)`(입력 전체가 이름에 있으면 110~140, 아니면 단어마다 이름 5~8 또는 보조 낱말 1~4의 평균, 하나라도 없으면 0), `rankQuick(items, q, limit)`(점수 내림차순, 같으면 원래 순서, 빈 입력은 그대로), `moveHot(i, n, dir)`(↑/↓, 돌기).
- `src/QuickSwitch.tsx`: `QuickSwitch({ panels? })`가 스토어 `quickOpen`이 참이면 `document.body` 포털로 `QuickPanel`을 그려요. `useEntries`가 `repos`, `worktrees`, `activeRoot`, 확인할 일 개수(`useAttentionCount`), 설정 `agentView`, 패널 상태로 항목을 만들어요. 에이전트 순서는 `flatAgentOrder(repos, worktrees, [])`(sidebarModel.ts, 접힌 프로젝트도 포함). 에이전트 행의 글리프는 `AgentGlyph`(status.tsx, 승인 대기는 스토어에서 읽음).
- 동작이 부르는 것: `openSettings()`, `setAttentionOpen(true)`, `nextItem(selectAttention(s), selectedPath)` → `select(path, true)`, `setActive(root)` + `openNewAgent({ root, mode })`, `addProject()`(Sidebar.tsx), `useSettings.getState().set({ agentView })`(바뀌면 이 에이전트만 예외는 store.ts 구독이 모두 지워요), `panels.setLeft/setRight`, `openSystem("docs" | "playbooks")`, 에이전트는 `select(path)`, 프로젝트는 `setActive(root)`.
- 열림 상태: 스토어 `quickOpen` / `setQuickOpen`(창마다). 확인할 일 팝오버의 열림도 이번에 스토어 `attentionOpen` / `setAttentionOpen`으로 옮겼어요(`AttentionButton`의 로컬 state였어요). → [확인할 일](attention.md)
- Ctrl+K: `shortcuts.ts`의 `quickSwitch`(범위 `outside-code`), `ui/useShortcuts.ts`의 `ACTIONS.quickSwitch` = `setAttentionOpen(false)` + `setQuickOpen(true)`. 팔레트의 뒤 막이 `.modal-backdrop` 클래스라 열린 동안 전역 단축키 가드는 대화상자로 보고 막아요. 팔레트 입력칸의 `onKeyDown`이 Ctrl+K(닫기), Ctrl+,(닫고 창으로 넘김), ↑/↓/Enter/Esc/Tab을 받아요. 다룬 키는 `stopPropagation`해서 창의 다른 Esc 리스너에 가지 않아요.
- 포커스: 열 때 입력칸, 열린 동안 밖에서 포커스를 가져가면(터미널이 스스로 포커스) 입력칸으로 되돌려요(`focusin` 캡처). 행은 `mousedown`에서 기본 동작을 막아 입력칸이 포커스를 잃지 않아요. 접근성: 입력칸 `role="combobox"` + `aria-activedescendant`, 목록 `role="listbox"`, 행 `role="option"`.
- 마운트: `WindowLayers`(Workspace.tsx)가 그려서 메인 창 `App`과 분리 창 `WorkspaceWindow` 둘 다에 있어요. 메인 창만 `panels`(왼쪽·오른쪽 패널 상태와 세터, App.tsx)를 넘겨요.
- CSS(styles.css): `.quick-backdrop`(z-index 60: 대화상자·토스트 위, 메뉴 65·확인 창 70 아래), `.quick`, `.quick-input`, `.quick-list`, `.quick-row[data-hot]`, `.quick-lead`, `.quick-label`, `.quick-meta`, `.quick-logo`, `.quick-empty`, `.quick-foot`. 키캡은 공용 `kbd.keycap`.
- 아이콘: `Search`를 `src/icons.tsx`에 더했어요.
- 시험: `scripts/quick-check.ts`(`npm test`)가 정규화, `matchTier` 단계, 순위(시작 > 단어 시작 > 안쪽, 전체 일치가 먼저), 대소문자, 보조 낱말이 이름 뒤에 오는 것, 여러 단어(순서 무관, 하나라도 없으면 빠짐), 같은 점수의 원래 순서, 빈 입력, `limit`, `moveHot`을 확인해요. 가드(입력칸 허락, 편집기·터미널·대화상자 거절)는 `scripts/chrome-check.ts`.

## 제약과 알려진 문제

- 파일 찾기(Ctrl+P)는 없어요. 프로젝트 파일 목록을 읽는 백엔드 명령이 없어서예요(계획의 비목표).
- 초성 검색("ㄹㄱㅇ" → "로그인")과 오타를 봐주는 검색은 없어요. 글자가 그대로 들어 있어야 맞아요.
- 최근에 연 순서를 기억하지 않아요. 빈 입력의 순서는 늘 사이드바 순서예요.
- "에이전트 보기를 …로"는 화면 버튼 중 유일하게 전역 에이전트 보기를 바꾸는 곳이에요(설정 → 일반 말고). 이름에 "에이전트 보기"를 넣어 전역 값이라는 것을 드러냈어요.
- 앱을 실행해 보지 않았어요. 타입 검사, 빌드, 순수 함수 시험만 통과했어요. 크기·위치·포커스 이동은 오너가 확인해야 해요.

## 결정 기록

- 2026-11-13 (UI 구조 재구성 단계 O1): 계획(S22)대로 에이전트·프로젝트·동작만 찾고 Rust는 고치지 않았어요. 계획의 동작 목록에 문서·플레이북과 "다음 확인할 일"을 더했어요. 둘 다 사이드바 바닥과 Ctrl+J에 이미 있는 동작이라 새 경로가 아니고, 팔레트에서 이름으로 찾을 수 있으면 좋아서예요.
- 같은 단계: 순위 함수는 `quickModel.ts`로 따로 뒀어요. 순수 모듈만 Node 시험(`--experimental-strip-types`)에서 불러올 수 있고, 팔레트는 스토어·React를 가져와요.
- 같은 단계: 에이전트를 고르면 사이드바 행을 누른 것처럼 미리보기 탭으로 열어요. 이리저리 찾아다니는 동안 탭이 쌓이지 않게 하려는 것이에요(Ctrl+Shift+↑/↓와 같음). 오래 둘 탭은 더블클릭이나 탭 메뉴로 고정해요.
- 같은 단계: "새 에이전트"와 "마에스트로 만들기"는 프로젝트마다 한 줄씩 만들고 보고 있는 프로젝트를 먼저 뒀어요. 프로젝트를 고르는 둘째 단계를 두지 않고, 프로젝트 이름을 입력하면 바로 그 줄이 나와요.
- 같은 단계: Ctrl+K는 입력칸 안에서도 받아요(계획은 입력칸이면 넘기는 가드). 전체 화면에서는 채팅 입력창에 포커스가 있는 때가 많아요. → [단축키](shortcuts.md)
- 같은 단계: 확인할 일 팝오버의 열림 상태를 스토어로 옮겼어요. 팔레트의 "확인할 일"이 같은 팝오버를 열어야 해서예요. 새 팝오버를 따로 만들지 않았어요.
