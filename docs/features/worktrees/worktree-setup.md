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

새 워크트리에는 git이 추적하는 파일만 있어서, 워커는 프로젝트의 `.env`도 설치된 의존성도 없이 시작했어요. 이제 워크트리를 만들 때 프로젝트 폴더에서 git이 추적하지 않는 로컬 파일(기본 `.env*`) 중 새 워크트리에서 git이 무시하는 것을 같은 자리에 복사하고, 프로젝트의 준비 명령(예: `npm install`)이 있으면 새 워크트리에서 백그라운드로 돌려요. 두 설정은 설정 → 프로젝트 → "워크트리 준비"에 있고 `<project_dir>/project.json`의 `copyFiles`·`setupCommand`에 저장돼요. 복사나 준비 명령이 실패해도 만들기는 실패하지 않고, 에이전트가 알게 돼요.

## 동작

- **언제**: Maestro가 워크트리를 새로 만들 때마다예요. 새 에이전트 창(에이전트·워커), `maestro worktree create` 둘 다 `create_worktree`를 거쳐요. Lead(프로젝트 폴더 자체)와 Maestro 밖에서 만든 워크트리에 에이전트를 붙일 때는 하지 않아요.
- **복사할 파일**(`copyFiles`): 프로젝트 폴더 기준 상대 경로 패턴 목록이에요. 화면에서는 쉼표로 나눠 써요.
  - `*`는 경로 한 칸(폴더 하나) 안에서만 아무 글자와 맞아요(`.env*`, `config/*.json`). 폴더를 건너뛰는 `**`는 없어요(`**`도 한 칸 안의 `*`와 같아요). `*`는 `.`으로 시작하는 이름과도 맞아요.
  - `config/local.json`처럼 하위 경로를 그대로 쓸 수 있어요. `\`도 `/`로 읽어요.
  - 파일만 복사해요. 폴더는 패턴에 맞아도 복사하지 않아요.
  - git이 추적하는 파일은 건너뛰어요(체크아웃에 이미 있어요).
  - 추적하지 않는 파일 중 **새 워크트리에서 git이 무시하는 것만** 복사해요(그 워크트리 커밋의 `.gitignore`, 모든 워크트리가 같이 쓰는 `.git/info/exclude`, 전역 무시 설정). 무시되지 않는 파일(예: `.gitignore`에 빠진 `.env.local`)은 워크트리에서 변경으로 보여 커밋될 수 있어서 복사하지 않고, 복사하지 못한 파일 목록에 "이 워크트리의 .gitignore에 없어 복사하지 않았어요"로 들어가요. 그래서 복사가 끝난 워크트리의 `git status`는 깨끗해요.
  - 새 워크트리에 이미 있는 파일은 절대 덮어쓰지 않아요(조용히 건너뛰어요).
  - 처음 값은 `.env*`예요. 비우면 아무것도 복사하지 않아요(다시 기본값으로 돌아가지 않아요).
  - 빈 항목은 저장할 때 말없이 빼요. 절대 경로(`/…`, `C:…`, `\\server…`), `:`가 든 것, `..`, `.git` 안, 파일을 가리키지 않는 것(`.`, `./`)은 저장할 때 이유와 함께 거절해요. 링크를 따라 프로젝트 밖으로 나가는 파일·폴더는 찾을 때 건너뛰어요. 새 워크트리 쪽에서 링크로 된 폴더(그 워크트리 커밋이 링크로 둔 폴더)를 지나 워크트리 밖에 쓰게 되는 파일은 복사하지 않고 실패 목록에 넣어요.
- **준비 명령**(`setupCommand`): 비어 있으면(처음 값) 아무것도 돌리지 않아요. 있으면 새 에이전트가 뜬 뒤 그 워크트리에서 백그라운드로 돌려요. 만들기와 에이전트 시작을 기다리게 하지 않아요.
  - Windows는 `cmd /S /C "<명령>"`, 그 밖은 `sh -c <명령>`이에요. 한 줄만 돼요(여러 줄은 저장할 때 거절).
  - 30분 안에 끝나지 않으면 프로세스 트리째 멈춰요.
  - 출력(stdout·stderr)은 `<project_dir>/setup-logs/<워크트리 폴더>.log`에 써요. 맨 위에 `$ <명령>`, 맨 끝 줄은 `[maestro] ` 뒤에 끝난 모양이에요: `종료 코드 N`, `시간이 다 돼 멈췄어요`, `종료 코드 없이 멈췄어요`(신호로 끝남), 또는 기다리다 난 오류 글.
- **에이전트가 듣는 것**:
  - 시작할 때(브리프): 준비 명령이 도는 동안 시작한 에이전트의 브리프에 `## 워크트리 준비` 절이 붙어요. "준비 명령 `…`을 백그라운드에서 돌리기 시작했다(로그 경로), 끝나면 `[maestro] 준비 명령` 메모가 온다, 그 전에는 같은 설치를 직접 하지 말고 설치가 있어야 하는 일(빌드, 테스트)은 메모를 받은 뒤에 하라". 복사하지 못한 파일(`.gitignore`에 없어 복사하지 않은 것 포함)이 있으면 이유와 함께 그 목록도 이 절에 있어요.
  - 끝날 때(메모): `[maestro] 준비 명령이 끝났어요: npm install (종료 코드 0). 로그: …`. 실패하면 `준비 명령이 실패했어요 … (종료 코드 N)`, 시간 초과면 `…30분 안에 끝나지 않아 멈췄어요`, 실행하지 못하면 `준비 명령을 실행하지 못했어요 … (이유)`. 실패한 경우에는 로그의 마지막 15줄과 "필요한 설치는 이제 직접 하세요"가 붙어요. 그 15줄에서는 제어 문자(ESC, 줄 가운데의 `\r` 등, 탭은 남김)를 빼요. Windows에서 UTF-8이 아닌 줄은 OEM 코드 페이지로 읽어요(cmd가 파일로 내보내는 자기 메시지, 예: `'npm'은(는) 내부 또는 외부 명령…`).
  - 메모는 워커 완료 메모와 같은 길(`control::tell`)로 가요: 입력창이 떠 있으면 바로 붙여 넣고, 꺼져 있거나 신뢰 확인 중이면 `.notes.json`에 보관했다가 다음에 뜰 때 보내요(키 `setup:<워크트리 경로>`).
