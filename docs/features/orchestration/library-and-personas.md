---
title: 문서(Maestro 공용 문서)와 페르소나, 참조
area: orchestration
status: done
files:
  - src-tauri/src/library.rs
  - src-tauri/src/main.rs
  - src-tauri/src/agent.rs
  - src-tauri/src/control.rs
  - src/LibraryPanel.tsx
  - src/Sidebar.tsx
  - src/ProjectMap.tsx
  - src/Canvas.tsx
  - src/nodes/WorktreeCard.tsx
related:
  - features/canvas/markdown-blocks.md
  - features/orchestration/maestro-cli.md
  - features/orchestration/maestro-agent.md
  - architecture/agent-runtime.md
  - reference/tauri-commands.md
  - reference/storage-layout.md
updated: 2026-11-13
---

# 문서(Maestro 공용 문서)와 페르소나, 참조

## 요약

문서는 Maestro가 직접 관리하는, 모든 프로젝트가 같이 쓰는 마크다운이에요. 페르소나(`~/.maestro/personas`)와 분류별 폴더(`~/.maestro/library/<분류>/`)로 나뉘고, 처음부터 기본 제공 파일이 들어 있어요. 마에스트로가 워커를 부리는 절차는 따로 **플레이북**(`~/.maestro/playbooks`)이에요. 프로젝트처럼 여는 **시스템**(카드 대시보드)과 에이전트 캔버스의 **문서** 버튼 패널에서 보여 줘요. 문서를 캔버스로 끌어 에이전트에 이으면 "참조"가 돼요. 참조는 파일 경로를 가리킬 뿐 파일을 복사하지 않아요. 프로젝트 자기 문서는 따로 등록하지 않고 오른쪽 파일 트리에서 끌어와요.

## 동작

- **기본 제공 파일**: 앱에 내장돼 있다가(`src-tauri/seed/`) 처음 실행할 때 빠진 것만 채워져요. 지운 기본 파일은 다시 생기지 않고, 고친 파일은 덮어쓰지 않아요.
  - 페르소나: 기획자, 구현 담당, 리뷰어, 테스터, 아키텍트, 디자이너, 문서 담당
  - 지침: 에이전트 공통 규칙(CLAUDE.md 시작점), 코딩 규칙, 커밋과 PR 규칙
  - PRD: PRD 템플릿, 기능 명세 템플릿, 결정 기록(ADR) 템플릿
  - 디자인: 디자인 원칙, UI 점검표, 모션 가이드
  - RAG: RAG 문서 작성 규칙, 용어집 템플릿, 문제 해결 기록 템플릿
  - 플레이북: 기본 지휘, 아키텍처 먼저, 경쟁 실행, 나눠서 구현, 구현 → 리뷰 반복, 계획 먼저, 버그 추적. 모두 맨 위에 `구성` 표(역할, 페르소나 파일, 인원, 맡는 일)가 있어요. 마에스트로는 워커를 만들 때 이 표대로 페르소나를 이어요
