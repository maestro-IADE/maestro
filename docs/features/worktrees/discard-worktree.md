---
title: 워크트리 폐기 (안전 삭제)
area: worktrees
status: done
files:
  - src-tauri/src/worktree.rs
  - src-tauri/src/main.rs
  - src/nodes/AgentNode.tsx
  - src/ipc.ts
  - src/types.ts
  - src/styles.css
related:
  - features/worktrees/create-worktree.md
  - features/worktrees/branch-selection.md
  - features/worktrees/external-worktrees.md
  - features/orchestration/maestro-agent.md
  - reference/tauri-commands.md
  - reference/storage-layout.md
updated: 2026-11-11
---

# 워크트리 폐기 (안전 삭제)

## 요약

에이전트 블록의 휴지통 버튼으로 워크트리를 지울 때, Maestro는 먼저 지우면 잃는 작업(커밋하지 않은 파일, 기준 브랜치에 없는 커밋)이 있는지 점검해요(`check_discard`). 없으면 확인 한 번 뒤 `git worktree remove`(force 없이)와 `git branch -d`로 지우고, 있으면 목록을 보여 주고 보관 / 버리기 / 취소를 고르게 해요. 버릴 때도 `refs/maestro/trash/<브랜치>`에 백업 ref를 남겨서 되돌릴 수 있어요. 점검과 삭제는 git 명령을 여러 번 부르므로, 실행 중에는 휴지통 버튼이 꺼지고 옆에 "확인 중…" / "지우는 중…"이 보여요(백신이 프로세스 시작을 붙잡는 PC에서는 몇 초씩 걸려요).

## 동작

- 마에스트로와 외부 워크트리: 지금처럼 `confirm`으로 묻고 에이전트만 떼요. 폴더와 브랜치는 건드리지 않아요.
- Maestro가 만든 워크트리:
  - 잃는 작업이 없으면: "워크트리와 브랜치 '<브랜치>'를 지웁니다. 잃는 작업은 없어요: 커밋은 모두 '<기준>'에 들어가 있어요." 확인 후 지워요. 기존 브랜치를 체크아웃한 워크트리(`keepBranch`)는 "브랜치는 남겨요"로 안내하고 폴더만 지워요.
  - 잃는 작업이 있으면 `워크트리 폐기 · <이름>` 창이 떠요.
    - `커밋하지 않은 파일 N개`: `git status --porcelain` 줄(`XY 경로`), 최대 8개와 `… 외 N개`.
    - `'<기준>'에 없는 커밋 N개`: `abc1234 제목`, 최대 8개.
    - `보관`(기본 버튼): 폴더와 에이전트만 지우고 브랜치는 남겨요. 커밋하지 않은 파일은 그 브랜치에 `maestro: 커밋하지 않은 작업 보관 (<브랜치>)` 커밋으로 넣어요. 나중에 `기존 브랜치`로 워크트리를 다시 만들면 이어서 볼 수 있어요.
    - `버리기`: 브랜치까지 지워요(`keepBranch`면 브랜치는 남아요). 지우기 전에 백업 ref를 남겨요.
    - `취소`(또는 Esc, 바깥 클릭): 아무것도 하지 않아요. 에이전트도 끄지 않아요.
- 되돌리기: `git branch <새 이름> refs/maestro/trash/<브랜치>`. 커밋하지 않은 파일이 있었으면 그 ref는 브랜치 끝 위에 그 파일들을 담은 커밋 하나가 더 얹힌 모양이에요. 백업 목록은 `git for-each-ref refs/maestro/trash`로 봐요.

## 구현

- 프론트(`AgentNode.tsx`의 `AgentPanel`):
  - `discard()`: 마에스트로·외부면 `confirm` 후 `finish("clean")`. 아니면 `ipc.checkDiscard(root, path, branch)`로 점검하고, `changes`나 `commits`가 있으면 `losing` 상태에 넣어 `DiscardDialog`를 `createPortal`로 `document.body`에 띄워요. 없으면 `confirm` 후 `finish("clean")`.
  - `finish(choice)`: `ipc.discardWorktree(root, path, branch, choice)` 후 `remove(path)`.
  - `DiscardDialog`: `.modal.discard` 창, 목록은 `.discard-list`(styles.css). 버튼 순서는 `취소`, `버리기`(`danger`), `보관`(`primary`, autoFocus). 실행 중에는 버튼이 비활성이에요.
