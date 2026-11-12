---
title: 디스크 저장 위치
area: reference
status: done
files: [src-tauri/src/main.rs, src-tauri/src/durable.rs, src-tauri/src/worktree.rs, src-tauri/src/agent.rs, src-tauri/src/control.rs, src-tauri/src/approval.rs, src-tauri/src/library.rs, src-tauri/src/antigravity.rs, src-tauri/src/commands.rs, src/Canvas.tsx, src-tauri/src/claude.rs, src-tauri/src/transcript.rs]
related: [reference/tauri-commands.md, reference/settings-keys.md, architecture/overview.md]
updated: 2026-11-12
---

# 디스크 저장 위치

## 요약

Maestro가 디스크에 쓰는 것은 모두 홈 폴더의 `~/.maestro` 아래에 있어요. 저장소 목록, 공용 페르소나와 문서(`library/`), `maestro` CLI 복사본은 바로 아래에 있고, 나머지(워크트리, 에이전트 meta, 캔버스)는 프로젝트별 폴더 `~/.maestro/projects/<이름>_<해시>/`에 있어요. 채팅 기록은 Maestro가 따로 저장하지 않고, 에이전트 CLI가 자기 폴더에 쓰는 대화 기록 파일을 읽어요.

## `~/.maestro` 구조

```
~/.maestro/
├─ repos.json (+ .bak)
├─ merge-requests.json (+ .bak)
├─ pull-requests.json (+ .bak)
├─ runtimes.json (+ .bak)
├─ bin/maestro(.exe)
├─ personas/*.md
└─ projects/<이름>_<해시>/
   ├─ worktrees/<브랜치>/
   ├─ agents/
   │  ├─ <워크트리 폴더>.json (+ .json.bak)
   │  ├─ <워크트리 폴더>.settings.json
   │  ├─ <워크트리 폴더>.brief.md
   │  ├─ <워크트리 폴더>.notes.json
   │  ├─ maestro.json (+ .settings.json, .brief.md, .notes.json)
   │  └─ ext-<경로 id>.json (+ .settings.json, .brief.md, .notes.json)
   ├─ canvases/<key>.json (+ .json.bak)
   ├─ project.json (+ .bak)
   └─ library.json
```

