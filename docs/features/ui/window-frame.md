---
title: 창 프레임과 위쪽 막대
area: ui
status: done
files: [src/App.tsx, src/Workspace.tsx, src/nodes/Attention.tsx, src/UpdateNotice.tsx, src/shortcuts.ts, src/ui/useShortcuts.ts, src/styles.css, src-tauri/tauri.conf.json, src-tauri/capabilities/default.json]
related: [features/ui/attention.md, features/ui/side-panels.md, features/ui/themes-and-motion.md, features/ui/shortcuts.md, features/ui/settings-dialog.md, features/ui/auto-update.md, features/ui/split-and-windows.md]
updated: 2026-11-14
---

# 창 프레임과 위쪽 막대

## 요약

Maestro는 운영체제의 기본 제목 표시줄을 끄고(`decorations: false`), 직접 그린 두껍고 둥근 프레임을 써요. 맨 위에는 높이 36px 막대가 늘 있어요. 설정, 뒤로/앞으로, 패널 토글, 업데이트가 여기 있어서 사이드바를 숨기거나 분리 창에 있어도 같은 자리에서 찾아요.

## 동작

메인 창 막대(왼쪽부터):

```
[←] [→] [왼쪽 패널] ······ 빈 곳(끌기) ······ [확인할 일] [업데이트] [설정*] [오른쪽 패널] [_ □ ×]
* 사이드바를 숨겼을 때만
```

| 요소 | 모양 | 동작 |
| --- | --- | --- |
| 왼쪽 패널 | 28×28 `PanelLeft` | 사이드바 숨기기/보이기. 켜져 있으면 글자색이 진해요 |
| 뒤로 / 앞으로 | 28×28 `ArrowLeft` / `ArrowRight` | 이 창에서 다녀온 곳을 오가요. 갈 곳이 없으면 흐려요(투명도 .4). 툴팁 "뒤로 (Alt+←)", "앞으로 (Alt+→)" → [가운데 화면 탭](stage-tabs.md) |
| 빈 곳 | 남은 폭 전부 | 끌면 창이 움직이고, 더블클릭하면 최대화/복원 |
| 확인할 일 | 높이 28, `Bell` 16 + 개수 알약(높이 16, 11px/600, 경고색 글자 + 같은 색 16% 면). 0이면 종만 흐리게 | 누르면 400px 팝오버. 툴팁 "확인할 일 (Ctrl+J 다음)" → [확인할 일](attention.md) |
| 업데이트 | 새 버전이 있을 때만. `ArrowDownToLine` + "업데이트"(받는 중이면 "업데이트 42%") | 누르면 288px 팝오버 → [자동 업데이트](auto-update.md) |
| 설정 | 28×28 `Settings`. 사이드바를 숨겼을 때만(평소에는 사이드바 왼쪽 아래 "설정" 줄). 분리 창은 늘 있어요 | 설정 창을 열어요. 툴팁 "설정 (Ctrl+,)" → [설정 창](settings-dialog.md) |
| 오른쪽 패널 | 28×28 `PanelRight` | 오른쪽 패널 숨기기/보이기 |
| 창 버튼 | 46×36 셋 | 최소화 / 최대화 / 닫기(닫기 hover는 빨강) |

- 막대 왼쪽 안쪽 8px, 요소 사이 4px. 아이콘 버튼은 28×28(아이콘 16)이에요.
- 선택한 프로젝트 이름은 메인 창 막대에 보이지 않아요(사이드바와 탭에 있어요).
- 창 제목(작업 표시줄과 Alt+Tab에 보이는 이름)은 "Maestro"예요. 확인할 일이 N개 있으면 "(N) Maestro"가 돼요. 창에 포커스가 없을 때 에이전트를 멈춰 세우는 일(도구 승인, 터미널 질문, 병합 요청)이 새로 생기면 메인 창의 작업 표시줄 단추가 깜박여요. → [확인할 일](attention.md)
- 분리 창 막대는 `[프로젝트 이름] [←] [→] ··· [확인할 일] [설정] [메인 창으로] [창 버튼]`이에요(패널 토글과 업데이트는 없어요). → [분할과 분리 창](split-and-windows.md)
- 사이드바, 스테이지, 오른쪽 패널은 프레임 안에 둥근 카드처럼 떨어져 놓여요. 가장자리에 붙지 않아요.

