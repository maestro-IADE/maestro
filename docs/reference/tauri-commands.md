---
title: Tauri 명령 목록
area: reference
status: done
files: [src-tauri/src/main.rs, src/ipc.ts, src-tauri/src/worktree.rs, src-tauri/src/agent.rs, src-tauri/src/approval.rs, src-tauri/src/catalog.rs, src-tauri/src/cli.rs, src-tauri/src/commands.rs, src-tauri/src/library.rs, src-tauri/src/antigravity.rs, src-tauri/src/runtime.rs]
related: [architecture/overview.md, reference/events.md, reference/storage-layout.md, architecture/runtime-events.md]
updated: 2026-11-11
---

# Tauri 명령 목록

## 요약

`src-tauri/src/main.rs`의 `generate_handler!`에 등록된 `#[tauri::command]` 42개 전체 목록이에요. 프런트엔드는 모두 `src/ipc.ts`의 `ipc.*` 래퍼를 거쳐서 `invoke`로 불러요. 인자 이름은 Tauri v2 규칙대로 Rust의 snake_case가 JS에서 camelCase로 바뀌어요(`project_root` → `projectRoot`). 아래 표의 인자는 JS 쪽 이름이에요.

실패는 모두 `Result<_, String>`의 `Err(문자열)`로 돌아오고, JS에서는 `invoke`가 그 문자열로 reject돼요. 아래 "반환"은 성공 값만 적었어요.

## 저장소·워크트리

| Rust 명령 | `ipc.*` | 인자 | 반환 | 용도 |
|---|---|---|---|---|
| `list_repos` | `listRepos` | 없음 | `RepoInfo[]` (`{ root, name, defaultBranch }`) | `~/.maestro/repos.json`에 등록된 저장소 목록 |
| `add_repo` | `addRepo` | `path` | `RepoInfo` | `git rev-parse --show-toplevel`로 저장소 루트를 찾아 `repos.json`에 추가(이미 있으면 그대로) |
| `repo_state` | `repoState` | `path` | `"ready" \| "empty" \| "none"` | 고른 폴더가 그대로 프로젝트가 될 수 있는지: 커밋 있음 / 저장소지만 커밋 없음 / 저장소 아님 |
| `init_repo` | `initRepo` | `path` | `()` | 저장소가 아니면 `git init -b main`, 커밋이 없으면 있는 파일로 첫 커밋(`Initial commit`, git 사용자 정보가 없으면 Maestro 이름으로). 사이드바가 물어본 뒤에만 불러요 |
| `remove_repo` | `removeRepo` | `root` | 없음 | `repos.json`에서만 빼요. 디스크의 워크트리는 건드리지 않아요 |
| `list_worktrees` | `listWorktrees` | `projectRoot` | `WorktreeInfo[]` | 마에스트로(있으면 맨 앞) + 저장소의 모든 연결 워크트리(외부 포함) |
| `list_branches` | `listBranches` | `projectRoot` | `Branch[]` (`{ name, remote, inUse }`) | 로컬 브랜치, 그다음 원격에만 있는 브랜치(짧은 이름) |
| `create_agent` | `createAgent` | `req: { projectRoot, adapterId, permissionMode, branchName, baseBranch, existingBranch, name, description, model }` | `WorktreeInfo` | 새 워크트리 + 에이전트 meta를 만들고 바로 실행. 시작 실패 시 워크트리와 meta를 지워요 |
| `create_maestro` | `createMaestro` | `req: { projectRoot, adapterId, permissionMode, model, baseBranch }` | `WorktreeInfo` | 프로젝트 폴더 자체에서 도는 마에스트로(role `maestro`)를 만들고 실행. 이미 있으면 에러 |
| `attach_agent` | `attachAgent` | `req: { projectRoot, worktreePath, adapterId, permissionMode, model }` | 없음 | 에이전트가 없는 워크트리(Maestro 밖에서 만든 것)에 에이전트를 붙이고 실행 |
| `check_discard` | `checkDiscard` | `projectRoot`, `worktreePath`, `branch` | `DiscardCheck` (`{ base, changes, commits, keepBranch }`) | 폐기하면 잃는 작업: 커밋하지 않은 파일(미추적 포함)과 기준 브랜치에 없는 커밋(`git cherry`·squash로 같은 내용이면 병합으로 봐요). 에이전트는 끄지 않아요. 마에스트로·외부 워크트리는 빈 결과 |
| `discard_worktree` | `discardWorktree` | `projectRoot`, `worktreePath`, `branch`, `choice` (`"clean"` / `"keep"` / `"discard"`) | `string \| null` (남긴 브랜치나 백업 ref) | 에이전트 종료 → `worktree::discard`: `clean`은 잃는 게 있으면 거절, 아니면 `git worktree remove`(force 없음) + `git branch -d`. `keep`은 커밋하지 않은 파일을 브랜치에 커밋하고 폴더만 지움. `discard`는 `refs/maestro/trash/<브랜치>` 백업 후 폴더와 브랜치 삭제(`keepBranch`면 브랜치 유지). 그다음 다른 에이전트의 `workers`에서 빼고 meta와 옆 파일 삭제. 마에스트로·외부 워크트리는 meta만 지워요 → [워크트리 폐기](../features/worktrees/discard-worktree.md) |
| `link_worktrees` | `linkWorktrees` | `projectRoot`, `parent`, `child` | 없음 | `child`를 `parent`의 워커로. 트리라서 이전 부모에서 빠지고, 고리가 생기거나 마에스트로를 자식으로 두면 거절 |
| `unlink_worktrees` | `unlinkWorktrees` | `projectRoot`, `parent`, `child` | 없음 | `parent` meta의 `workers`에서 `child`를 빼요 |

