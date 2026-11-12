---
title: 설정 창
area: ui
status: done
files: [src/SettingsDialog.tsx, src/MergeModeRow.tsx, src/agents.tsx, src/NewAgentDialog.tsx, src/AgentSettingsDialog.tsx, src/settings.ts, src/store.ts, src/App.tsx, src/Workspace.tsx, src/ui/useShortcuts.ts]
related: [features/ui/window-frame.md, features/ui/shortcuts.md, features/ui/auto-update.md, features/orchestration/pr-mode.md, features/agents/chat-runtime.md, reference/settings-keys.md, features/agents/cli-install.md, features/agents/answer-summaries.md, features/agents/away-stop.md, features/ui/themes-and-motion.md]
updated: 2026-11-13
---

# 설정 창

## 요약

설정은 팝업 창으로 열려요. 탭은 에이전트 / 실행 / 파일 / 프로젝트 / 스타일 / 정보예요. 위쪽 막대의 톱니(`Settings`)나 Ctrl+,로 열어요. 메인 창과 분리 창 어디서든, 사이드바를 숨겼어도 같아요. 에이전트 CLI가 하나도 없으면 앱을 켤 때 자동으로 열려요.

## 동작

**여는 곳**
- 위쪽 막대의 설정 버튼(툴팁 "설정 (Ctrl+,)"). → [창 프레임](window-frame.md)
- Ctrl+, (macOS는 ⌘,): 입력칸·편집기 안에서도 열려요. 터미널에 포커스가 있으면 터미널이 키를 가져요. → [단축키](shortcuts.md)
- 앱을 켤 때 설치된 에이전트 CLI가 하나도 없으면 에이전트 탭으로 저절로 열려요(메인 창만).
- 분리 창에서도 그 창 위에 열려요. 바꾼 값은 `maestro.settings` 저장소 이벤트로 다른 창에 바로 퍼져요.
- 닫기: 오른쪽 위 X, 바깥(어두운 바탕) 누르기, 창 안에 포커스가 있을 때 Esc.

**에이전트 탭**
- 기본값
  - 기본 에이전트: 자동 또는 쓸 수 있는 에이전트 칩(로고 포함). 자동은 쓸 수 있는 첫 에이전트예요.
  - 새 에이전트의 권한: 새로 만드는 에이전트가 처음 갖는 권한 모드. 목록은 기본 에이전트(자동이면 쓸 수 있는 첫 에이전트)가 받는 모드만이에요(`permissionsFor`. Claude Code는 "매번 묻기"부터 다섯, Antigravity는 넷). 설명 둘째 줄 "새로 만드는 에이전트에만 적용돼요. 이미 있는 에이전트는 에이전트 설정에서 바꿔요". 저장된 값을 기본 에이전트가 받지 않으면(기본 에이전트를 바꾼 뒤) 그 값을 맨 위 항목으로 남기고 "기본 에이전트가 받지 않는 권한이에요…" 안내를 보여요. 새 에이전트 창은 에이전트마다 받는 값으로 바꿔 시작해요(`permissionFor`). → [에이전트 설정](../agents/agent-settings.md)
  - 보조 모델: 요약 같은 가벼운 일에 쓸 에이전트와 모델, 또는 끔 → [답변 요약](../agents/answer-summaries.md)
- 에이전트 목록: 설치된 것과 설치 가능한 것을 타일로 보여 줘요.
  - 타일마다 상태(설치 필요 / 버전 낮음 / 로그인 필요 / 사용 중 / 꺼짐 / 준비 중), 켜기/끄기, "기본으로", 실행 명령(`launch_preview`), 펼치면 기본 모델과 설치·로그인 터미널이 있어요. 준비되지 않았으면 이유가 한 줄 붙어요. → [CLI 탐지·설치](../agents/cli-install.md)
  - "↻ 다시 확인"으로 상태를 다시 읽어요(`recheck: true`, 막힌 채팅 런타임도 풀어요).
  - 에이전트 목록, 이름, 설치 안내, 보조 모델 기본값은 모두 백엔드 카탈로그(`agent_catalog`)에서 와요. → [에이전트 카탈로그](../agents/agent-adapters.md)

**실행 탭**
- 자리 비움: "자리 비운 프로젝트 정리" 드롭다운(5분 / 15분 / 1시간 / 끄지 않음) → [자리 비움](../agents/away-stop.md)

**파일 탭**
- 파일 편집: 자동 저장 켜기/끄기(기본 끔). 켜면 입력을 멈추고 1초 뒤에 저장해요. → [코드 편집기](../editor/code-editor.md)

