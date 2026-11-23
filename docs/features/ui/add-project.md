---
title: 프로젝트 추가 메뉴 (폴더 열기, URL로 복제, 새 프로젝트)
area: ui
status: done
files: [src/Sidebar.tsx, src/ui/ProjectForm.tsx, src/projectForm.ts, src/App.tsx, src/QuickSwitch.tsx, src/Workspace.tsx, src/ipc.ts, src/icons.tsx, src/styles.css, src-tauri/src/newproject.rs, src-tauri/src/worktree.rs, src-tauri/src/exec.rs, src-tauri/src/main.rs, scripts/project-form-check.ts]
related: [features/ui/side-panels.md, features/ui/menus-and-dialogs.md, features/ui/quick-switch.md, reference/tauri-commands.md]
updated: 2026-11-24
---

# 프로젝트 추가 메뉴

## 요약

사이드바 머리의 `FolderPlus` "프로젝트 추가"(와 프로젝트가 없을 때의 [프로젝트 추가] 버튼)는 폴더 고르기로 바로 가지 않고 작은 메뉴를 열어요: 폴더 열기… · URL로 복제… · 새 프로젝트…. 폴더 열기는 예전과 똑같아요. URL로 복제는 Git URL을 받아 새 폴더에 `git clone`하고, 새 프로젝트는 빈 폴더를 만들어 `git init`과 첫 커밋을 한 뒤, 둘 다 폴더 열기처럼 프로젝트로 더하고 지도를 열어요.

## 동작

**메뉴**(누른 버튼 아래, 앱의 메뉴 부품)
1. `FolderOpen` 폴더 열기…: OS 폴더 고르기 → 예전 "프로젝트 추가"와 같아요(저장소가 아니지만 안에 저장소가 있으면 가져오기 창, 저장소가 아니면 "Git 저장소가 아니에요", 커밋이 없으면 "아직 커밋이 없어요" 확인 창). → [사이드바](side-panels.md)
2. `Link` URL로 복제…: 아래 복제 창.
3. `FolderPlus` 새 프로젝트…: 아래 새 프로젝트 창.
- 사이드바 머리 버튼, 사이드바 빈 상태의 [프로젝트 추가], 가운데 빈 화면의 [프로젝트 추가]가 같은 메뉴를 열어요. 메뉴가 열린 동안 머리 버튼은 눌린 면으로 남아요.
- 빠른 이동(Ctrl+K)에는 세 동작이 따로 있어요: "폴더 열기", "URL로 복제", "새 프로젝트"(셋 다 "프로젝트 추가"로도 찾아져요). → [빠른 이동](quick-switch.md)

**URL로 복제 창**(입력 대화상자)
- 제목 `Link` "URL로 복제". 칸: URL(처음 포커스) → 위치(경로 입력칸 + [찾기]) → 폴더 이름.
- 폴더 이름은 사용자가 고치기 전까지 URL의 마지막 부분에서 `.git`을 뺀 이름으로 채워져요(`https://github.com/a/repo.git` → `repo`, `git@github.com:a/repo.git` → `repo`). 한 번 고치면 URL을 바꿔도 그대로예요.
- 받는 URL: `https://`, `http://`, `ssh://`, `사용자@호스트:경로`(scp 꼴, `git@github.com:a/b.git`). 로컬 경로, `file://`, `git://`, `ext::` 같은 다른 전송, `-`로 시작하는 것은 거절해요("https://, http://, ssh:// 주소나 git@호스트:경로만 복제할 수 있어요").
- 바닥 [취소] [복제 `Ctrl ↵`]. 도는 동안 버튼이 "복제 중…"이고, 칸과 [취소]가 잠겨요. Esc와 바깥 클릭도 그동안은 닫지 않아요.
- 실패하면 창이 그대로 남고 바닥 위 빨간 상자에 이유가 나와요: "복제하지 못했어요" + git의 마지막 몇 줄(예: `fatal: repository '…' not found`), 10분을 넘기면 "10분 안에 끝나지 않아 복제를 멈췄어요". 반쯤 만들어진 폴더는 지워져서 고쳐서 다시 누를 수 있어요.
- 비밀번호·호스트 키를 물어야 하는 저장소는 묻지 않고 실패해요(창이 멈춰 기다리지 않게). 저장된 자격 증명(Git Credential Manager, ssh-agent의 키)은 그대로 쓰여요.
- 성공하면 창이 닫히고 폴더 열기처럼 프로젝트로 더해져 지도가 열려요. 빈 저장소(커밋 없음)를 복제했으면 "아직 커밋이 없어요" 확인 창이 이어서 떠요.