| 경로 | 내용 | 쓰는 곳 |
|---|---|---|
| `~/.maestro/repos.json` | 등록한 저장소 루트 경로 목록: 맨 배열 `[경로…]`(형식 1이라 버전을 모르는 예전 빌드도 읽어요) | main.rs `add_repo`, `remove_repo` (한 번에 하나씩, `durable::write_json`) |
| `~/.maestro/merge-requests.json` | 사용자의 답을 기다리는 병합 요청(`MergeRequest`, camelCase) 목록: 맨 배열 `[...]`(형식 1). 워커마다 하나. 앱을 다시 켜도 카드가 남아요. 다시 읽을 때 다음 id는 가장 큰 id + 1. R6 필드(없으면 기본값이라 R5 파일도 읽혀요): `mode`(`local`/`pr`), `workerDirty`(PR 모드의 커밋하지 않은 추적 파일), `remote`, `cmpRef`, `existingPr`(요청 때 열린 PR, "다시 확인"이 찾은 PR로도 바뀌어요), `pr`(PR 모드 승인의 쓰기 전 기록: `step`(`none`/`committing`/`committed`/`pushing`/`pushed`/`creating`/`unconfirmed`), `commitMessage`, `commitParent`, `appCommit`, `commitMismatch`(앱 커밋의 파일이 카드와 다를 때의 이유: 그 커밋은 푸시하지 않음), `pushSha`, `createAttemptAt`, `title`, `body`, `draft`, `lastEmptyLookupAt`, `error`, `notice`(오류가 아닌 안내: "다시 확인"이 찾은 PR)), `prDraft`(`{ title, body, draft }`) | merge.rs `Merges` (바뀔 때마다 잠금 안에서 `durable::write_json`. 쓰기 전 기록(`pr`)은 `try_update`: 저장하지 못하면 바꾸지 않고 그 단계도 하지 않아요) → [병합 요청](../features/orchestration/merge-request.md), [PR 모드](../features/orchestration/pr-mode.md) |
| `~/.maestro/pull-requests.json` | 앱이 아는 워커 PR(`PrRecord`, camelCase) 목록: 맨 배열 `[...]`(형식 1). 워커마다 하나, 번호를 알 때만 생겨요. 상태·리뷰·CI 요약(`checks`), `queued`(자동 병합 또는 머지 큐 `isInMergeQueue`), 병합 시도(`mergeAttempt`, `gh pr merge` 전에 씀), 병합(`merged`), 합쳐진 머리 목록(`mergedHeads`: 폐기 판정과 계획에 씀, 새 PR로 바뀌어도 남아요), 병합 뒤 프로젝트 폴더 결과(`afterMerge`), 오류가 아닌 안내(`notice`: 마지막 승인 결과(찾아서 이어 씀, 갱신, 본문 고치기 실패)나 병합 클릭의 상태("병합 대기열/자동 병합에 들어갔어요", "gh는 병합 요청을 받았지만 PR이 아직 열려 있어요 …")), `checkedAt`(이 기록을 마지막으로 쓴 조회·병합 시각: 그 전에 시작한 조회의 답은 버려요), `hidden`, `noteOwed`(사용자 클릭으로 합쳐졌고 지휘자 메모를 아직 못 보냄: 빨리감기 중 앱이 꺼지면 다음 시작 때 보내요). `checks.headSha`는 그 체크가 어느 커밋의 것인지예요(PR 머리와 다르면 화면도 병합도 "확인 중"으로 봐요). 워커를 폐기하면 지워요(마에스트로 폐기는 기록을 남기고 `maestroPath`만 비워요: 그 PR 카드는 워커 블록으로 옮겨 가요) | pr.rs `Prs` (잠금 안에서 `durable::write_json`. `mergeAttempt`는 `try_update`: 저장하지 못하면 `gh pr merge`를 부르지 않아요) → [PR 모드](../features/orchestration/pr-mode.md) |
| `~/.maestro/bin/maestro` (Windows `maestro.exe`) | 앱 옆에 빌드된 `maestro-cli`의 복사본. 에이전트 PATH 맨 앞에 들어가요 | agent.rs `install_cli()`, 앱 시작마다 복사. 실행 중이라 복사가 실패하면 다음 시작에 바꿔요 |
| `~/.maestro/personas/*.md` | 모든 프로젝트가 같이 쓰는 페르소나. 폴더가 없을 때 한 번 `planner.md`, `implementer.md`, `reviewer.md`, `tester.md`를 만들어요 | library.rs `seed_personas()`(앱 시작), `create_persona()`(`persona_create`) |
| `~/.maestro/projects/<이름>_<해시>/` | 프로젝트 폴더. `<이름>`은 저장소 폴더 이름(영숫자·`-` 밖은 `_`), `<해시>`는 저장소 루트 문자열의 FNV-1a 32비트 16진수 8자리 | worktree.rs `project_dir()` |
| `…/worktrees/<브랜치>/` | Maestro가 만든 git 워크트리. 폴더 이름은 브랜치 이름(영숫자·`-` 밖은 `_`). 이름이 겹치면 `-2`, `-3` …을 붙여요 | worktree.rs `create()` |
| `…/agents/<워크트리 폴더>.json` | 에이전트 meta(`AgentMeta`, `version` 키 포함) | agent.rs `Files::new`, `write_meta()` (`durable::write_json`) |
| `…/agents/maestro.json` | 마에스트로(프로젝트 폴더 자체에서 도는 에이전트)의 meta | main.rs `files()` |
| `…/agents/ext-<경로 id>.json` | Maestro 밖에서 만든 워크트리에 붙인 에이전트의 meta. `<경로 id>`는 소문자·`/`로 맞춘 경로의 FNV-1a 8자리 | main.rs `files()`, worktree.rs `path_id()` |
| `…/agents/<이름>.settings.json` | Claude의 `--settings` 파일: `Bash(maestro:*)` 허용 + 명령 가드(`permissions.deny`와 `PreToolUse` 훅: `/pretool/…`, 실패하면 `exit 2`로 막음) + `PermissionRequest` 훅(로컬 서버 `/permission/…`로 curl) + 신호용 `Stop`·`SessionStart` 훅(`/hook/…`, `\|\| exit 0`, timeout 10초). 포트가 실행마다 바뀌어서 시작할 때마다 다시 써요 | approval.rs `write_settings()` (`ClaudeAgent::launch`, 카탈로그 `features.approval_hook`이 켜진 에이전트만) |
| `…/agents/<이름>.brief.md` | Claude의 `--append-system-prompt-file` 내용: `maestro` CLI 안내, 마에스트로 역할·기준 브랜치, 페르소나, 참조 문서 목록. 시작할 때마다 다시 써요 | agent.rs `system_brief()` (`Agent::spawn`, Claude) |
| `…/agents/<이름>.notes.json` | 이 에이전트가 꺼져 있거나 신뢰 확인 중이라 못 받은 워커 완료 메모와 병합 결과 메모(`[키, 메모]` 쌍의 JSON 배열, 오래된 순, 키마다 마지막 하나. 키는 워커 이름, `merge:<워커 경로>`, PR 모드의 `pr:<워커 경로>`, 병합 방식 변경의 `mode`, 그리고 `maestro worktree create --prompt`가 보내지 못한 첫 지시의 `prompt:<만든 에이전트 경로>`. 첫 지시는 메모가 아니라 지시 글 그대로 새 워커 자신의 파일에 있어요). 다음에 시작되거나 준비되면(신뢰 확인 뒤, 늦게 뜬 시작·로그인 뒤, `agent_ready`, `maestro send` 앞) 붙여 넣고 지워요. 프로젝트에 마에스트로가 없을 때(폐기한 뒤) 지휘자에게 가는 PR 메모는 여기 쓰지 않고 버려요. meta와 따로 둔 이유는 meta를 통째로 다시 쓰는 곳이 많아서예요 | control.rs `pass_notes()` (`worker_done`, `Agent::deliver_kept_notes`) |
| `…/agents/<이름>.brief/AGENTS.md` | 같은 브리프를 Antigravity용 규칙 파일로 쓴 것. 이 폴더를 `--add-dir`로 넘기면 agy가 규칙으로 읽어요 | antigravity.rs `AgyAgent::launch` |
| `…/project.json` | 프로젝트 설정: `{ "version": 1, "mergeMode": "local" \| "pr" }`. 없거나 못 읽으면 `local`. 마에스트로 meta가 아닌 곳에 둔 이유: 마에스트로를 지워도 남고, 요약이 meta를 통째로 다시 써서예요 | project.rs `write_mode` (`set_merge_mode`, `durable::write_json`) |
| `…/canvases/<key>.json` | 캔버스 상태. `key`는 마에스트로 캔버스면 `overview`, 워크트리 캔버스면 워크트리 폴더 이름(영숫자·`-`·`_` 밖은 `_`) | main.rs `save_canvas_state` (`durable::replace_text`: 내용은 화면이 정하고, 앞의 파일만 `.bak`로 남기거나 옆으로 옮겨요) |
| `~/.maestro/library/<분류>/*.md` | Maestro 공용 문서(분류 폴더마다). 예전 프로젝트별 `…/library.json`은 더 이상 읽지 않아요 | library.rs `seed_docs` / `create_doc` / `import_docs` |