## 구현

- `<header className="topbar">`(App.tsx, 분리 창은 Workspace.tsx `WorkspaceWindow`). 높이 `var(--h-chrome)`(36px), 버튼 `.tb-icon`(`var(--icon-btn-lg)` 28px), 글 버튼 `.tb-text`(높이 28, 12px).
- 드래그 영역(`data-tauri-drag-region`)은 빈 칸(`.spacer`)과 분리 창의 프로젝트 이름(`Brand`의 `.brand`)에만 있어요. 막대 자체(`header`)에는 달지 않아요. 드래그 영역은 자식에게 이어지지 않고, 버튼에 걸리면 클릭 대신 창 끌기가 돼서예요.
- 부품(Workspace.tsx): `Brand`(분리 창의 프로젝트 이름. 메인 창 막대에는 로고도 이름도 없어요), `NavButtons`(뒤로/앞으로, `navTarget`·`goNav`), `SettingsButton`(`openSettings()`), `WindowControls`, `WindowLayers`(창마다 한 번, 이 순서로: 새 에이전트 창 `NewAgentHost`, 에이전트 설정 창 `AgentSettingsHost`, 설정 창, 빠른 이동 `QuickSwitch`, 메뉴 `MenuHost`, 확인 창 `ConfirmHost`, 토스트. 설정 창이 두 대화상자 뒤라 "설정에서 설치" 같은 링크가 그 위에 열려요). 툴팁은 `shortcuts.ts`의 `tip(id)`로 만들어요. → [단축키](shortcuts.md)
- 확인할 일은 `AttentionButton`(nodes/Attention.tsx)이에요. 메인 창과 분리 창 막대 모두 설정 앞에 있어요.
- 창 제목과 작업 표시줄: `useWindowAttention(main)`(Workspace.tsx)을 `App`(main = true)과 `WorkspaceWindow`(false)가 불러요. 개수가 바뀔 때만 `getCurrentWindow().setTitle()`, 메인 창만 `requestUserAttention(UserAttentionType.Informational)`.
- 창 버튼: `getCurrentWindow().minimize()` / `toggleMaximize()` / `close()`
- 권한(capabilities/default.json): start-dragging, minimize, toggle-maximize, close, set-title, request-user-attention. 권한 없이 부르면 실행 중에 거부돼요
- 로고(`LogoMotion`, LogoMotion.tsx): 검은 공으로 만든 메타볼 M이에요. 양쪽 두 덩어리(오케스트라)는 위·아래 공이 목으로 이어져 있고, 가운데 아래 작은 공이 지휘자예요. 공들을 흐린 뒤 알파 경계로 다시 자르는 SVG 필터로 이음새를 매끄럽게 해요. 색은 테마의 글자색(`--mae-text`, 글자 옆에서는 그 글자색)을 따라서 타일 없이 그려요. 정사각형이에요(viewBox `-4 -6.5 56 56`). 위쪽 막대에서는 정지 상태예요. 움직이는 로딩 로고는 멈춰 있을 땐 정지 로고와 똑같고, 돌기 시작하면 색이 있는 빛 버전으로 서서히 바뀌었다가(`lit`) 제자리에 돌아오기 직전 다시 원래 색으로 돌아와요. 각 덩어리는 한 바퀴를 다 돌아 자기 자리로 돌아와요. 빛 버전은 공 다섯 개에 색(`BALL_COLORS`)을 주고 공마다 중심이 흰색이에요. 덩어리끼리 겹치는 곳은 `mix-blend-mode: plus-lighter`(가산 혼합)로 더해지고, 은은한 빛 번짐과 로고 중심의 흰빛이 있어요(시험 중).
- 앱 아이콘(앱 안 로고와 달리 옅은 타일 위 검은 로고): 같은 모양을 `src-tauri/icons/icon-source.svg`로 두고 `npx tauri icon src-tauri/icons/icon-source.svg`로 모든 크기를 만들어요(안드로이드·iOS 폴더는 지웠어요). 아이콘을 바꾼 뒤에는 앱을 다시 빌드해야 실행 파일에 들어가요(`build.rs`를 건드려 다시 빌드).
- 로딩 화면(`LogoLoading`): 같은 로고가 움직여요(한 단계 4.2초 루프).
  - 로고 모양으로 잠깐 멈춰요.
  - 지휘자가 먼저 출발하고 오케스트라가 조금 늦게 따라와요.
  - 셋이 가운데 쪽으로 모여(완전히 한 점이 되지 않고 세 갈래가 보이게, `GATHER` 0.62) 녹아 붙은 채 한 바퀴 더 돌아요. 그동안 공들이 각자 조금씩 엇갈려 퍼졌다 모였다를 세 번 반복해서 꼬물거려요(`WIGGLES`, `WIGGLE`). 공마다 크기도 제 박자로 커졌다 작아졌다 해요(`PULSES`, `PULSE`).
  - 다음 자리로 퍼지면서 각자 그 자리 모양이 돼요. 그래서 매 단계 끝에 정확히 로고 모양이에요.
  - 화면의 모든 로딩 로고는 시계 하나로 함께 움직여요(`LogoMotion.tsx`의 `moving` 집합 + `requestAnimationFrame` 하나). 로고가 화면에 붙는 순간 ref 콜백으로 등록되고 떨어지면 빠져서, 어느 컴포넌트에서 쓰든 따로 신경 쓸 게 없어요. 로딩 로고가 하나도 없으면 시계도 멈춰요.
  - 설정에서 모션을 끄면 멈춘 로고로 보여요. 운영체제의 동작 줄이기(Windows의 애니메이션 효과 끔)는 따르지 않아요. 로딩 표시까지 멈추면 로딩 중인지 알 수 없어서예요.
  - "프로젝트와 에이전트를 읽는 중…" 화면에 써요(`LogoLoading`). 글자 옆 작은 로딩 표시도 모두 같은 애니메이션이에요(`Spinner`, 16px). 예전 회전 원(`.spinner`)은 지웠어요.

