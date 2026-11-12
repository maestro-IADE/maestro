---
title: 병합 요청 (maestro merge-request와 앱 병합, 명령 가드)
area: orchestration
status: done
files:
  - src-tauri/src/merge.rs
  - src-tauri/src/durable.rs
  - src-tauri/src/guard.rs
  - src-tauri/src/worktree.rs
  - src-tauri/src/control.rs
  - src-tauri/src/approval.rs
  - src-tauri/src/agent.rs
  - src-tauri/src/acp_runtime.rs
  - src-tauri/src/main.rs
  - src/nodes/MergeCard.tsx
  - src/MergeDiffDialog.tsx
  - src/monaco.ts
  - src/nodes/ApprovalCards.tsx
  - src/nodes/Attention.tsx
  - src/attention.ts
  - src/store.ts
  - src/Workspace.tsx
  - src-tauri/src/pr.rs
  - src-tauri/src/project.rs
related:
  - features/orchestration/pr-mode.md
  - features/orchestration/maestro-agent.md
  - features/orchestration/maestro-cli.md
  - features/orchestration/worker-hierarchy.md
  - features/orchestration/worker-done-notes.md
  - features/agents/approvals.md
  - features/ui/attention.md
  - architecture/local-server.md
  - features/worktrees/discard-worktree.md
  - reference/tauri-commands.md
  - reference/events.md
  - reference/storage-layout.md
updated: 2026-11-13
---

# 병합 요청 (maestro merge-request와 앱 병합, 명령 가드)

## 요약

워커 브랜치를 기준 브랜치에 합치는 일은 에이전트가 아니라 앱이 해요(PRD D4). 지휘자(마에스트로)는 검증을 마친 뒤 `maestro merge-request --to <워커> --verified "…"`로 요청만 하고, 사용자가 마에스트로 블록의 병합 카드에서 승인하면 Maestro(Rust)가 프로젝트 폴더에서 `git merge --no-ff`로 합쳐요. 프로젝트의 병합 방식이 "PR"이면(R6, 설정 > 프로젝트) 승인은 대신 브랜치를 푸시하고 `gh`로 PR을 만들어요. 이 문서는 기본인 "로컬" 방식을 다루고, PR 방식은 [PR 모드](pr-mode.md)를 보세요. 로컬 방식의 결과(병합, 거부, 기준 브랜치가 바뀜, 충돌)는 `[maestro] 병합 …` 메모로 지휘자에게 가요. Claude Code 에이전트는 명령 가드(`guard.rs`, `PreToolUse` 훅)가 기준 브랜치에서의 `git merge`, 모든 `git push`, 되돌리기 어려운 git 명령을 실행 전에 막아요.

## 동작

**요청 (지휘자)**
- `maestro merge-request <워커>` 또는 `maestro merge-request --to <워커> [--verified "확인한 내용과 결과"]`. 워커는 이름·브랜치·폴더 이름·경로 중 하나예요(`feat/x` 같은 브랜치도 그대로 써요).
- 그 프로젝트의 마에스트로만 쓸 수 있어요. 워커가 부르면 "merge-request는 이 프로젝트의 마에스트로만 쓸 수 있어요 …" 오류예요. 대상은 프로젝트 폴더가 아닌 이 프로젝트의 어떤 워크트리든 돼요(하위 워커, 외부 워크트리 포함). 카드가 대상을 보여 줘요.
- 요청할 때 확인하는 것(하나라도 걸리면 지휘자에게 오류로 돌려줘요, 카드 없음):
  - 기준 브랜치(`base_branch`)가 로컬 브랜치예요(`HEAD`는 안 돼요).
  - 프로젝트 폴더가 기준 브랜치에 있어요.
  - 워커가 브랜치 위에 있고(detached 아님) 그 브랜치가 기준 브랜치가 아니에요.
  - 워커에 커밋하지 않은 추적 파일 변경이 없어요. 추적하지 않는 새 파일은 막지 않고 개수만 경고로 보여요.
  - 워커 브랜치에 기준 브랜치에 없는 커밋이 있어요("이미 들어 있어요"가 아님).
  - `git merge-tree --write-tree`가 충돌을 예상하지 않아요(git 2.38 이상. 못 쓰면 `conflictsChecked: false`로 카드에 경고).
- 요청은 바로 답해요(`{"requested": id, "replaced"?: 이전 id, "worker", "branch", "base", "commits", "files", "mode", "note", "pr"?}`). 사용자를 기다리지 않아요. 같은 워커의 새 요청은 이전 카드를 바꿔요(`replaced`). `mode`는 그 프로젝트의 병합 방식(`local` | `pr`)이고 요청에 기록돼요: 승인은 요청 때의 방식대로 해요. PR 모드에서는 프로젝트 폴더를 보지 않고, 워커의 커밋하지 않은 추적 파일을 거절하지 않고 기록해요. → [PR 모드](pr-mode.md)
- 병합 방식을 바꾸면 그 프로젝트의 대기 요청은 메모 없이 거둬요(`withdrawn`). 다른 방식으로 계획한 요청을 승인하지 않게 하려는 것이에요. 단, PR 모드 요청이 끝나지 않았으면(PR 작업 중이거나, 만들기 결과가 애매해 PR을 아직 찾지 못함) 방식을 바꿀 수 없어요 → [PR 모드](pr-mode.md)
- 같은 워커의 새 요청이 PR 모드 요청을 바꿀 때, 앞 요청의 애매한 PR 만들기(`createAttemptAt`)는 브랜치가 같으면 새 요청으로 넘어가요(`Merges::add`, `PrProgress::carried`). 새 승인도 먼저 찾기만 해서 중복 PR을 만들지 않아요.

**카드 (사용자)**
- 마에스트로의 블록(허브 캔버스의 에이전트 블록·막대, 채팅·CLI 뷰, 전체 보기)에서 도구 승인 카드 위에 `병합 요청` 카드가 떠요(`ApprovalCards`가 함께 그려요). 사이드바의 마에스트로 점과 카드에는 "승인 대기"가 보이고, 프로젝트 캔버스의 워커 카드에는 `병합 요청` 칩이 붙어요.
- 카드 내용: 워커 이름, `브랜치 → 기준`, `기준sha..워커sha`, `+추가 −삭제`, 지휘자의 검증 글(8줄 넘으면 "더 보기", 없으면 "지휘자가 검증 내용을 적지 않았어요"), 경고(요청 때 프로젝트 폴더에 커밋하지 않은 변경, 워커의 추적 안 하는 새 파일, 충돌 사전 점검 못 함), 커밋 목록(10개, "더 보기"), 변경 파일 목록(코드 `A M D R T`, 경로, `+a −d`, 바이너리).
- **확인할 일 팝오버**(상단 막대): 대기 중인 병합 요청이 마에스트로의 프로젝트 아래 한 줄씩 떠요. 1줄 `GitMerge` + 워커 이름, 2줄 "<브랜치> → <기준> · 파일 N · 검증 있음/없음", 버튼 [변경 보기](아래 비교 창) · [열기](마에스트로를 고정 탭으로). 팝오버에서는 병합·거부하지 않아요. 보지 않고 누르지 않게, 병합과 거부는 이 카드에서 해요(변경 보기 창 바닥의 병합·거부는 UI 구조 재구성 단계 10). PR 모드에서 손이 필요한 PR(CI 실패, 병합할 수 있음)도 같은 팝오버에 "PR #N · …"으로 떠요. → [확인할 일](../ui/attention.md)
- `변경 보기`나 파일 줄을 누르면 비교 창이 열려요. 왼쪽 파일 목록, 오른쪽 읽기 전용 Monaco 비교. 내용은 요청 때 기록한 병합 기준(merge-base)과 워커 끝 커밋이에요. 워커가 그 뒤에 바꾼 파일이 아니에요. 바이너리나 2MB 넘는 파일은 안내만 나와요.
- `병합`: 누르는 동안 "합치는 중…". 끝나면 카드가 사라져요. 기준 브랜치가 바뀌었거나 충돌·실패면 화면 오류 알림으로도 알려요.
  - 프로젝트 폴더가 준비 안 됐으면(다른 브랜치, 커밋하지 않은 추적 파일 변경, 다른 git 작업 중, 병합이 더할 파일 자리에 추적하지 않는 파일이나 무시된 파일(`.env` 등)이 있음, git이 시작 전에 거부) 카드가 남고 이유가 카드 안에 보여요. 정리한 뒤 다시 누르면 돼요.
