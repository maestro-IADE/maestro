---
title: 설정 창
area: ui
status: done
files: [src/settings/SettingsDialog.tsx, src/settings/GeneralSection.tsx, src/settings/AgentsSection.tsx, src/settings/ProjectsSection.tsx, src/settings/LookSection.tsx, src/settings/AboutSection.tsx, src/settings/parts.tsx, src/settings/agentState.ts, src/MergeModeRow.tsx, src/agents.tsx, src/Sidebar.tsx, src/settings.ts, src/store.ts, src/App.tsx, src/Workspace.tsx, src/ui/useShortcuts.ts, src-tauri/src/pty.rs, scripts/settings-check.ts]
related: [features/ui/window-frame.md, features/ui/shortcuts.md, features/ui/auto-update.md, features/orchestration/pr-mode.md, features/agents/chat-runtime.md, reference/settings-keys.md, features/agents/cli-install.md, features/agents/answer-summaries.md, features/agents/away-stop.md, features/ui/themes-and-motion.md, features/agents/agent-settings.md, features/editor/code-editor.md]
updated: 2026-11-13
---

# 설정 창

## 요약

설정은 팝업 창으로 열려요. 섹션은 다섯이에요: **일반 / 에이전트 / 프로젝트 / 모양 / 정보**. 위쪽 막대의 톱니(`Settings`)나 Ctrl+,로 열고, 메인 창과 분리 창 어디서든 같아요. 다른 화면에서 특정 에이전트 행이나 프로젝트 카드로 바로 열 수 있어요(딥 링크: 그 자리로 스크롤하고 1.5초 동안 테두리를 그려요). 에이전트 CLI가 하나도 없으면 앱을 켤 때 에이전트 섹션으로 저절로 열려요. Esc는 창 어디서든 닫아요.

## 동작

**여는 곳**
- 위쪽 막대의 설정 버튼(툴팁 "설정 (Ctrl+,)"). → [창 프레임](window-frame.md)
- Ctrl+, (macOS는 ⌘,): 입력칸·편집기 안에서도 열려요. 터미널에 포커스가 있으면 터미널이 키를 가져요. → [단축키](shortcuts.md)
- 앱을 켤 때 설치된 에이전트 CLI가 하나도 없으면 에이전트 섹션으로 저절로 열려요(메인 창만).
- 딥 링크(`openSettings({ section, agent?, root? })`):
  - 에이전트 헤더의 "터미널로 실행 중" 칩 → 에이전트 섹션, 그 에이전트 행이 "관리"가 펼쳐진 채로. → [채팅 보기](../agents/chat-view.md)
  - 새 에이전트 창의 "설정에서 설치"/"설정에서 켜기" → 에이전트 섹션, 고른 에이전트 행. → [에이전트 만들기](../worktrees/create-worktree.md)
  - 마에스트로 헤더의 병합 칩, 사이드바 프로젝트 메뉴의 "프로젝트 설정…" → 프로젝트 섹션, 그 프로젝트 카드.
- 설정이 열려 있을 때 다른 딥 링크가 오면 그 섹션으로 옮기고 그 자리를 다시 강조해요.
- 분리 창에서도 그 창 위에 열려요. 바꾼 값은 `maestro.settings` 저장소 이벤트로 다른 창에 바로 퍼져요.
- 닫기: 오른쪽 위 X(28×28), 바깥(어두운 바탕) 누르기, Esc. Esc는 포커스가 창 어디에 있든 닫아요(설정 창 밖이어도). 설치 터미널 안에 포커스가 있으면 Esc는 터미널이 가져요. 확인 창이나 메뉴가 위에 떠 있으면 Esc는 그것부터 닫아요.