## 제약과 알려진 문제

- 막대 왼쪽 안쪽 여백(12px)과 버튼 사이 4px 틈은 드래그 영역이 아니에요. 창을 끌려면 가운데 빈 곳(분리 창은 프로젝트 이름도)을 잡아요.
- 분리 창이 아주 좁으면 프로젝트 이름이 먼저 말줄임으로 줄어요(최대 240px).

## 결정 기록

- 2026-11-14: 메인 창 위쪽 막대에서 로고와 "Maestro" 글자를 뺐어요. 요즘 데스크톱 앱(Linear, Zed, Orca 등)처럼 막대에는 조작만 두고, 앱 이름은 창 제목과 작업 표시줄이 보여 줘요. 분리 창은 어느 프로젝트 창인지 알아야 해서 이름만 남겼어요.
- 참고 영상(Codex 앱)처럼 프레임을 두껍고 둥글게 하고, 위쪽 막대를 프레임과 합쳤어요.
- 2026-11-13 (UI 구조 재구성 단계 2): 막대를 44px에서 36px로 줄이고 설정·뒤로/앞으로·업데이트를 이리로 모았어요. 예전에는 설정이 사이드바 아래와(사이드바를 숨기면) 캔버스 줌 막대 톱니에 있어서, 사이드바를 숨긴 채 전체 화면이거나 분리 창에 있으면 설정을 열 수 없었어요. 뒤로/앞으로는 마지막 탭 묶음 끝에 있어서 분할하면 자리가 움직였어요. 탭 줄을 창 위끝으로 올리는 Orca 방식은 패널 스타일과 분리 창이 막대를 전제로 해서 하지 않았어요.
- 드래그 영역을 막대 전체가 아니라 빈 칸(과 분리 창의 이름)에만 달았어요. 버튼이 드래그 영역 안에 있으면 클릭이 먹지 않을 수 있어요.
- 2026-11-13 (UI 구조 재구성 단계 5): 확인할 일 버튼을 채우고, 창 밖에서도 알 수 있게 제목 앞 "(N) "과 작업 표시줄 깜박임을 더했어요. OS 알림(알림 센터)과 작업 표시줄 배지 아이콘은 하지 않았어요(비목표).