- **플레이북 고르기**: 마에스트로 캔버스의 아일랜드(왼쪽 위 도구 막대)에 `플레이북` 버튼이 있어요. 누르면 아래로 목록이 펼쳐지고, 맨 위 `자동`(강조, 기본값: 마에스트로가 요청에 맞게 고름) 아래에 플레이북들이 나와요. 하나를 고르면 버튼에 이름이 붙고, 아래 입력창에 `플레이북 · 이름` 칩이 생겨서 보내는 메시지마다 "[플레이북: 이름] 경로 를 읽고 이 절차를 따라."가 앞에 붙어요. 칩의 ×나 `자동`으로 되돌려요. 캔버스마다 앱이 켜져 있는 동안 기억해요(`PlaybookPanel`, Canvas `pickedBooks`).
- **플레이북**: 사이드바 `플레이북` → 시스템의 플레이북 보드. 문서와 똑같이 만들기·가져오기·열기·지우기를 해요(분류는 없어요). 마에스트로는 시작할 때 안내문으로 플레이북 목록(제목과 경로)을 받고, 일을 나누기 전에 맞는 것을 읽고 따르라는 지시를 받아요.
- **어디서 보나**: 사이드바 바닥의 `문서`·`플레이북` 줄을 누르면 [시스템](../ui/system-tab.md)이 프로젝트처럼 열려요(카드마다 제목과 첫 줄). 에이전트 캔버스 도구 막대의 `문서` 버튼은 같은 목록을 도구 막대 바로 아래(왼쪽 위) 목록 패널로 띄워요. 처음엔 분류가 모두 접혀 있어요(`DocsList folded`).
- **목록**: 맨 위 `♟ 페르소나`, 그다음 분류 폴더가 이름순으로 나와요. 분류 줄을 누르면 접고 펼쳐요. 각 줄에 문서 수가 보여요.
- **만들기**: 맨 위 `+`는 새 분류, 분류 줄에 마우스를 올리면 나오는 `+`는 그 분류의 새 문서(페르소나 줄이면 새 페르소나)예요. 이름을 쓰고 Enter, Esc로 취소해요. 파일 이름은 영숫자·`-`·`_` 외 문자를 `-`로 바꿔요.
- **가져오기**: 분류 줄의 `⤓`로 바깥 마크다운을 골라 그 분류에 **복사해서** 넣어요(문서함에 더하는 동작). 이름이 겹치면 `-2`, `-3`을 붙여요.
- **열기·지우기**: 문서를 누르면 편집기 탭으로 열려요. 마우스를 올리면 나오는 `×`를 누르면 확인 창("'<제목>' 문서를 지울까요?", 빨간 [지우기])이 뜨고, 확인하면 지워요(그 문서를 참조하던 에이전트는 더 이상 열 수 없어요). → [메뉴와 확인 창](../ui/menus-and-dialogs.md)
- **참조로 쓰기**: 문서를 캔버스로 끌면 마크다운 블록이 생기고, 블록을 에이전트에 이으면 참조가 돼요. 원본 파일을 가리키므로 문서를 고치면 참조하는 모든 에이전트가 고친 내용을 읽어요.
- 참조 연결 방법은 세 가지예요.
  - 캔버스에서 마크다운 블록과 에이전트를 선으로 잇기
  - 프로젝트 캔버스 카드의 `+`/`+N` 버튼 → `RefPicker`에서 체크(문서 목록과, 파일 트리에서 끌어온 프로젝트 파일)
  - `maestro ref add --path … [--to …]`
- 카드에는 연결된 페르소나가 `♟ <제목>`, 다른 문서가 `≡ <제목>` 칩으로 보여요.
- 실행 중인 에이전트에 페르소나를 연결하거나 해제하면 바로 `[Maestro] 이 워크트리의 페르소나가 '<제목>'(으)로 정해졌어요…` 또는 `…해제됐어요…` 메시지가 입력창으로 보내져요.

## 구현

- 페르소나(`library.rs`):
  - `personas_dir(home)` = `~/.maestro/personas`.
  - `seed_personas(home)`: 폴더가 없을 때만 만들고 `SEED` 4개 파일을 써요. 앱 시작 때 `main()`의 `setup`에서 불러요.
  - `create_persona(home, name)`: 영숫자·`-`·`_` 외 문자는 `-`로 바꾼 `<file>.md`를 만들고 `# <name>` 제목과 안내 한 줄을 써요. 이름이 비면 `"페르소나 이름을 적어 주세요"`, 같은 파일이 있으면 `"같은 이름의 페르소나가 있어요"`. 명령 `persona_create`.
  - `is_persona(home, md)`: 경로가 페르소나 폴더 아래인지(대소문자·구분자 무시).
- 마크다운 목록:
  - `title_of(path)`: frontmatter를 건너뛴 첫 `# ` 제목, 없으면 파일 이름. 명령 `md_title`.
  - `scan(folder)`: 깊이 4, 최대 500개, `.`으로 시작하는 폴더·`node_modules`·`target` 제외, 경로순 정렬.
