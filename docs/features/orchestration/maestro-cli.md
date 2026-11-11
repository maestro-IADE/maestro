---
title: maestro CLI (에이전트가 Maestro를 조작하는 명령)
area: orchestration
status: done
files:
  - src-tauri/src/bin/maestro-cli.rs
  - src-tauri/src/control.rs
  - src-tauri/src/approval.rs
  - src-tauri/src/agent.rs
  - src-tauri/src/main.rs
  - src-tauri/Cargo.toml
  - src-tauri/tauri.conf.json
  - src/App.tsx
  - src-tauri/src/events.rs
  - src-tauri/src/runtime.rs
related:
  - architecture/local-server.md
  - architecture/agent-runtime.md
  - features/orchestration/maestro-agent.md
  - features/orchestration/worker-hierarchy.md
  - features/orchestration/library-and-personas.md
  - features/worktrees/branch-selection.md
  - reference/storage-layout.md
  - architecture/runtime-events.md
updated: 2026-11-11
---

# maestro CLI (에이전트가 Maestro를 조작하는 명령)

## 요약

`maestro`는 Maestro 안에서 실행 중인 에이전트가 쓰는 명령줄 도구예요. 워크트리 목록 보기·만들기, 다른 에이전트에게 지시 보내기, 답변 읽기, 참조 문서 연결을 할 수 있어요. CLI는 `maestro <단어…> --플래그 값` 을 `POST /api/<단어/단어>`로 바꿔 Maestro 로컬 서버에 보내고, 서버는 `control::handle`에서 처리해요. 삭제·폐기 명령은 일부러 없어요.

## 동작

`maestro --help`(또는 인자 없음, `-h`, `help`)는 아래 목록을 출력해요(`control::ROUTES`).

| 명령 | 플래그 | 하는 일 |
|---|---|---|
| `maestro whoami` | 없음 | 프로젝트 경로, 내 워크트리, 상위(`parent`), 내 워커들, 나에게 연결된 참조 문서(`references`: path, title) |
| `maestro worktree list` | 없음 | 프로젝트의 모든 에이전트 카드. 각 항목에 `role`(`"maestro"` 또는 `""`)과 `parent` 이름 |
| `maestro worktree create` | `--name`(필수) `--prompt` `--agent claude-code\|antigravity` `--model` `--description` `--branch` `--base` `--checkout` | 새 워크트리와 에이전트를 만들고 나의 워커로 연결. `--prompt`가 있으면 첫 지시로 보냄 |
| `maestro branch list` | 없음 | `base`(마에스트로 기준 브랜치)와 `branches`(name, remote, inUse) |
| `maestro library list` | 없음 | Maestro 문서: 페르소나 폴더와 분류 폴더(`~/.maestro/library/<분류>`)의 마크다운 목록 |
| `maestro ref list` | `--to` | 그 에이전트(기본은 나)의 참조 문서. 각 항목에 `persona` 여부 |
| `maestro ref add` | `--path`(필수) `--to` | 마크다운 참조 연결. 그 에이전트 캔버스에 마크다운 블록이 생김 |
| `maestro ref remove` | `--path`(필수) `--to` | 참조 해제 |
| `maestro send` | `--to`(필수) `--text`(필수) | 다른 에이전트에게 지시 보내기. 꺼져 있으면 시작(재개)함 |
| `maestro read` | `--to` | 그 에이전트(기본은 나)의 상태와 마지막 답변(`lastAnswer`) |
| `maestro selection` | 없음 | 사용자가 내 캔버스에서 지금 선택한 블록들(`selected`): 에이전트(name, branch, path), 문서(title, path), 프롬프트 블록(name), 구조 블록(kind `block`: title, body, area, to), 영역(kind `area`: title, holds, to). 나 자신의 블록은 빠져요. → [구조 그리기](../canvas/structure-drawing.md) |
| `maestro draw` | `--file 구조.json` | 내 캔버스에 구조를 그려요. 형식 `{"areas":[{"title","in"?}], "blocks":[{"title","body"?,"area"?}], "arrows":[{"from","to","label"?}]}`. 이름이 이미 있는 블록은 내용만 고쳐 쓰고, 화살표는 라벨만 바꿔요. 이름 오타(화살표 끝, 속한 영역)는 그리기 전에 오류로 돌려줘요. → [구조 그리기](../canvas/structure-drawing.md) |

