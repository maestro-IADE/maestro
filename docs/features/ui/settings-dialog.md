---
title: 설정 창
area: ui
status: done
files: [src/SettingsDialog.tsx, src/NewAgentDialog.tsx, src/settings.ts, src/App.tsx, src/Sidebar.tsx, src/Canvas.tsx]
related: [features/agents/chat-runtime.md, reference/settings-keys.md, features/agents/cli-install.md, features/agents/answer-summaries.md, features/agents/away-stop.md, features/ui/themes-and-motion.md]
updated: 2026-11-11
---

# 설정 창

## 요약

설정은 팝업 창으로 열려요. 탭은 에이전트 / 실행 / 스타일 세 개예요. 사이드바 아래 설정 버튼으로 열고, 사이드바를 숨겼을 때는 캔버스 줌 바의 톱니바퀴로 열어요. 에이전트 CLI가 하나도 없으면 앱을 켤 때 자동으로 열려요.

## 동작

**에이전트 탭**
- 기본값
  - 기본 에이전트: 자동 또는 쓸 수 있는 에이전트 칩(로고 포함). 자동은 쓸 수 있는 첫 에이전트예요.
  - 권한: 새 워크트리의 권한 모드
  - 보조 모델: 요약 같은 가벼운 일에 쓸 에이전트와 모델, 또는 끔 → [답변 요약](../agents/answer-summaries.md)
- 에이전트 목록: 설치된 것과 설치 가능한 것을 타일로 보여 줘요.
  - 타일마다 상태(설치 필요 / 버전 낮음 / 로그인 필요 / 사용 중 / 꺼짐 / 준비 중), 켜기/끄기, "기본으로", 실행 명령(`launch_preview`), 펼치면 기본 모델과 설치·로그인 터미널이 있어요. 준비되지 않았으면 이유가 한 줄 붙어요. → [CLI 탐지·설치](../agents/cli-install.md)
  - "↻ 다시 확인"으로 상태를 다시 읽어요(`recheck: true`, 막힌 채팅 런타임도 풀어요).
  - 에이전트 목록, 이름, 설치 안내, 보조 모델 기본값은 모두 백엔드 카탈로그(`agent_catalog`)에서 와요. → [에이전트 카탈로그](../agents/agent-adapters.md)

**실행 탭**
- 자리 비움: "자리 비운 프로젝트 정리" 드롭다운(5분 / 15분 / 1시간 / 끄지 않음) → [자리 비움](../agents/away-stop.md)

**파일 탭**
- 파일 편집: 자동 저장 켜기/끄기(기본 끔). 켜면 입력을 멈추고 1초 뒤에 저장해요. → [코드 편집기](../editor/code-editor.md)

**스타일 탭**
- 테마 견본, 패널 구분(분리 / 통합 / 와이드), 캔버스 배경(점 / 격자 / 십자 / 없음), 모션 사용, 이징, 속도, 미리보기 → [테마와 모션](themes-and-motion.md)

- **채팅 런타임 행(Claude Code 칸, "모델 · 로그인"을 펼치면 기본 모델 아래):** 카탈로그 `runtimes`에 `acp`가 있고 `cli_status`가 `acp`를 준 에이전트만 보여요. → [채팅 런타임](../agents/chat-runtime.md)
  - "채팅 방식: 자동 | 터미널(PTY)": `set_chat_runtime` 뒤 상태를 다시 받아요. 바꾸면 설명 끝에 "다음 시작부터 적용돼요"가 붙어요(실행 중인 에이전트는 그대로예요).
  - 상태 한 줄 "채팅 런타임: …": 준비됐으면 "준비됨 · 어댑터 0.102.0 · Node.js 24.x"(개발용 경로면 "(개발용 경로)"), 아니면 백엔드 메시지(`acp.message`). `blocked`(이번 실행에서 시작 실패·로그인 필요·잇단 크래시)면 옆에 "다시 확인" 버튼(`cli_status { recheck: true }`)이 있어요. 설정 창을 여는 것만으로는 막힘이 풀리지 않아요.
  - 설치 버튼: 상태가 `notInstalled`면 "채팅 런타임 설치", `broken`이면 "다시 설치". Node가 없거나 낮으면 버튼을 끄고 "Node.js 22 이상 필요 · winget install OpenJS.NodeJS.LTS"를 보여요.
  - 설치 중: 스피너, [취소](`acp_install_cancel`), `acp://install`의 최근 6줄(흐린 고정폭 상자). 끝(`done`)이나 실패(`failed`, 오류는 빨간 글)면 상태를 다시 받아요.

## 구현

- `SettingsDialog`(SettingsDialog.tsx): `SECTIONS`(`agents`, `run`, `files`, `style`)로 탭을 나눠요.
- 구성 요소: `AgentTile`, `AssistantRow`, `InstalledDetails`, `ChatRuntimeRow`(+ `acpLine`), `SetupTerminal`, `InstallHelp`, `useModels`, `GearIcon`
- 채팅 방식(`~/.maestro/runtimes.json`)은 백엔드에 저장해요. `maestro send`나 자리 비움 재시작처럼 화면 없이 에이전트를 띄울 때도 같은 값을 써야 해서예요.
- 값은 모두 `useSettings`(zustand persist, 키 `maestro.settings`)에 저장돼요. → [설정 키](../../reference/settings-keys.md)

## 결정 기록

- 테마 설정은 자주 쓰지 않아서 사이드바가 아니라 팝업에 넣었어요(Antigravity 앱의 설정 창 참고).
- 탭 이름 "일반"은 "에이전트"로 바꿨어요. 레이아웃은 Orca를 너무 닮지 않게 자체 스타일로 다듬었어요.
- 자리 비움 설정은 에이전트 설정과 성격이 달라서 새 "실행" 탭으로 뺐어요.
- 편집기 설정은 "파일" 탭에 모아요. 지금은 자동 저장뿐이고, 줄 바꿈·글자 크기·탭 크기·저장할 때 끝 공백 지우기 같은 항목이 들어갈 자리예요.