**모양**
- 960×680 모달. 왼쪽 탐색 200px: 항목 28px = 아이콘 16(`Settings2`, `Bot`, `FolderGit2`, `Palette`, `Info`) + 13px 이름. 활성 항목은 선택 면과 1px 안쪽 선.
- 본문: 안쪽 24 32 40, 최대 폭 680. 제목 20/600과 설명 12 muted. 묶음 제목은 카드 밖(13/600), 카드까지 8, 묶음 사이 24.
- 카드: 반경 10, 1px 테두리, 행 사이 1px 선. 행은 왼쪽 라벨(13) + 설명(12 muted, 최대 2줄), 오른쪽 컨트롤(높이 28, 스위치 20×34). 설명이 있으면 최소 56px, 없으면 44px.

**일반**
- 화면
  - 에이전트 보기 [채팅 | 터미널] (`agentView`, 설명 "다음에 여는 대화부터 이 보기로 열려요. 진행 중인 대화는 터미널 위 막대로 넘겨요"): 전체 화면, 캔버스 블록, 분리 창이 모두 이 보기예요. 한 에이전트만 잠시 다르게 보려면 그 에이전트 헤더의 보기 아이콘을 써요. 바꾸면 그런 예외는 모두 없어져요. 이 값을 쓰는 곳은 여기 한 곳뿐이에요. → [채팅 뷰](../agents/chat-view.md)
  - 열 때 화면 — 에이전트 [전체 화면 | 캔버스] (`agentLayout`): 마에스트로가 아닌 에이전트를 새 탭으로 열 때 처음 화면. 탭 막대의 전환 버튼은 그 탭만 바꿔요.
  - 열 때 화면 — 마에스트로 [캔버스 | 전체 화면] (`maestroLayout`).
  - 사이드바 줄 [두 줄 | 한 줄] (`sidebarDensity`). → [사이드바](side-panels.md)
- 자리 비움: "자리 비운 프로젝트 정리"(5분 / 15분 / 1시간 / 끄지 않음, `awayMinutes`). → [자리 비움](../agents/away-stop.md)
- 파일 편집: 자동 저장 켜기/끄기(`autoSave`, 기본 끔). → [코드 편집기](../editor/code-editor.md)

**에이전트**
- 에이전트 CLI가 하나도 없으면 맨 위에 경고 상자 "에이전트 CLI가 하나도 설치돼 있지 않아요. 아래 목록에서 하나를 설치해야 에이전트를 만들 수 있어요".
- 기본값 카드
  - 기본 에이전트: [자동 | 쓸 수 있는 에이전트(로고 포함)]. 기본 에이전트를 고르는 곳은 여기 하나뿐이에요. 자동은 쓸 수 있는 첫 에이전트예요.
  - 새 에이전트의 권한: 기본 에이전트(자동이면 쓸 수 있는 첫 에이전트)가 받는 모드만(`permissionsFor`. Claude Code는 "매번 묻기"부터 다섯, Antigravity는 넷). 설명 "새로 만드는 에이전트에만 적용돼요. 이미 있는 에이전트는 에이전트 설정에서 바꿔요". 저장된 값을 기본 에이전트가 받지 않으면 그 값을 맨 위 항목으로 남기고 설명이 "기본 에이전트가 받지 않는 권한이에요. 새 에이전트 창에서 파일 수정 자동 허용으로 시작해요"로 바뀌어요. 고른 모드의 뜻은 선택 상자 툴팁이에요. → [에이전트 설정](../agents/agent-settings.md)
  - 보조 모델: 요약 같은 가벼운 일에 쓸 에이전트와 모델, 또는 끔. → [답변 요약](../agents/answer-summaries.md)