`WorktreeInfo` 필드(camelCase): `path`, `branch`, `adapterId?`, `permissionMode?`, `name?`, `description?`, `state` (`{ kind }`), `isMaestro`, `workers`, `refs`, `external`, `personas`(제목), `baseBranch?`, `docs`(제목), `runtime`(`"acp"` / `"pty"`, 에이전트가 꺼져 있으면 `null`), `runtimeNote`(`FallbackReason` = `{ kind, …필드, message }` 또는 `null`). 화면은 스토어의 `Worktree.runtime`/`runtimeNote`로 받아 헤더 이름표 툴팁에 써요.

## 에이전트 실행

| Rust 명령 | `ipc.*` | 인자 | 반환 | 용도 |
|---|---|---|---|---|
| `send_message` | `send` | `projectRoot`, `worktreePath`, `text`, `filesAttached` (기본 `[]`) | 없음 | 채팅 입력을 에이전트 터미널에 붙여 넣어요. 필요하면 먼저 시작/이어서 실행(`ensure`). 신뢰 확인 창이 떠 있거나 시작에 실패하면(로그인 필요 등) 그 이유로 에러 |
| `ensure_agent` | `ensureAgent` | `projectRoot`, `worktreePath` | 없음 | CLI 뷰가 붙을 터미널이 있게 에이전트를 시작/재개 |
| `agent_ready` | `agentReady` | `projectRoot`, `worktreePath` | `{ trustPrompt: boolean, failure: { kind: "loginRequired" \| "exited" \| "timeout", message } \| null, runtime: "acp" \| "pty", runtimeNote: FallbackReason \| null }` | TUI가 뜰 때까지 기다려요(최대 45초). 폴더 신뢰 확인 창이면 `trustPrompt: true`. 못 뜨면 `failure`에 이유. 실행 파일이 없으면 명령 자체가 에러예요 → [에이전트 실행 방식](../architecture/agent-runtime.md) |
| `accept_trust` | `acceptTrust` | `worktreePath` | 없음 | 실행 중인 에이전트의 신뢰 확인 창을 수락. 그 뒤 입력창이 뜨면 보관해 둔 워커 완료 메모를 붙여 넣어요 |
| `stop_agent` | `stop` | `worktreePath` | 없음 | 현재 턴만 중단(TUI에 Esc). 에이전트 프로세스는 계속 돌아요 |
| `get_transcript` | `transcript` | `projectRoot`, `worktreePath` | `{ epoch, seq, events: RuntimeEvent[] }` | 대화를 공통 이벤트로(`runtime::snapshot`): 실행 중인 런타임이 자기 기록을 가지면(ACP, R4) 그것, 아니면 에이전트 CLI의 대화 기록 파일을 처음부터 읽어 변환. 두 번의 epoch 읽기 사이에 읽고, 다르면 한 번 더 읽어요. 에이전트가 실행 중이 아니어도 돼요 |
| `list_slash_commands` | `slashCommands` | `worktreePath`, `adapterId` | `{ name, description, source, note? }[]` | 그 에이전트의 `/` 메뉴: 카탈로그의 기본 명령, 그다음 그 에이전트가 읽는 사용자 명령·스킬(Claude: `.claude/commands`, `.claude/skills`; 워크트리와 홈). Antigravity는 빈 목록. ACP로 도는 중이면 어댑터가 안 받는 것에 "채팅에서는 안 돼요 · CLI 보기에서" 메모, 어댑터만 아는 것은 끝에 `source: "에이전트"`로 (`commands::with_runtime`) |
| `switch_runtime` | `switchRuntime` | `projectRoot`, `worktreePath`, `view` (`"chat"` / `"cli"`), `when` (`"ifIdle"` / `"afterTurn"` / `"interrupt"`) | `SwitchResult { status: "done" \| "busy" \| "stuck" \| "superseded", runtime, note, busy: "turn" \| "approval" \| "shell" \| null }` | 채팅↔CLI 전환(`Agent::switch`). 실행 중이 아니면(기다리는 동안 꺼진 경우도) 그 보기의 런타임으로 시작해요. `afterTurn`은 턴이 끝날 때까지(최대 30분) 응답을 미뤄요 → [채팅 런타임](../features/agents/chat-runtime.md) |
| `start_view` | `startView` | `projectRoot`, `worktreePath`, `view` (`"chat"` / `"cli"`) | `SwitchResult`(늘 `done`, 지금 도는 런타임) | 보기가 뜰 때(채팅의 `useAgentBoot`, CLI의 XTerm `start`): 꺼져 있으면 그 보기의 런타임으로 시작하고, 돌고 있으면 그대로 둬요(마운트로 대화를 가져가지 않아요) |
| `fork_session` | `forkSession` | `projectRoot`, `worktreePath`, `view` | `SwitchResult` | `stuck` 뒤 사용자가 동의했을 때만: 대화를 `session/fork`로 복사해 그 보기로 열어요. 복사하는 사이 옛 프로세스가 끝나 에이전트가 꺼졌으면 사본 위에 그 보기의 런타임으로 한 번만 시작해요 |

