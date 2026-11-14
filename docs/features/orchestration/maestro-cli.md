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
  - src-tauri/src/catalog.rs
related:
  - features/orchestration/merge-request.md
  - architecture/local-server.md
  - architecture/agent-runtime.md
  - features/orchestration/maestro-agent.md
  - features/orchestration/worker-hierarchy.md
  - features/orchestration/library-and-personas.md
  - features/worktrees/branch-selection.md
  - reference/storage-layout.md
  - architecture/runtime-events.md
updated: 2026-11-14
---

# maestro CLI (에이전트가 Maestro를 조작하는 명령)

## 요약

`maestro`는 Maestro 안에서 실행 중인 에이전트가 쓰는 명령줄 도구예요. 워크트리 목록 보기·만들기, 다른 에이전트에게 지시 보내기, 답변 읽기, 참조 문서 연결을 할 수 있어요. Lead는 `merge-request`로 워커 브랜치의 병합을 사용자에게 요청해요. CLI는 `maestro <단어…> --플래그 값` 을 `POST /api/<단어/단어>`로 바꿔 Maestro 로컬 서버에 보내고, 서버는 `control::handle`에서 처리해요. 삭제·폐기 명령은 일부러 없어요.

## 동작

`maestro --help`(또는 인자 없음, `-h`, `help`)는 아래 목록을 출력해요(`control::ROUTES`).