`.settings.json`, `.brief.md`, `.notes.json`은 meta 파일 경로의 확장자를 바꿔서 만들어요(`meta.with_extension(...)`). 그래서 `maestro.json`이면 `maestro.settings.json`, `maestro.brief.md`, `maestro.notes.json`이 돼요.

## 저장 방식과 형식 버전

`~/.maestro`와 프로젝트 폴더의 상태 파일(`repos.json`, `merge-requests.json`, `pull-requests.json`, `runtimes.json`, 에이전트 meta, `project.json`, 캔버스)은 모두 durable.rs로 써요.

- **원자적 쓰기**: 같은 폴더의 `.<이름>.<pid>.<번호>.tmp`에 쓰고 `sync_all`로 디스크에 내린 뒤 원래 이름으로 rename해요. 임시 파일 이름이 쓰기마다 달라서 두 쓰기가 같은 임시 파일을 나눠 쓰지 않아요. Windows에서 백신·색인기가 파일을 잡고 있으면(권한 거부, 공유 위반) 25ms부터 두 배씩 네 번 더 시도해요. Unix에서는 폴더도 `sync_all`해요. Windows는 std로 폴더 핸들을 열 수 없어서 하지 않아요(NTFS가 rename을 저널에 남겨요).
- **`.bak`**: 바꾸기 전 파일을 이 빌드가 읽을 수 있으면 `<이름>.bak`(meta·캔버스는 `<이름>.json.bak`)로 남겨요. 내용이 그대로면 아무것도 쓰지 않아요.
- **손상 파일**: JSON이 아니거나 이 빌드가 읽는 모양이 아니면 덮어쓰지 않고 `<이름>.corrupt-<ms>`로 옮긴 뒤 새로 써요(옮기지 못하면 쓰지 않아요). 읽을 때 본 파일이 손상됐고 `.bak`가 읽히면 `.bak`를 써요. 읽기는 파일을 옮기지 않아요. 본 파일이 없으면 `.bak`가 있어도 되살리지 않아요(지운 건 지운 거예요).
- **형식 버전**: 객체 파일은 `"version": 1` 키를 써요(예전 빌드는 모르는 키라 무시해요). 목록 파일(`repos.json`, `merge-requests.json`, `pull-requests.json`)은 형식 1이면 맨 배열 그대로 쓰고, 형식을 2 이상으로 올릴 때만 `{ "version": 2, "items": [...] }`로 감싸요(`durable::write_json`). `version`이 없는 파일(맨 배열, 키 없는 객체)은 버전 1로 읽고, 감싼 `{version, items}`도 읽어요. 캔버스는 화면이 쓰는 `"version": "1.0.0"`의 앞 숫자를 봐요. 이 빌드보다 높은 버전의 파일은 읽지도 쓰지도 않아요(`.bak`도 만들지 않아요).
- **사용자에게 알리기**: 손상돼 `.bak`로 읽었거나, 옆으로 옮겼거나, 더 새 Maestro의 파일이거나, 저장에 실패하면 "Maestro 저장 파일" 경고 창이 떠요. 같은 파일은 한 번 실행에 한 번만 떠요. 로그(`[maestro] …`)에는 매번 남아요.
- **못 쓰는 파일일 때 하는 일**: 병합 요청·PR 기록은 빈 목록으로 시작하고, 더 새 파일이면 이번 실행의 변경은 저장되지 않아요. 프로젝트 목록(`list_repos`)은 오류를 보여 주고, 손상된 경우 프로젝트를 다시 추가하면 손상 파일을 옆에 옮기고 새로 만들어요. `create_maestro`·`attach_agent`는 읽지 못하는 meta를 "없음"으로 보지 않고 거절해요. 캔버스는 불러오기 오류를 보여 주고 저장하지 않아요. `set_merge_mode`·채팅 방식 설정은 더 새 파일이면 오류를 돌려줘요.
- **예전 빌드와 같이 쓸 때**: 같은 `~/.maestro`를 쓰는 예전 빌드(설치된 Maestro 안에서 띄운 다른 브랜치의 개발 빌드, 되돌린 설치)도 형식 1 파일은 그대로 읽어요. 목록은 맨 배열이고 객체의 `version` 키는 무시하니까요. 다만 예전 빌드는 `.bak`·버전 확인 없이 덮어써요. 나중에 목록 형식을 2로 올리면 그때부터 예전 빌드는 그 목록을 못 읽어요: `repos.json`이면 "repos.json 손상" 오류로 프로젝트 목록·추가·`maestro` CLI가 모두 실패하고, 병합 요청·PR 기록이면 빈 목록으로 시작해 다음 변경 때 이 빌드의 기록을 덮어써요.
- `<이름>.notes.json`(전달되면 지우는 메모 대기열)은 원자적으로만 쓰고 버전과 `.bak`는 없어요.