- 카드 영역(`.approvals`)은 블록 높이의 60%까지만 차지하고 넘치면 스크롤돼요. 병렬 라운드 뒤처럼 병합 카드가 여러 장 쌓여도 아래 입력창이나 터미널이 밀려나지 않아요(캔버스 휠 확대도 그 안에서는 안 돼요: `nowheel`).
- `거부`: 사유 입력(선택)이 열리고 `확인`이면 거부해요. 사유는 지휘자 메모에 들어가요.
- 요청은 앱을 다시 켜도 남아요. 워커를 폐기하면 그 요청은 메모 없이 사라져요. 마에스트로를 폐기하면 그 마에스트로의 대기 요청도 메모 없이 사라져요.

**결과 메모 (지휘자 입력창)** — 워커 완료 메모와 같은 길(입력창에 붙이거나, 꺼져 있으면 보관했다가 다음 시작 때)로 가요.
- 병합: `[maestro] 병합 완료: 워커 '<이름>' (<브랜치>) → <기준>, 병합 커밋 <sha7> (커밋 N개). 프로젝트 폴더에서 빌드·테스트로 확인하세요.` 요청 뒤 워커 브랜치에 커밋이 더 생겼으면 "(… 이번 병합에는 들어가지 않았어요)"가 붙어요.
- 거부: `[maestro] 병합 요청 거부: 워커 '<이름>' (<브랜치> → <기준>). 사용자 사유: <사유|없음>. 사용자 지시를 기다리세요.`
- 기준 브랜치가 바뀜: `[maestro] 병합하지 않음: 요청 뒤 기준 브랜치(<기준>)가 바뀌었어요 (<old7> → <new7>). …다시 요청하세요.`
- 충돌: `[maestro] 병합 충돌로 취소: … 충돌 파일: a, b … 프로젝트 폴더는 원래 상태로 돌렸어요. …`
- 실패: 폴더를 바꾸기 전이면 "프로젝트 폴더는 바뀌지 않았어요", 병합을 시작한 뒤 되돌리지 못했거나, 병합 커밋이 기준 브랜치가 아닌 곳이나 요청 때와 다른 기준 커밋 위에 생겼으면(마지막 확인과 `git merge` 사이에 누가 브랜치를 바꾸거나 커밋함) "합치기 도중 상태로 남았을 수 있어요. 직접 고치지 말고 사용자에게 확인을 부탁하세요".

**명령 가드 (Claude Code)**
- 에이전트가 Bash·PowerShell·Monitor 도구로 다음을 실행하려 하면 실행 전에 거부되고, 이유(어떻게 하라는 안내 포함)가 에이전트에게 가요.
  - 어디서나: `git push`(모든 형태, `--force` 포함), `send-pack`, gh: 로컬 모드 프로젝트는 R5 그대로 `gh pr create/merge`(별칭 `gh pr new` 포함)와 그 우회로(쓰는 `gh api`: `-iXPUT` 같은 짧은 플래그 묶음도 풀어서 봐요, `gh alias set|import`), gh 별칭·확장 실행(`gh co 3` 포함)과 아래 관리 명령, PR 모드 프로젝트(또는 프로젝트를 모를 때)는 읽기가 아닌 모든 gh 명령(`gh pr create/edit/merge/review/comment`, `gh api`의 쓰기 등. 허용 목록은 [PR 모드](pr-mode.md)의 가드 항목). 어느 모드든 gh의 관리 명령(`gh repo delete|edit|create|sync|fork`, `gh release create|delete`, `gh secret|variable set|delete`, `gh workflow run`, `gh run rerun|cancel`, `gh auth login|logout|switch|setup-git`, `gh config set`, `gh extension install`, `gh pr close|review`, `gh issue delete|transfer`, `gist create` 등: 관리 그룹은 보기만 돼요)은 `--help`만 붙은 게 아니면 막아요(`GH_ADMIN`). gh는 cobra가 명령을 찾는 방식대로 읽어서, 명령 단어 앞·사이에 `-R`/`--repo`·`--help` 말고 플래그가 오면 두 모드 모두 막아요(`gh pr -b view merge 12`는 실제로 `pr merge`예요). 어느 모드든 토큰을 출력하는 `gh auth token`·`gh auth status -t/--show-token`·`gh auth git-credential`·`gh config get … token`(줄 어디든 `auth`와 `token`이 같이 있으면)·`git credential fill`·`git credential-<도우미> get`(`git-credential-manager get` 포함), 명령에서 넣는 자격 증명 도우미(`-c credential.helper=…`, `credential.<주소>.helper`, `git config credential.helper …`, `GIT_CONFIG_KEY_<n>=credential.helper`: 도우미가 로그인 토큰을 받아요), `GIT_TRACE_REDACT`를 적은 줄(추적이 `Authorization` 헤더를 보여요), git의 원격 도우미(`git remote-https`·`git-remote-https.exe` 등 `remote-*`: 표준 입력의 `push`로 푸시해요, 푸시 문구), `receive-pack`·`http-backend`, `git replace`(목록 `git replace`·`-l`·`--format`만 허용: 교체 ref는 공유 저장소의 모든 git 읽기에서 기준 끝을 바꿔요)와 `refs/replace`로 받는 `fetch`, `reset --hard`, `clean -f`(`-n` 없이), `branch -D/-M/-C/-f`(`-d`+force 포함), 기준 브랜치로의 `branch -m`, `update-ref`, `filter-branch`, 쓰기 `symbolic-ref`, `worktree remove/prune/move`, `worktree add -f`, `checkout/switch -f/--discard-changes/--ignore-other-worktrees`, 트리 전체 되돌리기(`checkout .`, `restore .`. `restore --staged .`는 허용), 기준 브랜치에 쓰는 `fetch/pull` refspec(`HEAD:main`), `fetch --update-head-ok`, `git config alias.*` 쓰기, `git -c alias.*`, git이나 gh를 가리키는 셸 별칭 정의(`alias p='git push'`, `Set-Alias g git`).
  - 긴 옵션은 git처럼 앞부분만 써도 같은 옵션으로 봐요(`reset --ha` = `--hard`, `branch --del --forc` = `-D`, `commit --amen`).
  - 프로젝트 폴더·기준 브랜치 체크아웃·알 수 없는 곳에서: `merge`, `pull`, `rebase`, `cherry-pick`, `revert`, `am`(`--abort`만 허용), `commit --amend`, `reset --soft/--mixed/--merge/--keep`과 경로 없는 `reset <커밋>`, `switch`, 경로 복원이 아닌 `checkout`(`checkout -- 파일`은 허용), `restore --source`, `stash`(`list`·`show`만 허용).
  - 앱이 그 프로젝트 폴더에 병합하는 동안: 그 폴더에서 읽기 전용(`status log diff show rev-parse ls-files blame cat-file` 등, 목록만 보는 `branch`) 말고는 모든 git 명령.
- 지휘자의 일반 `git commit`(기준 브랜치에서 작은 수정)은 허용이에요. 워커의 자기 워크트리 안 `git merge main`·`git rebase main`·`git pull origin main`도 허용이에요.

## 구현