**새 프로젝트 창**(입력 대화상자)
- 제목 `FolderPlus` "새 프로젝트". 칸: 이름(처음 포커스) → 위치(경로 + [찾기]). 바닥 [취소] [만들기 `Ctrl ↵`], 도는 동안 "만드는 중…".
- 위치 안에 그 이름의 폴더를 새로 만들고, 그 폴더를 자기 저장소로 `git init -b main` 한 뒤 빈 첫 커밋(`Initial commit`)을 만들어 프로젝트로 더하고 지도를 열어요. 위치가 다른 저장소 안이어도 새 폴더는 따로 된 저장소예요.

**두 창이 같이 쓰는 것**
- 위치: 처음엔 마지막으로 쓴 위치(성공했을 때 저장, 두 창이 같이 써요), 없으면 목록 맨 끝 프로젝트가 있는 폴더, 그것도 없으면 홈 폴더예요. [찾기]는 OS 폴더 고르기("위치 선택")를 지금 위치에서 열어요. 경로를 직접 쓰거나 붙여 넣어도 돼요.
- 폴더 이름(이름) 규칙: 비어 있지 않고, 폴더 하나(`/`, `\` 없음), `..` 없음, `-`나 `.`으로 끝나거나 `-`로 시작하지 않음, Windows가 못 쓰는 글자(`: * ? " < > |`)와 장치 이름(CON, NUL, COM1 등) 아님. 어기면 칸이 빨간 테두리가 되고 "폴더 이름으로 쓸 수 없어요" 한 줄이 나오며 주 버튼이 눌리지 않아요.
- 위치 안에 같은 이름의 폴더가 이미 있으면 "이미 있는 폴더예요: <경로>"로 거절하고 아무것도 건드리지 않아요. 위치 폴더가 없거나 절대 경로가 아니면 "위치 폴더가 없어요".
- 키: Ctrl+Enter가 주 버튼, 한 줄 칸의 그냥 Enter는 아무것도 하지 않아요, Esc·바깥 클릭은 취소(도는 동안 빼고). 다른 입력 대화상자와 같아요. → [메뉴와 확인 창](menus-and-dialogs.md) "입력 대화상자"

## 구현

- 메뉴: `addProjectMenu(at, owner?)`(Sidebar.tsx)가 `openMenu`로 세 항목을 열어요. 항목은 `addProject()`(폴더 열기), `cloneProject()`, `newProject()`. 사이드바 머리 `.sb-head-btn`, `.sb-empty-btn`, 가운데 빈 화면(App.tsx)의 버튼이 `addProjectMenu(e.currentTarget)`를 불러요(`aria-haspopup="menu"`). 머리 버튼의 눌린 면은 `.sb-head-btn.icon[data-menu-open]`.
- `addFolder(dir)`(Sidebar.tsx): 폴더 하나를 프로젝트로 더하는 공통 길. `repo_state` → (저장소 아님 + 안에 저장소) 가져오기 창 / (저장소 아님·커밋 없음) 확인 창 → `init_repo` → 스토어 `addRepo`(= `add_repo` + `setActive` + 워크트리 읽기). 폴더 열기, 복제, 새 프로젝트가 모두 이 함수로 끝나요.
- 창: `openProjectForm(kind: "clone" | "new"): Promise<string | null>`(src/ui/ProjectForm.tsx). 가져오기 창과 같은 모양의 모듈 요청 하나(`current`) + `useSyncExternalStore`, `<ProjectFormHost />`가 창마다 하나(`WindowLayers`, Workspace.tsx). 창이 작업(`ipc.cloneRepo` / `ipc.createProject`)을 직접 돌리고, 성공하면 만든 폴더로 풀려요. 부른 쪽(`cloneProject`/`newProject`)이 그 폴더를 `addFolder`에 넘겨요. 실패는 창 안 `.dialog-error`.
- 모양: `.modal.form-dialog.project-form`(입력 대화상자 규칙 그대로). 위치 칸은 `<div class="field">` 안의 `.path-pick`(입력칸 flex 1 + [찾기] 32px). 키는 `NewAgentDialog`와 같은 `onKey` + `useBodyKeys`.
- 위치 기억: localStorage `maestro.projectParent`(성공한 위치). 없으면 `parentOf(마지막 프로젝트 root)`, 비면 `homeDir()`(`@tauri-apps/api/path`, 사용자가 먼저 쓰면 덮지 않아요). 찾기는 `open({ directory: true, title: "위치 선택", defaultPath })`(`@tauri-apps/plugin-dialog`).
- 순수 부분 `src/projectForm.ts`: `repoNameFromUrl(url)`(폴더 이름 채우기), `folderNameOk(name)`(이름 칸 검사), `parentOf(path)`. 테스트 `scripts/project-form-check.ts`(`npm test`). 백엔드와 같은 규칙이라 시험 경우도 Rust 테스트와 같아요.
- 백엔드 `src-tauri/src/newproject.rs`(main.rs의 `clone_repo`, `create_project`가 `spawn_blocking`으로 불러요. 둘 다 새 폴더 경로를 돌려줘요):
  - `clone_url(url)`: 앞뒤 공백을 떼고, 빈 값·`-`로 시작·공백/제어 문자 거절. `https://`·`http://`·`ssh://`(대소문자 무시)면 `사용자@호스트[:포트]` 부분에 호스트가 있고 사용자·호스트가 `-`로 시작하지 않아야 해요. 아니면 scp 꼴(`scp_like`: 첫 `:` 앞에 `/`·`\` 없음, `@` 앞뒤가 비지 않고 `-`로 시작 안 함, 경로 있음)만 받아요.
  - `repo_name_from_url(url)`: `?`·`#` 뒤를 떼고, 끝 `/`·`\`와 `.git`을 뗀 뒤 마지막 `/`·`\`·`:` 뒤. `check_name`을 못 넘으면 빈 값. 이름이 비어 오면 `clone`이 이걸로 채워요.
  - `check_name(name)`: 위의 폴더 이름 규칙(`reserved`가 장치 이름).
  - `destination(parent, name)`: 이름 검사, 위치가 있는 절대 경로 폴더인지, `<위치>/<이름>`이 아직 없는지(`symlink_metadata`라 끊어진 링크도 있는 것으로 봐요).
  - `clone(url, parent, name)` → `clone_into(url, dest)`: 폴더를 먼저 `create_dir`로 만들고(같은 순간 다른 것이 만든 폴더를 가로채거나 지우지 않게), `exec::run(Cmd::git(위치, ["clone", "--", url, dest]).network().timeout(600))`. `exec`가 `GIT_TERMINAL_PROMPT=0`, 빈 `GIT_ASKPASS`, `GCM_INTERACTIVE=never`, 사용자가 ssh 명령을 정하지 않았으면 `ssh -o BatchMode=yes`를 줘서 묻는 대신 실패하고, 시간이 지나면 프로세스 트리를 끝내요. 실패하면 `remove_made`(잠긴 파일 때문에 300ms 간격으로 세 번까지)로 폴더를 지우고 `clone_error`: 시간 초과, git 없음, 아니면 "복제하지 못했어요\n" + `last_lines(stderr, 4)`("Cloning into"와 빈 줄 뺀 마지막 네 줄).
  - `create(parent, name)`: `destination` → `create_dir` → `worktree::init_new_repo`(늘 `git init -b main`, 그다음 `init_repo`의 첫 커밋. `init_repo`만 쓰면 위치가 다른 저장소 안일 때 그 저장소를 찾아 아무것도 안 해요). 실패하면 폴더를 지워요.
  - 테스트(`cargo test newproject`): URL 받기·거절, URL에서 이름, 이름 규칙, `last_lines`, 새 프로젝트(다른 저장소 안에서도 자기 저장소, 브랜치 main, 있는 이름·나쁜 이름·없는 위치 거절), 실패한 복제가 폴더를 남기지 않음, 로컬 저장소 복제.
- 빠른 이동 항목: QuickSwitch.tsx의 `addProject`(폴더 열기), `cloneProject`, `newProject`.

## 제약과 알려진 문제

- 복제는 중간에 멈출 수 없어요. 도는 동안 창이 잠기고(최대 10분), 진행률도 보이지 않아요. 큰 저장소는 "복제 중…"만 오래 보여요.
- 비밀번호나 처음 보는 ssh 호스트 키를 물어야 하는 저장소는 실패해요. 터미널에서 한 번 복제하거나 자격 증명을 저장한 뒤 다시 해요.
- `git://`와 `사용자@` 없는 scp 꼴(`host:repo`)은 받지 않아요.
- 분리 창에서도 빠른 이동으로 세 동작을 쓸 수 있지만, 더한 프로젝트는 그 창의 스토어에만 바로 들어가요. 코드로 보면 메인 창은 앱이 켜질 때만 목록을 읽어요(`loadRepos`). 예전 "프로젝트 추가"도 같았어요. 화면에서는 확인 안 됨.
- 창 모양과 동작은 화면에서 확인하지 않았어요(타입 검사·빌드와 테스트만).

## 결정 기록

- 2026-11-24 (사용자: "프로젝트 추가 할 때 사이드에서 누르면 첨부한 이미지(오르카)처럼 이런 방향도 선택할 수 있는 것도 추가해줘"): Orca의 프로젝트 추가 팝업처럼 폴더 열기 · URL로 복제 · 새 프로젝트를 고르는 메뉴를 넣었어요. Orca의 호스트 고르기는 Maestro에 원격 호스트가 없어서 뺐어요. 메뉴 부품에 제목 줄이 없어서 제목 없이 세 항목만 두었어요(공용 `MenuItem` 모양을 바꾸지 않으려고).
- 같은 날: 복제·새 프로젝트는 폴더 경로만 돌려주고, 프로젝트로 더하는 건 폴더 열기와 같은 `addFolder`가 해요. 빈 저장소를 복제했을 때의 첫 커밋 확인까지 한 길로 가고, `add_repo`를 두 군데서 부르지 않아요.
- 같은 날: 복제할 폴더를 git보다 먼저 만들어요. git이 만들게 두면, 같은 이름을 동시에 만든 다른 것과 겹쳤을 때 실패 정리가 남의 폴더를 지울 수 있어요.
- 같은 날: 복제 중 취소는 넣지 않았어요. `exec::run`에는 밖에서 멈추는 길이 없고, 창을 닫아도 복제를 계속 돌게 두면 사라진 줄 안 프로젝트가 나중에 저절로 열려요. 대신 묻는 일을 모두 막고(프롬프트 없음, ssh BatchMode) 10분 한도를 뒀어요.
- 같은 날: URL은 `https`·`http`·`ssh`·scp 꼴만 받아요. `file://`과 로컬 경로는 "폴더 열기"로 충분하고, `ext::` 같은 전송은 명령을 돌릴 수 있어요. scp 꼴의 사용자는 `git`만이 아니라 아무 이름이나 받아요(Azure DevOps 등이 다른 이름을 써요). 어느 쪽이든 `--` 뒤에 넘겨 옵션으로 읽히지 않아요.
- 같은 날: 폴더 이름 채우기는 백엔드(`repo_name_from_url`)와 프런트(`repoNameFromUrl`)에 같은 규칙을 두 번 썼어요. 칸을 칠 때마다 백엔드를 부르지 않으려는 것이고, 두 테스트가 같은 경우를 시험해 어긋나지 않게 해요.
- 같은 날: 키는 다른 입력 대화상자처럼 Ctrl+Enter로 했어요. 한 줄 칸만 있는 창이라 그냥 Enter도 될 법하지만, 입력 대화상자끼리 키가 다르면 손이 헷갈려요.
