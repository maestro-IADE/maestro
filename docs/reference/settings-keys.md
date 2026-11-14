---
title: 설정 키 목록
area: reference
status: done
files: [src/settings.ts, src/palette.ts, src/settingsMigrate.ts, scripts/view-check.ts, src/App.tsx, src/store.ts, src/StageTabs.tsx, src/UpdateNotice.tsx, src/Sidebar.tsx, src/RightPanel.tsx, src/nodes/PrCard.tsx, src/rightPanelModel.ts]
related: [reference/storage-layout.md, architecture/overview.md]
updated: 2026-11-14
---

# 설정 키 목록

## 요약

사용자 설정은 `src/settings.ts`의 `useSettings`(zustand `persist`)가 localStorage의 `maestro.settings` 키 하나에 JSON으로 저장해요. 그 밖에 패널 표시·너비와 마지막 위치를 `App.tsx`와 `store.ts`가 각각 다른 localStorage 키에 저장해요. 모두 프런트엔드(웹뷰) 저장소라서 디스크의 `~/.maestro`와는 따로예요.

## `maestro.settings` (useSettings)

| 키 | 타입 | 기본값 | 뜻 |
|---|---|---|---|
| `customThemes` | `{ id, name, base, accent, contrast }[]` | `[]` | 내 팔레트. `base`·`accent`는 `#rrggbb`, `contrast`는 0–100. 나머지 색은 `derivePalette`(palette.ts)가 계산해요. 저장 형식 버전은 그대로(없으면 빈 목록) |
| `theme` | `"default"` / `"dark"` / `"graphite"` / `"mocha"` / `"light"` / `"ivory"` / `"stone"` / `"custom:<id>"` | `"default"` | 테마. `<html data-theme>`로 적용돼요. `custom:<id>`는 `customThemes`의 팔레트이고, `data-theme="custom"`(라이트면 `"custom-light"`)에 색 토큰을 `<html>` 인라인으로 넣어요. 목록에 없는 값(예: 지워진 `neon`, `forest`, 지운 팔레트)은 `default`으로 보여요. `light`, `ivory`, `stone`(과 밝은 내 팔레트)은 캔버스와 xterm도 밝은 팔레트를 써요. 저장 형식 v1로 올라갈 때 한 번 `default`으로 바뀌어요 |
| `easing` | `"standard"` / `"smooth"` / `"bouncy"` / `"linear"` | `"standard"` | 모든 UI 모션의 곡선. CSS 변수 `--mae-ease` |
| `speed` | `"fast"` (150ms) / `"normal"` (240ms) / `"slow"` (360ms) | `"normal"` | 모션 길이. CSS 변수 `--mae-dur` |
| `motion` | `boolean` | `true` | 모션 켜기. 끄거나 OS가 `prefers-reduced-motion: reduce`면 `--mae-dur`가 `0ms` |
| `canvasBg` | `"dots" \| "lines" \| "cross" \| "none"` | `"lines"` | 캔버스 배경 무늬. 두 캔버스 모두 `CanvasBackground`로 그려요 |
| `edgeShape` | `"curve" \| "straight"` | `"curve"` | 연결선 모양(곡선 / 직선. 직선은 가로·세로로만 꺾여요. 예전 `"step"`은 직선으로 읽어요). 에이전트 캔버스와 프로젝트 지도의 모든 선(edgePath.ts) |
| `lineLabels` | `"always" \| "selected"` | `"always"` | 선 라벨을 언제 보일지(항상 / 선택할 때). 설정 → 모양 → 캔버스의 "선 라벨", 캔버스 범례의 토글. → [관계](../features/canvas/relations.md) |
| `canvasGap` | `number` | `24` | 캔버스 무늬 간격(px): 12 / 24 / 48 / 96 |
| `canvasAlpha` | `number` | `100` | 캔버스 무늬 진하기(%), 10–100 |
| `panelStyle` | `"cards" \| "lines" \| "full"` | `"cards"` | 패널 구분: 분리 / 통합 / 와이드(여백 없음). `<html data-panels>`로 들어가요 |
| `autoSave` | `boolean` | `false` | 코드 편집기 자동 저장(입력을 멈추고 1초 뒤). 설정 → 일반 → 파일 편집 |
| `defaultAgent` | `string` | `"auto"` | 새 에이전트의 기본 에이전트 CLI id. `"auto"`는 쓸 수 있는 첫 에이전트. 설정 → 에이전트 → 기본값 카드 한 곳에서만 골라요 |
| `defaultPermission` | `string` | `"acceptEdits"` | 새 에이전트가 시작할 권한 모드(설정 → 에이전트 "새 에이전트의 권한"). 고른 에이전트가 받지 않는 값이면 새 에이전트 창이 `acceptEdits`(없으면 그 에이전트의 첫 모드)로 바꿔 시작해요(`permissionFor`, agents.tsx). 이미 있는 에이전트는 에이전트 설정 창에서 바꿔요 |
| `disabledAgents` | `string[]` | `[]` | 설치돼 있지만 꺼 둔 에이전트(설정 → 에이전트 행의 스위치). 새 에이전트 창에서 고를 수 없어요 |
| `agentModels` | `Record<string, string>` | `{}` | 에이전트별 새 에이전트의 기본 `--model`(설정 → 에이전트 → 관리 → 기본 모델). 없으면 CLI 기본값 |
| `assistantAgent` | `string` | `"claude-code"` | 가벼운 백그라운드 작업(프로젝트 캔버스 요약)에 쓸 에이전트 CLI, 또는 `"off"` |
| `assistantModel` | `string` | `""` | 보조 모델. `""`은 그 에이전트의 싼 기본값(Claude는 Haiku) |
| `agentLayout` | `"full"` / `"canvas"` | `"full"` | Lead가 아닌 에이전트의 새 탭이 처음 여는 화면(전체 화면 또는 블록이 있는 캔버스). 탭 줄의 전환 버튼은 그 탭만 바꾸고 이 값을 쓰지 않아요(예전에는 마지막으로 고른 값이 여기 남았어요). 설정 → 일반 → "열 때 화면 — 에이전트" → [2단계 캔버스](../features/canvas/two-level-canvas.md) |
| `maestroLayout` | `"full"` / `"canvas"` | `"canvas"` | Lead의 새 탭이 처음 여는 화면. 새 키라 저장 형식 버전은 그대로예요(없으면 기본값). 설정 → 일반 → "열 때 화면 — Lead" |
| `agentView` | `"chat"` / `"cli"` (`AgentView`) | `"chat"` | 에이전트 보기: 전체 화면, 캔버스 블록, 분리 창이 모두 쓰는 채팅/터미널. 바꾸면 다음에 여는 대화부터 이 보기로 열리고(열린 패널은 보기만 바뀌고 돌고 있는 대화는 터미널 위 띠로 넘겨요), "이 에이전트만" 예외(스토어 `viewOverride`, 저장 안 함)가 모두 없어져요. 메인 창이 `set_view_default`로 백엔드에 알려 화면 없는 시작도 이 값을 따라요. 설정 → 일반 → "에이전트 보기"에서만 써요(헤더 아이콘·메뉴·"터미널에서 답하기"는 쓰지 않아요). 저장 형식 v3에서 `fullView`의 값을 이어받았어요 → [채팅 뷰](../features/agents/chat-view.md) |
| `awayMinutes` | `number` (선택지 `5`, `15`, `60`, `0`) | `15` | 떠난 지 이 분이 지난 프로젝트의 쉬는 에이전트를 멈춰요. `0`은 끄지 않음. `ipc.setAway`로 백엔드에 전달. 설정 → 일반 → 자리 비움 |
| `sidebarDensity` | `"two"` / `"one"` | `"two"` | 사이드바 세션 행: 두 줄(44px, 이름 + 브랜치·PR 또는 기다리는 일) / 한 줄(28px, 둘째 줄은 툴팁). 새 키라 저장 형식 버전은 그대로예요(없으면 기본값). 설정 → 일반 → "사이드바 줄" → [사이드바](../features/ui/side-panels.md) |