## 저장소 안에 남기는 것 (git ref)

| ref | 내용 | 쓰는 곳 |
|---|---|---|
| `refs/maestro/trash/<브랜치>` (겹치면 `-2`, `-3` …) | 폐기에서 `버리기`를 고른 워크트리의 백업. 브랜치 끝 커밋이고, 커밋하지 않은 파일이 있었으면 그 위에 그 파일들을 담은 커밋 하나가 더 있어요. `<브랜치>`는 영숫자·`-` 밖을 `_`로 바꾼 이름. squash로 합쳐져 `git branch -d`가 거절한 브랜치를 지울 때도 남아요. 자동으로 지우지 않고, 기본 `git push`로 올라가지 않아요. 되살리기: `git branch <이름> refs/maestro/trash/<브랜치>` | worktree.rs `backup()` (`discard`) |
| `refs/remotes/<원격>/<기준>` | PR 모드에서 승인과 PR 병합 뒤 빨리감기 때 `git fetch --no-tags --no-write-fetch-head <원격> +refs/heads/<기준>:refs/remotes/<원격>/<기준>`으로 원격 기준을 받아요. 작업 트리와 로컬 브랜치는 바꾸지 않아요(프로젝트 폴더는 사용자의 병합 클릭 뒤 조건이 맞을 때만 `merge --ff-only`) | pr.rs `fetch_base` → [PR 모드](../features/orchestration/pr-mode.md) |
| 원격의 `refs/heads/<워커 브랜치>` | PR 모드 승인이 기록한 커밋을 `<커밋>:refs/heads/<브랜치>`로 푸시해요(강제 없음). 앱은 원격 브랜치를 지우지 않아요 | pr.rs `approve` |