- 에이전트 목록 카드(묶음 제목 오른쪽 "↻ 다시 확인" = `cli_status { recheck: true }`, 막힌 채팅 런타임도 풀어요). 설치된 에이전트가 먼저, 그다음 나머지. 에이전트마다 한 행(최소 56px):
  - 로고 24 → 이름 13/500 + 상태(6px 점 + 11px 글) → 둘째 줄 실행 명령(11px 고정폭, `launch_preview`. 설치 안 됐으면 설치 명령 또는 "앱과 함께 설치돼요", 아직 연결 안 된 에이전트는 "연결 준비 중이에요").
  - 상태는 CLI와 채팅 런타임을 합쳐 하나로 정해요(앞 조건이 먼저): 확인 중 → 준비 중 → **설치 필요** → **버전 낮음** → **로그인 필요** → **채팅 런타임 필요**(채팅 런타임이 있는 에이전트에서 채팅 연결이 자동인데 런타임이 준비되지 않았을 때) → 사용 중 / 꺼짐. 굵게 쓴 넷은 경고색, 사용 중은 초록, 나머지는 흐린 색이에요. 상태 글 툴팁은 CLI가 준 이유(`message`)예요.
  - 채팅 런타임이 있는 에이전트(Claude Code)는 셋째 줄에 채팅 런타임을 늘 보여요: "채팅 런타임 · 준비됨" 또는 이유(예: "설치되지 않았어요"), 채팅 연결이 터미널 경유면 " · 터미널 경유로 채팅해요". 버튼은 많아야 하나예요: [설치](설치 안 됨) / [다시 설치](깨짐) / [다시 확인](이번 실행에서 막힘) / 설치 중이면 "설치하는 중…" + [취소]. Node가 없거나 낮으면 버튼 없이 이유만 보여요. 런타임이 필요한데 준비되지 않았거나 설치가 실패했으면 이 줄이 경고색이에요. 툴팁은 버전까지 넣은 전체 상태예요.
  - 오른쪽: "기본" 배지(새 에이전트가 처음 고르는 에이전트일 때. 자동이면 쓸 수 있는 첫 에이전트에 붙어요) → 켜기/끄기 스위치(설치 안 됐거나 버전이 낮으면 꺼짐) → "관리 ⌄".
  - "관리"를 펼치면 행 아래(안쪽 12 16 16 52)에 소제목(12/500 muted)과 함께 이 순서로 보여요:
    1. 상태·로그인: CLI 상태 한 줄(이유 또는 "준비됐어요" + 버전)과 설치·로그인 터미널. 설치 안 됐으면 설치 명령과 "설치하고 로그인", 버전이 낮으면 이유와 같은 명령으로 업데이트.
    2. 채팅 런타임: 전체 상태("준비됨 · 어댑터 0.102.0 · Node.js 24.x", 개발용 경로면 "(개발용 경로)"), 설치가 하는 일(`@agentclientprotocol/claude-agent-acp@<버전>`을 앱 데이터 폴더에 설치), Node 안내("Node.js 22 이상 필요 · winget install OpenJS.NodeJS.LTS"), 설치 중 npm 최근 6줄, 실패 이유(빨간 글). 설치가 실패하면 행이 저절로 펼쳐져요.
    3. 기본 모델: 새 에이전트가 시작할 모델(CLI 기본값 또는 목록).
    4. 채팅 연결 [자동 | 터미널 경유]: `set_chat_runtime`. 바꾸면 설명 끝에 "다음 시작부터 적용돼요".
  - 설치·로그인 터미널(340px)은 프로세스가 종료 코드 0으로 끝나면 저절로 닫히고 그 자리에 "끝났어요"가 남아요. 0이 아니면 열린 채로 "끝났지만 오류가 있었어요 · 종료 코드 N"을 보여 출력을 읽을 수 있어요(닫기는 직접). 끝나면 어느 쪽이든 설치 상태를 다시 읽어요. → [CLI 탐지·설치](../agents/cli-install.md)
  - 에이전트 목록, 이름, 설치 안내, 보조 모델 기본값은 모두 백엔드 카탈로그(`agent_catalog`)에서 와요. → [에이전트 카탈로그](../agents/agent-adapters.md)