- **설정 화면**: 설정 → 프로젝트의 프로젝트 카드, 병합 방식 바로 아래에 "워크트리 준비" 작은 제목과 두 줄이 있어요. "복사할 파일"(placeholder `.env*`)과 "준비 명령"(placeholder `npm install`)은 글칸이고, 칸을 벗어나거나 Enter를 누르면 저장해요(바뀐 게 없으면 쓰지 않아요). 저장된 값은 백엔드가 다듬은 것으로 다시 보여요(공백·빈 항목·겹친 항목 제거, `\` → `/`). 거절되면 줄 아래에 빨간 글로 이유가 나오고 칸은 쓴 그대로예요.
- **폐기**: 준비 명령이 아직 도는 워크트리를 폐기하면 그 명령을 프로세스 트리째 멈추고 메모는 보내지 않아요. 준비 명령이 뜨기 직전에 폐기되면 띄우지 않고, 뜨는 중이었으면 뜨자마자 멈춰요. Maestro가 만든 워크트리면 로그 파일도 지워요.

## 구현

- `prep.rs`(새 모듈):
  - 패턴: `segments(pattern)`(검사하고 칸으로 나눔, 오류는 한국어), `clean_patterns(list)`(저장할 모양), `matches(pat, name, fold)`(칸 하나의 `*` 맞추기, `fold`는 대소문자 무시. Windows·macOS에서 켜요). 모두 순수 함수이고 단위 시험이 있어요.
  - 찾기: `find(root, segs, fold)`가 칸마다 이름 그대로 붙이거나(`*` 없음) `read_dir`로 맞는 이름을 골라 내려가요(이름 순). 이름이 `.git`이면 건너뛰고, `canonicalize`한 경로가 프로젝트 폴더(`canonicalize`) 밖이면 버려요. 마지막 칸은 파일, 중간 칸은 폴더만 남겨요. 한 번에 500개까지만 복사해요(넘으면 그 사실이 실패 목록에 들어가요).
  - `copy_local(root, worktree, patterns) -> Copied { copied, failed }`: 찾은 경로를 `worktree::tracked_among`(`git --literal-pathspecs ls-files -z -- <경로…>`, 200개씩)으로 추적 여부를 보고, 추적하지 않고 새 워크트리에 아직 없는 것을 `worktree::ignored_among`(새 워크트리에서 `git -c core.quotePath=false check-ignore -- ./<경로…>`, 200개씩. 없는 경로도 물을 수 있어요. 종료 코드 1은 "하나도 아님")으로 물어, 무시되는 것만 `copy_new`로 복사하고 나머지는 이유와 함께 `failed`에 넣어요. git이 답하지 못하면 아무것도 복사하지 않아요. `check-ignore`는 `--literal-pathspecs`를 받지 않아서 `./`를 붙여 `:`로 시작하는 이름이 pathspec 마법으로 읽히지 않게 해요. git이 따옴표로 감싸는 이름(`"`, `\`, 제어 문자)은 그대로 돌아오지 않아 무시되지 않는 것으로 쳐요(복사 안 함).
  - `copy_new(root, from, to)`: 대상에 무엇이든 있으면 건너뛰어요. `inside(root, to)`로 대상 폴더의 있는 데까지를 `canonicalize`해 워크트리(`canonicalize`) 안인지 보고, 아니면(링크·정션을 지나 밖으로 나가면) 폴더를 만들기 전에 거절해요. `create_new`로 열어 쓰는 사이에 생긴 파일도 덮어쓰지 않아요. Unix에서는 원본의 권한으로 만들어요(`OpenOptionsExt::mode`, 0600인 `.env`가 쓰는 동안 남에게 읽히지 않게). 다 쓴 뒤 원본의 권한(실행 비트, Windows의 읽기 전용)을 옮겨요.
  - 브리프 기록: 프로세스 메모리의 표(`PREPS`, 워크트리 경로 키 `util::path_key`)에 `Prep { failed, setup: Option<Running> }`를 둬요. `begin(worktree, copied, setup)`이 에이전트가 시작하기 **전에** 기록하고 실행 id를 돌려줘요. 말할 것이 없으면(`quiet`) 기록하지 않아요. `brief(worktree)`가 `agent::system_brief`에서 `## 워크트리 준비` 절을 만들어요. `forget(worktree)`는 기록을 지우고 돌던 준비 명령을 멈춰요(`proc::Tree::kill`, 없으면 `proc::kill_tree`).
  - 실행: `start_setup(app, root, worktree, command, log, id)`가 스레드를 띄우고, 기록이 아직 이 실행 id의 것이면(그 사이 `forget`되지 않았으면) `run`을 불러요. `run`은 로그 파일을 만들고 `shell(command)`(Windows `raw_arg`로 따옴표 하나만 감쌈)로 띄워요. stdin은 null, stdout·stderr는 로그 파일, 환경은 다른 프로세스처럼 `agent::host_session_env`로 호스트 Claude 세션 변수를 빼요. `proc::contain` + `proc::Tree::adopt`로 트리를 잡고, `cli::quiet`(Windows `CREATE_NO_WINDOW`). 띄운 뒤 `started` 콜백이 트리와 pid를 기록에 넣으며 아직 원하는 실행인지 답해요. 아니면(`forget`이 먼저 와서 멈출 사람이 없으면) 바로 트리째 멈춰요(`stop`). 200ms마다 끝났는지 보고 `SETUP_TIMEOUT`(30분)이 지나면 `stop`해요. 끝나면 기록이 아직 이 실행 id의 것일 때만(폐기로 `forget`되지 않았을 때) 기록에서 `setup`을 지우고 `done_note`(메모 글, 순수)와 `tail_lines`(로그 끝 15줄, 줄마다 300자, 탭 말고 제어 문자 제거)로 메모를 만들어 `control::tell(app, root, worktree, "setup:<경로>", note)`로 보내요. 로그는 `decode_log`로 읽어요: 줄마다 UTF-8이면 그대로, 아니면 Windows에서 `GetOEMCP` + `MultiByteToWideChar`(kernel32), 그 밖에서는 깨진 글자를 바꿔 읽어요.
  - `log_path(project_dir, worktree)` = `<project_dir>/setup-logs/<워크트리 폴더 이름>.log`.
- `project.rs`: `ProjectSettings`에 `copy_files`(`#[serde(default = "default_copy_files")]`, 없으면 `[".env*"]`)와 `setup_command`(없으면 `""`)를 더했어요. 형식 버전(`FILE_VERSION` 1)은 그대로예요. 예전 파일은 기본값으로 읽히고, 예전 빌드는 모르는 필드를 무시해요. `prep(home, root) -> Prep`, `write_prep(home, root, copy_files?, setup_command?)`(빠진 값은 그대로, 패턴은 `clean_patterns`, 명령은 앞뒤 공백 제거, 여러 줄이면 거절).
- `main.rs`:
  - `create_worktree`: `worktree::create` 다음, `Agent::spawn` 전에 `project::prep` → `prep::copy_local` → `prep::begin`. 시작이 성공하면 준비 명령이 있을 때 `prep::start_setup`. 시작이 실패하면 `prep::forget` 뒤에 워크트리를 지워요(`DiscardChoice::Clean`). 복사한 파일은 모두 무시되는 것이라 워크트리가 깨끗해서 이 지우기가 그대로 돼요.
  - `discard_worktree`: 에이전트를 빼면서 `prep::forget`(멈췄으면 에이전트를 끈 것처럼 300ms 기다려 파일 잠금을 풀어요), 폴더를 지운 뒤 로그 파일을 지워요.
  - 명령 `project_prep(root)`, `set_project_prep(root, copy_files?, setup_command?)`.
- `agent.rs` `system_brief`: 워커 PR 안내 뒤에 `prep::brief(worktree)`를 붙여요. 브리프는 시작할 때마다 만들어요. 준비 명령이 끝난 뒤 다시 시작한 에이전트의 브리프에서는 준비 명령 문단만 빠지고, 복사하지 못한 파일 목록은 앱이 켜져 있는 동안 그 절에 남아요(목록이 없으면 절도 없어요). 앱을 다시 켜면 목록도 없어요.
- 프런트:
  - `ipc.projectPrep(root)`, `ipc.setProjectPrep(root, { copyFiles?, setupCommand? })`, 타입 `ProjectPrep`(types.ts).
  - `settings/ProjectsSection.tsx` `PrepRows`: 처음에 읽어 오고, 칸마다 blur에서 저장(Enter는 blur, 한글 조합 중 Enter는 무시). "복사할 파일"은 `settings/prepText.ts`의 `parseCopyFiles`(쉼표로 나눔, 공백·빈 것·겹침 제거) / `copyFilesText`(", "로 이음) / `copyFilesChanged`로 바뀐 것만 보내요. `scripts/prep-check.ts`가 `npm test`에서 확인해요.
  - CSS: `.set-prep`, `.set-prep-title`(styles.css).

## 제약과 알려진 문제

- 실행 확인(2026-11-24): 개발 빌드를 WebView2 원격 디버깅으로 띄워 페이지 안에서 버튼을 누르고 결과 글자를 읽어 돌렸어요. 화면을 눈으로 보지는 않았어요. 새 프로젝트(`.gitignore`에 `.env*`, 루트에 `.env`·`.env.local`)에 준비 명령 `echo prep-ok> prep-marker.txt`를 두고, Lead가 `maestro worktree create`로 워커를 만들게 했어요(모델 haiku). 워커 폴더에 `.env`·`.env.local`이 같은 내용으로 복사됐고, 준비 명령이 `prep-marker.txt`를 만들었고, `setup-logs/agent-2.log`에 명령과 "[maestro] 종료 코드 0"이 남았고, 워커 폴더의 `git status`는 무시된 파일 셋뿐이었어요. 준비 끝 메모 "[maestro] 준비 명령이 끝났어요: … (종료 코드 0). 로그: …"는 Lead가 준 첫 지시와 한 메시지로 워커에게 갔어요.
- 새 프로젝트의 워커는 Claude Code의 폴더 신뢰 확인에서 멈춰요(위 실행에서 확인). 첫 지시는 보관되고, Lead의 `worktree create` 답에 "그 워크트리는 폴더 신뢰 확인이 먼저 필요해요 (Maestro 화면에서 확인)"가 와요. 사용자가 화면에서 신뢰를 누르면 보관된 지시(와 준비 끝 메모)가 그때 가요. 워커마다 새 폴더라 워커를 만들 때마다 한 번씩 눌러야 해요.
- 준비 명령의 기록(브리프 절, 메모를 보낼지)은 앱 메모리에만 있어요. 준비 명령이 도는 중에 앱을 끄면 메모는 오지 않고, 에이전트의 다음 브리프에도 그 절이 없어요. Windows에서는 명령도 함께 끝나요(작업 객체를 닫으면 트리째 끝나요). 다른 OS에서는 명령이 계속 돌 수 있어요(확인 안 됨).
- Windows에서는 준비 명령이 끝날 때 그 명령이 띄워 두고 간 프로세스(백그라운드 서버 등)도 함께 끝나요(같은 작업 객체). 준비 명령으로 서버를 띄워 두는 용도는 맞지 않아요.
- 복사는 만들 때 한 번이에요. 나중에 프로젝트 폴더의 `.env`를 바꿔도 이미 있는 워크트리에는 가지 않아요.
- 쉼표로 나누므로 이름에 쉼표가 든 파일은 화면에서 쓸 수 없어요. `:`가 든 이름도 쓸 수 없어요(Windows의 드라이브·스트림 때문에 모든 OS에서 막았어요).
- 준비 명령 출력은 로그 파일에만 있고 화면에는 보이지 않아요. 결과는 에이전트에게 가는 메모로만 알 수 있어요.
- 준비 명령은 에이전트가 쓰는 `MAESTRO_*` 환경 변수와 `maestro` CLI 경로를 받지 않아요.
- 새 워크트리에서 무시되지 않는 파일은 복사하지 않아요. 워커에게 필요한 로컬 파일은 프로젝트의 `.gitignore`에 넣어 **커밋**하거나(새 워크트리는 커밋된 `.gitignore`를 따라서, 프로젝트 폴더에서 커밋하지 않은 수정은 쓰이지 않아요) `.git/info/exclude`에 넣어야 복사돼요.
- 워크트리 쪽 링크 검사는 복사할 때 한 번이에요. 검사와 쓰기 사이에 누가 폴더를 링크로 바꾸는 경우는 막지 않아요.
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
- 2026-11-24 (리뷰 반영): 위의 "무시되지 않는 미추적 파일도 복사한다"를 뒤집어, **새 워크트리에서 git이 무시하는 파일만** 복사해요. 무시되지 않는 복사본은 워크트리의 `git status`에 변경으로 보여서 세 가지가 깨졌어요. (1) 에이전트 시작이 실패했을 때의 `Clean` 지우기가 거절되고 그 오류가 버려져, 워크트리·브랜치·복사한 비밀 파일이 고아로 남았어요. (2) 손대지 않은 워커도 폐기할 때마다 보관/버리기를 물었어요. (3) 그 두 선택 모두 `snapshot`(`git add -A`)이 그 파일을 커밋했어요. `보관`이면 워커 브랜치에 들어가 병합·푸시될 수 있어요. 브리프의 "커밋하지 마세요"로는 Maestro 자신의 커밋을 막지 못해요. 버린 대안: 시작 실패 때만 복사본을 지우기((1)만 고쳐요), 복사한 경로를 `.git/info/exclude`에 넣기(모든 워크트리와 프로젝트 폴더가 같이 쓰는 파일이라 사용자의 프로젝트 폴더에서도 그 파일이 무시돼 버려요). 무시 여부는 파일이 생기기 전에 물어야 해서 `ls-files --others` 대신 없는 경로도 받는 `check-ignore`를 써요.
- 2026-11-24 (리뷰 반영): 실패 메모의 로그 끝 줄에서 제어 문자를 빼요. 메모는 PTY 에이전트에게 괄호 붙여 넣기(`ESC[200~ … ESC[201~` + `\r`)로 가서, 설치 스크립트가 일부러 실패하며 `ESC[201~`를 찍으면 붙여 넣기가 끝나고 나머지가 키로 읽혔어요(`\r`은 보내기, `ESC[Z`는 권한 모드 바꾸기). merge.rs `clean_text`, pr.rs와 같은 처리예요.
- 2026-11-24 (리뷰 반영): 복사 대상 쪽 링크도 막아요. 워크트리의 커밋은 프로젝트 폴더와 다를 수 있어서, 그 커밋이 `config`를 링크로 두면 `create_dir_all`과 `create_new`가 링크를 따라 비밀 파일을 워크트리 밖에 썼어요. 대상 폴더를 있는 데까지 `canonicalize`해 워크트리 안인지 보는 쪽을 골랐어요(링크 종류를 가리지 않고, Windows 정션도 걸러요). Unix에서는 파일을 처음부터 원본 권한으로 만들어, 다 쓴 뒤 권한을 옮기기 전 남이 열어 둘 틈을 없앴어요.
- 2026-11-24 (리뷰 반영): 준비 명령이 기록에 트리를 넣기 전에 폐기되면 아무도 멈출 수 없어 30분까지 돌 수 있었어요. 띄우기 전에 한 번 보고, 띄운 뒤 기록이 사라졌으면 바로 멈춰요.
- 2026-11-24 (리뷰 반영): Windows에서 cmd가 파일로 내보내는 자기 메시지는 OEM 코드 페이지(한국어 Windows는 CP949)라 UTF-8로 읽으면 깨졌어요. 가장 흔한 실패(`'npm'은(는) 내부 또는 외부 명령…`)의 설명 줄이 메모에서 사라져서, UTF-8이 아닌 줄만 OEM으로 읽어요. 명령 앞에 `chcp 65001`을 붙이는 안은 사용자 명령이 띄우는 프로그램의 출력 방식까지 바꿔서 고르지 않았어요.
