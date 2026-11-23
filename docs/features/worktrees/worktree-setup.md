---
title: 워크트리 준비 (로컬 파일 복사와 준비 명령)
area: worktrees
status: done
files:
  - src-tauri/src/prep.rs
  - src-tauri/src/project.rs
  - src-tauri/src/main.rs
  - src-tauri/src/worktree.rs
  - src-tauri/src/agent.rs
  - src-tauri/src/control.rs
  - src/settings/ProjectsSection.tsx
  - src/settings/prepText.ts
  - scripts/prep-check.ts
  - src/ipc.ts
  - src/types.ts
  - src/styles.css
related:
  - features/worktrees/create-worktree.md
  - features/worktrees/discard-worktree.md
  - features/ui/settings-dialog.md
  - features/orchestration/maestro-cli.md
  - reference/storage-layout.md
  - reference/tauri-commands.md
updated: 2026-11-24
---

# 워크트리 준비 (로컬 파일 복사와 준비 명령)

## 요약

새 워크트리에는 git이 추적하는 파일만 있어서, 워커는 프로젝트의 `.env`도 설치된 의존성도 없이 시작했어요. 이제 워크트리를 만들 때 프로젝트 폴더에서 git이 추적하지 않는 로컬 파일(기본 `.env*`)을 같은 자리에 복사하고, 프로젝트의 준비 명령(예: `npm install`)이 있으면 새 워크트리에서 백그라운드로 돌려요. 두 설정은 설정 → 프로젝트 → "워크트리 준비"에 있고 `<project_dir>/project.json`의 `copyFiles`·`setupCommand`에 저장돼요. 복사나 준비 명령이 실패해도 만들기는 실패하지 않고, 에이전트가 알게 돼요.

## 동작

- **언제**: Maestro가 워크트리를 새로 만들 때마다예요. 새 에이전트 창(에이전트·워커), `maestro worktree create` 둘 다 `create_worktree`를 거쳐요. Lead(프로젝트 폴더 자체)와 Maestro 밖에서 만든 워크트리에 에이전트를 붙일 때는 하지 않아요.
- **복사할 파일**(`copyFiles`): 프로젝트 폴더 기준 상대 경로 패턴 목록이에요. 화면에서는 쉼표로 나눠 써요.
  - `*`는 경로 한 칸(폴더 하나) 안에서만 아무 글자와 맞아요(`.env*`, `config/*.json`). 폴더를 건너뛰는 `**`는 없어요(`**`도 한 칸 안의 `*`와 같아요). `*`는 `.`으로 시작하는 이름과도 맞아요.
  - `config/local.json`처럼 하위 경로를 그대로 쓸 수 있어요. `\`도 `/`로 읽어요.
  - 파일만 복사해요. 폴더는 패턴에 맞아도 복사하지 않아요.
  - git이 추적하는 파일은 건너뛰어요(체크아웃에 이미 있어요). 무시된 파일과 추적하지 않는 파일만 복사해요.
  - 새 워크트리에 이미 있는 파일은 절대 덮어쓰지 않아요.
  - 처음 값은 `.env*`예요. 비우면 아무것도 복사하지 않아요(다시 기본값으로 돌아가지 않아요).
  - 쓸 수 없는 패턴: 빈 것, 절대 경로(`/…`, `C:…`, `\\server…`), `:`가 든 것, `..`, `.git` 안. 설정에서 저장할 때 이유와 함께 거절해요. 링크를 따라 프로젝트 밖으로 나가는 파일·폴더는 찾을 때 건너뛰어요.
- **준비 명령**(`setupCommand`): 비어 있으면(처음 값) 아무것도 돌리지 않아요. 있으면 새 에이전트가 뜬 뒤 그 워크트리에서 백그라운드로 돌려요. 만들기와 에이전트 시작을 기다리게 하지 않아요.
  - Windows는 `cmd /S /C "<명령>"`, 그 밖은 `sh -c <명령>`이에요. 한 줄만 돼요(여러 줄은 저장할 때 거절).
  - 30분 안에 끝나지 않으면 프로세스 트리째 멈춰요.
  - 출력(stdout·stderr)은 `<project_dir>/setup-logs/<워크트리 폴더>.log`에 써요. 맨 위에 `$ <명령>`, 맨 끝에 `[maestro] 종료 코드 N`.
- **에이전트가 듣는 것**:
  - 시작할 때(브리프): 준비 명령이 도는 동안 시작한 에이전트의 브리프에 `## 워크트리 준비` 절이 붙어요. "준비 명령 `…`을 백그라운드에서 돌리기 시작했다(로그 경로), 끝나면 `[maestro] 준비 명령` 메모가 온다, 그 전에는 같은 설치를 직접 하지 말고 설치가 있어야 하는 일(빌드, 테스트)은 메모를 받은 뒤에 하라". 복사하지 못한 파일이 있으면 그 목록도 이 절에 있어요.
  - 복사한 파일 중 새 워크트리의 `.gitignore`에 없는 것(무시하지 않는 미추적 파일, 예: `.gitignore`에 빠진 `.env`)이 있으면, 같은 절에 그 목록과 "비밀 값이 들었을 수 있으니 절대 커밋하지 마라(`git add -A` 대신 파일을 골라서 add)"가 붙어요. 워커가 `git add -A`로 커밋해 브랜치(PR 모드면 원격)에 올리는 것을 막으려는 것이에요.
  - 끝날 때(메모): `[maestro] 준비 명령이 끝났어요: npm install (종료 코드 0). 로그: …`. 실패하면 `준비 명령이 실패했어요 … (종료 코드 N)`, 시간 초과면 `…30분 안에 끝나지 않아 멈췄어요`, 실행하지 못하면 `준비 명령을 실행하지 못했어요 … (이유)`. 실패한 경우에는 로그의 마지막 15줄과 "필요한 설치는 이제 직접 하세요"가 붙어요.
  - 메모는 워커 완료 메모와 같은 길(`control::tell`)로 가요: 입력창이 떠 있으면 바로 붙여 넣고, 꺼져 있거나 신뢰 확인 중이면 `.notes.json`에 보관했다가 다음에 뜰 때 보내요(키 `setup:<워크트리 경로>`).
