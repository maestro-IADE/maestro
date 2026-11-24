---
title: 프로젝트 추가 메뉴 (폴더 열기, 저장소 클론, 새 프로젝트)
area: ui
status: done
files: [src/Sidebar.tsx, src/ui/ProjectForm.tsx, src/projectForm.ts, src/App.tsx, src/QuickSwitch.tsx, src/Workspace.tsx, src/ipc.ts, src/icons.tsx, src/styles.css, src-tauri/src/newproject.rs, src-tauri/src/worktree.rs, src-tauri/src/exec.rs, src-tauri/src/main.rs, src-tauri/src/project.rs, scripts/project-form-check.ts]
related: [features/ui/side-panels.md, features/ui/menus-and-dialogs.md, features/ui/quick-switch.md, features/agents/chat-runtime.md, reference/tauri-commands.md, reference/settings-keys.md, reference/storage-layout.md]
updated: 2026-11-24
---

# 프로젝트 추가 메뉴

## 요약

사이드바 머리의 `FolderPlus` "프로젝트 추가"(와 프로젝트가 없을 때의 [프로젝트 추가] 버튼)는 폴더 고르기로 바로 가지 않고 작은 메뉴를 열어요: 폴더 열기… · 저장소 클론… · 새 프로젝트…. 폴더 열기는 예전과 똑같아요. 저장소 클론은 Git URL을 받아 새 폴더에 `git clone`하고, 새 프로젝트는 빈 폴더를 만들어 `git init`과 첫 커밋을 한 뒤, 둘 다 폴더 열기처럼 프로젝트로 더하고 지도를 열어요. 두 창은 URL이나 이름 하나만 묻고, 놓을 곳은 접힌 "위치" 줄 하나예요(처음엔 `~/maestro/projects`).

## 동작

**메뉴**(누른 버튼 아래, 앱의 메뉴 부품)
1. `FolderOpen` 폴더 열기…: OS 폴더 고르기 → 예전 "프로젝트 추가"와 같아요(저장소가 아니지만 안에 저장소가 있으면 가져오기 창, 저장소가 아니면 "Git 저장소가 아니에요", 커밋이 없으면 "아직 커밋이 없어요" 확인 창). → [사이드바](side-panels.md)
2. `Link` 저장소 클론…: 아래 클론 창.
3. `FolderPlus` 새 프로젝트…: 아래 새 프로젝트 창.
- 사이드바 머리 버튼, 사이드바 빈 상태의 [프로젝트 추가], 가운데 빈 화면의 [프로젝트 추가]가 같은 메뉴를 열어요. 메뉴가 열린 동안 머리 버튼은 눌린 면으로 남아요.
- 빠른 이동(Ctrl+K)에는 세 동작이 따로 있어요: "폴더 열기", "저장소 클론", "새 프로젝트"(셋 다 "프로젝트 추가"로도 찾아져요. 저장소 클론은 "git clone"과 예전 이름의 "복제"로도 찾아져요). → [빠른 이동](quick-switch.md)