- 타입(`types.ts`): `DiscardCheck { base, changes, commits, keepBranch }`, `DiscardChoice = "clean" | "keep" | "discard"`.
- 명령(`main.rs`):
  - `owns_folder(app, root, path)`: 마에스트로(`is_root`)가 아니고 `worktree::is_managed`(경로가 `<project_dir>/worktrees` 아래)일 때만 참이에요. 이때만 폴더와 브랜치를 건드려요.
  - `check_discard(projectRoot, worktreePath, branch)`: `owns_folder`가 아니면 빈 `DiscardCheck`. 아니면 meta의 `keep_branch`와 `base_branch(app, root)`(마에스트로의 기준 브랜치, 없으면 저장소 기본 브랜치)로 `worktree::check_discard`. 에이전트는 끄지 않아요.
  - `discard_worktree(projectRoot, worktreePath, branch, choice)`: 에이전트를 끄고(300ms 대기), `owns_folder`면 `worktree::discard(..., choice)`, 그다음 `forget_agent`. 돌려주는 값은 작업이 간 곳(남긴 브랜치 이름이나 백업 ref), 없으면 `null`이에요. `worktree::discard`가 실패하면 meta는 지우지 않아요.
- 점검(`worktree::check_discard(root, path, branch, base, keep_branch)`):
  - `changes`: `status(path)` = `git status --porcelain=v1 -uall`(미추적 포함).
  - 기준: `base^{commit}`이 풀리지 않으면(지워진 브랜치 등) 프로젝트 폴더의 현재 브랜치와 HEAD로 비교해요. 비교는 커밋 해시(`base_commit`, 직렬화 안 함)로 해서 워크트리 안의 `HEAD`와 헷갈리지 않아요.
  - 프론트가 보낸 `branch`는 캔버스가 마지막으로 본 값이라, 에이전트가 워크트리 안에서 `git switch`/`checkout`을 했으면 실제와 달라요. 그래서 워크트리의 실제 브랜치(`head_branch()` = `git symbolic-ref -q HEAD`, detached면 없음)도 봐요.
  - `commits`: 워크트리 `HEAD` 기준 `unmerged()`. 단, HEAD가 남는 브랜치 위에 있으면(다른 브랜치로 바꿨거나, `branch` 위인데 `keep_branch`라 남음) 비워요. detached HEAD의 커밋은 폴더와 함께 사라지므로 `keep_branch`여도 세요.
  - HEAD가 `branch`를 떠났고 `branch`가 지워질 거면, `branch` 끝 기준 `unmerged()`도 더해요(중복은 빼요).
- `unmerged(dir, base, tip)`:
  1. `git rev-list --count base..tip`이 0이면 없음.
  2. `git cherry -v base tip`에서 `+`로 시작하는 줄만 남겨요(`-`는 같은 내용의 커밋이 기준에 있음).
  3. 남은 게 있으면 squash 판정: `git merge-tree --write-tree base tip`의 결과 트리가 `base^{tree}`와 같으면(합쳐도 바뀌는 게 없음) 모두 병합된 것으로 봐요. `merge-tree --write-tree`가 없는 git(2.38 미만)이나 충돌이면, 브랜치 전체 변경을 merge-base 위의 커밋 하나로 만들어(`commit-tree`) `git cherry`로 확인해요.
- 삭제(`worktree::discard(root, path, branch, base, keep_branch, choice)`):
  - `Clean`: 다시 점검해서 잃는 게 있으면 거절해요("다시 폐기를 눌러 확인해 주세요"). 점검과 클릭 사이에 에이전트가 작업했을 수 있어서예요. `git worktree remove <path>`(force 없음) 후 `git branch -d`. `-d`는 HEAD(또는 upstream)에 합쳐졌는지만 봐서 거절할 수 있어요. 그때 브랜치 끝이 기준의 조상이면 그냥 `-D`, 아니면(squash로 합쳐진 경우) 백업 ref를 남기고 `-D`.
  - `Keep`: `snapshot()`으로 커밋하지 않은 파일을 커밋으로 만들어, 워크트리가 지금 체크아웃한 브랜치(`head_branch()`, `branch`와 다를 수 있음)를 `update-ref refs/heads/<그 브랜치> <커밋> <커밋의 부모>`로 옮기고, `git worktree remove --force`. 옛 값을 함께 줘서 브랜치가 스냅샷의 부모에 있지 않으면 실패해요(다른 커밋을 덮지 않음). 돌려주는 값은 그 브랜치 이름이에요. detached 워크트리는 백업 ref에 남겨요.
  - `Discard`: `snapshot()` 커밋(없으면 워크트리 `HEAD`)을 `backup()`으로 `refs/maestro/trash/<sanitize(브랜치)>`에 남겨요. `branch`를 지울 거고 그 끝이 이 백업의 조상이 아니면(에이전트가 HEAD를 다른 곳으로 옮김) `branch` 끝도 따로 백업해요(`-2` …). 그다음 `git worktree remove --force`, `keep_branch`가 아니면 `git branch -D`. 백업이 둘이면 돌려주는 값은 `"<HEAD 백업>, <브랜치 백업>"`이에요.