- **설정 화면**: 설정 → 프로젝트의 프로젝트 카드, 병합 방식 바로 아래에 "워크트리 준비" 작은 제목과 두 줄이 있어요. "복사할 파일"(placeholder `.env*`)과 "준비 명령"(placeholder `npm install`)은 글칸이고, 칸을 벗어나거나 Enter를 누르면 저장해요(바뀐 게 없으면 쓰지 않아요). 저장된 값은 백엔드가 다듬은 것으로 다시 보여요(공백·빈 항목·겹친 항목 제거, `\` → `/`). 거절되면 줄 아래에 빨간 글로 이유가 나오고 칸은 쓴 그대로예요.
- **폐기**: 준비 명령이 아직 도는 워크트리를 폐기하면 그 명령을 프로세스 트리째 멈추고 메모는 보내지 않아요. Maestro가 만든 워크트리면 로그 파일도 지워요.

## 구현

- `prep.rs`(새 모듈):
  - 패턴: `segments(pattern)`(검사하고 칸으로 나눔, 오류는 한국어), `clean_patterns(list)`(저장할 모양), `matches(pat, name, fold)`(칸 하나의 `*` 맞추기, `fold`는 대소문자 무시. Windows·macOS에서 켜요). 모두 순수 함수이고 단위 시험이 있어요.
  - 찾기: `find(root, segs, fold)`가 칸마다 이름 그대로 붙이거나(`*` 없음) `read_dir`로 맞는 이름을 골라 내려가요(이름 순). 이름이 `.git`이면 건너뛰고, `canonicalize`한 경로가 프로젝트 폴더(`canonicalize`) 밖이면 버려요. 마지막 칸은 파일, 중간 칸은 폴더만 남겨요. 한 번에 500개까지만 복사해요(넘으면 그 사실이 실패 목록에 들어가요).
  - `copy_local(root, worktree, patterns) -> Copied { copied, unignored, failed }`: 찾은 경로를 `worktree::tracked_among`(`git --literal-pathspecs ls-files -z -- <경로…>`, 200개씩)으로 추적 여부를 보고, 추적하지 않는 것만 `copy_new`로 복사해요. 복사한 것은 새 워크트리에서 `worktree::unignored_among`(`ls-files --others --exclude-standard`)으로 다시 물어, 무시되지 않는 것을 `unignored`에 담아요(git이 답하지 못하면 복사한 것 모두). `copy_new`는 대상에 무엇이든 있으면 건너뛰고, `create_new`로 열어 쓰는 사이에 생긴 파일도 덮어쓰지 않으며, 원본의 권한(실행 비트)을 옮겨요.
  - 브리프 기록: 프로세스 메모리의 표(`PREPS`, 워크트리 경로 키 `util::path_key`)에 `Prep { failed, unignored, setup: Option<Running> }`를 둬요. `begin(worktree, copied, setup)`이 에이전트가 시작하기 **전에** 기록하고 실행 id를 돌려줘요. 말할 것이 없으면(`quiet`) 기록하지 않아요. `brief(worktree)`가 `agent::system_brief`에서 `## 워크트리 준비` 절을 만들어요. `forget(worktree)`는 기록을 지우고 돌던 준비 명령을 멈춰요(`proc::Tree::kill`, 없으면 `proc::kill_tree`).
  - 실행: `start_setup(app, root, worktree, command, log, id)`가 스레드를 띄워 `run`을 불러요. `run`은 로그 파일을 만들고 `shell(command)`(Windows `raw_arg`로 따옴표 하나만 감쌈)로 띄워요. stdin은 null, stdout·stderr는 로그 파일, 환경은 다른 프로세스처럼 `agent::host_session_env`로 호스트 Claude 세션 변수를 빼요. `proc::contain` + `proc::Tree::adopt`로 트리를 잡고, `cli::quiet`(Windows `CREATE_NO_WINDOW`). 200ms마다 끝났는지 보고 `SETUP_TIMEOUT`(30분)이 지나면 트리째 죽여요. 끝나면 기록이 아직 이 실행 id의 것일 때만(폐기로 `forget`되지 않았을 때) 기록에서 `setup`을 지우고 `done_note`(메모 글, 순수)와 `tail_lines`(로그 끝 15줄, 줄마다 300자)로 메모를 만들어 `control::tell(app, root, worktree, "setup:<경로>", note)`로 보내요.
  - `log_path(project_dir, worktree)` = `<project_dir>/setup-logs/<워크트리 폴더 이름>.log`.
- `project.rs`: `ProjectSettings`에 `copy_files`(`#[serde(default = "default_copy_files")]`, 없으면 `[".env*"]`)와 `setup_command`(없으면 `""`)를 더했어요. 형식 버전(`FILE_VERSION` 1)은 그대로예요. 예전 파일은 기본값으로 읽히고, 예전 빌드는 모르는 필드를 무시해요. `prep(home, root) -> Prep`, `write_prep(home, root, copy_files?, setup_command?)`(빠진 값은 그대로, 패턴은 `clean_patterns`, 명령은 앞뒤 공백 제거, 여러 줄이면 거절).
- `main.rs`:
  - `create_worktree`: `worktree::create` 다음, `Agent::spawn` 전에 `project::prep` → `prep::copy_local` → `prep::begin`. 시작이 성공하면 준비 명령이 있을 때 `prep::start_setup`. 시작이 실패하면 `prep::forget` 뒤에 워크트리를 지워요.
  - `discard_worktree`: 에이전트를 빼면서 `prep::forget`(멈췄으면 에이전트를 끈 것처럼 300ms 기다려 파일 잠금을 풀어요), 폴더를 지운 뒤 로그 파일을 지워요.
  - 명령 `project_prep(root)`, `set_project_prep(root, copy_files?, setup_command?)`.
- `agent.rs` `system_brief`: 워커 PR 안내 뒤에 `prep::brief(worktree)`를 붙여요. 브리프는 시작할 때마다 만들어서, 준비 명령이 끝난 뒤 다시 시작한 에이전트의 브리프에는 그 절이 없어요(복사 실패 목록은 앱이 켜져 있는 동안 남아요).
- 프런트:
  - `ipc.projectPrep(root)`, `ipc.setProjectPrep(root, { copyFiles?, setupCommand? })`, 타입 `ProjectPrep`(types.ts).
  - `settings/ProjectsSection.tsx` `PrepRows`: 처음에 읽어 오고, 칸마다 blur에서 저장(Enter는 blur, 한글 조합 중 Enter는 무시). "복사할 파일"은 `settings/prepText.ts`의 `parseCopyFiles`(쉼표로 나눔, 공백·빈 것·겹침 제거) / `copyFilesText`(", "로 이음) / `copyFilesChanged`로 바뀐 것만 보내요. `scripts/prep-check.ts`가 `npm test`에서 확인해요.
  - CSS: `.set-prep`, `.set-prep-title`(styles.css).

## 제약과 알려진 문제

- 준비 명령의 기록(브리프 절, 메모를 보낼지)은 앱 메모리에만 있어요. 준비 명령이 도는 중에 앱을 끄면 메모는 오지 않고, 에이전트의 다음 브리프에도 그 절이 없어요. Windows에서는 명령도 함께 끝나요(작업 객체를 닫으면 트리째 끝나요). 다른 OS에서는 명령이 계속 돌 수 있어요(확인 안 됨).
- Windows에서는 준비 명령이 끝날 때 그 명령이 띄워 두고 간 프로세스(백그라운드 서버 등)도 함께 끝나요(같은 작업 객체). 준비 명령으로 서버를 띄워 두는 용도는 맞지 않아요.
- 복사는 만들 때 한 번이에요. 나중에 프로젝트 폴더의 `.env`를 바꿔도 이미 있는 워크트리에는 가지 않아요.
- 쉼표로 나누므로 이름에 쉼표가 든 파일은 화면에서 쓸 수 없어요. `:`가 든 이름도 쓸 수 없어요(Windows의 드라이브·스트림 때문에 모든 OS에서 막았어요).
- 준비 명령 출력은 로그 파일에만 있고 화면에는 보이지 않아요. 결과는 에이전트에게 가는 메모로만 알 수 있어요.
- 준비 명령은 에이전트가 쓰는 `MAESTRO_*` 환경 변수와 `maestro` CLI 경로를 받지 않아요.
- 무시되지 않는 복사 파일을 커밋하지 말라는 것은 브리프의 안내일 뿐이라, 에이전트가 따르지 않으면 커밋될 수 있어요. 그런 파일은 폐기 점검에서도 "커밋하지 않은 파일"로 보이고, 거기서 `보관`을 고르면 워커 브랜치에 커밋돼요. 비밀 파일은 프로젝트의 `.gitignore`에 넣어 두는 게 안전해요.
- 형식 버전을 올리지 않아서, 이 기능 전의 Maestro(같은 `~/.maestro`를 쓰는 개발 빌드나 되돌린 버전)가 병합 방식을 바꾸면 `project.json`을 다시 쓰면서 두 설정이 빠져요. 그 뒤로는 기본값(`.env*`, 준비 명령 없음)으로 읽혀요.
- 화면에서 확인하지 않았어요(설정 줄의 모양, 실제 `npm install` 실행).

## 결정 기록

- 2026-11-24: 새 워크트리에는 git이 추적하는 파일만 있어서 워커가 `.env`도 의존성도 없이 시작했어요. 워커가 설치부터 하거나 환경 변수가 없어 실패하지 않게, 만들 때 채워 줘요. 기본값은 복사 `.env*`, 준비 명령 없음이라 설정하지 않은 프로젝트에서는 `.env*` 복사 말고 달라지는 게 없어요.
- 2026-11-24: "준비 명령이 돌고 있다"는 첫 메모나 첫 지시 대신 **브리프**에 넣었어요. 시작하자마자 메모를 붙여 넣으면 사용자가 아무 말도 하지 않았는데 턴이 하나 돌고(평범하게 쓰는 길에 의식이 생겨요), 사용자의 첫 글에 덧붙이면 사용자 글을 고치게 돼요. 브리프는 그 에이전트가 시작할 때 읽고 다시 시작하면 사라져서, 도는 동안에만 말하는 데 맞아요. 끝났다는 소식은 그때 일어난 일이라 메모로 보내요.
- 2026-11-24: 복사하지 못한 파일도 같은 이유로 메모가 아니라 브리프에 넣었어요. 준비 명령이 없으면 끝 메모도 없어서, 실패를 알릴 자리가 브리프뿐이에요.
- 2026-11-24: 무시되지 않는 미추적 파일도 복사해요(작업 지시: "무시되거나 추적하지 않는 로컬 파일"). 대신 그런 파일은 브리프에서 이름을 대고 커밋하지 말라고 해요. 프로젝트 폴더에서는 사용자가 지켜보지만, 워커는 `git add -A`를 자주 써서 `.gitignore`에 빠진 `.env`가 워커 브랜치에 커밋되고 PR 모드면 원격에 올라갈 수 있어요. 무시된 파일만 복사하는 안은 작업 지시와 달라서 고르지 않았어요.
- 2026-11-24: 패턴은 칸 하나 안의 `*`만 둬요. 폴더를 건너는 `**`나 gitignore 문법은 찾는 범위가 커지고(큰 `node_modules`를 다 돌 수 있어요) 프로젝트 밖으로 나가는 경우를 막기 어려워요. 찾는 범위가 패턴의 깊이로 정해져요.
- 2026-11-24: 추적 여부는 경로를 찾은 뒤 `git ls-files`로 물어요. git의 pathspec `*`는 폴더를 건너서 맞아 위 규칙과 달라서, 맞추기는 직접 하고 git에는 그 경로만 `--literal-pathspecs`로 물어요.
- 2026-11-24: 준비 명령을 폐기 때 멈춰요. Windows에서는 `npm install`이 잡은 파일 때문에 폴더 지우기가 실패하고, 지운 폴더에서 30분 돌 수도 있어서예요.