**저장소 클론 창**(입력 대화상자)
- 제목 `Link` "저장소 클론". 칸: URL(처음 포커스) → 접힌 위치 줄(아래 "두 창이 같이 쓰는 것"). 폴더 이름 칸은 위치 줄을 펼치면 위치 칸 아래에 있어요.
- 폴더 이름은 사용자가 고치기 전까지 URL의 마지막 부분에서 `.git`을 뺀 이름으로 채워져요(`https://github.com/a/repo.git` → `repo`, `git@github.com:a/repo.git` → `repo`). 한 번 고치면 URL을 바꿔도 그대로예요. URL에서 쓸 수 있는 이름을 얻지 못하면 입력이 0.6초 멈춘 뒤 위치 줄이 펼쳐져서 폴더 이름 칸이 보여요(손으로 치는 도중의 URL로는 펼치지 않아요). 이름이 없거나 틀린 채 Ctrl+Enter를 눌러도 펼쳐져요.
- 받는 URL: `https://`, `http://`, `ssh://`, `사용자@호스트:경로`(scp 꼴, `git@github.com:a/b.git`). 로컬 경로, `file://`, `git://`, `ext::` 같은 다른 전송, `-`로 시작하는 것은 거절해요("https://, http://, ssh:// 주소나 git@호스트:경로만 클론할 수 있어요").
- 바닥 [취소] [클론 `Ctrl ↵`]. 도는 동안 버튼이 "클론하는 중…"이고, 칸과 [취소]가 잠겨요. Esc와 바깥 클릭도 그동안은 닫지 않아요.
- 실패하면 창이 그대로 남고 바닥 위 빨간 상자에 이유가 나와요: "클론하지 못했어요" + git이 한 말 네 줄까지(예: `remote: Repository not found.`, `fatal: repository '…' not found`), 10분을 넘기면 "10분 안에 끝나지 않아 클론을 멈췄어요", git이 없으면 "git을 찾지 못했어요. Git을 설치해 주세요". 받기는 됐는데 파일 풀기(checkout)에서 실패하면 첫 `error:` 두 줄(어느 파일이 왜)과 `fatal:` 줄이 나오고, "git status로 살펴보라"는 git의 안내는 빼요(폴더는 이미 지워졌어요). 이유에 `Filename too long`이 있으면 "경로가 너무 긴 파일이 있어요. git config --global core.longpaths true 후 다시 해 보세요" 한 줄이 붙어요.
- 반쯤 만들어진 폴더는 지워져서 고쳐서 다시 누를 수 있어요. 잠긴 파일 때문에 못 지웠으면 "폴더를 지우지 못했어요: <경로>" 한 줄이 붙어요(그 폴더를 지우거나 다른 이름을 써야 해요).
- 비밀번호·호스트 키를 물어야 하는 저장소는 묻지 않고 실패해요(창이 멈춰 기다리지 않게). 저장된 자격 증명(Git Credential Manager, ssh-agent의 키)은 그대로 쓰여요.
- 성공하면 창이 닫히고 폴더 열기처럼 프로젝트로 더해져 지도가 열려요. 빈 저장소(커밋 없음)를 클론했으면 "아직 커밋이 없어요" 확인 창이 이어서 떠요.
- 클론한 프로젝트의 에이전트는 채팅에서 처음 한 번 "이 프로젝트를 신뢰할까요?"를 물어요. 같은 경로에 예전에 신뢰한 프로젝트가 있었어도 클론이 그 답을 지워요. → [채팅 런타임](../agents/chat-runtime.md)

**새 프로젝트 창**(입력 대화상자)
- 제목 `FolderPlus` "새 프로젝트". 칸: 이름(처음 포커스, 비어 있으면 흐린 `my-project`) → 접힌 위치 줄. 바닥 [취소] [만들기 `Ctrl ↵`], 도는 동안 "만드는 중…".
- 위치 안에 그 이름의 폴더를 새로 만들고, 그 폴더를 자기 저장소로 `git init -b main` 한 뒤 빈 첫 커밋(`Initial commit`)을 만들어 프로젝트로 더하고 지도를 열어요. 위치가 다른 저장소 안이어도 새 폴더는 따로 된 저장소예요.
- git을 찾지 못하면 아무 폴더도 만들기 전에 "git을 찾지 못했어요. Git을 설치해 주세요"로 멈춰요(클론과 같은 글). git이 있는지는 [만들기]를 누를 때 봐요.
- 만든 프로젝트는 처음부터 신뢰한 프로젝트예요. 그 프로젝트의 Lead와 워커는 채팅에서 "이 프로젝트를 신뢰할까요?"를 묻지 않아요. 저장소 클론과 폴더 열기로 더한 프로젝트는 처음 한 번 물어요. 터미널 보기의 Claude Code는 자기 기록만 봐서 새 프로젝트에서도 자기 신뢰 창을 한 번 띄워요. → [채팅 런타임](../agents/chat-runtime.md)