**프로젝트**
- 사이드바 순서대로 프로젝트마다 카드 하나. 머리: 이름 13/600 + 경로 11px 고정폭(넘치면 말줄임, 툴팁 전체 경로).
- 병합 방식: `이름 · 기준 <기준 브랜치>` 대신 "병합 방식 · 기준 <기준 브랜치>"로 쓰는 같은 `MergeModeRow`(`로컬 | PR`). PR을 고를 때 백엔드가 원격·gh·로그인을 확인하고, 안 되면 이유와 "다시 확인"이 그 줄 아래에 떠요. 끝나지 않은 PR 요청이 있으면 어느 쪽으로든 바꾸지 못해요(`pr::PR_UNRESOLVED`). → [PR 모드](../orchestration/pr-mode.md)
- 마에스트로: 있으면 "Claude Code · 파일 수정 자동 허용 · (모델) · 기준 main"과 [마에스트로 설정…](에이전트 설정 창). 없으면 한 줄 설명과 [마에스트로 만들기](새 에이전트 창, 마에스트로 모드). 둘 다 설정 창을 닫고 대화상자를 열어요.
- 목록에서 제거: "사이드바에서만 빼요. 디스크의 파일과 워크트리는 그대로예요" + 빨간 글 버튼 [목록에서 제거…]. 사이드바 프로젝트 메뉴와 같은 확인 창을 거쳐요.
- 카드 아래 안내: PR 모드는 GitHub 원격과 로그인된 gh(2.40 이상)가 필요하고, 앱은 로그인 정보를 보관하지 않고 강제 푸시·원격 브랜치 삭제를 하지 않으며, squash는 PR 모드에서만 골라요.
- 프로젝트가 없으면 "프로젝트가 없어요. 사이드바의 프로젝트 추가로 폴더를 더해요".

**모양**
- 테마 견본, 패널 구분(분리 / 통합 / 와이드), 캔버스 배경(무늬 점 / 격자 / 십자 / 없음, 간격, 진하기), 모션(사용, 이징, 속도, 미리보기). → [테마와 모션](themes-and-motion.md)

**정보**
- "Maestro v0.3.2"처럼 설치된 버전(`getVersion()`)과 `업데이트 확인` 버튼.
- 설명 줄: 처음엔 "켜진 지 10초 뒤와 그 뒤 6시간마다 저절로 확인해요" 누르면 "확인하는 중…" → "최신 버전이에요" / "새 버전 vX.Y.Z이 있어요 · 위쪽 막대의 업데이트 버튼에서 설치해요"(분리 창이면 "메인 창 위쪽 막대의…") / "확인하지 못했어요: <오류>".
- 새 버전이 있으면 위쪽 막대의 업데이트 팝오버를 바로 열어요(이미 본 버전이어도). → [자동 업데이트](auto-update.md)

## 구현

- 열림 상태는 창마다 스토어에 있어요: `settings: SettingsTarget | null`(`{ section?, agent?, root? }`), `openSettings(target?)`, `closeSettings()`(store.ts). `WindowLayers`(Workspace.tsx)가 메인 창 `App`과 분리 창 `WorkspaceWindow`에 한 번씩 있어서 `settings`가 있으면 `<SettingsDialog target={settings} />`를 그려요. `key`를 주지 않아서 열린 채로 새 링크가 오면 다시 마운트하지 않고 옮겨 가요.
- 파일(`src/settings/`):
  - `SettingsDialog.tsx`: 탐색, 본문, 딥 링크, Esc. 섹션 id는 `general`, `agents`, `projects`, `look`, `about`(`agentState.ts` `SECTION_IDS`, 모르는 id는 `general`).
  - `GeneralSection.tsx`, `LookSection.tsx`, `AboutSection.tsx`: 값은 모두 `useSettings`.
  - `AgentsSection.tsx`: `AgentsSection`, `DefaultPermissionRow`, `AssistantRow`, `AgentRow`, `InstalledDetails`, `ChatRuntimeLine`(행의 한 줄), `ChatRuntimeRow`(관리 안의 자세한 상태), `ChatConnectionRow`, `SetupTerminal`, `InstallHelp`, `useAcpInstall`(설치 진행·실패, 한 줄과 관리가 같이 씀), `useCommandPreview`(권한은 `permissionFor`로 그 에이전트가 받는 값).
  - `ProjectsSection.tsx`: `ProjectCard`마다 `MergeModeRow`(`label` prop으로 "병합 방식"), 마에스트로 행, 목록에서 제거(`removeFromList`, Sidebar.tsx가 내보내고 프로젝트 메뉴도 같이 써요).
  - `parts.tsx`: `Group`(카드 밖 제목 + 카드), `Row`(`data-target`을 달 수 있음), `Switch`, `Seg`.
  - `agentState.ts`(순수, `import type`만): `agentRowState`(합친 상태), `chatRuntimeNeeded`, `acpAction`(한 줄의 버튼), `acpShort`, `acpLine`, `sectionOf`. `scripts/settings-check.ts`가 `npm test`에서 확인해요.