**프로젝트 탭** (R6)
- 병합 방식: 프로젝트마다 한 줄(`이름 · 기준 <기준 브랜치>`)과 `로컬 | PR` 버튼. 설명 줄은 두 방식과 "바꾸면 기다리던 병합 요청은 거둬요"를 말해요.
- `PR`을 누르면 `MergeModeRow`가 `set_merge_mode`만 불러요. 백엔드(`pr::set_merge_mode`)가 "확인 중…" 동안 원격·gh·로그인·저장소·원격 기준 브랜치를 새로 확인하고(`github::readiness(recheck=true)`, 캐시 안 씀) 되면 모드를 써요. 안 되면 이유(예: "gh(GitHub CLI)가 없어요. cli.github.com에서 설치한 뒤 '다시 확인'을 누르세요. 설치했는데도 이렇게 나오면 앱을 다시 시작하세요 …". 앱은 PATH 말고 Windows 설치 위치도 찾아봐요)가 그 줄 아래 빨간 글로 보이고 옆에 "다시 확인"이 있어요. 버튼은 `로컬`에 남아요. "다시 확인"은 PR로 바꾸다 실패했을 때만 보여요(그 PR 확인을 다시 해요). 그 프로젝트에 끝나지 않은 PR 요청이 있으면(커밋·푸시·만들기 중, 또는 앱이 만든 PR을 아직 찾지 못함) 어느 쪽으로 바꾸든(`로컬`로 돌아갈 때도) 거절하고 `pr::PR_UNRESOLVED` 문구를 보여요: "PR 작업 중이거나, 앱이 만든 PR을 아직 찾지 못한 병합 요청이 있어요. 병합 요청 카드에서 '다시 확인'으로 PR을 찾거나 요청을 거부한 뒤 다시 해 주세요".
- 같은 줄(`MergeModeRow`)이 마에스트로 캔버스 도구 막대의 `병합: 로컬/PR` 칩을 누르면 아래로 펼쳐져요. 프로젝트 캔버스의 마에스트로 카드 아래쪽에는 `기준 main · PR`처럼 보여요.
- 탭 아래 안내: PR 모드는 GitHub 원격과 로그인된 gh(2.40 이상)가 필요하고, 앱은 로그인 정보를 보관하지 않고 강제 푸시·원격 브랜치 삭제를 하지 않으며, squash는 PR 모드에서만 골라요. → [PR 모드](../orchestration/pr-mode.md)

**스타일 탭**
- 테마 견본, 패널 구분(분리 / 통합 / 와이드), 캔버스 배경(점 / 격자 / 십자 / 없음), 모션 사용, 이징, 속도, 미리보기 → [테마와 모션](themes-and-motion.md)

**정보 탭**
- "Maestro v0.3.1"처럼 설치된 버전(`getVersion()`, `tauri.conf.json`의 `version`)과 `업데이트 확인` 버튼.
- 설명 줄: 처음엔 "켜진 지 10초 뒤와 그 뒤 6시간마다 저절로 확인해요" 누르면 "확인하는 중…" → "최신 버전이에요" / "새 버전 vX.Y.Z이 있어요 · 위쪽 막대의 업데이트 버튼에서 설치해요"(분리 창이면 "메인 창 위쪽 막대의…") / "확인하지 못했어요: <오류>".
- 새 버전이 있으면 위쪽 막대의 업데이트 팝오버를 바로 열어요(이미 본 버전이어도). → [자동 업데이트](auto-update.md)

- **채팅 런타임 행(Claude Code 칸, "모델 · 로그인"을 펼치면 기본 모델 아래):** 카탈로그 `runtimes`에 `acp`가 있고 `cli_status`가 `acp`를 준 에이전트만 보여요. → [채팅 런타임](../agents/chat-runtime.md)
  - "채팅 방식: 자동 | 터미널(PTY)": `set_chat_runtime` 뒤 상태를 다시 받아요. 바꾸면 설명 끝에 "다음 시작부터 적용돼요"가 붙어요(실행 중인 에이전트는 그대로예요).
  - 상태 한 줄 "채팅 런타임: …": 준비됐으면 "준비됨 · 어댑터 0.102.0 · Node.js 24.x"(개발용 경로면 "(개발용 경로)"), 아니면 백엔드 메시지(`acp.message`). `blocked`(이번 실행에서 시작 실패·로그인 필요·잇단 크래시)면 옆에 "다시 확인" 버튼(`cli_status { recheck: true }`)이 있어요. 설정 창을 여는 것만으로는 막힘이 풀리지 않아요.
  - 설치 버튼: 상태가 `notInstalled`면 "채팅 런타임 설치", `broken`이면 "다시 설치". Node가 없거나 낮으면 버튼을 끄고 "Node.js 22 이상 필요 · winget install OpenJS.NodeJS.LTS"를 보여요.
  - 설치 중: 스피너, [취소](`acp_install_cancel`), `acp://install`의 최근 6줄(흐린 고정폭 상자). 끝(`done`)이나 실패(`failed`, 오류는 빨간 글)면 상태를 다시 받아요.