- 성공하면 JSON을 보기 좋게 출력하고 종료 코드 0이에요. 서버 오류는 `오류: <메시지>`를 stderr에 쓰고 1, 플래그 값이 빠지면 2예요.
- `--to`는 이 프로젝트 워크트리의 이름, 브랜치, 폴더 이름, 전체 경로 중 하나로 찾아요(대소문자 무시).
- `worktree create`의 기본값:
  - `--agent`: 호출한 에이전트와 같은 어댑터, 없으면 `claude-code`
  - 권한 모드: 호출한 에이전트의 `permission_mode`, 없으면 `acceptEdits`
  - `--branch`: `agent-<워크트리 수 + 1>`
  - `--base`: `base_branch()`(마에스트로 기준 브랜치, 없으면 저장소 기본 브랜치)
  - `--checkout`을 주면 새 브랜치 대신 그 기존 브랜치를 체크아웃해요.
- 만들거나 참조를 바꾸면 `maestro://worktrees` 이벤트로 UI가 그 프로젝트 워크트리를 다시 읽어요(`App.tsx`).

## 구현

- 바이너리: `src-tauri/src/bin/maestro-cli.rs`. std와 `serde_json`만 써요.
  - `parse(args)`: `--`로 시작하지 않는 인자는 경로 단어, `--k v`는 JSON 본문 필드예요. 테스트: `words_become_route_and_flags_become_body`.
  - `call(route, body)`: `TcpStream`으로 `MAESTRO_URL`에 HTTP/1.1 `POST /api/<route>`를 보내요. 헤더 `X-Maestro-Token: $MAESTRO_TOKEN`, `X-Maestro-Worktree: $MAESTRO_WORKTREE`.
  - `MAESTRO_URL` 또는 `MAESTRO_TOKEN`이 없으면 "Maestro가 띄운 에이전트 터미널에서 실행하세요" 오류예요.
- 환경 변수: `agent::maestro_env()`가 에이전트를 띄울 때 넣어요.
  - `MAESTRO_URL`: `127.0.0.1:<port>` (`Approvals::addr()`, 실행마다 포트가 바뀜)
  - `MAESTRO_TOKEN`: 실행마다 새로 만든 토큰 (`Approvals::token()`)
  - `MAESTRO_WORKTREE`: 그 에이전트의 워크트리 경로(호출자 식별)
  - `PATH`: 앞에 `~/.maestro/bin` 추가
- 설치: `agent::install_cli(home)`이 앱 시작 때(`main()`의 `setup`) 앱 실행 파일 옆의 `maestro-cli(.exe)`를 `~/.maestro/bin/maestro(.exe)`로 복사해요. `.cmd` 대신 복사본을 쓰는 이유는 Git Bash(Windows에서 Claude의 Bash 도구)도 찾게 하려는 것이에요. 실행 중인 에이전트가 이전 복사본을 쓰고 있으면 복사가 조용히 실패하고 다음 시작 때 바뀌어요.
- 빌드: `Cargo.toml`의 `default-run = "maestro"`, `tauri.conf.json`의 `beforeDevCommand`가 `cargo build --bin maestro-cli`를 먼저 실행해요.
- 서버 라우팅: `approval.rs`의 `Approvals::handle`이 경로가 `/api/`로 시작하면 `x-maestro-token` 헤더를 확인하고(틀리면 403 `"토큰이 맞지 않아요"`), `x-maestro-worktree` 헤더를 호출자로, 본문을 JSON으로 읽어 `control::handle(app, route, caller, input)`를 불러요. 성공 200, 실패 400 `{ "error": ... }`예요. 본문은 최대 4MB만 읽어요.
- `control::handle`:
  - `help`는 호출자 없이도 `ROUTES`를 돌려줘요. 나머지는 `MAESTRO_WORKTREE`가 비면 오류예요.
  - `locate()`: `worktree::main_checkout(caller)`로 저장소를 먼저 추측하고, 안 되면 등록된 모든 저장소를 훑어 호출자 워크트리를 찾아요(Windows에서 git 호출이 약 0.3초라 추측을 먼저 해요).
  - `worktree/create` → `create_worktree()`(새 워크트리 대화상자와 같은 함수) → `link(app, root, me, new)` → `deliver()`.
  - `deliver()`: `ensure()`로 에이전트를 시작(재개)하고 `wait_ready()`가 신뢰 확인이거나 시작 실패(로그인 필요 등)면 그 이유로 오류, 아니면 `agent.send()`.
  - `ref/add`·`ref/remove` → `md_path()`(상대 경로는 호출자 워크트리 기준, 추가는 존재하는 `.md`만) → `set_reference()`(캔버스와 같은 함수).
  - `read` → `last_answer()`: `runtime::history()`(채팅 뷰와 같은 공통 이벤트) + `events::last_answer`. 마지막 `agentText`가 있는 단위(Claude: 트랜스크립트 한 줄, Antigravity: 한 단계)의 글 블록을 `\n`으로 이어요. 턴 도중에 부르면 지금까지 쓴 마지막 글이 나와요(빈 글 블록도 포함). 같은 `itemId`로 다시 온 글(ACP의 빈 글 → 전체 글 upsert)은 블록 하나로, 마지막 글만 써요. 에이전트가 실행 중이고 그 런타임이 자기 기록을 가지면(ACP, R4) 그 기록을 읽어요. R3 이전과 결과가 같아요.
  - 모르는 경로는 `"모르는 명령: …"` 오류예요.
