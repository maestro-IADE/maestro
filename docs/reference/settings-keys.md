---
title: 설정 키 목록
area: reference
status: done
files: [src/settings.ts, src/App.tsx, src/store.ts, src/UpdateNotice.tsx, src/Sidebar.tsx]
related: [reference/storage-layout.md, architecture/overview.md]
updated: 2026-11-13
---

# 설정 키 목록

## 요약

사용자 설정은 `src/settings.ts`의 `useSettings`(zustand `persist`)가 localStorage의 `maestro.settings` 키 하나에 JSON으로 저장해요. 그 밖에 패널 표시·너비와 마지막 위치를 `App.tsx`와 `store.ts`가 각각 다른 localStorage 키에 저장해요. 모두 프런트엔드(웹뷰) 저장소라서 디스크의 `~/.maestro`와는 따로예요.

## `maestro.settings` (useSettings)

| 키 | 타입 | 기본값 | 뜻 |
|---|---|---|---|
| `theme` | `"default"` / `"dark"` / `"graphite"` / `"mocha"` / `"light"` / `"ivory"` | `"default"` | 테마. `<html data-theme>`로 적용돼요. 목록에 없는 값(예: 지워진 `neon`, `forest`)은 `default`으로 보여요. `light`, `ivory`는 캔버스와 xterm도 밝은 팔레트를 써요. 저장 형식 v1로 올라갈 때 한 번 `default`으로 바뀌어요 |
| `easing` | `"standard"` / `"smooth"` / `"bouncy"` / `"linear"` | `"standard"` | 모든 UI 모션의 곡선. CSS 변수 `--mae-ease` |
| `speed` | `"fast"` (150ms) / `"normal"` (240ms) / `"slow"` (360ms) | `"normal"` | 모션 길이. CSS 변수 `--mae-dur` |
| `motion` | `boolean` | `true` | 모션 켜기. 끄거나 OS가 `prefers-reduced-motion: reduce`면 `--mae-dur`가 `0ms` |
| `canvasBg` | `"dots" \| "lines" \| "cross" \| "none"` | `"dots"` | 캔버스 배경 무늬. 두 캔버스 모두 `CanvasBackground`로 그려요 |
| `canvasGap` | `number` | `24` | 캔버스 무늬 간격(px): 12 / 24 / 48 / 96 |
| `canvasAlpha` | `number` | `100` | 캔버스 무늬 진하기(%), 10–100 |
| `panelStyle` | `"cards" \| "lines" \| "full"` | `"cards"` | 패널 구분: 분리 / 통합 / 와이드(여백 없음). `<html data-panels>`로 들어가요 |
| `autoSave` | `boolean` | `false` | 코드 편집기 자동 저장(입력을 멈추고 1초 뒤). 설정 → 파일 |
| `defaultAgent` | `string` | `"auto"` | 새 워크트리의 기본 에이전트 CLI id. `"auto"`는 쓸 수 있는 첫 에이전트 |
| `defaultPermission` | `string` | `"acceptEdits"` | 새 워크트리가 시작할 권한 모드 |
| `disabledAgents` | `string[]` | `[]` | 설치돼 있지만 꺼 둔 에이전트. 워크트리를 만들 때 고를 수 없어요 |
| `agentModels` | `Record<string, string>` | `{}` | 에이전트별 새 워크트리 기본 `--model`. 없으면 CLI 기본값 |
| `assistantAgent` | `string` | `"claude-code"` | 가벼운 백그라운드 작업(프로젝트 캔버스 요약)에 쓸 에이전트 CLI, 또는 `"off"` |
| `assistantModel` | `string` | `""` | 보조 모델. `""`은 그 에이전트의 싼 기본값(Claude는 Haiku) |
| `agentLayout` | `"full"` / `"canvas"` | `"full"` | 워크트리 안에서 에이전트 전체 화면 또는 블록이 있는 캔버스 |
| `fullView` | `"chat"` / `"cli"` (`AgentView`) | `"chat"` | 전체 화면 에이전트 뷰의 채팅/터미널. 캔버스 블록은 각자 따로 가져요 |
| `awayMinutes` | `number` (선택지 `5`, `15`, `60`, `0`) | `15` | 떠난 지 이 분이 지난 프로젝트의 쉬는 에이전트를 멈춰요. `0`은 끄지 않음. `ipc.setAway`로 백엔드에 전달 |

저장 형식은 zustand `persist` 기본 형식(`{ state: {...}, version }`)이고 지금 `version`은 2예요(v0 → v1: 테마를 기본 테마, 이징을 `standard`로 한 번 바꿔요. v1 → v2: 이 테마의 id가 `chrome`에서 `default`로 바뀌어서 옮겨요). `set` 함수는 JSON에 들어가지 않아요.

## 그 밖의 localStorage 키

| 키 | 값 | 기본값 | 쓰는 곳 | 뜻 |
|---|---|---|---|---|
| `maestro.showLeft` | `"1"` / `"0"` | 보임(`"0"`이 아니면 보임) | `App.tsx` `usePanelPref` | 왼쪽 사이드바 표시 |
| `maestro.showRight` | `"1"` / `"0"` | 보임 | `App.tsx` `usePanelPref` | 오른쪽 패널 표시 |
| `maestro.leftWidth` | 숫자 문자열(px) | `280` (읽을 때와 저장할 때 220~480으로 자름) | `App.tsx` `usePanelWidth` | 왼쪽 사이드바 너비. 예전 범위(180~)로 저장된 값도 220 아래면 220으로 읽어요 |
| `maestro.rightWidth` | 숫자 문자열(px) | `320` (220~560으로 자름) | `App.tsx` `usePanelWidth` | 오른쪽 패널 너비 |
| `maestro.foldedProjects` | JSON 문자열 배열(프로젝트 root) | `[]` (모두 펼침) | `Sidebar.tsx` `useFolded` (`FOLD_KEY`) | 사이드바에서 접힌 프로젝트. 저장할 때 목록에 없는 root는 빠져요 → [사이드바](../features/ui/side-panels.md) |
| `maestro.projectsShut` | `"1"` / `"0"` | 없음 | `Sidebar.tsx` (`OLD_SHUT_KEY`, 이행만) | 옛 키(프로젝트 목록 전체 접기). 목록을 처음 읽을 때 지워요. `"1"`이고 `maestro.foldedProjects`가 없으면 그때의 모든 프로젝트를 접힌 것으로 옮겨요 |
| `maestro.location` | JSON `{ root, path }` | `{}` | `store.ts` (`LOCATION_KEY`) | 마지막으로 연 프로젝트(`activeRoot`)와 에이전트(`selectedPath`). 새로 고침 후 그 자리로 돌아가요 |
| `maestro.updateSeen` | 버전 문자열(예: `"0.3.3"`) | 없음 | `UpdateNotice.tsx` (`SEEN_KEY`) | 업데이트 팝오버가 저절로 열린 마지막 버전. 같은 버전이면 다시 저절로 열지 않고 버튼만 보여요 → [자동 업데이트](../features/ui/auto-update.md) |

## 제약과 알려진 문제

- `App.tsx`, `store.ts`, `UpdateNotice.tsx`, `Sidebar.tsx`의 키는 localStorage를 못 쓰는 환경이면 try/catch로 기본값을 써요(`maestro.updateSeen`을 못 읽으면 팝오버가 확인할 때마다 저절로 열려요). `maestro.settings`는 zustand `persist`에 맡겨요.
- 너비를 두 번 클릭해 기본값으로 되돌리는 것도 같은 키에 저장해요(`Resizer`).