**요청 경로:** `control.rs`의 `merge-request` 경로(`r == "merge-request" || r.starts_with("merge-request/")`).
1. `merge_target(route, --to)`: `merge-request/` 뒤 단어(CLI가 `/`로 이어 붙여서 `feat/x`가 그대로 와요)가 먼저, 없으면 `--to`. 둘 다 없으면 오류.
2. `target()`으로 워크트리를 찾고, `check_merge_caller(me.is_maestro && is_root(root, me.path), is_root(root, w.path))`.
3. `worktree::plan_merge(root, base_branch(root), w.path)` → `MergePlan`. 커밋은 `log --reverse --format=%h %s base_sha..tip_sha`(최대 100, `rev-list --count`로 전체 수). R1의 `unmerged()`는 쓰지 않아요: `git cherry`는 같은 패치의 커밋을 숨기지만 `--no-ff`는 그것까지 다 가져와요. 파일은 `diff -z --name-status -M merge_base tip`과 `--numstat`(최대 500).
4. `MergeRequest { id, root, maestroPath: me.path, workerPath, workerName, verified, requestedAt, ...plan }`를 `Merges::insert`가 저장하고 `maestro://merge`를 보내요.

**저장:** `merge.rs`의 `Merges`(관리 상태, `main.rs` setup에서 `Merges::load(home)`).
- `~/.maestro/merge-requests.json`: 대기 중인 요청 목록, 맨 배열 `[...]`(형식 1이라 버전을 모르는 예전 빌드도 읽어요). 바뀔 때마다 잠금 안에서 다시 써요. 다시 읽으면 `next_id`는 가장 큰 id + 1이에요.
- 워커당 하나: 키는 `workerPath` 소문자·슬래시 통일. 새 요청이 이전 것을 바꾸고 이전 것에 `maestro://merge-done {outcome: "replaced"}`.
- `lock_root(root)`: 프로젝트 폴더마다 병합 하나(RAII `BusyGuard`, 키 `root:<폴더>`). 잡혀 있으면 "이 프로젝트는 지금 합치는 중이에요". `merging(root)`는 가드가 써요. R6에서 `lock_key`로 일반화했어요: `lock_pr(worker)`(키 `pr:<워커>`)는 워커당 PR 동작 하나, `commit_lock(worker, root)`는 앱이 워커 폴더에서 커밋하는 동안 가드가 그 폴더의 git 쓰기(와 `gh pr checkout`)를 막게 해요. 잠금은 (워커, 프로젝트) 쌍으로 기억하고, 가드는 `committing(Some(root))`로 에이전트의 프로젝트 것만 읽어요. 그래서 위치를 모르는 git 쓰기(`cd $X && git commit`)를 막는 것도 같은 프로젝트에서 커밋 중일 때뿐이고, 에이전트의 프로젝트를 모르면 `committing(None)`으로 모든 프로젝트 것을 봐요(엄하게).
- 저장은 `durable::write_json`이에요: 임시 파일에 쓰고 `sync_all` 뒤 이름 바꾸기, 이전 파일은 `.bak`, 손상된 파일은 `.corrupt-<ms>`로 옮기기, 더 새 Maestro의 파일(높은 `version`)은 덮어쓰지 않기([디스크 저장 위치](../../reference/storage-layout.md)의 저장 방식). `update(id, f)`가 요청을 고치고 저장해요(저장 실패는 경고 창으로 한 번 알려요). PR 모드의 단계 기록(`pr`)은 외부 호출 전에 디스크에 있어야 해서 `try_update(id, f)`로 써요: 저장하지 못하면 메모리의 변경도 되돌리고 Err를 돌려줘서 그 호출을 하지 않아요. 예전 요청 파일도 그대로 읽혀요(새 필드는 모두 기본값). 손상된 파일은 `.bak`가 읽히면 그것으로, 아니면 빈 목록으로 시작해요.
- `withdraw_worker(path)`: `discard_worktree`가 성공하면 불러요. `merge-done {outcome: "withdrawn"}`, 메모 없음.
- `withdraw_maestro(path)`: 폐기한 것이 마에스트로(`is_root`)면 `maestroPath`가 그 경로인 요청을 모두 같은 방식으로 빼요. 그 카드를 그릴 블록이 없고, 같은 폴더에 새 마에스트로가 생겨도 그가 한 요청이 아니라서예요. PR 모드 요청이 끝나지 않았거나(`Merges::pr_active_in`) 그 프로젝트 PR의 병합 클릭이 진행 중이면 폐기 자체를 거절해요. 폐기한 뒤에 오는 PR 메모(CI 실패, 닫힘, 병합)는 마에스트로가 없으니 버려요(`AppHost::note`).

**승인:** `answer_merge(id, approve, reason, pr)` → `merge::answer`(`spawn_blocking`). 요청의 `mode`가 `pr`이면 `pr::approve`/`pr::reject`로 가고(`pr`은 카드의 제목·본문·커밋 메시지 등 `PrOptions`), 아래는 로컬 방식이에요. 결과 `outcome`은 로컬이면 `merged | rejected | stale | conflict | failed`, PR이면 `pr | recovered | pr-updated | found | rejected`. `found`는 `다시 확인`(또는 재시작)이 브랜치의 열린 PR을 찾았지만 이 요청이 푸시할 것이 아직 거기 없거나 커밋할 변경이 남은 경우예요: PR만 기록하고 요청은 남아서(카드가 사라지지 않음, 메모 없음) 다음 승인이 커밋·푸시해 그 PR을 갱신해요. → [PR 모드](pr-mode.md)
- `lock_root` 뒤 요청이 아직 있는지 다시 봐요(다른 창에서 이미 답했으면 "이미 처리된 요청이에요").
- 거부: 요청 삭제 → `merge-done "rejected"` → 메모.
- 승인: `worktree::merge_into_base(root, req)`:
  1~2. `on_base`: 프로젝트 폴더가 기준 브랜치가 아니면 `Block::NotOnBase`. `refs/heads/<기준>`과 `HEAD`가 기록한 `base_sha`와 다르면 `Outcome::Stale` (최종: 카드가 사라지고 지휘자에게 알림). 추적 파일 변경(`status --porcelain=v1 -z --untracked-files=no`, `tracked_changes`: 실제 이름, 이름 바뀐 파일은 새 이름)이 있으면 `Block::Dirty`(추적 안 하는 파일은 막지 않아요). 이 확인은 `git merge` 바로 앞(7)에서 한 번 더 해요.
  3. `rev-parse --git-path`로 `MERGE_HEAD`, `rebase-merge`, `rebase-apply`, `CHERRY_PICK_HEAD`, `REVERT_HEAD`, `BISECT_LOG`, `index.lock`이 있으면 `Block::Busy`.
  5. `cat-file -e <tip>^{commit}` 실패면 `Outcome::Failed { touched: false }`.
  6. `merge-tree --write-tree --name-only --no-messages -z base tip` 사전 점검(충돌 파일은 실제 이름). 충돌이면 `Outcome::Conflict`(파일은 그대로).
  6-1. `in_the_way`: `diff --name-only -z --no-renames --diff-filter=A base tip`(잘리지 않은 전체 목록)의 각 경로가 프로젝트 폴더 디스크에 있거나 그 상위 폴더 자리가 파일이면 `Block::Git("합치면 … 덮어써요: …")`. 단 그 디스크 항목이 `ls-tree -r --name-only base`의 경로나 그 폴더면(Windows·macOS는 대소문자 무시) 빼요: 대소문자만 바꾼 이름 변경(`Readme.md` → `README.md`)과 추적 파일이 폴더가 되는 경우(`f` → `f/y`)는 git이 직접 바꾸는 추적 파일이라 막을 이유가 없어요. git은 무시된 파일(`.env`, 로컬 설정)을 경고 없이 덮어쓰고, `--no-ff`(ort) 경로에서는 `--no-overwrite-ignore`도 듣지 않아서 미리 막아요.
  7. `git [-c user.name=Maestro -c user.email=maestro@localhost] merge --no-ff --no-edit --no-overwrite-ignore -m "Merge branch '<브랜치>' into <기준>" -m "Maestro merge-request #<id> (워커 <이름>): 사용자 승인" <tip_sha>`. 환경 `GIT_MERGE_AUTOEDIT=no`, `GIT_EDITOR=true`, `GIT_TERMINAL_PROMPT=0`. 신원은 `config user.name/email`이 없을 때만 붙이고 병합 전에 정해요(다시 실행하지 않게). 저장소 훅은 그대로 돌아요(`--no-verify` 없음). **브랜치 이름이 아니라 기록한 tip SHA를 합쳐요.** 위 검사들이 git을 여러 번 부르는 동안(Windows에서 2~3초) 누가 커밋하거나 브랜치를 바꿀 수 있어서, 실행 바로 앞에서 `on_base`를 다시 봐요.
  8. 실패(종료 코드로만 판단, 지역화된 stderr는 읽지 않음) → `recover`: 병합이 한 일만 되돌려요.
     - `MERGE_HEAD`가 있으면 `diff --name-only -z --diff-filter=U`로 충돌 파일(실제 이름)을 모으고 `merge --abort`.
     - `MERGE_HEAD`가 없고 `HEAD == base_sha`(병합이 시작되지 않음)면 `leftover`로 남은 것을 봐요. 추적 변경이 없으면 `Block::Git(stderr)`(훅 거부, 파일 잠금: 다시 시도 가능). 변경이 있으면 `merge-tree --write-tree`로 만든 병합 트리와 비교해요: 스테이지된 것이 HEAD와도 병합 트리와도 다르거나, 파일이 인덱스와도 병합 트리와도 다르면 다른 사람의 변경(검사와 병합 사이에 사용자나 훅 없는 에이전트가 스테이지함: ort가 손대지 않고 거부한 경우)이라 건드리지 않고 `Block::Dirty`. 모든 변경이 병합 트리와 같은 스테이지된 쓰기뿐이면(체크아웃이 중간에 끊김) `reset --merge`. 그 밖(판단 불가)은 그대로 두고 `Outcome::Failed { touched: true }`.
     - 그다음 `HEAD == base_sha`, `MERGE_HEAD` 없음, 추적 변경 없음을 확인해요. 돌아왔고 충돌이 있으면 `Outcome::Conflict`, 돌아왔고 충돌이 없으면 `Block::Git(stderr)`, 못 돌아왔으면 `Outcome::Failed { touched: true }`. `reset --hard`는 쓰지 않고, 남의 스테이지 작업을 지우는 `reset --merge`는 하지 않아요.
  9. 성공: `HEAD`가 병합 커밋. 그래도 마지막 확인과 git이 `HEAD`를 읽는 사이가 남아서, `symbolic-ref --short HEAD`가 기준 브랜치가 아니거나 `HEAD^1 != base_sha`면 `Merged`가 아니라 `Outcome::Failed { touched: true }`("합치는 사이 … 다른 브랜치/다른 커밋 위에 …")로 알려요. 지휘자가 기준에 들어갔다고 오해하지 않게요. `rev-list --count tip..refs/heads/<브랜치>`가 요청 뒤 생긴 커밋 수(`later`).