## 터미널(PTY)

| Rust 명령 | `ipc.*` | 인자 | 반환 | 용도 |
|---|---|---|---|---|
| `pty_snapshot` | `ptySnapshot` | `worktreePath` | `{ dataB64, seq, epoch, terminal }` | 스크롤백(최대 1MB)을 base64로, 그 PTY의 epoch와 함께. 에이전트가 없으면 빈 값과 지금 epoch(`stream::pty_epoch`), `terminal: true`. ACP 런타임이 대화를 쥐고 있으면 `terminal: false` |
| `pty_write` | `ptyWrite` | `worktreePath`, `data` | 없음 | 키 입력을 PTY에 써요. 에이전트가 없으면 아무것도 안 해요. ACP 런타임이 대화를 쥐고 있으면 에러예요(XTerm은 무시하고 "CLI로 전환" 띠를 보여요) |
| `pty_resize` | `ptyResize` | `worktreePath`, `cols`, `rows` | 없음 | PTY 크기 변경 |

`worktreePath` 자리에 `setup:<id>` 키를 넣으면 설정 창의 설치 터미널에도 똑같이 동작해요.

## 승인

| Rust 명령 | `ipc.*` | 인자 | 반환 | 용도 |
|---|---|---|---|---|
| `list_approvals` | `listApprovals` | 없음 | `Approval[]` | 지금 대기 중인 권한 요청(이벤트를 놓친 블록용) |
| `answer_approval` | `answerApproval` | `id`, `choice` (`"allow"` / `"always"` / `"deny"` / `"cli"`) | 없음 | 권한 요청에 답해요. `cli`는 터미널 프롬프트로 되돌려요 |
| `list_merges` | `listMerges` | 없음 | `MergeRequest[]` | 대기 중인 병합 요청(이벤트를 놓친 블록, 앱 재시작 뒤). → [병합 요청](../features/orchestration/merge-request.md) |
| `answer_merge` | `answerMerge` | `id`, `approve`, `reason` (거부 사유, 선택) | `MergeResult { outcome: "merged" \| "rejected" \| "stale" \| "conflict" \| "failed", commit, message }` | 승인이면 앱이 프로젝트 폴더에서 `git merge --no-ff <기록한 tip>`. 오류(문자열)는 지금 합칠 수 없다는 뜻이고(다른 브랜치, 커밋하지 않은 변경, 다른 git 작업 중, git 거부, 이미 처리됨) 카드가 남아요. `spawn_blocking` |
| `merge_file` | `mergeFile` | `id`, `path` (그 요청의 파일 중 하나) | `MergeFileDiff { original, modified, binary, tooLarge }` | 병합 요청의 한 파일을 기록한 merge-base와 tip에서 읽어요. 한쪽이 2MB를 넘거나 바이너리면 내용 없이 표시만 |