| 명령 | 플래그 | 하는 일 |
|---|---|---|
| `maestro whoami` | 없음 | 프로젝트 경로, 내 워크트리, 상위(`parent`), 내 워커들, 나에게 연결된 참조 문서(`references`: path, title) |
| `maestro worktree list` | 없음 | 프로젝트의 모든 에이전트 카드. 각 항목에 `role`(`"maestro"` 또는 `""`)과 `parent` 이름 |
| `maestro worktree create` | `--name`(필수) `--prompt` `--agent claude-code\|antigravity` `--model` `--description` `--branch` `--base` `--checkout` | 새 워크트리와 에이전트를 만들고 나의 워커로 연결. `--prompt`가 있으면 첫 지시로 보냄. 결과는 카드에 `prompt: {delivered, error?, kept?}`가 붙어요. 지시를 못 보내도(신뢰 확인 필요, 로그인 필요, 시작 실패 등) 워커는 이미 있으니 성공(종료 코드 0)이고 `warning`이 붙어요. 그 지시는 새 워커의 `.notes.json`에 보관했다가 워커의 입력창이 뜨면 보내요: 늦게 끝난 시작이나 CLI 보기에서 마친 로그인 뒤(워커가 켜져 있는 동안 지켜봐요), 신뢰 확인 뒤, 다시 시작할 때. 그 전에 `maestro send`로 다른 지시를 보내면 보관한 지시가 먼저 가요 |
| `maestro branch list` | 없음 | `base`(Lead 기준 브랜치)와 `branches`(name, remote, inUse) |
| `maestro library list` | 없음 | Maestro 문서: 페르소나 폴더와 분류 폴더(`~/.maestro/library/<분류>`)의 마크다운 목록 |
| `maestro ref list` | `--to` | 그 에이전트(기본은 나)의 참조 문서. 각 항목에 `persona` 여부 |
| `maestro ref add` | `--path`(필수) `--to` | 마크다운 참조 연결. 그 에이전트 캔버스에 마크다운 블록이 생김 |
| `maestro ref remove` | `--path`(필수) `--to` | 참조 해제 |
| `maestro send` | `--to`(필수) `--text`(필수) | 다른 에이전트에게 지시 보내기. 꺼져 있으면 시작(재개)함 |
| `maestro read` | `--to` | 그 에이전트(기본은 나)의 상태와 마지막 답변(`lastAnswer`) |
| `maestro selection` | 없음 | 사용자가 내 캔버스에서 지금 선택한 블록들(`selected`): 에이전트(name, branch, path), 문서(title, path), 프롬프트 블록(name), 구조 블록(kind `block`: title, body, area, to), 영역(kind `area`: title, holds, to), 선(kind `line`: from, to, relation, label, body — "from —관계: 라벨→ to" 명제). 나 자신의 블록은 빠져요. → [구조 그리기](../canvas/structure-drawing.md) |
| `maestro merge-request` | `<워커>` 또는 `--to <워커>`, `--verified "확인한 내용과 결과"` | Lead 전용. 워커 브랜치를 기준 브랜치에 합쳐 달라고 사용자에게 요청해요. 바로 `{ requested, replaced?, worker, branch, base, commits, files, mode, note, pr? }`로 답해요. 사용자가 카드에서 승인하면, 로컬 모드(`mode: "local"`)는 앱이 `--no-ff`로 합치고 결과가 `[maestro] 병합 …` 메모로 와요. PR 모드(`mode: "pr"`, `note: "… 승인되면 앱이 푸시하고 PR을 만들어요 …"`)는 앱이 브랜치를 푸시하고 PR을 만들고 `[maestro] PR …` 메모가 와요. 그 워커에 열린 PR이 있으면 `pr: N`(같은 PR이 갱신돼요). 워커가 부르면 오류예요. → [병합 요청](merge-request.md), [PR 모드](pr-mode.md) |
| `maestro draw` | `--file 구조.json` | 내 캔버스에 구조를 그려요. 형식 `{"areas":[{"title","in"?}], "blocks":[{"title","body"?,"area"?}], "arrows":[{"from","to","label"?,"as"?,"body"?}], "relations"?:[{"name","pattern"?,"head"?,"color"?,"description"?,"extends"?}]}`. `as`는 선의 관계 이름(없는 이름은 기본 모양으로 선언), `label`은 선 위 짧은 동사구, `body`는 긴 설명. 이름이 이미 있는 블록은 내용만 고쳐 쓰고, 화살표는 라벨·본문·관계만 바꿔요. 이름 오타(화살표 끝, 속한 영역)는 그리기 전에 오류로 돌려줘요. → [구조 그리기](../canvas/structure-drawing.md) |
| `maestro relation list` | 없음 | 이 프로젝트의 관계: `builtin`(기본 관계·지휘·지시·참조, 뜻과 모양) + `relations`(선언된 것). 새로 선언하기 전에 먼저 보고 다시 써요. → [관계](../canvas/relations.md) |
| `maestro erase` | `--title 이름` \| `--from 이름 --to 이름` \| `--file 지울것.json` \| `--all true` | 내 캔버스에서 그린 것을 지워요. `--title`은 그 이름의 블록이나 영역(영역 안의 것은 남아 한 단계 바깥으로), `--from/--to`는 그 방향의 화살표, `--file`은 `{"areas":[이름], "blocks":[이름], "arrows":[{"from","to"}]}`, `--all true`는 그린 블록·영역·화살표 전부(혼자만 써요). 이름은 마지막으로 저장된 캔버스에서 확인해서, 없는 이름이면 아무것도 지우지 않고 오류예요. 에이전트·문서·프롬프트 블록과 그 선은 지우지 않아요. 사용자는 Ctrl+Z로 되돌려요. → [구조 그리기](../canvas/structure-drawing.md) |
| `maestro relation remove` | `--name 이름` | 선언한 관계를 지워요. 그 관계로 그린 선은 남아 기본 관계(동작 있는 선은 그 내장 관계)로 돌아가요. 이 관계를 잇던 관계는 이 관계의 부모를 이어요(동작 유지). 내장 관계는 못 지워요. |
| `maestro relation define` | `--name` `[--description]` `[--pattern solid\|dashed\|dotted]` `[--head one\|both\|none]` `[--color blue\|green\|amber\|red\|violet\|teal]` `[--extends 관계]` | 관계를 선언하거나(같은 이름이면) 고쳐요. 비운 칸은 그대로예요. 모양은 이 고정 값들 중에서만, 뜻은 자유. 내장 관계는 `--color`만 돼요. 열린 캔버스가 바로 다시 읽어요(`maestro://relations`). |