- 딥 링크: 섹션이 대상 행에 `data-target`을 달아요(에이전트 행은 `target.agent === id`, 프로젝트 카드는 `samePath(target.root, root)`). 에이전트 행은 처음부터 펼친 채로 그려지고, 열린 채 새 링크가 오면 다시 펼쳐요. `SettingsDialog`가 그 섹션이 그려진 뒤 두 프레임을 기다려(펼친 행의 높이가 정해진 뒤) `scrollIntoView`를 불러요. 보통은 `block: "center"`, 행이 본문보다 높으면 `"start"`예요. 그리고 `data-flash`를 1.5초 달아 `outline: 1px solid var(--mae-line-strong)`을 그려요. 같은 링크는 섹션을 오가도 다시 강조하지 않아요.
- Esc: `window` keydown(버블링)으로 받아요. 확인 창(`ConfirmHost`)과 메뉴(`MenuHost`)는 캡처 단계에서 키를 멈추고, 새 에이전트 창·에이전트 설정 창의 Esc도 React에서 전파를 멈춰서 위에 뜬 것만 닫혀요. IME 조합 중이거나 대상이 `.xterm` 안이면 넘겨요.
- 설치 터미널 저절로 닫기: PTY 감시 스레드(pty.rs `PtyRuntime::start`)가 프로세스가 끝나면 `agent://state` TERMINATED보다 먼저 `pty://exit { worktreePath, code }`를 보내요. `SetupTerminal`은 자기 키(`setup:<id>`)의 `code === 0`이면 `setup_stop`을 부르고 닫아요. → [이벤트](../../reference/events.md)
- 설정 창 안의 버튼이 여는 대화상자(에이전트 설정, 새 에이전트)는 `WindowLayers`에서 설정 창 아래 층이라, 설정 창을 먼저 닫고 열어요.
- 채팅 연결(`~/.maestro/runtimes.json`)은 백엔드에 저장해요. `maestro send`나 자리 비움 재시작처럼 화면 없이 에이전트를 띄울 때도 같은 값을 써야 해서예요. 병합 방식은 `<project_dir>/project.json`에 있어요.
- 나머지 값은 모두 `useSettings`(zustand persist, 키 `maestro.settings`)에 저장돼요. 이번 재구성에서 키 이름은 하나도 바뀌지 않았어요. → [설정 키](../../reference/settings-keys.md)
- CSS: `.set-*`(탐색, 본문, 묶음, 카드, 행), `.agent-row*`, `.agent-state`, `.agent-badge`, `.agent-manage`, `.agent-sub*`, `.set-project-head`, `[data-flash]`(styles.css).

## 제약과 알려진 문제

- 설정 창 안에서 연 에이전트 설정 창·새 에이전트 창을 닫아도 설정 창으로 돌아오지 않아요(설정 창을 닫고 열었어요).
- 대화상자(새 에이전트, 에이전트 설정)가 떠 있을 때 Ctrl+,나 대화상자 안의 "설정에서 설치"를 누르면 설정이 그 위에 열려요. 설정을 닫으면 쓰던 대화상자로 돌아가요.
- 설치 중에 설정 창을 닫았다 열면 진행 줄은 다시 보이지 않아요(설치는 계속돼요). 끝나면 "↻ 다시 확인"으로 상태를 볼 수 있어요.
- 설치 터미널은 종료 코드만 봐요. 로그인 화면을 그냥 빠져나와 0으로 끝나도 닫히고, 상태는 다시 읽은 값(예: 로그인 필요)으로 보여요.
- 에이전트 보기 설명은 지금 동작(전체 화면에만 적용)을 말해요. 단계 12가 전역 보기로 바꿀 때 설명도 바꿔요.
- 설명은 2줄에서 잘려요. 긴 설명(병합 방식)은 `set-label`을 쓰지 않아 잘리지 않아요.