## 요약·모델

| Rust 명령 | `ipc.*` | 인자 | 반환 | 용도 |
|---|---|---|---|---|
| `summarize_agent` | `summarize` | `projectRoot`, `worktreePath`, `assistantAgent`, `assistantModel` | `string \| null` | 최근 답변의 짧은 요약(프로젝트 캔버스 카드용). meta의 `summary`/`summaryKey`에 캐시. 한 번에 하나씩만 실행 |
| `agent_models` | `agentModels` | `id` | `{ id, label, description }[]` | 에이전트가 고를 수 있는 모델(카탈로그 `models`): Claude 별칭 목록, Antigravity는 `agy models`. 없으면 빈 목록 |

## CLI 설치

| Rust 명령 | `ipc.*` | 인자 | 반환 | 용도 |
|---|---|---|---|---|
| `agent_catalog` | `agentCatalog` | 없음 | `AgentDef[]` | 에이전트 카탈로그(`catalog.rs`): id, 이름, 로고 파일, 실행 파일, 설치, 런타임, ACP 어댑터, 최소 버전, 기능 플래그. 탐지는 안 해요. 프런트가 첫 렌더 전에 한 번 받아요 |
| `cli_status` | `cliStatus` | `recheck?`(기본 false) | `{ agents: { id, status, installed, path, version, minVersion, loggedIn, installCmd, message, acp: AcpStatus \| null }[], npm, node: string \| null }` | 카탈로그 에이전트마다 실행 파일 경로·버전·로그인을 확인한 상태(`missing` / `tooOld` / `loginRequired` / `ready`)와 npm 유무, Node 버전. `acp`(ACP 에이전트만) = `{ state: "ready" \| "notInstalled" \| "broken" \| "nodeMissing" \| "nodeTooOld" \| "cliUnsupported" \| "loggedOut" \| "blocked", message, node, nodeMin, adapterVersion, pinned, dev, mode: "auto" \| "pty" }`. 채팅 런타임 확인 캐시를 먼저 비워요. `recheck: true`("다시 확인")일 때만 이번 실행의 막힘(`blocked`)도 풀어요. 네트워크 없음 → [CLI 탐지](../features/agents/cli-install.md) |
| `acp_install` | `acpInstall` | `id` | 없음 | 채팅 런타임 어댑터 설치를 시작해요(별도 스레드, 에이전트마다 하나). 진행은 `acp://install` |
| `acp_install_cancel` | `acpInstallCancel` | `id` | 없음 | 진행 중인 설치의 npm을 끝내요 |
| `set_chat_runtime` | `setChatRuntime` | `id`, `mode` (`"auto"` / `"pty"`) | 없음 | `~/.maestro/runtimes.json`에 저장. 실행 중인 에이전트는 다음 시작부터 |
| `launch_preview` | `launchPreview` | `id`, `permissionMode`, `model` | `string` | 설정 타일에 보여 줄 실행 명령(세션·훅·브리프 인자는 빼고) |
| `setup_start` | `setupStart` | `id` | `string` (`setup:<id>`) | 설치(없거나 버전이 낮으면) 후 로그인용으로 CLI를 띄우는 설정 창 터미널. 이미 살아 있으면 그 키를 돌려줘요 |
| `setup_stop` | `setupStop` | `id` | 없음 | 설치 터미널 종료 |

## 캔버스 상태

| Rust 명령 | `ipc.*` | 인자 | 반환 | 용도 |
|---|---|---|---|---|
| `load_canvas_state` | `loadCanvas` | `projectRoot`, `key` | `string \| null` | `canvases/<key>.json` 원문. 없으면 `null` |
| `save_canvas_state` | `saveCanvas` | `projectRoot`, `key`, `stateJson` | 없음 | 캔버스 JSON을 원자적으로(.tmp 후 rename) 저장 |

`key`는 마에스트로 캔버스면 `overview`, 워크트리 캔버스면 워크트리 폴더 이름이에요(`Canvas.tsx`). 영숫자·`-`·`_` 밖의 문자는 `_`로 바뀌어요.