- 자동 허용: `Approvals::write_settings()`가 에이전트별 `--settings` 파일(`<meta>.settings.json`)에 `"permissions": { "allow": ["Bash(maestro:*)"] }`를 넣어요. `maestro` 명령마다 승인을 묻지 않게 해요.
- 에이전트에게 주는 안내: `agent::system_brief()`의 `MAESTRO_HINT`가 Claude에는 `--append-system-prompt-file`(`<meta>.brief.md`)로, Antigravity에는 `--add-dir`로 넘긴 규칙 폴더(`<meta>.brief/AGENTS.md`)로 전달돼요. "`maestro`가 PATH에 있고, `worktree list`/`worktree create`/`send`/`read`로 위임·병렬화하고, `whoami`로 상위·워커·참조 문서를 보고 그 문서를 먼저 읽어라"는 내용이에요. 마에스트로에게는 `COORDINATOR_HINT`가 더 붙어요.
- 개발 빌드에서는 시작할 때 `[maestro] MAESTRO_URL=… MAESTRO_TOKEN=…`을 stderr에 출력해서 아무 터미널에서나 시험할 수 있어요(`MAESTRO_WORKTREE`는 직접 설정).

- **채팅 런타임(ACP)에서:** `maestro send`는 ACP 큐에 들어가 차례대로 가요. 에이전트가 ACP로 돌아도 `maestro` 명령은 묻지 않고 돌아요(`allowedTools: ["Bash(maestro:*)"]`, 환경 변수는 어댑터를 거쳐 CLI에 그대로 가요). `maestro read`는 같은 트랜스크립트를 읽어서 런타임과 상관없이 같은 답을 줘요.

## 제약과 알려진 문제

- 삭제·폐기·연결 해제 명령은 없어요(`ref remove`는 있음). 워크트리를 없애는 건 사용자가 UI에서만 해요.
- 호출자는 같은 프로젝트의 워크트리만 다룰 수 있어요(`target()`이 `root` 안에서만 찾음).
- `MAESTRO_WORKTREE`는 헤더로 오는 값이라 토큰을 가진 쪽은 어떤 워크트리로든 자신을 소개할 수 있어요. 토큰은 실행마다 새로 만들고 서버는 `127.0.0.1`에만 열려요.
- `Bash(maestro:*)` 자동 허용 규칙은 Claude Code에만 있어요. Antigravity는 허용 규칙을 넘기는 방법을 확인하지 못해서, `maestro` 명령을 처음 쓸 때 권한을 물을 수 있어요("항상 허용"하면 그 뒤로는 안 물어요). 안내 문구(브리프)는 Antigravity도 받아요.
- 릴리스 빌드에서 `maestro-cli`가 앱 옆에 함께 번들되는지는 확인 안 됨이에요(`beforeBuildCommand`는 `npm run build`뿐이에요).
- 응답 파싱은 단순해요. `Connection: close`로 받은 전체를 `\r\n\r\n`에서 나눠요.

- 선택: 캔버스가 선택이 바뀔 때마다 `set_canvas_selection(owner, items)`로 알려 `AppState.selection`(워크트리 경로별, 메모리에만)에 두고, `selection` 경로가 호출한 에이전트 것을 돌려줘요. 캔버스가 닫히면 비워요. 에이전트 안내문(`MAESTRO_HINT`)에 "사용자가 '선택한 것', '이것들'이라고 하면 먼저 `maestro selection`을 보라"는 규칙이 있어요.

## 결정 기록

- 삭제 명령을 넣지 않았어요. 에이전트는 워크트리를 추가하고 서로 대화할 수만 있고, 아무것도 허물 수 없게 하려는 것이에요(`control.rs` 머리 주석). 이 덕분에 `Bash(maestro:*)`를 자동 허용해도 된다고 판단했어요(`approval.rs` 주석: 매번 물으면 조율이 멈춰요).
- 별도 서버 대신 승인용 로컬 서버(`approval.rs`)에 `/api/` 경로를 더했어요.
- 브리프는 파일로 넘겨요. 여러 줄 인자는 `cmd.exe /c`가 첫 줄바꿈에서 잘라서예요.