## 구현

- 열림 상태는 창마다 스토어에 있어요: `settings: { section?, agent?, root? } | null`, `openSettings(target?)`, `closeSettings()`(store.ts). `WindowLayers`(Workspace.tsx)가 메인 창 `App`과 분리 창 `WorkspaceWindow`에 한 번씩 있어서 `settings`가 있으면 `SettingsDialog`를 그려요. 예전 `App.tsx` 로컬 상태와 `onOpenSettings` prop 사슬(사이드바, 캔버스, 프로젝트 지도)은 없앴어요.
- `SettingsDialog({ section, onClose })`: `section`이 `SECTIONS`의 id면 그 탭으로, 아니면 에이전트 탭으로 열려요. `agent`·`root`(특정 에이전트 행·프로젝트 카드로 바로 가기)는 아직 읽지 않아요(UI 구조 재구성 단계 9).
- `SettingsDialog`(SettingsDialog.tsx): `SECTIONS`(`agents`, `run`, `files`, `projects`, `style`, `about`)로 탭을 나눠요. `about`은 `AboutSection`이에요. `projects`는 `ProjectsSection`이 `useStore.repos`마다 `MergeModeRow`(MergeModeRow.tsx)를 그려요. 병합 방식은 앱 설정(`useSettings`)이 아니라 백엔드의 `<project_dir>/project.json`에 있어요(마에스트로가 시작할 때 브리프에 넣고, 요청 때 계획이 따라야 해서예요).
- 구성 요소: `AboutSection`, `AgentTile`, `DefaultPermissionRow`, `AssistantRow`, `InstalledDetails`, `ChatRuntimeRow`(+ `acpLine`), `SetupTerminal`, `InstallHelp`. 에이전트 목록·로고·권한·모델 목록(`AGENTS`, `AgentLogo`, `permissionsFor`, `permissionOf`, `defaultAgentOf`, `useModels`, `STATUS_LABEL`)은 `agents.tsx`에서 가져와요(`useModels`는 새 에이전트 창·에이전트 설정 창과 같이 써요). 닫기 버튼은 `X` 아이콘이에요
- 채팅 방식(`~/.maestro/runtimes.json`)은 백엔드에 저장해요. `maestro send`나 자리 비움 재시작처럼 화면 없이 에이전트를 띄울 때도 같은 값을 써야 해서예요.
- 값은 모두 `useSettings`(zustand persist, 키 `maestro.settings`)에 저장돼요. → [설정 키](../../reference/settings-keys.md)

## 제약과 알려진 문제

- Esc는 포커스가 설정 창 안에 있을 때만 닫아요(창 전체 Esc는 UI 구조 재구성 단계 9).
- 대화상자(새 에이전트, 에이전트 설정)가 떠 있을 때 Ctrl+,나 대화상자 안의 "설정에서 설치"를 누르면 설정이 그 위에 열려요. 설정을 닫으면 쓰던 대화상자로 돌아가요.

## 결정 기록

- 테마 설정은 자주 쓰지 않아서 사이드바가 아니라 팝업에 넣었어요(Antigravity 앱의 설정 창 참고).
- 탭 이름 "일반"은 "에이전트"로 바꿨어요. 레이아웃은 Orca를 너무 닮지 않게 자체 스타일로 다듬었어요.
- 자리 비움 설정은 에이전트 설정과 성격이 달라서 새 "실행" 탭으로 뺐어요.
- 편집기 설정은 "파일" 탭에 모아요. 지금은 자동 저장뿐이고, 줄 바꿈·글자 크기·탭 크기·저장할 때 끝 공백 지우기 같은 항목이 들어갈 자리예요.
- 2026-11-13 (UI 구조 재구성 단계 2): 여는 곳을 위쪽 막대 하나(+ Ctrl+,)로 모았어요. 사이드바 아래 설정 행과 캔버스 줌 막대의 톱니를 없앴어요. 사이드바를 숨긴 채 전체 화면에 있거나 분리 창에 있으면 설정을 열 길이 없었어요. 열림 상태를 스토어로 옮겨 어느 부품에서든 `openSettings()`로 열 수 있게 했어요(뒤 단계의 딥 링크가 같은 함수를 써요).
- 2026-11-13 (UI 구조 재구성 단계 8): 기본 권한 목록을 기본 에이전트가 받는 모드로 거르고 "매번 묻기"를 보이게 했어요. 이름을 "새 에이전트의 권한"으로 바꾼 것은 이미 있는 에이전트에는 적용되지 않는다는 것을 라벨에서 알게 하려는 것이에요.