## 문서·참조

| Rust 명령 | `ipc.*` | 인자 | 반환 | 용도 |
|---|---|---|---|---|
| `library_list` | `libraryList` | 없음 | `{ folder, persona, files: { path, title }[] }[]` | 페르소나 폴더(맨 앞, `persona: true`) + 문서 분류 폴더 |
| `playbook_list` | `playbookList` | 없음 | `{ folder, persona, files }[]` (폴더 하나) | 플레이북 목록 |
| `docs_create` | `docsCreate` | `folder`, `name` | `string` (파일 경로) | 페르소나·분류 폴더에 `<name>.md` 새로 만들기 |
| `docs_import` | `docsImport` | `folder`, `files` | 없음 | 바깥 마크다운을 그 폴더로 복사(겹치면 `-2`…) |
| `docs_delete` | `docsDelete` | `path` | 없음 | 페르소나·분류 폴더의 문서 하나 지우기 |
| `docs_category` | `docsCategory` | `name` | 없음 | 새 분류 폴더 |
| `persona_create` | `personaCreate` | `name` | `string` (파일 경로) | `~/.maestro/personas/<name>.md` 새로 만들기. 같은 이름이 있으면 에러 |
| `md_title` | `mdTitle` | `path` | `string` | Markdown 파일의 맨 위 `#` 제목(파일 트리에서 끌어 놓을 때) |
| `set_ref` | `setRef` | `projectRoot`, `worktree`, `md`, `on` | 없음 | 에이전트 meta의 `refs`에 Markdown 참조 추가/제거. `worktree`가 `""`이고 `on=false`면 모든 에이전트에서 제거. 페르소나면 실행 중인 에이전트에 바로 안내 메시지를 보내요 |

## 파일 편집기

| Rust 명령 | `ipc.*` | 인자 | 반환 | 용도 |
|---|---|---|---|---|
| `read_text_file` | `readTextFile` | `base`, `rel` | `{ text, mtime }` | 편집기용 텍스트 읽기. 5MB 초과·NUL 바이트(바이너리)는 거절 |
| `write_text_file` | `writeTextFile` | `base`, `rel`, `text`, `expected`, `force` (기본 `false`) | `number` (새 mtime) | 읽었을 때의 mtime(`expected`)과 다르면 `"CHANGED"`로 거절(`force`면 덮어씀) |
| `file_mtime` | `fileMtime` | `base`, `rel` | `number` | 파일 mtime(ms). 없으면 `0`. 편집기가 폴링해요 |
| `file_at_head` | `fileAtHead` | `base`, `rel` | `string` | HEAD 시점의 파일 내용(diff용). 새 파일이면 `""` |

`read_text_file`, `write_text_file`, `file_mtime`은 `inside()`로 `rel`이 `base` 밖(`..`, 절대 경로, 밖으로 나가는 링크)이면 거절해요. `file_at_head`는 철자만 검사해요(절대 경로나 `..` 거절).

## 오른쪽 패널·git

| Rust 명령 | `ipc.*` | 인자 | 반환 | 용도 |
|---|---|---|---|---|
| `list_dir` | `listDir` | `path` | `{ name, isDir }[]` | 폴더 목록(`.git` 제외, 폴더 먼저, 이름순) |
| `git_status` | `gitStatus` | `worktreePath` | `{ code, path }[]` | 변경된 파일 목록 |

## 자리 비움

| Rust 명령 | `ipc.*` | 인자 | 반환 | 용도 |
|---|---|---|---|---|
| `set_away` | `setAway` | `activeRoot` (`string \| null`), `minutes` | 없음 | 화면에 있는 프로젝트와 자리 비움 기준(분, `0`=끔)을 알려요. 30초마다 도는 `reap_away`가 이 값으로 다른 프로젝트의 쉬는 에이전트를 멈춰요 |

## 제약과 알려진 문제

- `ipc.ts`에 래퍼가 없는 명령은 없어요(42개 모두 래퍼가 있어요).
- `ensure()`를 거치는 명령(`send_message`, `ensure_agent`, `agent_ready`)은 meta가 없는 워크트리면 "이 워크트리에는 에이전트 설정이 없습니다"로 실패해요.
- 카탈로그에서 런타임이 없는 에이전트(`codex`)는 `cli_status`에는 나오지만 에이전트로 시작하면 "아직 워크트리 에이전트로 실행할 수 없어요" 에러예요.