**두 창이 같이 쓰는 것**
- 위치 줄: 테두리 상자 안의 한 줄(높이 36). 왼쪽 "위치", 가운데 만들 폴더의 전체 경로(`<위치>/<이름>`, 고정폭 12px, 홈 폴더는 `~`로 줄여 쓰고 넘치면 말줄임, 툴팁은 줄이지 않은 경로), 오른쪽 쉐브론. 누르면 펼쳐져서(쉐브론이 뒤집혀요) 위치 입력칸과 [찾기]가 나오고, 클론 창은 그 아래 폴더 이름 칸도 나와요. 다시 누르면 접혀요. 제목 아래 설명 글은 없어요.
- 위치 줄은 입력하는 동안 저절로 열렸다 닫혔다 하지 않아요. 펼쳐지는 때는 사용자가 누를 때와, 저절로는 셋뿐이에요: 클론에서 URL이 쓸 수 있는 폴더 이름을 주지 않은 채 입력이 0.6초 멈췄을 때, 클론에서 폴더 이름이 없거나 틀린 채 Ctrl+Enter를 눌렀을 때, 오류가 "이미 있는 폴더예요"일 때(두 창 다). 한 번 펼쳐지면 사용자가 접을 때까지 그대로예요(폴더 이름을 고쳐 써도 접히지 않아요).
- 위치 처음 값: 마지막으로 쓴 위치(성공했을 때 저장, 두 창이 같이 써요), 없으면 `~/maestro/projects`(홈 폴더 안)예요. [찾기]는 OS 폴더 고르기("위치 선택")를 지금 위치에서 열어요. 경로를 직접 쓰거나 붙여 넣어도 돼요.
- 위치 칸에는 줄이 보여 주는 꼴대로 `~`, `~/code`, `~\code`처럼 써도 돼요. 보내기 전에 홈 폴더를 넣어 전체 경로로 바꾸고, 저장하는 위치와 [찾기]가 여는 곳도 그 전체 경로예요. `~이름/…`처럼 다른 꼴은 그대로 보내요.
- 위치는 전체 경로여야 해요(`~`는 위처럼 바꾼 뒤에 봐요). 아니면 "위치는 전체 경로로 써 주세요: <위치>". 그 자리에 폴더가 아닌 것(파일 등)이 있으면 "위치가 폴더가 아니에요: <위치>". 아직 없는 위치는 중간 폴더까지 통째로 만들어요(처음 쓰는 `~/maestro/projects`도 이렇게 생겨요). 만들지 못하면 "위치 폴더를 만들지 못했어요: <이유>". 이렇게 만든 위치 폴더는 그 뒤 클론이나 첫 커밋이 실패해도 남아요. 실패해서 지우는 건 프로젝트 폴더 하나뿐이에요.
- 폴더 이름(이름) 규칙: 비어 있지 않고, 폴더 하나(`/`, `\` 없음), `..` 없음, `-`로 시작하거나 `.`으로 끝나지 않음, Windows가 못 쓰는 글자(`: * ? " < > |`)와 장치 이름(CON, NUL, COM1 등) 아님. 어기면 칸이 빨간 테두리가 되고 바로 그 칸 아래에 "폴더 이름으로 쓸 수 없어요" 한 줄이 나오며 주 버튼이 눌리지 않아요.
- 새 프로젝트를 만들다 실패해도(`git init`·첫 커밋) 만든 폴더를 지우고, 못 지우면 클론처럼 "폴더를 지우지 못했어요: <경로>"가 붙어요.
- 위치 안에 같은 이름의 폴더가 이미 있으면 비어 있어도 "이미 있는 폴더예요: <경로>"로 거절하고 아무것도 건드리지 않아요.
- 키: Ctrl+Enter가 주 버튼, 한 줄 칸의 그냥 Enter는 아무것도 하지 않아요, Esc·바깥 클릭은 취소(도는 동안 빼고). 다른 입력 대화상자와 같아요. → [메뉴와 확인 창](menus-and-dialogs.md) "입력 대화상자"

## 구현

- 메뉴: `addProjectMenu(at, owner?)`(Sidebar.tsx)가 `openMenu`로 세 항목을 열어요. 항목은 `addProject()`(폴더 열기), `cloneProject()`, `newProject()`. 사이드바 머리 `.sb-head-btn`, `.sb-empty-btn`, 가운데 빈 화면(App.tsx)의 버튼이 `addProjectMenu(e.currentTarget)`를 불러요(`aria-haspopup="menu"`). 머리 버튼의 눌린 면은 `.sb-head-btn.icon[data-menu-open]`.
- `addFolder(dir)`(Sidebar.tsx): 폴더 하나를 프로젝트로 더하는 공통 길. `repo_state` → (저장소 아님 + 안에 저장소) 가져오기 창 / (저장소 아님·커밋 없음) 확인 창 → `init_repo` → 스토어 `addRepo`(= `add_repo` + `setActive` + 워크트리 읽기). 폴더 열기, 클론, 새 프로젝트가 모두 이 함수로 끝나요.
- 창: `openProjectForm(kind: "clone" | "new"): Promise<string | null>`(src/ui/ProjectForm.tsx). 가져오기 창과 같은 모양의 모듈 요청 하나(`current`) + `useSyncExternalStore`, `<ProjectFormHost />`가 창마다 하나(`WindowLayers`, Workspace.tsx). 창이 작업(`ipc.cloneRepo` / `ipc.createProject`)을 직접 돌리고, 성공하면 만든 폴더로 풀려요. 부른 쪽(`cloneProject`/`newProject`)이 그 폴더를 `addFolder`에 넘겨요. 실패는 창 안 `.dialog-error`.
- 모양: `.modal.form-dialog.project-form`(입력 대화상자 규칙 그대로). 위치 줄은 `.place`(테두리, 펼치면 `[data-open]`) 안의 `button.place-head`(`aria-expanded`. `.field-label` "위치" + `.place-path`(고정폭 12px, 말줄임, `title`은 전체 경로) + `ChevronDown` 14, 펼치면 180° 돌아감)와, 펼쳤을 때만 그리는 `.place-body`(`.path-pick`: 입력칸(`aria-label="위치"`) flex 1 + [찾기] 32px. 클론은 그 아래 폴더 이름 `<label class="field">`)예요. 펼침은 `placeOpen` 하나예요. 머리를 누르면 뒤집히고, 그 밖의 곳은 켜기만 해요: 클론에서 URL이 있고 이름이 빈 채 0.6초가 지나면(`useEffect` 안의 `setTimeout`, URL이나 이름이 바뀔 때마다 다시 세요), `submit`이 `can`이 아닐 때 클론에서 URL이 있고 이름이 없거나 틀리면(Ctrl+Enter), 오류 글에 "이미 있는 폴더"가 있으면. 보낼 위치는 `place = expandHome(parent.trim(), home)`, 만들 폴더는 `target = joinPath(place, name)`이고, 줄에는 `shortPath(target, home)`, 툴팁에는 `target`을 써요. 이름이 틀렸다는 줄(`nameHint`, `span.field-hint[data-err]`)은 이름 칸 `<label class="field">` 안, 입력칸 바로 아래에 있어요(새 프로젝트는 이름, 클론은 펼친 위치 줄의 폴더 이름). 키는 `NewAgentDialog`와 같은 `onKey` + `useBodyKeys`.
- 위치 기억: localStorage `maestro.projectParent`(`PARENT_KEY`, 성공한 위치의 `place`라 `~`가 풀린 전체 경로, 처음 값은 `savedParent()`). 없으면 `defaultParent(homeDir())`(`@tauri-apps/api/path`, 사용자가 먼저 쓰면 덮지 않아요). 홈 폴더는 늘 읽어서 위치 줄의 `~`와 `expandHome`에 써요. 찾기는 `open({ directory: true, title: "위치 선택", defaultPath: place })`(`@tauri-apps/plugin-dialog`). `ipc.cloneRepo`·`ipc.createProject`도 `place`를 받아요.
- 순수 부분 `src/projectForm.ts`: `repoNameFromUrl(url)`(폴더 이름 채우기), `folderNameOk(name)`(이름 칸 검사). 백엔드와 같은 규칙이라 시험 경우도 Rust 테스트와 같아요. 위치 줄 쪽은 `joinPath(parent, name)`(위치가 쓰는 슬래시로 이어요: `\`가 있으면 `\`, 아니면 `/`. 끝 슬래시는 하나로 줄이고, 드라이브 루트 `C:\`나 `/`는 그 슬래시 하나만. 이름이 비면 위치 그대로), `defaultParent(home)`(`<홈>/maestro/projects`를 홈의 슬래시로, 홈이 비면 빈 값), `shortPath(path, home)`(홈 폴더를 `~`로. 폴더 경계에서만 줄여서 `C:\Users\meg`는 `C:\Users\me`의 안으로 보지 않아요. 홈이 Windows 경로면 대소문자와 슬래시 종류를 가리지 않아요), `expandHome(path, home)`(`~` 하나나 `~/`·`~\`로 시작하는 위치의 `~`를 홈 폴더(끝 슬래시 뗌)로 바꿔요. `~me/code` 같은 다른 꼴, 다른 경로, 홈을 아직 모를 때는 그대로). 예전 `parentOf`는 없앴어요. 테스트 `scripts/project-form-check.ts`(`npm test`, `expandHome(shortPath(…))`가 같은 경로로 돌아오는지도 봐요).
- 백엔드 `src-tauri/src/newproject.rs`(main.rs의 `clone_repo`, `create_project`가 `spawn_blocking`으로 불러요. 둘 다 새 폴더 경로를 돌려줘요):
  - `clone_url(url)`: 앞뒤 공백을 떼고, 빈 값·`-`로 시작·공백/제어 문자 거절. `https://`·`http://`·`ssh://`(대소문자 무시)면 `사용자@호스트[:포트]` 부분에 호스트가 있고 사용자·호스트가 `-`로 시작하지 않아야 해요. 아니면 scp 꼴(`scp_like`: 첫 `:` 앞에 `/`·`\` 없음, 마지막 `@` 앞뒤(ssh가 나누는 곳)가 비지 않고 `-`로 시작 안 함, 경로 있음)만 받아요.
  - `repo_name_from_url(url)`: `?`·`#` 뒤를 떼고, 끝 `/`·`\`와 `.git`을 뗀 뒤 마지막 `/`·`\`·`:` 뒤. `check_name`을 못 넘으면 빈 값. 이름이 비어 오면 `clone`이 이걸로 채워요.
  - `check_name(name)`: 위의 폴더 이름 규칙(`reserved`가 장치 이름).
  - `destination(parent, name)`: 이름 검사 → 빈 위치("위치를 골라 주세요") → 절대 경로인지("위치는 전체 경로로 써 주세요") → 그 자리에 폴더가 아닌 것이 있는지(`symlink_metadata`, "위치가 폴더가 아니에요") → `create_dir_all`로 없는 위치를 통째로 만듦("위치 폴더를 만들지 못했어요") → `<위치>/<이름>`이 아직 없는지(`symlink_metadata`라 끊어진 링크도 있는 것으로 봐요). 여기서 만든 위치 폴더는 뒤에서 실패해도 지우지 않아요.
  - `clone(url, parent, name)` → `clone_into(url, dest)`: 폴더를 먼저 `create_dir`로 만들고(같은 순간 다른 것이 만든 폴더를 가로채거나 지우지 않게), `exec::run(Cmd::git(위치, ["clone", "--", url, dest]).network().timeout(600))`. `exec`가 `GIT_TERMINAL_PROMPT=0`, 빈 `GIT_ASKPASS`, `GCM_INTERACTIVE=never`, 사용자가 ssh 명령을 정하지 않았으면 `ssh -o BatchMode=yes`를 줘서 묻는 대신 실패하고, 시간이 지나면 프로세스 트리를 끝내요. 실패하면 `clone_error`: 시간 초과, git 없음(`NO_GIT`, 새 프로젝트와 같은 글), 아니면 "클론하지 못했어요\n" + `clone_why(stderr)`("Cloning into", 빈 줄, checkout 실패 뒤 git의 안내 세 줄 `CLONE_ADVICE`를 빼고, 첫 `error:` 두 줄 + 나머지 줄의 마지막 것들, 합쳐 네 줄까지, git이 쓴 순서. `exec`는 stderr를 앞에서부터 256KB만 남기니 파일마다 `error:`가 쏟아져도 첫 줄들은 남아요). stderr에 `Filename too long`이 있으면 `core.longpaths` 한 줄을 더해요. 그다음 `left_behind`가 `remove_made`(잠긴 파일 때문에 300ms 간격으로 세 번까지, 지워졌는지 돌려줘요)로 폴더를 지우고, 못 지웠으면 "폴더를 지우지 못했어요: <경로>"를 붙여요.
  - `create(parent, name)`: `check_name` → `git --version`을 돌려 git 실행 파일이 없으면(`spawn_err`가 `NotFound`) `NO_GIT`으로 멈춤(아무 폴더도 만들기 전) → `destination` → `create_dir` → `worktree::init_new_repo`(늘 `git init -b main`, 그다음 `init_repo`의 첫 커밋. `init_repo`만 쓰면 위치가 다른 저장소 안일 때 그 저장소를 찾아 아무것도 안 해요). 실패하면 `left_behind`로 폴더를 지워요.
  - 신뢰: main.rs `create_project`가 `create` 뒤에 `project::trust_new(home, dir)`를 불러요. 저장소 루트를 `add_repo`처럼 `worktree::toplevel`로 정하고, 그 프로젝트 데이터 폴더(`worktree::project_dir`)의 `project.json`에 `trusted: true`를 써요(`set_trusted_in`). 실패하면 로그(`[maestro] new project not marked trusted`)만 남기고 만들기는 성공이에요. `clone_repo`는 반대로 클론한 뒤 `project::untrust(home, toplevel)`로 그 데이터 폴더의 `trusted`를 지워요(나머지 설정은 그대로, 실패하면 로그 `[maestro] clone's old trust not cleared`). 테스트: project.rs `a_new_project_starts_trusted_and_a_clone_doesnt`, `a_clone_or_a_removed_project_is_no_longer_trusted`. → [디스크 저장 위치](../../reference/storage-layout.md)
  - 테스트(`cargo test newproject`): URL 받기·거절(`a@b@-host:repo` 같은 겹친 `@` 포함), URL에서 이름, 이름 규칙, `clone_why`/`clone_error`(저장소 없음, Windows의 긴 경로 checkout 실패, 잘린 stderr), 새 프로젝트(`create_makes_its_own_repo_with_a_commit`: 다른 저장소 안에서도 자기 저장소, 브랜치 main, 있는 이름·나쁜 이름 거절, 상대 경로 위치는 "전체 경로"로 거절, 두 겹이 다 없는 위치(`new/place`)를 만들고 그 안에 프로젝트를 만듦, 파일인 위치는 "폴더가 아니에요"로 거절하고 파일은 그대로), 실패한 클론이 폴더를 남기지 않음, 로컬 저장소 클론.
- 빠른 이동 항목: QuickSwitch.tsx의 `addProject`(폴더 열기), `cloneProject`, `newProject`.

## 제약과 알려진 문제

- 클론은 중간에 멈출 수 없어요. 도는 동안 창이 잠기고(최대 10분), 진행률도 보이지 않아요. 큰 저장소는 "클론하는 중…"만 오래 보여요.
- 비밀번호나 처음 보는 ssh 호스트 키를 물어야 하는 저장소는 실패해요. 터미널에서 한 번 클론하거나 자격 증명을 저장한 뒤 다시 해요.
- `git://`와 `사용자@` 없는 scp 꼴(`host:repo`)은 받지 않아요.
- 분리 창에서도 빠른 이동으로 세 동작을 쓸 수 있지만, 더한 프로젝트는 그 창의 스토어에만 바로 들어가요. 코드로 보면 메인 창은 앱이 켜질 때만 목록을 읽어요(`loadRepos`). 예전 "프로젝트 추가"도 같았어요. 화면에서는 확인 안 됨.
- Orca와 달리 이미 있는 빈 폴더에는 만들지 않아요("이미 있는 폴더예요"). 이미 있는 폴더는 폴더 열기로 더해요.
- Orca와 달리 git이 있는지 미리 보고 버튼을 막지 않아요. git이 없으면 누른 뒤에 오류 글로 알아요.
- 실패해도 그때 새로 만든 위치 폴더(처음의 `~/maestro/projects` 등)는 남아요. 클론은 git이 있는지 먼저 보지 않아서, git이 없어 실패할 때도 위치 폴더는 생겨요.
- 기본 위치 `~/maestro/projects`는 앱 데이터 폴더 `~/.maestro/projects/`(점 있음)와 이름이 비슷하지만 다른 폴더예요. → [디스크 저장 위치](../../reference/storage-layout.md)
- 실행 확인(2026-11-24): 개발 빌드를 WebView2 원격 디버깅으로 띄워 페이지 안에서 버튼을 누르고 결과 글자를 읽어 돌렸어요. 화면을 눈으로 보지는 않았어요. 프로젝트 추가 메뉴(폴더 열기… · 저장소 클론… · 새 프로젝트…), 새 프로젝트 창(처음 위치 줄 `~maestroprojects`, 이름을 치면 `~maestroprojects<이름>`, 만들기 → 그 폴더가 새로 생기고 프로젝트로 더해져 열림), 저장소 클론 창(`…/con.git`은 바로 펼치지 않고 0.6초 뒤 위치 줄이 펼쳐지고 버튼이 잠김, `https://github.com/octocat/Spoon-Knife.git`이면 위치 줄 `~maestroprojectsSpoon-Knife`, "클론하는 중…" 뒤 프로젝트로 더해짐)이 그대로 동작했어요. 백엔드도 직접 불러 봤어요: 없는 위치를 통째로 만들기, 같은 이름 다시 → "이미 있는 폴더예요", 상대 경로 → "위치는 전체 경로로 써 주세요", `con` → "폴더 이름으로 쓸 수 없어요", 없는 GitHub 저장소 → "클론하지 못했어요" + `remote: Repository not found.` 두 줄(폴더 남지 않음), `file://` → 거절. 시험이 만든 폴더와 프로젝트는 지웠어요.
- 창 모양은 화면에서 확인하지 않았어요(위의 실행은 글자와 동작만). 2026-11-24에 바꾼 위치 줄(접기·펼치기, 0.6초 뒤·Ctrl+Enter·"이미 있는 폴더예요"에 펼쳐짐, `~` 경로와 `~` 입력)과 저장소 클론이라는 이름도 화면에서 확인하지 않았어요.

## 결정 기록

- 2026-11-24 (리뷰): 저장소 클론이 그 경로의 프로젝트 신뢰를 지워요(`project::untrust`). 프로젝트 데이터 폴더는 경로만으로 정해져서, 예전에 같은 경로에서 신뢰한 저장소의 답을 인터넷에서 새로 받은 내용이 물려받을 수 있었어요.
- 2026-11-24 (사용자: 새 프로젝트의 워커가 모두 신뢰 확인에서 멈춘 실행 뒤 "방법 찾아봐", 제안 뒤 "응, 고쳐줘"): 새 프로젝트는 만들 때 신뢰한 것으로 남겨요. 안에 든 것(빈 폴더, 빈 첫 커밋)을 모두 Maestro가 만들어서 물을 것이 없어요. 저장소 클론은 내용이 인터넷에서 온 것이라 처음 한 번 묻고, 폴더 열기도 어디서 온 폴더인지 몰라서 한 번 물어요. Claude Code의 설정에는 쓰지 않아요(사용자의 전역 Claude 설정을 바꾸지 않는다는 규칙). 그래서 터미널 보기의 신뢰 창은 새 프로젝트에서도 한 번 떠요. → [채팅 런타임](../agents/chat-runtime.md)
- 2026-11-24 (사용자: "프로젝트 추가 할 때 사이드에서 누르면 첨부한 이미지(오르카)처럼 이런 방향도 선택할 수 있는 것도 추가해줘"): Orca의 프로젝트 추가 팝업처럼 폴더 열기 · URL로 복제 · 새 프로젝트를 고르는 메뉴를 넣었어요. Orca의 호스트 고르기는 Maestro에 원격 호스트가 없어서 뺐어요. 메뉴 부품에 제목 줄이 없어서 제목 없이 세 항목만 두었어요(공용 `MenuItem` 모양을 바꾸지 않으려고).
- 같은 날: 복제·새 프로젝트는 폴더 경로만 돌려주고, 프로젝트로 더하는 건 폴더 열기와 같은 `addFolder`가 해요. 빈 저장소를 복제했을 때의 첫 커밋 확인까지 한 길로 가고, `add_repo`를 두 군데서 부르지 않아요.
- 같은 날: 복제할 폴더를 git보다 먼저 만들어요. git이 만들게 두면, 같은 이름을 동시에 만든 다른 것과 겹쳤을 때 실패 정리가 남의 폴더를 지울 수 있어요.
- 같은 날: 복제 중 취소는 넣지 않았어요. `exec::run`에는 밖에서 멈추는 길이 없고, 창을 닫아도 복제를 계속 돌게 두면 사라진 줄 안 프로젝트가 나중에 저절로 열려요. 대신 묻는 일을 모두 막고(프롬프트 없음, ssh BatchMode) 10분 한도를 뒀어요.
- 같은 날: URL은 `https`·`http`·`ssh`·scp 꼴만 받아요. `file://`과 로컬 경로는 "폴더 열기"로 충분하고, `ext::` 같은 전송은 명령을 돌릴 수 있어요. scp 꼴의 사용자는 `git`만이 아니라 아무 이름이나 받아요(Azure DevOps 등이 다른 이름을 써요). 어느 쪽이든 `--` 뒤에 넘겨 옵션으로 읽히지 않아요.
- 같은 날: 폴더 이름 채우기는 백엔드(`repo_name_from_url`)와 프런트(`repoNameFromUrl`)에 같은 규칙을 두 번 썼어요. 칸을 칠 때마다 백엔드를 부르지 않으려는 것이고, 두 테스트가 같은 경우를 시험해 어긋나지 않게 해요.
- 같은 날: 키는 다른 입력 대화상자처럼 Ctrl+Enter로 했어요. 한 줄 칸만 있는 창이라 그냥 Enter도 될 법하지만, 입력 대화상자끼리 키가 다르면 손이 헷갈려요.
- 2026-11-24 (리뷰): 복제 실패 이유를 "마지막 네 줄"에서 첫 `error:` 두 줄 + 마지막 줄들로 바꿨어요. Windows에서 흔한 checkout 실패(`Filename too long`, `aux.c` 같은 장치 이름)는 마지막 네 줄이 모두 git의 "git status로 살펴보라" 안내라 진짜 이유가 안 보였고, 그 폴더는 이미 지운 뒤였어요. stderr는 앞에서부터만 남으니 첫 `error:` 줄은 늘 남아요. `core.longpaths`는 사용자 설정이라 Maestro가 켜지 않고 한 줄로 알려만 줘요.
- 2026-11-24 (리뷰): 반쯤 만든 폴더를 못 지우면 이유에 "폴더를 지우지 못했어요: <경로>"를 붙여요. 말없이 남기면 다시 누를 때 "이미 있는 폴더예요"만 나와요.
- 2026-11-24 (리뷰): scp 꼴의 사용자/호스트를 첫 `@`가 아니라 마지막 `@`에서 나눠요. ssh가 그렇게 나눠서, `a@b@-o…:repo`가 호스트 검사를 비껴가지 않게요.
- 2026-11-24 (리뷰): 이름이 틀렸다는 줄을 폼 끝이 아니라 이름 칸 바로 아래에 둬요. 새 프로젝트는 이름 칸이 맨 위라 끝에 두면 위치 칸이 틀린 것처럼 보였어요. 이름 규칙 설명에서 "`-`로 끝나지 않음"을 뺐어요: 코드는 그걸 막은 적이 없고, 막을 이유도 없어요.
- 2026-11-24 (사용자: "url로 복제라는 말보다는 클론이 좀더 맞는 라이팅 같은데? 클론이란말 다 쓰잖아."): "URL로 복제"를 "저장소 클론"으로 바꿨어요. 메뉴 "저장소 클론…", 빠른 이동 "저장소 클론", 창 제목, 버튼 "클론"·"클론하는 중…", 오류 글("클론하지 못했어요", "10분 안에 끝나지 않아 클론을 멈췄어요", "…만 클론할 수 있어요")이 모두 바뀌었어요. 아이콘(`Link`)은 그대로예요. 빠른 이동에는 "복제"를 보조 낱말로 남겨 예전 말로 찾아도 나와요. 위 기록들의 "URL로 복제"·"복제"는 그때의 이름이에요.
- 2026-11-24 (사용자가 Orca의 새 프로젝트 만들기 창을 보여 주며: "orca는 보이는 이미지와 같이 적절한 기본값으로 만들어 준다는데 그것도 우리가 해줘야 하지 않을까?"): Orca(`repo-creation-handlers.ts`, `AddRepoCreateStep.tsx`)처럼 창마다 하나만 묻고(새 프로젝트는 이름, 클론은 URL), 놓을 곳은 접힌 위치 줄 하나로 보여 줘요. Orca의 줄은 "Git 리포지토리 in ~/orca/projects"와 전체 경로이고, Maestro는 "위치"와 만들 폴더의 전체 경로예요. 기본 위치는 Orca의 `~/orca/projects`에 맞춰 `~/maestro/projects`이고, 없으면 Orca의 mkdir -p처럼 통째로 만들어요. 마지막으로 쓴 위치를 기억하는 것은 그대로라, 다른 곳을 한 번 쓰면 그다음부터 그곳이 처음 값이에요. 예전 기본값(목록 맨 끝 프로젝트가 있는 폴더, 없으면 홈 폴더)은 없앴어요. 클론 창은 Orca의 이 창에 없는 것이라 같은 위치 줄을 쓰고, 폴더 이름은 펼친 줄 안에 두었어요. URL에서 이름을 얻지 못하면 이름을 고쳐야 넘어갈 수 있어서 그때만 저절로 펼쳐요. 제목 아래 설명 글은 두지 않았어요(화면 글은 짧게).
- 같은 날: 없는 위치를 "위치 폴더가 없어요"로 거절하던 것을 만들도록 바꾸면서, 상대 경로와 파일은 이유를 따로 말해요("위치는 전체 경로로 써 주세요", "위치가 폴더가 아니에요"). 실패했을 때 지우는 것은 이번에 만든 프로젝트 폴더 하나로 두고, 만든 위치 폴더는 지우지 않아요.
- 같은 날: 새 프로젝트도 git이 있는지 먼저 봐요(`git --version`). 없으면 클론이 쓰던 글("git을 찾지 못했어요. Git을 설치해 주세요", `NO_GIT`)로 멈추고, 그래서 위치 폴더도 만들지 않아요. Orca가 하는 두 가지는 하지 않았어요: 이미 있는 빈 폴더 받기(그 경우는 폴더 열기가 맡아요)와, git을 미리 확인해 버튼을 막기(누를 때 알려요).
- 2026-11-24 (리뷰): 위치 줄이 입력하는 동안 저절로 열리고 닫히지 않게 했어요. 전에는 URL이나 이름이 바뀔 때마다 펼침을 다시 계산해서, URL을 손으로 치면 도중의 모양이 이름을 주지 않아 줄이 세 번 깜박였어요. 나쁜 URL 때문에 열린 폴더 이름 칸은 거기에 처음 맞는 글자를 치는 순간 사라져서 나머지 입력을 잃었어요. 이제 펼침은 켜기만 하고(접는 건 사용자), 저절로 켜는 때는 입력이 0.6초 멈춘 뒤, 이름이 없거나 틀린 채 누른 Ctrl+Enter, "이미 있는 폴더예요" 오류예요. 마지막 것은 그 오류를 고칠 칸(폴더 이름, 위치)이 줄 안에 있어서예요.
- 2026-11-24 (리뷰): 위치 칸에 쓴 `~`를 홈 폴더로 바꿔 보내요(`expandHome`). 줄은 위치를 `~`로 보여 주는데, 그 꼴대로 쓰면 백엔드가 "위치는 전체 경로로 써 주세요"로 거절했어요. 저장하는 위치와 [찾기]가 여는 곳도 바꾼 전체 경로예요.