- 기본 제공(`library.rs`): `BUILTIN`(폴더, 파일 이름, `include_str!` 본문) 목록을 `seed(home)`가 앱 시작 `setup`에서 써요. 폴더마다 `.seeded`에 한 번 쓴 파일 이름을 적어 두고, 거기 없고 파일도 없을 때만 써요.
- 문서 폴더: `docs_dir(home)` = `~/.maestro/library`. 플레이북: `playbooks_dir(home)` = `~/.maestro/playbooks`, `playbooks(home)`(명령 `playbook_list`). 마에스트로 브리프(`agent::system_brief`)에 `## Playbooks` 목록이 붙어요. `all(home)`이 페르소나 폴더 + 분류 폴더(이름순)를 `LibFolder`로 돌려줘요.
- 쓰기는 `managed()`가 허락한 폴더(페르소나 폴더, 플레이북 폴더, 또는 문서 폴더 바로 아래 분류)에서만 해요: `create_doc`(명령 `docs_create`), `import_docs`(`docs_import`, 복사), `delete_doc`(`docs_delete`), `create_category`(`docs_category`). 테스트: `docs_live_only_in_managed_folders`.
- 예전의 프로젝트별 보관함 등록(`<project_dir>/library.json`, `library_set`)은 없앴어요. 남아 있는 파일은 읽지 않아요. 이미 걸린 참조는 경로라서 그대로 동작해요.
- `library_list()` → `library_folders()` → `library::all()`. `maestro library list`도 같은 함수예요. 화면은 LibraryPanel.tsx의 `useDocs`(읽기·만들기·가져오기·지우기)를 캔버스 패널의 `DocsList`와 시스템의 `SystemDashboard`가 같이 써요. `MdFile.summary`는 본문 첫 줄(frontmatter·제목·코드 울타리 제외, 140자)이에요.
- 참조 저장: `AgentMeta.refs: Vec<String>`.
  - `set_ref(project_root, worktree, md, on)` → `set_reference()` → `set_ref_meta()`. 대소문자 무시로 중복을 막고 `/` 경로로 저장해요.
  - `worktree`가 빈 문자열이고 `on=false`면 프로젝트의 모든 에이전트에서 그 경로를 빼요(캔버스에서 마지막 블록을 지웠을 때).
  - 페르소나이고 대상 에이전트가 실행 중이면 별도 스레드에서 `wait_ready()` 후 페르소나 본문을 포함한 메시지를 `send()`해요.
- `WorktreeInfo`: `worktree_infos()`가 `refs`를 `is_persona`로 나눠 `personas`(제목 목록)와 `docs`(제목 목록)를 채워요. `refs`(경로)도 그대로 나가요.
- 에이전트에 전달: `agent::system_brief(home, meta)`가 `refs`를 나눠
  - 페르소나마다 `## Your persona (assigned in Maestro; follow it)` 아래 파일 본문 전체
  - 나머지는 `## Reference documents linked to you (read them before starting work)` 아래 `- <제목> (<경로>)` 목록
  - 을 붙여요. 이 브리프는 `Agent::spawn`에서 Claude는 `<meta>.brief.md`로 써서 `--append-system-prompt-file`로, Antigravity는 `<meta>.brief/AGENTS.md`로 써서 `--add-dir`로 넘겨요. 테스트: `brief_carries_persona_and_references`.
- `maestro whoami`도 `references`를 보여 줘요.

## 제약과 알려진 문제

- 문서 본문(페르소나 제외)은 브리프에 넣지 않고 경로만 줘요. 에이전트가 직접 읽어야 해요.
- 브리프는 에이전트가 시작할 때만 만들어져요. 실행 중에 문서 참조를 바꾸면 페르소나만 즉시 메시지로 알리고, 일반 문서는 다음 시작 때 반영돼요.
- 브리프는 Claude Code에 `--append-system-prompt-file`로, Antigravity에는 `--add-dir`로 넘긴 규칙 폴더(`<meta>.brief/AGENTS.md`)로 가요. 둘 다 시작할 때 페르소나·참조를 받아요. 실행 중 페르소나 메시지는 어댑터와 상관없이 보내져요.
- `scan` 한도(깊이 4, 500개)를 넘는 파일은 목록에 안 나와요.
- 페르소나 파일을 지우는 UI는 없어요.

## 결정 기록

- 페르소나는 프로젝트가 아니라 사용자 홈에 두어 모든 프로젝트가 같이 써요(`library.rs` 주석).
- 페르소나 폴더는 항상 첫 번째로 나와요(`LibFolder.persona` 주석).
- 보관함을 프로젝트별 폴더 등록에서 Maestro 공용 문서로 바꿨어요. 프로젝트 문서는 파일 트리에서 끌어오면 되고, 여러 프로젝트에서 다시 쓸 PRD 템플릿·지침·페르소나를 둘 곳이 필요했어요. 이름은 "보관함" 대신 "문서"예요. Maestro를 쓰는 협업 절차는 따로 "플레이북" 탭으로 만들 예정이에요.