폐기에서 `보관`을 고르면 커밋하지 않은 파일이 그 워크트리 브랜치(`refs/heads/<브랜치>`)에 `maestro: 커밋하지 않은 작업 보관 (<브랜치>)` 커밋으로 들어가요(worktree.rs `snapshot()`).

## 에이전트 meta (`AgentMeta`, camelCase JSON)

| 필드 | 타입 | 뜻 |
|---|---|---|
| `adapterId` | string | `claude-code` / `antigravity` 등 |
| `permissionMode` | string | 권한 모드 |
| `cliSessionId` | string \| null | Claude 세션 id 또는 Antigravity 대화 id. 다시 시작할 때 이어서 실행(`--resume` / `--conversation`) |
| `name`, `description` | string \| null | 사용자가 붙인 이름과 설명 |
| `model` | string \| null | 시작할 때마다 넘기는 `--model`. `null`이면 CLI 기본값 |
| `role` | string \| null | `"maestro"`면 프로젝트 조율자 |
| `workers` | string[] | 하위 워크트리 경로(캔버스의 계층 선) |
| `refs` | string[] | 참조하는 Markdown 파일 경로(페르소나 포함) |
| `keepBranch` | boolean | 이미 있던 브랜치를 체크아웃한 워크트리. 버릴 때 브랜치를 남겨요 |
| `baseBranch` | string \| null | 마에스트로만: 워커가 갈라져 나가고 병합되는 브랜치 |
| `summary`, `summaryKey` | string \| null | 최근 답변 요약 캐시와 그 요약을 만든 입력의 해시 |
| `lastView` | string \| null | `"cli"`면 사용자가 마지막에 CLI 뷰를 봤어요. 다음 시작이 터미널(PTY)로 해요. `null`이면 채팅(ACP를 쓸 수 있으면 ACP) |
| `acpTrusted` | boolean | ACP 채팅에서 이 폴더를 신뢰한다고 답했어요(TUI의 신뢰 창은 ACP에서 안 떠서 Maestro가 따로 물어요) |
| `cliTranscript` | `{ sessionId, path }` (없으면 생략) | Claude가 훅(SessionStart, Stop)의 `transcript_path`로 알려 준 트랜스크립트 위치. `sessionId`가 `cliSessionId`와 같을 때만 써요. 계산한 위치보다 먼저 봐요(`CLAUDE_CONFIG_DIR`, 해시로 줄인 긴 폴더 이름) |

## 캔버스 파일 (`canvases/<key>.json`)

`Canvas.tsx`가 바뀐 뒤 500ms 지나서 저장해요. 필드는 `version: "1.0.0"`, `rootPath`, `activeSkillId`, `activeThemeId`, `viewport`, `tree: 1`, `nodes`(`id`, `type`, `position`, `width`, `height`, `data`), `edges`(`id`, `source`, `target`, `data`)예요. 마에스트로 계층 선(`data.kind === "orchestrates"`)은 저장하지 않고 meta의 `workers`로 다시 그려요.

## 채팅 런타임(ACP) 파일

- `~/.maestro/runtimes.json`: `{"version":1,"chat":{"claude-code":"auto"|"pty"}}`. 설정의 "채팅 방식"(`set_chat_runtime`). 없으면 auto. 화면(localStorage)이 아니라 여기 두는 건 `maestro send`·자리 비움 재시작처럼 화면 없이 에이전트를 띄울 때도 읽어야 해서예요.
- `<app_local_data_dir>/acp/claude-agent-acp@0.102.0/`(Windows: `%LOCALAPPDATA%/dev.maestro.app/acp/…`, 앱 식별자 `dev.maestro.app`): 관리형 어댑터 설치. 설정의 "채팅 런타임 설치"만 만들어요(`node_modules/@agentclientprotocol/claude-agent-acp/…`). 영수증 `.install-complete.json` = `{ package, version, entry, node, omitOptional, installedAt }`가 있어야 설치된 걸로 쳐요. 설치 중에는 같은 폴더 옆 `.staging-<pid>-<nanos>/`를 써요(실패하면 지워요).
- `<app_local_data_dir>/acp/logs/<경로 id>/agent.log`: `MAESTRO_ACP_LOGS=1`일 때만 어댑터 로그.
- 개발용: `MAESTRO_ACP_CLAUDE_DIR`가 가리키는 폴더(읽기만 해요).