저장 형식은 zustand `persist` 기본 형식(`{ state: {...}, version }`)이고 지금 `version`은 3이에요(`SETTINGS_VERSION`). 옛 형식은 순수 함수 `migrateSettings(old, from)`(src/settingsMigrate.ts)가 단계를 차례로 모두 적용해요: v0 → v1 테마를 기본 테마, 이징을 `standard`로 한 번 바꿔요. v1 → v2 이 테마의 id가 `chrome`에서 `default`로 바뀌어서 옮겨요. v2 → v3 `fullView`(전체 화면의 채팅/터미널)를 `agentView`로 이름을 바꿔요(값 `"chat"`/`"cli"`는 이어받고 옛 키는 지워요. 다른 값이면 버리고 기본값). 예전에는 v0에서 바로 돌려줘 뒤 단계를 건너뛰었어요. `npm test`의 `scripts/view-check.ts`가 v0·v1·v2 입력을 확인해요. `set` 함수는 JSON에 들어가지 않아요.

## 그 밖의 localStorage 키

| 키 | 값 | 기본값 | 쓰는 곳 | 뜻 |
|---|---|---|---|---|
| `maestro.showLeft` | `"1"` / `"0"` | 보임(`"0"`이 아니면 보임) | `App.tsx` `usePanelPref` | 왼쪽 사이드바 표시 |
| `maestro.showRight` | `"1"` / `"0"` | 보임 | `App.tsx` `usePanelPref` | 오른쪽 패널 표시 |
| `maestro.leftWidth` | 숫자 문자열(px) | `280` (읽을 때와 저장할 때 220~480으로 자름) | `App.tsx` `usePanelWidth` | 왼쪽 사이드바 너비. 예전 범위(180~)로 저장된 값도 220 아래면 220으로 읽어요 |
| `maestro.rightWidth` | 숫자 문자열(px) | `320` (220~560으로 자름) | `App.tsx` `usePanelWidth` | 오른쪽 패널 너비 |
| `maestro.rightTab` | `"files"` / `"changes"` | `"files"` (다른 값도 파일 탭) | `RightPanel.tsx` (`RIGHT_TAB_KEY`, `parseRightTab`) | 오른쪽 패널에서 마지막으로 연 탭. 앱을 다시 켜도 그 탭으로 열려요 → [오른쪽 패널](../features/editor/file-explorer.md) |
| `maestro.prMethod.<프로젝트>` | `"merge"` / `"squash"` | 없음(허용된 첫 방식: 병합 커밋 → squash) | `nodes/PrCard.tsx` (`prMethodKey`, `pickPrMethod`) | 그 프로젝트에서 마지막으로 고른 PR 병합 방식. `<프로젝트>`는 프로젝트 폴더 경로를 소문자·`/`로 맞추고 끝 `/`를 뗀 것(예: `maestro.prMethod.c:/repo/maestro`). 접힌 PR 카드의 `PR 병합`이 이 방식으로 바로 합쳐요. 저장소가 그 방식을 막았으면 허용된 첫 방식을 써요 → [PR 모드](../features/orchestration/pr-mode.md) |
| `maestro.foldedProjects` | JSON 문자열 배열(프로젝트 root) | `[]` (모두 펼침) | `Sidebar.tsx` `useFolded` (`FOLD_KEY`) | 사이드바에서 접힌 프로젝트. 저장할 때 목록에 없는 root는 빠져요. Ctrl+Shift+↑/↓도 이 값을 읽어 접힌 프로젝트를 건너뛰어요(`readFolded`) → [사이드바](../features/ui/side-panels.md) |
| `maestro.projectsShut` | `"1"` / `"0"` | 없음 | `Sidebar.tsx` (`OLD_SHUT_KEY`, 이행만) | 옛 키(프로젝트 목록 전체 접기). 목록을 처음 읽을 때 지워요. `"1"`이고 `maestro.foldedProjects`가 없으면 그때의 모든 프로젝트를 접힌 것으로 옮겨요 |
| `maestro.location` | JSON `{ root, path }` | `{}` | `store.ts` (`LOCATION_KEY`) | 마지막으로 연 프로젝트(`activeRoot`)와 에이전트(`selectedPath`). 새로 고침 후 그 자리로 돌아가요 |
| `maestro.updateSeen` | 버전 문자열(예: `"0.3.3"`) | 없음 | `UpdateNotice.tsx` (`SEEN_KEY`) | 업데이트 팝오버가 저절로 열린 마지막 버전. 같은 버전이면 다시 저절로 열지 않고 버튼만 보여요 → [자동 업데이트](../features/ui/auto-update.md) |

## 제약과 알려진 문제

- `App.tsx`, `store.ts`, `UpdateNotice.tsx`, `Sidebar.tsx`, `RightPanel.tsx`, `PrCard.tsx`의 키는 localStorage를 못 쓰는 환경이면 try/catch로 기본값을 써요(`maestro.updateSeen`을 못 읽으면 팝오버가 확인할 때마다 저절로 열려요). `maestro.settings`는 zustand `persist`에 맡겨요.
- `maestro.prMethod.*`은 프로젝트를 목록에서 빼도 지우지 않아요(값 하나라 작고, 다시 추가하면 그대로 써요).
- 너비를 두 번 클릭해 기본값으로 되돌리는 것도 같은 키에 저장해요(`Resizer`).