- 성공하면 JSON을 보기 좋게 출력하고 종료 코드 0이에요. 결과에 `warning`이 있으면(예: `worktree create`가 워커는 만들었지만 첫 지시를 못 보냄) stderr에 `주의: <경고>`도 써요. 종료 코드는 그대로 0이에요. 서버 오류는 `오류: <메시지>`를 stderr에 쓰고 1, 플래그 값이 빠지면 2예요.
- `--to`는 이 프로젝트 워크트리의 이름, 브랜치, 폴더 이름, 전체 경로 중 하나로 찾아요(대소문자 무시).
- `worktree create`의 기본값:
  - `--agent`: 호출한 에이전트와 같은 어댑터, 없으면 `claude-code`
  - 권한 모드: 호출한 에이전트의 `permission_mode`. 새 워커의 에이전트가 그 권한을 받지 않으면 받는 것 중 그보다 느슨하지 않은 가장 가까운 것으로 만들어요(엄격한 순서: `plan` < `default` < `acceptEdits` < `auto` < `bypassPermissions`, 예: Claude Lead의 `default` → Antigravity 워커의 `plan`). 바뀌면 결과에 `permission` 메모가 붙어요. 그런 권한이 없거나 호출한 에이전트의 설정을 읽지 못하면(없음, 손상, 더 새 형식) 기본값 없이 오류예요
  - `--model`: 영문자·숫자와 `. _ : [ ] / @ -`만, `-`로 시작하지 않음. 다른 글자는 오류예요
  - 시작 런타임: 앱의 에이전트 보기(설정 → 일반)를 따라요. 터미널이면 워커도 터미널로 떠요
  - `--branch`: `agent-<워크트리 수 + 1>`
  - `--base`: `base_branch()`(Lead 기준 브랜치, 없으면 저장소 기본 브랜치)
  - `--checkout`을 주면 새 브랜치 대신 그 기존 브랜치를 체크아웃해요.
- 만들거나 참조를 바꾸면 `maestro://worktrees` 이벤트로 UI가 그 프로젝트 워크트리를 다시 읽어요(`App.tsx`).

## 구현

- 바이너리: `src-tauri/src/bin/maestro-cli.rs`. std와 `serde_json`만 써요.
  - `parse(args)`: `--`로 시작하지 않는 인자는 경로 단어, `--k v`는 JSON 본문 필드예요. 테스트: `words_become_route_and_flags_become_body`.
  - `call(route, body)`: `TcpStream`으로 `MAESTRO_URL`에 HTTP/1.1 `POST /api/<route>`를 보내요. 헤더 `X-Maestro-Token: $MAESTRO_TOKEN`, `X-Maestro-Worktree: $MAESTRO_WORKTREE`.
  - `MAESTRO_URL` 또는 `MAESTRO_TOKEN`이 없으면 "Maestro가 띄운 에이전트 터미널에서 실행하세요" 오류예요.
- 환경 변수: `agent::maestro_env()`가 에이전트를 띄울 때 넣어요.
  - `MAESTRO_URL`: `127.0.0.1:<port>` (`Approvals::addr()`, 실행마다 포트가 바뀜)
  - `MAESTRO_TOKEN`: 그 워크트리 전용 토큰 (`Approvals::agent_token(worktree)`, 실행마다 새 키로 만든 해시). 서버는 이 토큰이 `MAESTRO_WORKTREE`의 것과 맞을 때만 받아요.
  - `MAESTRO_WORKTREE`: 그 에이전트의 워크트리 경로(호출자 식별)
  - `PATH`: 앞에 `~/.maestro/bin` 추가