## Maestro가 읽기만 하는 외부 위치

| 경로 | 내용 | 읽는 곳 |
|---|---|---|
| `~/.claude/projects/<slug>/<세션 id>.jsonl` | Claude Code 대화 기록. `CLAUDE_CONFIG_DIR`가 있으면 `~/.claude` 대신 그 폴더예요. `<slug>`는 워크트리 경로에서 ASCII 영숫자가 아닌 문자(UTF-16 단위)를 모두 `-`로 바꾼 것. meta `cliTranscript`가 있으면 그 위치가 먼저예요 | claude.rs `session_file()` → 채팅 뷰, `get_transcript`, 요약, 상태 tail |
| `~/.gemini/antigravity-cli/brain/<대화 id>/.system_generated/logs/transcript.jsonl` | Antigravity(`agy`) 대화 기록 | antigravity.rs `transcript()` |
| `~/.gemini/antigravity-cli/cache/last_conversations.json` | 작업 폴더 → 최근 대화 id 맵. 새 Antigravity 에이전트의 대화 id를 여기서 찾아 meta `cliSessionId`에 저장해요 | antigravity.rs `conversation_for()` |
| `<워크트리>/.claude/commands/**`, `<워크트리>/.claude/skills/*/SKILL.md` | 프로젝트 슬래시 명령·스킬 | commands.rs `list()` (`list_slash_commands`) |
| `~/.claude/commands/**`, `~/.claude/skills/*/SKILL.md` | 사용자 슬래시 명령·스킬 | commands.rs `list()` |
| 페르소나·문서 폴더의 `*.md` | 깊이 4까지, 최대 500개. `.`으로 시작하는 폴더, `node_modules`, `target`은 건너뛰어요 | library.rs `scan()` |

## 제약과 알려진 문제

- 프로젝트 폴더 해시는 저장소 루트 문자열을 그대로 해시해요. 그래서 `add_repo`는 이미 등록된 루트와 같은 저장소(구분자 `\`/`/`, 끝 `/`, Windows·macOS에서는 대소문자 무시)면 처음 등록한 표기를 그대로 써요(`same_root`). 같은 저장소가 두 번 등록되거나 프로젝트 폴더가 둘로 갈리지 않아요.
- `discard_worktree`는 `forget_agent`로 meta(`.json`), `.json.bak`, `.settings.json`, `.brief.md`, `.notes.json`, `.brief/` 폴더를 모두 지워요(옆으로 옮긴 `.corrupt-…`는 남겨요). 워크트리 폴더 삭제(`worktree::discard`)가 실패하면 이 파일들은 남겨요.
- Claude가 아주 긴 slug(200자 초과)를 해시로 줄인 폴더는 계산으로는 찾지 못해요(claude.rs `slug_hashed`, `session_file`). 터미널(PTY)은 첫 훅(SessionStart, Stop)이 알려 준 위치(meta `cliTranscript`, runtime.rs `transcript_path`)를 읽어서 그 뒤로는 채팅이 보여요. 첫 훅 전에는 채팅 tail이 계산한(없는) 위치를 기다려요. ACP 채팅은 이런 폴더면 터미널로 시작해요(acp_setup.rs, `transcriptUnreachable`).
- meta의 `cliSessionId`는 실행 중에도 바뀔 수 있어요: 대화 없이 시작한 Antigravity 에이전트가 agy가 만든 대화를 가져올 때(R3 전과 같아요). `FOLLOW_SESSIONS`를 켜면 Claude `/clear`(SessionStart 훅)와 Antigravity의 더 최근 대화를 따라갈 때도 바뀌어요(`LiveTail::follow`, 읽고-고쳐-쓰기). R3에서는 꺼져 있어요.
- Antigravity 에이전트는 `.settings.json`과 `.brief.md` 대신 `.brief/AGENTS.md`를 써요. 승인 훅 설정은 없어요.
- 개발 빌드와 릴리스 빌드는 같은 `~/.maestro`를 써요. 둘을 같이 띄우면 마지막에 쓴 쪽이 이겨요([한 번에 하나만 실행](../features/ui/single-instance.md)).
