---
title: Maestro 문서 안내
area: meta
status: done
files: []
related: []
updated: 2026-11-24
---

# Maestro 문서

Maestro는 CLI 코딩 에이전트(Claude Code, Antigravity 등)를 프로젝트마다 격리된 git 워크트리에서 실행하고, 캔버스 위에 블록으로 보여 주는 데스크톱 앱이에요(Tauri v2 + React).

이 폴더는 두 가지 용도로 써요.

- 나중에 사용자 문서나 개발 문서를 만들 때의 원재료
- 개발 중에 검색(RAG)해서 "이 기능이 어디서 어떻게 구현됐지?"에 바로 답하는 지식 베이스

## 폴더 구조

| 폴더 | 내용 |
|---|---|
| [architecture/](architecture/) | 전체 구조, 프로세스 모델, 에이전트 실행 방식(런타임 인터페이스, PTY·ACP 런타임, 채팅↔CLI 전환), 공통 이벤트([runtime-events.md](architecture/runtime-events.md)), ACP 조사([acp-runtime-research.md](architecture/acp-runtime-research.md)), 로컬 서버 |
| [features/agents/](features/agents/) | 에이전트: 채팅/터미널 보기, 채팅 런타임(ACP, [chat-runtime.md](features/agents/chat-runtime.md)), 만든 뒤 설정 바꾸기([agent-settings.md](features/agents/agent-settings.md)), 캔버스 추천(Maestro 방식을 얼마나 먼저 쓰나: 끔·보통·적극·알아서, [suggest-level.md](features/agents/suggest-level.md)), 채팅 선택지와 Maestro 흐름 권하기([chat-choices.md](features/agents/chat-choices.md)), 채팅 창구(그린 것을 채팅에서 먼저 보고 캔버스에 올리기·버리기, [chat-visuals.md](features/agents/chat-visuals.md)), 승인, 슬래시 명령, 요약, 자리 비움, 어댑터 |
| [features/canvas/](features/canvas/) | 프로젝트 캔버스, 워크트리 캔버스, 미니맵, 블록 종류, 구조 그리기와 배치, 관계, 캔버스 읽기와 그림 검사([canvas-reading.md](features/canvas/canvas-reading.md)), 에이전트가 묻는 질문 블록([question-block.md](features/canvas/question-block.md)), 영역의 크기·접기·단계와 상태·묶음([zones.md](features/canvas/zones.md)), 이미지·HTML 파일을 띄우는 보기 블록([view-block.md](features/canvas/view-block.md)), 플레이리스트(보여 준 것을 플레이로 저장하고 다시 보기, 플레이 만들기, [canvas-flows.md](features/canvas/canvas-flows.md)) |
| [features/worktrees/](features/worktrees/) | 워크트리 만들기, 워크트리 준비(로컬 파일 복사와 준비 명령, [worktree-setup.md](features/worktrees/worktree-setup.md)), 브랜치 선택, 외부 워크트리, 안전한 폐기 |
| [features/orchestration/](features/orchestration/) | Lead 에이전트, 워커 계층, maestro CLI, 문서·페르소나, 병합 요청과 명령 가드([merge-request.md](features/orchestration/merge-request.md)), PR 모드([pr-mode.md](features/orchestration/pr-mode.md)), 런북 고르기(패널·입력창에서 이름으로, 고른 턴의 허용, [playbook-pick.md](features/orchestration/playbook-pick.md)) |
| [features/editor/](features/editor/) | 파일 탐색기, 변경사항, Monaco 편집기, 드래그 첨부 |
| [features/ui/](features/ui/) | 창 프레임, 패널, 프로젝트 추가 메뉴(폴더 열기·저장소 클론·새 프로젝트, 기본 위치 `~/maestro/projects`, [add-project.md](features/ui/add-project.md)), 가운데 화면 탭, 시스템 탭, 워크스페이스(분할·새 창), 테마·모션, 설정 창, 메뉴·확인 창·토스트([menus-and-dialogs.md](features/ui/menus-and-dialogs.md)), 단축키([shortcuts.md](features/ui/shortcuts.md)), 빠른 이동 Ctrl+K([quick-switch.md](features/ui/quick-switch.md)), 확인할 일과 창 밖 알림([attention.md](features/ui/attention.md)), 앱이 뒤에 있을 때의 OS 알림([os-notifications.md](features/ui/os-notifications.md)), 화면 글의 용어([terms.md](features/ui/terms.md)), 한 번에 하나만 실행([single-instance.md](features/ui/single-instance.md)), 자동 업데이트([auto-update.md](features/ui/auto-update.md)), 개발용 시험(dev 모드: 캔버스 CLI·채팅 창구·깊은 영역·OS 알림 시험, [dev-tools.md](features/ui/dev-tools.md)) |
| [reference/](reference/) | Tauri 명령, 이벤트, 설정 키, 디스크 저장 위치 목록, CI와 릴리스([release.md](reference/release.md)) |

## 문서 작성 규칙 (RAG용)

1. **한 파일에 한 주제.** 검색 조각이 섞이지 않게, 기능 하나나 개념 하나만 다뤄요.
2. **맨 위 frontmatter는 필수**예요.
   - `title`: 문서 제목
   - `area`: `architecture` / `agents` / `canvas` / `worktrees` / `orchestration` / `editor` / `ui` / `reference` / `meta`
   - `status`: `done`(구현 완료) / `partial`(일부만) / `planned`(계획만)
   - `files`: 구현이 있는 소스 파일 경로 목록
   - `related`: 같이 읽으면 좋은 문서 경로(`docs/` 기준, 예: `features/agents/chat-view.md`)
   - `updated`: 마지막으로 고친 날짜(YYYY-MM-DD)
3. **본문 순서**는 항상 같아요. 해당 없는 절은 빼요.
   - `## 요약`: 2~3문장. 이 조각만 검색돼도 뜻이 통하게 써요.
   - `## 동작`: 사용자가 보는 동작
   - `## 구현`: 파일·함수·명령 이름과 흐름
   - `## 제약과 알려진 문제`
   - `## 결정 기록`: 왜 이렇게 했는지, 버린 대안
4. **이름은 코드 그대로** 써요. 함수, 명령, 이벤트, CSS 클래스는 `코드` 표기로 적어서 grep과 검색 둘 다 걸리게 해요.
5. **추측은 쓰지 않아요.** 확인하지 못한 동작은 "확인 안 됨"이라고 적어요.
6. **기능을 바꾸면 같은 변경에서 문서도 고쳐요.** `updated` 날짜도 함께 고쳐요.