- 설치: `agent::install_cli(home)`이 앱 시작 때(`main()`의 `setup`) 앱 실행 파일 옆의 `maestro-cli(.exe)`를 `~/.maestro/bin/maestro(.exe)`로 복사해요. `.cmd` 대신 복사본을 쓰는 이유는 Git Bash(Windows에서 Claude의 Bash 도구)도 찾게 하려는 것이에요. 실행 중인 에이전트가 이전 복사본을 쓰고 있으면 복사가 조용히 실패하고 다음 시작 때 바뀌어요.
- 빌드: `Cargo.toml`의 `default-run = "maestro"`, `tauri.conf.json`의 `beforeDevCommand`가 `cargo build --bin maestro-cli`를 먼저 실행해요.
- 서버 라우팅: `approval.rs`의 `Approvals::handle`이 경로가 `/api/`로 시작하면 `x-maestro-worktree` 헤더를 호출자로 읽고, `x-maestro-token`이 그 호출자의 `agent_token`과 같은지 확인하고(틀리면 403 `"토큰이 맞지 않아요"`), 본문을 JSON으로 읽어 `control::handle(app, route, caller, input)`를 불러요. 성공 200, 실패 400 `{ "error": ... }`예요. 본문은 최대 4MB만 읽어요.
- `control::handle`:
  - `help`는 호출자 없이도 `ROUTES`를 돌려줘요. 나머지는 `MAESTRO_WORKTREE`가 비면 오류예요.
  - `locate()`: `worktree::main_checkout(caller)`로 저장소를 먼저 추측하고, 안 되면 등록된 모든 저장소를 훑어 호출자 워크트리를 찾아요(Windows에서 git 호출이 약 0.3초라 추측을 먼저 해요).
  - `worktree/create` → `worker_mode()`(호출자 meta를 `try_read_meta`로 읽고 `catalog::not_looser`) → `create_worktree()`(새 에이전트 창과 같은 함수: `check_new_agent`로 에이전트·권한·모델 확인, `start_want(None, view_default)`로 시작) → `link(app, root, me, new)` → `deliver()`. 테스트: `a_worker_takes_its_maestros_mode_never_a_looser_one`, `a_worker_never_gets_a_looser_mode_than_its_maestro`.
  - `deliver()`: `ensure()`로 에이전트를 시작(재개)하고 `wait_ready()`가 신뢰 확인이거나 시작 실패(로그인 필요 등)면 그 이유로 오류, 아니면 `agent.send()`.
  - `ref/add`·`ref/remove` → `md_path()`(상대 경로는 호출자 워크트리 기준, 추가는 존재하는 `.md`만) → `set_reference()`(캔버스와 같은 함수).
  - `read` → `last_answer()`: `runtime::history()`(채팅 뷰와 같은 공통 이벤트) + `events::last_answer`. 마지막 `agentText`가 있는 단위(Claude: 트랜스크립트 한 줄, Antigravity: 한 단계)의 글 블록을 `\n`으로 이어요. 턴 도중에 부르면 지금까지 쓴 마지막 글이 나와요(빈 글 블록도 포함). 같은 `itemId`로 다시 온 글(ACP의 빈 글 → 전체 글 upsert)은 블록 하나로, 마지막 글만 써요. 에이전트가 실행 중이고 그 런타임이 자기 기록을 가지면(ACP, R4) 그 기록을 읽어요. R3 이전과 결과가 같아요.
  - `merge-request`(`r == "merge-request" || r.starts_with("merge-request/")`): `merge_target()`(경로 뒤 단어 → `--to`) → `target()` → `check_merge_caller()`(호출자가 이 프로젝트의 Lead이고 대상이 프로젝트 폴더가 아님) → `project::merge_mode()` → `pr::plan_opts()`(PR 모드: 원격, 합쳐진 PR 머리, 마지막 squash, 열린 PR) → `worktree::plan_merge_with()` → PR 모드면 `pr::draft()`(카드의 제목·본문 초안) → `Merges::insert()`. 사용자를 기다리지 않고 바로 답해요. `--help`의 설명도 두 방식을 말해요("로컬 모드면 Maestro가 --no-ff로 합치고, PR 모드면 앱이 브랜치를 푸시하고 PR을 만들어요"). → [병합 요청](merge-request.md)
  - 모르는 경로는 `"모르는 명령: …"` 오류예요.