## 결정 기록

- 2026-11-13 (UI 구조 재구성 단계 12): "에이전트 보기"를 키 `agentView`(설정 저장 형식 v3, `fullView`에서 값을 이어받음)로 바꾸고 모든 자리의 기본으로 만들었어요. 화면의 다른 버튼은 이 값을 쓰지 않아요. → [설정 키](../../reference/settings-keys.md)
- 테마 설정은 자주 쓰지 않아서 사이드바가 아니라 팝업에 넣었어요(Antigravity 앱의 설정 창 참고).
- 2026-11-13 (UI 구조 재구성 단계 2): 여는 곳을 위쪽 막대 하나(+ Ctrl+,)로 모았어요. 사이드바를 숨긴 채 전체 화면에 있거나 분리 창에 있으면 설정을 열 길이 없었어요. 열림 상태를 스토어로 옮겨 어느 부품에서든 `openSettings()`로 열 수 있게 했어요.
- 2026-11-13 (UI 구조 재구성 단계 8): 기본 권한 목록을 기본 에이전트가 받는 모드로 거르고 "매번 묻기"를 보이게 했어요. 이름을 "새 에이전트의 권한"으로 바꾼 것은 이미 있는 에이전트에는 적용되지 않는다는 것을 라벨에서 알게 하려는 것이에요.
- 2026-11-13 (UI 구조 재구성 단계 9): 섹션을 여섯(에이전트 / 실행 / 파일 / 프로젝트 / 스타일 / 정보)에서 다섯으로 줄였어요. 항목 하나짜리 섹션(실행, 파일)은 일반으로 모았고, 설정에 자리가 없던 `agentLayout`·`maestroLayout`·`sidebarDensity`·`fullView`도 일반에 넣었어요. 처음 여는 섹션은 일반이에요(예전에는 에이전트).
- 같은 단계: 에이전트 타일 그리드를 행 목록으로 바꿨어요. 타일은 상태가 CLI만 말해서 채팅 런타임이 없는 Claude도 "사용 중"으로 보였고, 채팅 런타임 설치는 "모델 · 로그인"을 펼쳐야 보였어요. 이제 행 상태가 둘을 합치고 채팅 런타임 줄은 접힌 채로도 보여요. 타일의 "기본으로" 링크는 없앴어요(기본 에이전트를 고르는 곳이 둘이었어요). "기본" 배지는 저장값이 자동이어도 실제로 먼저 골라지는 에이전트에 붙여요.
- 같은 단계: "채팅 방식: 자동 | 터미널(PTY)"을 "채팅 연결: 자동 | 터미널 경유"로 바꿨어요. 일반의 "에이전트 보기: 채팅 | 터미널"과 같은 말을 써서 헷갈렸어요.
- 같은 단계: 설치 터미널을 종료 코드 0이면 저절로 닫아요. 프런트는 종료 코드를 몰라서 PTY 감시 스레드가 `pty://exit`를 보내게 했어요(Rust 몇 줄). 0이 아니면 출력을 읽을 수 있게 열어 둬요.
- 같은 단계: 딥 링크 스크롤은 `center`가 기본이지만, 펼친 에이전트 행(터미널 340px 포함)이 본문보다 높으면 머리가 위로 잘리지 않게 `start`로 해요.
- 같은 단계: 설정 창에서 여는 대화상자는 설정 창을 닫고 열어요. 층 순서(설정이 맨 위)는 대화상자 안의 "설정에서 설치" 링크를 위해 단계 8에서 정했어요. 층 순서를 마지막에 연 것 기준으로 바꾸는 것보다 단순해요.