- 최종 결과: 요청 삭제 → `merge-done {id, maestroPath, outcome}`(+ 병합이면 `maestro://worktrees`) → 메모. `Block`은 `Err(메시지)`라 카드가 남아요.
- 결과 `MergeResult { outcome, commit, message }`.

**메모:** `merge::merge_note(req, outcome, reason)`(순수 함수)가 글을 만들고, `control::tell(app, root, maestroPath, key, note)`가 보내요. 키는 `merge:<워커 경로 소문자>`라 같은 워커의 완료 메모(키 = 워커 이름)와 서로 지우지 않아요. 별도 스레드에서 보내요(마에스트로 입력창을 기다리므로). → [워커 완료 메모](worker-done-notes.md)

**비교 내용:** `merge_file(id, path)` → `merge::file_diff`. `path`는 그 요청의 `files[].path` 중 하나여야 해요(그래서 `..`나 절대 경로가 들어올 수 없어요). `original = show_at(root, merge_base, oldPath ‖ path)`, `modified = show_at(root, tip_sha, path)`, 없는 쪽은 `null`. 한쪽이 2MB를 넘으면 `tooLarge`, numstat가 `-`였거나 앞 8KB에 NUL이 있으면 `binary`(둘 다 내용은 보내지 않아요).

**화면:** `ApprovalCards`가 `useMerges(path)`(마에스트로 경로 대소문자·슬래시 무시)의 `MergeCard`를 도구 승인보다 먼저 그려요. 대기 표시는 `useWaiting(path)` = 도구 승인 또는 병합 요청. `Workspace.tsx`가 시작할 때 `listMerges()`로 채우고 `maestro://merge`(추가), `maestro://merge-done`(삭제)를 들어요. `addMerge`는 같은 워커의 이전 요청을 빼요. 비교 창 `MergeDiffDialog`는 lazy로 읽어서 Monaco가 열 때만 로드돼요. Monaco 설정(워커, 테마, `OPTIONS`, `languageOf`)은 `src/monaco.ts`로 옮겨 `CodeView`와 같이 써요.