- 자동 허용: `Approvals::write_settings()`가 에이전트별 `--settings` 파일(`<meta>.settings.json`)에 `"permissions": { "allow": ["Bash(maestro:*)"] }`를 넣어요. `maestro` 명령마다 승인을 묻지 않게 해요. 같은 파일에 명령 가드(`PreToolUse` 훅과 `permissions.deny`)도 있어요. `maestro …` 명령은 가드가 보지 않아요(인자가 메시지라서). → [병합 요청](merge-request.md)
- 에이전트에게 주는 안내: `agent::system_brief()`의 `MAESTRO_HINT`가 Claude에는 `--append-system-prompt-file`(`<meta>.brief.md`)로, Antigravity에는 `--add-dir`로 넘긴 규칙 폴더(`<meta>.brief/AGENTS.md`)로 전달돼요. "`maestro`가 PATH에 있고, `worktree list`/`worktree create`/`send`/`read`로 위임·병렬화하고, `whoami`로 상위·워커·참조 문서를 보고 그 문서를 먼저 읽어라"는 내용이에요. Lead에게는 `COORDINATOR_HINT`가 더 붙어요.
- 개발 빌드에서는 시작할 때 `[maestro] MAESTRO_URL=…`을 stderr에 출력해요. 토큰은 워크트리마다 달라서, 아무 터미널에서 시험하려면 그 워크트리에서 Maestro가 띄운 에이전트의 환경에 있는 `MAESTRO_TOKEN`과 `MAESTRO_WORKTREE`를 같이 써야 해요.

- **채팅 런타임(ACP)에서:** `maestro send`는 ACP 큐에 들어가 차례대로 가요. 에이전트가 ACP로 돌아도 `maestro` 명령은 묻지 않고 돌아요(`allowedTools: ["Bash(maestro:*)"]`, 환경 변수는 어댑터를 거쳐 CLI에 그대로 가요). `maestro read`는 같은 트랜스크립트를 읽어서 런타임과 상관없이 같은 답을 줘요.

## 제약과 알려진 문제

- 삭제·폐기·연결 해제 명령은 없어요(`ref remove`는 있음). 워크트리를 없애는 건 사용자가 UI에서만 해요. `merge-request`도 요청만 하고, 병합(또는 PR 모드의 푸시·PR 생성·PR 병합)은 사용자의 승인 뒤 앱이 해요. PR을 만들거나 합치는 CLI 명령은 없고, 에이전트의 `git push`는 가드가 막아요. PR 모드 프로젝트에서 gh는 읽기(`pr view/list/diff/checks/status`, `run view/list/watch`, `repo view`, GET뿐인 `gh api` 등)만 지나고, 그 밖의 gh 명령(`pr create/edit/merge/review/comment`, `api -X PUT …` 등)은 모두 막혀요. 로컬 모드 프로젝트는 R5의 `gh pr create/merge`에 더해 그 우회로(쓰는 `gh api`, GraphQL mutation, gh 별칭 정의, gh 별칭·확장 실행(`gh co` 포함), 변수로 쓴 gh 명령)가 막혀요. mutation이 아닌 `gh api graphql` 쿼리와 `-X GET`과 같이 쓴 필드, `gh pr comment`·`gh issue create` 같은 보통의 쓰기는 R5처럼 돼요. 어느 모드든 토큰을 출력하는 gh 명령(`gh auth token`, `gh auth status -t`)과 관리 명령(`gh repo delete|edit`, `gh release delete`, `gh secret set`, `gh workflow run`, `gh run rerun`, `gh auth login|logout`, `gh config set`, `gh extension install`, `gh pr close|review` 등: `--help`만 붙으면 됨)은 막혀요. gh는 cobra가 명령을 찾는 방식대로 읽어서, 명령 단어 앞이나 사이에 `-R`/`--repo`·`--help` 말고 플래그를 두면(`gh pr -b view merge 12`) 어느 모드든 막혀요: 플래그는 명령 단어 뒤에 쓰세요.
- 호출자는 같은 프로젝트의 워크트리만 다룰 수 있어요(`target()`이 `root` 안에서만 찾음).
- 토큰은 워크트리마다 달라요(R5). 에이전트가 `MAESTRO_WORKTREE`만 다른 워크트리로 바꾸면 403이에요. 다른 에이전트의 환경 변수(같은 사용자 프로세스)를 읽어 내면 그 에이전트로 행세할 수 있지만, 서버는 `127.0.0.1`에만 열리고 키는 실행마다 바뀌어요.
- `Bash(maestro:*)` 자동 허용 규칙과 명령 가드(`PreToolUse` 훅)는 Claude Code에만 있어요. Antigravity는 허용 규칙을 넘기는 방법을 확인하지 못해서, `maestro` 명령을 처음 쓸 때 권한을 물을 수 있어요("항상 허용"하면 그 뒤로는 안 물어요). 병합·푸시 금지도 Antigravity에는 브리프 지시뿐이에요. 안내 문구(브리프)는 Antigravity도 받아요.
- 릴리스 빌드에서 `maestro-cli`가 앱 옆에 함께 번들되는지는 확인 안 됨이에요(`beforeBuildCommand`는 `npm run build`뿐이에요).
- 응답 파싱은 단순해요. `Connection: close`로 받은 전체를 `\r\n\r\n`에서 나눠요.