- `snapshot(path, message)`: 임시 인덱스(`GIT_INDEX_FILE` = 임시 폴더의 `maestro-index-<해시>-<pid>`)에 `read-tree HEAD` → `add -A` → `write-tree` 후 `commit-tree <tree> -p HEAD`. 워크트리 자신의 인덱스는 건드리지 않아요. git 사용자 정보가 없으면 `Maestro <maestro@localhost>`로 서명해요(`commit_tree`, `init_repo`와 같은 방식).
- `backup(root, branch, commit)`: 이름이 이미 있으면 `-2`, `-3` …을 붙여서 앞의 백업을 덮어쓰지 않아요. 브랜치 이름의 `/` 등은 `_`로 바꿔요(ref 이름 충돌 방지).
- 워크트리 만들기에 실패했을 때의 정리(`create_worktree`)도 `DiscardChoice::Clean`을 써요.
- 테스트(`worktree.rs`): `discard_check_verdicts`(깨끗함 / 미추적 / 미병합 / cherry-pick / squash / 기준 없음), `clean_discard_removes_folder_and_branch_only_when_nothing_is_lost`, `discard_leaves_a_backup_ref_with_uncommitted_work`, `keep_commits_uncommitted_work_to_the_branch`, `discard_follows_a_branch_switch_inside_the_worktree`(브랜치를 바꾼 워크트리의 점검·보관·버리기, `keep_branch`인 detached 커밋).

## 제약과 알려진 문제

- `.gitignore`로 무시한 파일(빌드 결과, `node_modules` 등)은 점검에 안 잡히고 보관·백업에도 들어가지 않아요. 창에 안내 문구가 있어요.
- 기준은 마에스트로의 기준 브랜치(없으면 저장소 기본 브랜치) 하나예요. 워크트리를 만든 기준이 달랐어도(다른 브랜치에서 갈라짐) 그 브랜치와 비교해요. 워크트리별 기준은 meta에 저장하지 않아요.
- `git cherry`는 병합 커밋을 보지 않아요. 기준 브랜치를 받아 온 병합 커밋만 있는 브랜치는 깨끗하게 나와요.
- 백업 ref(`refs/maestro/*`)는 `git push` 기본 동작으로 올라가지 않고, 저절로 지워지지 않아요. 정리는 `git update-ref -d refs/maestro/trash/<이름>`으로 직접 해요.
- `보관`은 기존 브랜치(`keepBranch`)에도 커밋을 하나 얹어요. 그 브랜치를 다른 곳에서 쓰고 있었다면 커밋이 하나 늘어난 것으로 보여요.
- 서브모듈이 있는 워크트리는 `git worktree remove`(force 없음)가 거절해요. 그 오류가 그대로 보여요.
- 에이전트가 브랜치를 바꾼 워크트리에서 확인 창과 안내 문구의 브랜치 이름은 캔버스가 아는 옛 이름이에요. 점검과 삭제는 실제 HEAD와 그 이름의 브랜치를 둘 다 봐요. HEAD가 옮겨 간 다른 브랜치는 지우지 않고 남아요.

## 결정 기록

- PRD D5: 삭제 전 점검 → 있으면 보관/버리기/취소, 버릴 때 백업 ref, Maestro가 만든 워크트리만 지우고 브랜치는 `-d`로. 예전에는 항상 `git worktree remove --force` + `git branch -D`라서 커밋하지 않은 작업과 합치지 않은 커밋이 사라질 수 있었어요.
- `git cherry`로 같은 내용의 커밋을 병합된 것으로 보는 방식은 Nimbalyst `GitWorktreeService.ts`의 `uniqueCommitsAhead`를 참고했어요(코드는 새로 썼어요). squash 판정은 `git cherry`만으로는 여러 커밋을 하나로 합친 경우를 못 잡아서 `merge-tree`를 더했어요.
- `보관`의 뜻을 "폴더는 지우고 브랜치에 남기기"로 정했어요. 폴더까지 남기면 취소와 다를 게 없고, Maestro 관리 폴더에 에이전트 없는 워크트리가 남아요.
- 커밋하지 않은 파일은 stash 대신 임시 인덱스로 만든 커밋에 담아요. `git stash`는 워크트리가 지워지면 찾기 어렵고, 미추적 파일 처리도 옵션에 따라 달라요.
- 확인 창은 `confirm()`으로는 버튼 세 개를 만들 수 없어서, 다른 대화상자(`RefPicker`, `NewAgentDialog`)와 같은 `.modal` 모양으로 만들었어요.