**명령 가드:** `guard.rs`.
- `check(cmd, ctx, repo)`: 먼저 `worth_checking`으로 볼 만한지 정해요. 단어로서의 `git`·`gh`(`git.exe`, `/usr/bin/git`, `git-lfs`. `high`·`github`·`.git`·`digit` 안의 글자는 아님)가 원문, 따옴표·`\`·백틱을 뺀 글(`g''it`, `g\it`), 읽은 단어(`$'\x67it'`) 중 하나에 있으면 봐요. 없어도 이름을 알 수 없는 명령(변수, `& …`)이 git 동사(`push merge reset clean branch checkout switch rebase pull stash update-ref worktree cherry-pick revert am restore commit fetch config …`)나 git 같은 값을 넣는 변수(`G=gi`, `$g = 'git'`)와 같이 있으면 봐요. 그 밖의 명령은 바로 허용해요(그래서 `$PY highlight.py`처럼 git과 상관없는 명령은 읽지 못해도 막지 않아요). 볼 명령은 셸처럼 읽어요.
  - 나누기: `; && || | & 줄바꿈 ( ) { }`(bash의 `{`는 뒤가 공백일 때만. `{}`·`{a,b}`는 단어이고, 쉼표나 `..`가 든 중괄호 단어는 펼쳐지므로 알 수 없는 단어), 따옴표(`'…'`, `"…"`, `$'…'`: `\xHH`·8진수·`\u`를 풀고, 못 푸는 `\c`는 알 수 없는 단어), bash에서 따옴표 밖 `\글자`는 글자(`g\it` = `git`, `\git`. 단 `C:\p`, `..\x`처럼 경로 모양이면 백슬래시를 남겨요), `$( )`와 백틱(안의 명령은 서브셸로 따로 검사), heredoc(`<<EOF` 본문은 명령이 아니라 `<<`가 있는 명령의 데이터: Claude의 `git commit -m "$(cat <<'EOF' … EOF)"` 형태가 그대로 통과. 구분자에 따옴표가 없으면 바깥 셸이 본문의 `$( )`·백틱을 실행하므로 그것은 서브셸로 검사), here-string(`<<< 단어`도 그 명령의 데이터), 리디렉션, 단어 첫머리의 `#` 주석. PowerShell 도구는 백틱 이스케이프, 문자 그대로인 백슬래시, here-string(`@' … '@`)으로 읽어요. 따옴표나 괄호가 안 맞으면 "명령을 해석하지 못했어요"로 거부해요.
  - 경로: `\` → `/`, MSYS `/c/x` → `C:/x`, `.`·`..` 정리, 대소문자 무시 비교. `~`, `$VAR`, `%VAR%`, `$env:X`, `cd -`는 알 수 없는 곳이에요.
  - 명령마다: bash 예약어(`if then else elif do while until ! coproc {`)는 떼고 뒤를 명령으로 봐요(`if git merge x; then …`, `! git push`). `fi done esac }`만 있는 명령과 `for`·`select`·`case` 머리는 값이라 넘어가요(`for b in $(git branch)`의 치환은 따로 검사). 앞쪽 `NAME=값`(`GIT_DIR`, `GIT_WORK_TREE`, `GIT_COMMON_DIR`, `GIT_INDEX_FILE`, `GIT_CONFIG*`, `GIT_ALIAS*`면 알 수 없는 곳으로. `GIT_CONFIG_KEY_<n>`은 `-c`처럼 키로 판단해요: `alias.*`는 별칭 거부, `credential.*.helper`는 토큰 거부, 명령 키·`protocol.*.allow`·`ext::`와 값이 변수인 키는 알 수 없음. `GIT_CONFIG_PARAMETERS`는 git 내부 형식이라 늘 거부), `export GIT_DIR=…`·`$env:GIT_DIR = …`(뒤 명령 모두), 감싼 명령 풀기(`env`, `command`, `exec`(`-a 이름` 건너뜀), `nohup`, `time`, `nice`, `timeout`, `stdbuf`, `sudo`, `doas`, `winpty`, cmd의 `call`, `setsid`, `ionice`, `taskset`(CPU 마스크 건너뜀), `chronic`, `unbuffer`, `flock`(잠금 파일 건너뜀, `-c '…'`는 셸로 다시 읽기)), `xargs`(표준 입력 단어는 알 수 없는 단어: `-I`/`-i`의 자리 표시가 든 단어, 없으면 끝에 하나 더해요. 그래서 `echo push | xargs git`은 하위 명령을 알 수 없어 거부), 다시 읽기(`sh/bash/zsh -c`, `pwsh/powershell -Command`, `cmd /c`, `eval`, 표준 입력에서 명령을 읽는 셸(`-c`도 스크립트 파일도 없는 `sh`·`bash`, `-s`, `/c` 없는 `cmd`, `pwsh -Command -`, PowerShell의 `… | iex`): 그 명령의 heredoc·here-string이나, 파이프 앞 명령이 `echo`·`printf`·`Write-Output`·heredoc 있는 `cat`(PowerShell은 문자열 하나)이면 그 글을 셸로 다시 검사(`echo "git merge x" | sh`, `bash <<< "git push"`, `sh <<EOF`). `cat 파일 | sh`처럼 글을 모르면 보지 않아요, `wsl …`(배포판 셸로 다시 읽기, `-e`/`--exec`는 그대로, `--cd`는 알 수 없는 곳); 인코딩된 PowerShell 명령은 거부), 별칭 정의(bash `alias 이름=값`의 값, PowerShell `Set-Alias`·`sal`·`New-Alias`·`nal`의 인자가 git·gh를 가리키거나 변수면 거부: 다음 줄에서 다른 이름으로 부를 수 있어서), `cd`·`pushd`·`Set-Location`·`Push-Location`으로 이후 명령의 폴더 이동(서브셸·파이프 안의 `cd`는 바깥에 안 남음), `Start-Process`/`saps`/`start`(`-FilePath`나 첫 위치 인자가 프로그램, `-ArgumentList`나 다음 위치 인자가 인자. 쉼표 목록과 문자열 안 단어를 나눠요. `-WorkingDirectory`가 있으면 알 수 없는 곳), cmd의 `start`(`/옵션`과 빈 제목 건너뜀), `find -exec/-execdir/-ok/-okdir … ;|+`, `watch …`(`sh -c`로 다시 읽기), `script -c …`, PowerShell 호출 연산자 `& 명령`·`. 명령`(`& (식)`은 이름을 알 수 없는 명령). PowerShell에서 명령 단어 뒤에 오는 `( … )`는 값을 모르는 인자(`git ('push')`), `@`로 시작하는 단어(`@a` 스플래팅, `@( … )`)도 값을 모르는 단어예요. `[Diagnostics.Process]::Start(…)`·`ProcessStartInfo`가 git을 가리키는 줄은 거부해요. 프로그램 이름은 경로·`.exe`를 뗀 소문자(`"C:/Program Files/Git/bin/git.exe"` = `git`), `git-push` 꼴도 git으로 봐요. `maestro …`는 메시지라 보지 않아요. 여기 없는 프로그램은 인자를 실행할 수 있다고 봐요(fail-closed): 인자 중 git·gh를 가리키는 첫 단어부터를 명령으로 검사해요(`parallel git push ::: x`, 모르는 감싼 명령). 인자를 실행하지 않는 명령(`echo printf grep rg cat ls man which test touch rm cp mv sed awk jq code Write-Host Get-Content …`, `NOT_RUNNERS`)은 예외라 `man git-push`, `ls git-x`는 통과해요.
  - 프로그램 자리가 변수면: PowerShell에서 `&`·`.` 없이 `$`로 시작하면 명령이 아니라 값(`$?`, `$_.Trim()`, `$LASTEXITCODE -ne 0`)이라 넘어가요. 그 밖(bash의 `$X …`, `${G}t`, `& $g`)은 그 명령에 git 동사가 있거나 그 줄에서 변수에 git 같은 값을 넣었으면 "어떤 git 명령인지 알 수 없어요"로 거부하고, 아니면 넘어가요.
  - git이 대신 돌리는 명령도 셸로 다시 읽어요: 값을 명령으로 돌리는 하위 명령 옵션(`runs`: `rebase -x/--exec`, `difftool -x/--extcmd`, `fetch`·`pull`의 `--upload-pack`, `clone -u/--upload-pack`, `ls-remote`·`fetch-pack`의 `--upload-pack/--exec`, `archive --exec`. 값은 `parse`가 다른 옵션과 같은 방식으로 읽어서 `-qx V`, `-qxV`, `--exe V`, `--exe=V`도 같아요. 값이 변수면 거부), `submodule foreach …`(서브모듈 폴더는 알 수 없는 곳), `bisect run …`, 명령 값을 갖는 설정 키(`command_key`: `core.editor`, `sequence.editor`, `core.pager`, `core.sshCommand`, `core.gitProxy`, `core.alternateRefsCommand`, `diff.external`, `core.fsmonitor`, `core.askpass`, `gpg.program`, `uploadpack.packObjectsHook`, `interactive.diffFilter`, `pager.*`, `submodule.*.update`, `*.helper`(앞 `!` 뗌), `*.textconv`, `*.cmd`, `*.command`, `*.program`, `*.driver`, `*.uploadpack`, `*.receivepack`, `filter.*.clean/smudge/process`. git 전역 `-c`, `clone -c/--config`, 영구 설정 `git config 키 값` 모두. `--config-env`로 값이 환경 변수면 거부), `ext::` 전송(URL이 명령: `ext::`로 시작하는 인자나 `이름=ext::…`, `-c protocol.allow`·`protocol.*.allow`, 값이 `ext::`인 설정은 "어떤 git 명령인지 알 수 없어요"로 거부), 명령 앞·`env`·`export`·`$env:`의 `GIT_EDITOR`, `GIT_SEQUENCE_EDITOR`, `GIT_PAGER`, `GIT_SSH_COMMAND`, `GIT_EXTERNAL_DIFF`, `EDITOR`, `VISUAL`, `PAGER`(값이 변수면 `GIT_*`만 거부). `-c include.path`·`includeIf.*`·`core.hooksPath`와 명령 키는 설정을 바꾼 것으로 봐서, 별칭도 내장도 아닌 하위 명령은 거부해요.
  - git 전역 옵션: `-C`는 폴더에 더하고, `--git-dir`·`--work-tree`는 알 수 없는 곳으로, `-c alias.*`는 거부. 값을 다음 단어로 받는 옵션(`-C -c --config-env --git-dir --work-tree --namespace --super-prefix --attr-source --shallow-file`)은 값을 건너뛰어요. 내장 명령이 아니면 `git config --get alias.<이름>`으로 별칭을 풀어요(`!셸` 별칭은 셸로 다시 검사, 깊이 5, 순환 거부). 설정을 바꿨으면(`-c include.path`·`includeIf.*`·`core.hooksPath`·명령 키, `GIT_CONFIG*` 변수) 별칭이 있어도 "어떤 git 명령인지 알 수 없어요"예요: 앱의 조회는 디스크의 설정을 읽지만 git은 그 명령의 설정으로 별칭을 다시 정의할 수 있어서예요(`GIT_CONFIG_GLOBAL=x.cfg git st`). 별칭도 내장도 아니면 외부 `git-<이름>`(git-lfs 등)이라 허용이에요. 단 설정 변수를 바꿨거나, 바로 뒤 단어가 변수거나, 뒤에 오는 git 동사를 하위 명령으로 봤을 때 규칙에 걸리면 거부해요(모르는 전역 옵션의 값이 하위 명령처럼 보이는 경우: `git --새옵션 HEAD push`).
  - 하위 명령 옵션: 짧은 옵션 묶음은 펼치고(`-fdx` = `-f -d -x`), 긴 옵션은 규칙이 보는 옵션(`longs`: `--hard --soft --mixed --merge --keep --force --dry-run --delete --amend --discard-changes --update-head-ok --ignore-other-worktrees --source --staged --worktree --abort` 등)과 값을 받는 옵션(`takes`)의 앞부분이면 그 옵션으로 펼쳐요. 여러 개가 맞으면 목록 앞쪽(막는 쪽)을 골라요: git은 모호한 줄임을 거부하니 실제로 도는 명령과 어긋나지 않아요. `--no-…`는 펼치지 않아요. 엄격한 곳에서 `checkout`·경로 없는 `reset`의 인자가 변수(`$f`)면 커밋일 수 있다고 봐요.
  - 엄격한 곳(`strict`): 알 수 없는 곳, 프로젝트 정보 없음, `toplevel(폴더)`가 없거나 프로젝트 폴더와 같음, 그 폴더의 브랜치가 기준 브랜치.
  - 간접 실행 그물(`indirect`): 구조 검사를 통과해도, 읽는 줄마다(`bash -c`·별칭·`bisect run`·`-c core.editor`·`GIT_EDITOR=` 값처럼 다시 읽는 줄도) 명령들의 글(heredoc·here-string 포함)에 해석기(`python node perl ruby php lua awk sh bash pwsh powershell cmd source xargs parallel iex` 등)가 있고 같은 줄 60자 안에 `git` 뒤로 막는 명령(`push`, `update-ref`, `reset … --hard`, `branch … -D`, `clean … -f`)이 보이면 "간접 실행(python -c, | sh 등) 안의 …"로 거부해요(`git_verb_text`). `git … merge`(`--abort` 없음)는 그 줄의 시작 폴더가 엄격하거나, 줄에 `cd`류가 있거나, 앱이 합치는 중일 때 거부해요(`python -c "subprocess.run(['git','merge','feat'])"`). 글에서 빼는 명령: `maestro …`(메시지), 그리고 git·gh 호출 자체(예약어·`NAME=값` 다음 첫 단어가 git·gh. 구조 검사가 정확히 판단했고 커밋 메시지·검색어는 명령이 아니라서: `python -m pytest && git commit -m "fix push"`, `git log --grep push`, `git grep -l push | xargs sed …`는 통과). 단 git의 출력이 코드로 돌 수 있으면 남겨요: 파이프 다음 명령이 입력을 읽는 해석기(`xargs`·`awk` 말고. `git log --format='git push' | sh`)이거나, 괄호(`$( )`·서브셸) 안의 git이고 줄의 해석기 명령에 값을 모르는 단어가 있을 때(`sh -c "$(git log --format='git push')"`). 대입만 있는 명령, `echo`/`printf`, heredoc, 해석기 명령은 그대로 봐요(`X='git push'; sh -c "$X"`, `echo git push | sh`). `echo "git push"`만으로는 안 막아요.
- `check_tool(tool, input, …)`: `Bash`·`Monitor`(Bash와 같은 셸에서 명령을 돌리는 도구)·`PowerShell`의 `command`만 봐요(`shell_tool`). 다른 도구는 통과.
- `live_check(app, worktree, cwd, tool, input)`: 실행 중인 에이전트용. 프로젝트(`main_checkout`으로 찾고 등록된 저장소인지 확인), 기준 브랜치(`HEAD`면 없음), `merging`을 그때 찾고, 하나라도 실패하면 엄격으로 봐요. git 질문은 `GitRepo`(호출마다 캐시)가 `worktree::toplevel/head_branch/alias/is_rev`로 해요. 거부 이유 앞에 "Maestro가 막았어요: "를 붙여요.
- 끄는 스위치: `guard::GUARD`.

**훅 연결:** `approval.rs`.
- 터미널(PTY) Claude: `write_settings`가 쓰는 `--settings` 파일에 `PreToolUse` 훅(matcher `Bash|PowerShell|Monitor`, timeout 15초)과 `permissions.deny`(`STATIC_DENY`: `Bash(git push:*)`, `Bash(git reset --hard:*)`, `Bash(git branch -D:*)`, `Bash(git clean -f:*)`, `-fd`, `-fdx`)를 더했어요. 훅 명령: `curl -sfS --max-time 10 -X POST … "http://127.0.0.1:<port>/pretool/<세션 id>?t=<토큰>" || { echo 'maestro guard unreachable: command blocked' >&2; exit 2; }`. 서버가 4xx/5xx면 `-f`로 curl 실패, curl이 없거나 시간 초과·서버 꺼짐도 모두 `exit 2` → 그 호출은 막혀요(fail-closed).
- 서버 `/pretool/<키>`: 토큰 확인 → 키로 워크트리(없으면 훅 입력의 `cwd`) → `live_check`(패닉은 `catch_unwind`로 500 → 막힘) → 거부면 `{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":…}}`, 허용이면 빈 200.
- 채팅(ACP) Claude: `agent.rs`가 키 `acp-<path_id>`를 등록하고 `Approvals::acp_settings(key)`(`PreToolUse` 훅과 deny 목록만. `PermissionRequest`·`Stop`·`SessionStart`는 없음)를 `AcpCtx.settings`로 넘겨요(카탈로그 `approval_hook`이 켜진 에이전트만). `session_meta`가 이것을 `_meta.claudeCode.options.settings`에 넣고, 어댑터(0.102.0)가 SDK의 `settings` 옵션(= CLI `--settings` 층)으로 넘겨서 `bypassPermissions`·자동 허용 Bash에서도 훅이 돌아요. `fork_session`은 도구를 돌리지 않아 넘기지 않아요.
- ACP 권한 질문 앞단: `agent.rs`가 `Ask`를 감싸 `live_check`가 거부하면 카드 없이 `Deny`(→ `reject_once`)를 돌려줘요. 훅이 꺼져 있어도 묻는 호출은 막혀요. 묻지 않고 자동 허용되는 호출은 훅이 막아요.
- R4의 ACP → PTY 폴백도 PTY 설정에 같은 훅이 있어서 보호가 이어져요.

**`/api` 토큰:** `Approvals::agent_token(path)`(실행마다 새 키, 경로 소문자·슬래시 통일)을 `MAESTRO_TOKEN`으로 넣고, `/api`는 `x-maestro-token`이 `x-maestro-worktree`의 토큰과 같을 때만 받아요. 워커가 `MAESTRO_WORKTREE`를 프로젝트 폴더로 바꿔도 403이라 마에스트로인 척 `merge-request`를 부를 수 없어요. → [로컬 서버](../../architecture/local-server.md)

**테스트**
- `guard.rs`: `decides`(워커·프로젝트 폴더·기준 브랜치 워크트리에서 허용/거부 표: 체인, `git -C`, 따옴표, 감싼 명령, 별칭, 간접 실행, heredoc 커밋 메시지, 해석기와 같은 줄의 `git commit -m "… push …"`·`git log --grep push`·`git grep -l push | xargs sed`는 허용), `sees_through_spellings_and_git_run_commands`(`g''it`, `$'\x67it'`, `${G}t`, `{git,push}`, `rebase -x`와 `-qx V`·`-fx V`·`-qxV`·`--exe V`·`--exe=V`, `difftool -x/--extcmd`, `--upload-pack`(fetch·pull·clone·ls-remote·fetch-pack), `archive --exec`, `-c uploadPack.packObjectsHook`·`remote.*.uploadpack`·`diff.*.command`, `clone -c`, `git config core.pager …`, `ext::`·`protocol.*.allow`, `submodule foreach`, `bisect run python -c …`, `-c core.editor`, `GIT_EDITOR=`(안의 `python -c`도), `git log --format=… | sh`, `sh -c "$(git …)"`, `find -exec`, `watch`, `script -c`), `leaves_commands_without_git_alone`(`$PY highlight.py`, `"$JAVA_HOME/bin/java" … github …`, `Where-Object { $_.Name -like '*light*' }`), `powershell_idioms`(`if ($?)`·`ForEach-Object { $_.Trim() }`는 허용, `& ('g'+'it') push`·`. git push`·`Push-Location`·`Start-Process`·`$env:GIT_EDITOR`·`git ('push')`·`git @a`·`Set-Alias g git`·`[Diagnostics.Process]::Start('git','push')`는 거부, `Write-Output (git rev-parse HEAD)`·`@(git ls-files)`는 허용), `unknown_context_is_strict`, `hidden_subcommands_and_option_prefixes`(`--attr-source HEAD push`, `--shallow-file`, 모르는 전역 옵션, `| xargs git`, `xargs -I{} git {}`, `exec -a`, `wsl`, 다음 줄에서 쓰는 `alias`, `reset --ha`·`clean --forc`·`branch --del --forc`·`commit --amen` 같은 줄임은 거부, `--attr-source=HEAD log`·`xargs git log`·`git lfs pull`·`merge --abo`는 허용), `merging_blocks_writes_in_the_folder`, `gh_is_read_only_for_agents`(PR 모드: 쓰기·토큰 출력 거부, 읽기·`pr checkout`(프로젝트 폴더 밖)·`--help` 허용), `gh_reads_the_command_as_cobra_finds_it`(두 모드: 명령 단어 앞·사이의 플래그, `api` 앞의 gh api 플래그, `alias --shell p set`, 줄 어디든의 `auth … token`은 거부, `-R`/`--repo`·`--help`·`help pr create`는 허용), `gh_admin_commands_are_refused_in_both_modes`(두 모드·프로젝트 폴더: 저장소·릴리스·시크릿·변수·워크플로·CI·로그인·설정·확장·라벨 관리 명령과 `pr close|review`·`issue delete|transfer|develop`·`gist create`·`copilot`은 `GH_ADMIN`, `--help`만 붙으면 허용; 로컬 모드의 읽기와 보통 쓰기는 허용, `gh co 3`·`gh dash` 같은 별칭·확장은 `PR_LOCAL`), `credentials_traces_and_remote_helpers_are_refused`(`credential.helper` 주입·`GIT_TRACE_REDACT`는 `TOKEN`, `git remote-https`·`git-remote-https.exe`는 푸시 문구, `receive-pack`, 바꾼 설정 아래의 별칭, `git replace`), `gh_in_local_mode_blocks_pr_create_merge_and_ways_around`(로컬 모드: `pr create/merge`, 그 우회로(쓰는 `gh api`, GraphQL mutation·파일에서 읽는 쿼리, 별칭 정의, 변수로 쓴 명령), 토큰 출력만 거부. `pr checkout`·`issue create`·`pr comment`·`release list`·`search`, mutation이 아닌 `gh api graphql` 쿼리, `-X GET`과 같이 쓴 필드는 허용. PR 모드는 GraphQL 쿼리를 계속 막음), `reserved_words_stdin_shells_and_unknown_wrappers`(`if git merge feat; then …`·`for … do git branch -D`·`! git push`·`bash <<< "git push"`·`echo "git merge feat" | sh`·따옴표 없는 heredoc의 `$(git push)`·`python -c` 안의 `reset --hard`/`merge`·`setsid`/`flock`/`cmd /c call`/`parallel`/모르는 감싼 명령은 거부, 워커 폴더의 `if git merge main`·`man git-push`·`brew install gh`·`cat script.sh | sh`는 허용), `lexes_quotes_substitutions_and_heredocs`(heredoc 본문이 `<<`가 있는 명령에 붙음, here-string은 데이터), `normalizes_paths`, `follows_aliases`, `checks_shell_tools_only`(`Monitor`의 `command`도 봄).
- `worktree.rs`(임시 저장소): `plan_merge_lists_commits_and_files`, `plan_refuses`, `worker_untracked_only_warns`, `merge_no_ff_two_parents`, `merge_merges_the_recorded_tip`, `moved_base_is_stale`, `project_folder_blocks`(추적 변경·진행 중 작업·다른 브랜치는 막고, 추적 안 하는 파일은 안 막음), `conflict_aborts_clean`, `files_in_the_way_block`(무시된 `.env`, 추적 안 하는 파일, 폴더 자리의 파일), `tracked_files_are_not_in_the_way`(대소문자만 바꾼 이름 변경, 추적 파일이 폴더가 됨: 막지 않고 합쳐짐), `failed_merge_leaves_someone_elses_staged_work`(남의 스테이지 변경은 `Block::Dirty`로 그대로, 병합 자신의 스테이지 쓰기만 되돌림), `precheck_reports_conflict`, `identity_fallback`, `merged_worker_discards_clean`, `parses_status_z_entries`(`-z` 항목: 이름 바꾸기의 옛 경로, ` -> `·따옴표·한글이 든 이름), `status_keeps_korean_and_renamed_names`(`core.quotePath=true`에서도 한글·이름 바뀐 파일이 그대로, HEAD 내용도 같은 이름으로 읽힘).
- `merge.rs`: `notes_for_each_outcome`, `verified_text_is_cleaned_and_capped`, `pending_requests_persist_and_replace_per_worker`(마에스트로 폐기 때의 `remove_maestro` 포함). `control.rs`: `merge_target_from_route_or_flag`, `check_merge_caller_only_maestro`, `merge_notes_keep_worker_done`. `approval.rs`: `settings_add_signal_hooks`, `acp_settings_only_guard`, `pretool_reply_matches_schema`, `agent_tokens_differ_per_worktree_ignore_case_and_slashes`. `acp_runtime.rs`: `session_meta_appends_the_brief`, 실제 어댑터로 도는 `#[ignore] acp_live_pretool_blocks_push`.

## 제약과 알려진 문제

- Antigravity(훅 없는 에이전트, 카탈로그 `approval_hook: false`)는 브리프 지시로만 막혀요.
- 로컬 방식은 squash를 고를 수 없어요(D6). squash는 PR 모드의 PR 병합에서만 돼요. → [PR 모드](pr-mode.md)
- 가드는 샌드박스가 아니에요. 스크립트 파일, Makefile, npm 스크립트, 외부 `git-foo` 명령, MCP git 도구, `.git/config`를 직접 고치는 파일 편집은 보지 않아요. PowerShell 문법은 대략만 읽어요.
- 아직 보지 않는 git 경로: 저장소 훅 스크립트 안(`.git/hooks`나 `core.hooksPath`의 훅이 `git commit` 때 push), `git rebase -i` 할 일 목록에 편집기로 넣은 `exec` 줄, `.gitattributes`가 부르는 filter·diff 드라이버, `git config include.path …`로 영구히 들인 설정의 별칭(명령줄 `-c include.path`는 막음), 프로그램 경로 변수(`GIT_ASKPASS`, `GIT_SSH`), 인자를 문자열 하나로 받는 실행기(`nodemon --exec "git push"` 등. `parallel git push`처럼 단어로 오면 봐요), 내용을 모르는 표준 입력(`cat 파일 | sh`, `curl … | sh`), 파일을 거쳐 도는 git 출력(`git log --format='git push' > x.sh; sh x.sh`: 간접 그물은 git 호출의 글을 파이프·`$( )`로 해석기에 갈 때만 봐요). 변수 값도 따라가지 않아요(`$X`가 git이어도 git 동사가 같이 없으면 통과).
- 사용자 설정의 `disableAllHooks`·`allowManagedHooksOnly`는 훅을 꺼요. 그래도 `permissions.deny`와 ACP 질문 앞단 검사는 남아요.
- 잘못 막는 경우가 있어요: 프로젝트 폴더에서의 `git checkout <브랜치>`, 프로젝트 폴더에서 인자가 변수인 `git checkout $f`·`git reset $x`, 프로젝트 폴더에서의 `git lfs pull`(뒤의 `pull`을 하위 명령으로 봄), git 옵션의 모호한 줄임(git은 거부하는데 가드는 막는 쪽으로 펼침), heredoc이 아닌 여러 줄 글 안의 `git push`, 해석기 단어와 `git … push`가 같이 있는 명령(`git log --grep=push && python x.py`), 프로젝트 폴더에서 해석기와 `git … merge` 글이 같이 있는 명령, 모르는 프로그램의 인자로 온 git 명령(프로젝트 폴더에서 `node x.js git merge feat`), 이름이 변수인 명령에 git 동사 단어가 있는 경우(`$DOCKER push img`). 거부 이유가 다른 방법을 알려 줘요.
- 저장소 훅(`pre-merge-commit` 등)은 앱 병합에서도 돌아요. 훅이 거부하면 카드에 그 메시지가 보이고 다시 시도할 수 있어요.
- 마지막 확인과 `git merge` 사이에 아주 짧은 틈이 남아요(사용자, IDE, 훅 없는 에이전트). 훅이 있는 에이전트는 병합 중 그 폴더의 git 쓰기가 막혀요. 그 틈에 기준이 움직였거나 브랜치가 바뀌었으면 병합 커밋은 생겼지만 승인한 병합이 아니라서 `Failed { touched: true }`로 사용자 확인을 부탁해요(자동으로 되돌리지 않아요). `merge-tree` + `commit-tree` + `update-ref <기준> <새> <base_sha>`(비교 후 교체)로 완전히 막는 방법도 있지만, 저장소 병합 훅과 `git merge`의 충돌 처리를 그대로 두려고 쓰지 않았어요.
- 실행 중이던 에이전트는 다시 시작해야 새 브리프와 설정(훅)을 받아요.
- 시드 플레이북 `git-rules.md`는 아직 워커가 "요청받으면" 합치거나 푸시할 수 있다고 적혀 있어요. 브리프(R5)가 이를 덮어써요. 플레이북 전체 개정은 규율 단계에서 해요.
- ACP 훅 전달은 어댑터 0.102.0과 Claude Code에서 `acp_live_pretool_blocks_push`로 확인했어요(2026-11-11: `bypassPermissions`에서 `git push origin HEAD`가 `/pretool`에 닿아 거부되고, 원격에 아무것도 안 올라감). 어댑터가 바뀌면 이 테스트를 다시 돌려야 해요(`MAESTRO_ACP_CLAUDE_DIR` 필요).
- 앱 화면(카드, 비교 창)은 타입 검사만 했고, 실제 화면은 사용자가 확인해요.

## 결정 기록

- 합치기·푸시를 프롬프트가 아니라 코드로 막아요(D4). 프롬프트만으로 막은 도구에서 강제 푸시·무단 푸시 사례가 있었어요(ade-discipline-comparison 보고서).
- `--no-ff`: Nimbalyst·Orca와 같고, 병합 커밋이 남아 "합쳐졌나" 판정(R1의 `check_discard`)이 정확해요. 합친 뒤 워커 브랜치는 기준의 조상이라 워크트리가 깨끗하게 폐기되고 `branch -d`도 돼요.
- 브랜치 이름이 아니라 기록한 tip SHA를 합쳐요. 사용자가 본 것과 합쳐지는 것이 같아야 해서예요.
- 추적 안 하는 파일은 막지 않아요. 단 병합이 더할 경로에 이미 있는 파일(무시된 파일 포함)은 미리 `Block::Git`으로 막아요(다시 시도 가능). git은 무시된 파일을 버려도 되는 것으로 보고 덮어쓰는데, 프로젝트 폴더는 사용자의 작업 폴더라 `.env`·로컬 설정을 잃으면 안 돼서예요. `merge-tree` + `commit-tree` + `merge --ff-only --no-overwrite-ignore`로 바꾸는 방법도 있었지만, 저장소 병합 훅(`pre-merge-commit`)과 병합 커밋 흐름을 그대로 두려고 사전 검사를 골랐어요.
- 실패 뒤 되돌리기는 병합 트리와 비교해 병합 자신의 쓰기일 때만 해요. 경로 이름만 보면, ort가 "로컬 변경이 덮여요"로 거부한 경우(사용자가 병합이 바꿀 파일을 스테이지)도 병합 경로 안이라 `reset --merge`가 사용자의 스테이지 작업을 지워요.
- `merge-tree` 사전 점검: 파일을 건드리지 않고 충돌을 미리 알아서 프로젝트 폴더가 충돌 상태로 남는 일을 줄여요.
- 기준 브랜치가 바뀌면(Stale) 최종 거부, 폴더가 지저분하면(Dirty) 다시 시도: 앞의 것은 사용자가 본 diff가 더는 맞지 않고, 뒤의 것은 정리만 하면 같은 diff로 합칠 수 있어서예요.
- 훅은 fail-closed: 가드에 닿지 못하면 Bash 전체가 막혀요(알림 메시지로 이유를 알려 줘요). 막히는 쪽이 몰래 통과하는 쪽보다 낫다고 봤어요.
- `/api`를 에이전트별 토큰으로: 예전에는 전역 토큰과 헤더의 워크트리만 봐서, 워커가 `MAESTRO_WORKTREE`를 바꾸면 마에스트로로 행세할 수 있었어요.
- D6: PR 모드(푸시·PR·CI 표시)는 선택 기능이고 squash는 PR 모드에서만 골라요. 로컬이 기본이에요.
- D7(2026-11-11 사용자 확인): PR 모드는 Orca 방식으로 앱이 커밋부터 PR 생성·머지까지 git·gh로 해요. 만들기 직전에 다시 확인하고, 결과가 애매하면 기존 PR을 찾아 복구해요. 같은 `merge-request`와 같은 카드를 쓰고, 요청에 방식을 기록해 승인이 그 방식을 따라요. → [PR 모드](pr-mode.md)
- 2026-11-13 (UI 구조 재구성 단계 5): 병합 요청을 상단 막대의 확인할 일 팝오버에도 모았어요. 마에스트로 화면을 거치지 않고 변경을 볼 수 있어요. 팝오버의 버튼은 변경 보기와 열기뿐이에요. 병합을 diff를 보지 않고 누르게 하지 않으려는 것이에요.
- 병합 요청은 도구 승인(`Approvals::request`)과 따로 둬요. 런타임 스트림에 넣지 않고, 전환(`SwitchResult`)의 "바쁨"에도 세지 않아요. 지휘자는 요청 뒤에도 다른 일을 할 수 있어요.