- 선택: 캔버스가 선택이 바뀔 때마다 `set_canvas_selection(owner, items)`로 알려 `AppState.selection`(워크트리 경로별, 메모리에만)에 두고, `selection` 경로가 호출한 에이전트 것을 돌려줘요. 캔버스가 닫히면 비워요. 에이전트 안내문(`MAESTRO_HINT`)에 "사용자가 '선택한 것', '이것들'이라고 하면 먼저 `maestro selection`을 보라"는 규칙이 있어요.

## 결정 기록

- 2026-11-14 (같은 날): `erase`와 `relation remove`를 더했어요. 지우기 명령도 그리기처럼 열린 캔버스에 이벤트로 넘기고(닫혀 있으면 열 때), 이름은 저장된 캔버스로 먼저 확인해요. 관계 파일 변경은 모두 한 잠금 아래 "지금 파일 읽기 → 바꾸기 → 검사 → 쓰기"로 해서, 두 에이전트가 동시에 선언해도 한쪽이 사라지지 않아요. 그림의 화살표에 동작 있는 관계(참조·지휘·지시와 그 자식)를 `as`로 주면 오류예요(예전엔 조용히 기본 관계가 됐어요). `selection`에 선택한 선(kind `line`)이 들어가요.
- 2026-11-14: `relation list`/`relation define`과 `draw`의 `as`·`body`·`relations`를 더했어요(관계: 선의 종류를 선언하고 쓰기). 선언을 강제하지 않으려고 `as`의 새 이름은 자동 선언해요.
- 삭제 명령을 넣지 않았어요. 에이전트는 워크트리를 추가하고 서로 대화할 수만 있고, 아무것도 허물 수 없게 하려는 것이에요(`control.rs` 머리 주석). 이 덕분에 `Bash(maestro:*)`를 자동 허용해도 된다고 판단했어요(`approval.rs` 주석: 매번 물으면 조율이 멈춰요). `merge-request`가 생겨도 이 판단은 그대로예요: 요청은 카드만 띄우고, 저장소를 바꾸는 건 사용자의 승인 클릭이에요.
- `/api` 토큰을 워크트리별로 바꿨어요(R5). 전역 토큰과 헤더의 워크트리만 보면 워커가 Lead로 행세해 `merge-request`를 부를 수 있어서예요. CLI 바이너리는 바뀌지 않았어요.
- 별도 서버 대신 승인용 로컬 서버(`approval.rs`)에 `/api/` 경로를 더했어요.
- 브리프는 파일로 넘겨요. 여러 줄 인자는 `cmd.exe /c`가 첫 줄바꿈에서 잘라서예요.
- 2026-11-13 (리뷰 수정): `worktree create`가 Lead의 권한을 확인 없이 물려줘서, "매번 묻기" Claude Lead가 만든 Antigravity 워커가 accept-edits로 파일을 고쳤어요(agy는 모르는 모드를 accept-edits로 돌려요). 이제 `create_worktree`가 확인하고, 그 전에 권한을 느슨하지 않은 쪽으로 맞춰요. 호출자 meta를 못 읽을 때 `acceptEdits`로 넘어가던 기본값도 없앴어요: 기본값이 사용자가 고른 것보다 느슨할 수 있어서예요. 맞추지 않고 거절하면 그런 Lead는 agy 워커를 만들 수 없어서 맞추는 쪽을 골랐어요.
- 2026-11-13 (리뷰 수정): 워커도 앱의 에이전트 보기를 따라 시작해요. 늘 채팅 런타임으로 떠서, 에이전트 보기가 터미널이어도 워커는 채팅이 대화를 쥐고 있었어요.
