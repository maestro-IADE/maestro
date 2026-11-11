---
title: 디스크 저장 위치
area: reference
status: done
files: [src-tauri/src/main.rs, src-tauri/src/worktree.rs, src-tauri/src/agent.rs, src-tauri/src/control.rs, src-tauri/src/approval.rs, src-tauri/src/library.rs, src-tauri/src/antigravity.rs, src-tauri/src/commands.rs, src/Canvas.tsx, src-tauri/src/claude.rs, src-tauri/src/transcript.rs]
related: [reference/tauri-commands.md, reference/settings-keys.md, architecture/overview.md]
updated: 2026-11-11
---

# 디스크 저장 위치

## 요약

Maestro가 디스크에 쓰는 것은 모두 홈 폴더의 `~/.maestro` 아래에 있어요. 저장소 목록, 공용 페르소나와 문서(`library/`), `maestro` CLI 복사본은 바로 아래에 있고, 나머지(워크트리, 에이전트 meta, 캔버스)는 프로젝트별 폴더 `~/.maestro/projects/<이름>_<해시>/`에 있어요. 채팅 기록은 Maestro가 따로 저장하지 않고, 에이전트 CLI가 자기 폴더에 쓰는 대화 기록 파일을 읽어요.

## `~/.maestro` 구조

```
~/.maestro/
├─ repos.json
├─ merge-requests.json
├─ bin/maestro(.exe)
├─ personas/*.md
└─ projects/<이름>_<해시>/
   ├─ worktrees/<브랜치>/
   ├─ agents/
   │  ├─ <워크트리 폴더>.json
   │  ├─ <워크트리 폴더>.settings.json
   │  ├─ <워크트리 폴더>.brief.md
   │  ├─ <워크트리 폴더>.notes.json
   │  ├─ maestro.json (+ .settings.json, .brief.md, .notes.json)
   │  └─ ext-<경로 id>.json (+ .settings.json, .brief.md, .notes.json)
   ├─ canvases/<key>.json
   └─ library.json
```

| 경로 | 내용 | 쓰는 곳 |
|---|---|---|
| `~/.maestro/repos.json` | 등록한 저장소 루트 경로의 JSON 배열 | main.rs `add_repo`, `remove_repo` (`atomic_write`: `.json.tmp`에 쓴 뒤 rename) |
| `~/.maestro/merge-requests.json` | 사용자의 답을 기다리는 병합 요청(`MergeRequest`, camelCase) 배열. 워커마다 하나. 앱을 다시 켜도 카드가 남아요. 다시 읽을 때 다음 id는 가장 큰 id + 1 | merge.rs `Merges` (바뀔 때마다 잠금 안에서 다시 씀) → [병합 요청](../features/orchestration/merge-request.md) |
| `~/.maestro/bin/maestro` (Windows `maestro.exe`) | 앱 옆에 빌드된 `maestro-cli`의 복사본. 에이전트 PATH 맨 앞에 들어가요 | agent.rs `install_cli()`, 앱 시작마다 복사. 실행 중이라 복사가 실패하면 다음 시작에 바꿔요 |
| `~/.maestro/personas/*.md` | 모든 프로젝트가 같이 쓰는 페르소나. 폴더가 없을 때 한 번 `planner.md`, `implementer.md`, `reviewer.md`, `tester.md`를 만들어요 | library.rs `seed_personas()`(앱 시작), `create_persona()`(`persona_create`) |
| `~/.maestro/projects/<이름>_<해시>/` | 프로젝트 폴더. `<이름>`은 저장소 폴더 이름(영숫자·`-` 밖은 `_`), `<해시>`는 저장소 루트 문자열의 FNV-1a 32비트 16진수 8자리 | worktree.rs `project_dir()` |
| `…/worktrees/<브랜치>/` | Maestro가 만든 git 워크트리. 폴더 이름은 브랜치 이름(영숫자·`-` 밖은 `_`). 이름이 겹치면 `-2`, `-3` …을 붙여요 | worktree.rs `create()` |
| `…/agents/<워크트리 폴더>.json` | 에이전트 meta(`AgentMeta`) | agent.rs `Files::new`, `write_meta()` |
| `…/agents/maestro.json` | 마에스트로(프로젝트 폴더 자체에서 도는 에이전트)의 meta | main.rs `files()` |
| `…/agents/ext-<경로 id>.json` | Maestro 밖에서 만든 워크트리에 붙인 에이전트의 meta. `<경로 id>`는 소문자·`/`로 맞춘 경로의 FNV-1a 8자리 | main.rs `files()`, worktree.rs `path_id()` |
| `…/agents/<이름>.settings.json` | Claude의 `--settings` 파일: `Bash(maestro:*)` 허용 + 명령 가드(`permissions.deny`와 `PreToolUse` 훅: `/pretool/…`, 실패하면 `exit 2`로 막음) + `PermissionRequest` 훅(로컬 서버 `/permission/…`로 curl) + 신호용 `Stop`·`SessionStart` 훅(`/hook/…`, `\|\| exit 0`, timeout 10초). 포트가 실행마다 바뀌어서 시작할 때마다 다시 써요 | approval.rs `write_settings()` (`ClaudeAgent::launch`, 카탈로그 `features.approval_hook`이 켜진 에이전트만) |
| `…/agents/<이름>.brief.md` | Claude의 `--append-system-prompt-file` 내용: `maestro` CLI 안내, 마에스트로 역할·기준 브랜치, 페르소나, 참조 문서 목록. 시작할 때마다 다시 써요 | agent.rs `system_brief()` (`Agent::spawn`, Claude) |
| `…/agents/<이름>.notes.json` | 이 에이전트가 꺼져 있거나 신뢰 확인 중이라 못 받은 워커 완료 메모와 병합 결과 메모(`[키, 메모]` 쌍의 JSON 배열, 오래된 순, 키마다 마지막 하나. 키는 워커 이름 또는 `merge:<워커 경로>`). 다음에 시작되거나 준비되면 붙여 넣고 지워요. meta와 따로 둔 이유는 meta를 통째로 다시 쓰는 곳이 많아서예요 | control.rs `pass_notes()` (`worker_done`, `Agent::deliver_kept_notes`) |
| `…/agents/<이름>.brief/AGENTS.md` | 같은 브리프를 Antigravity용 규칙 파일로 쓴 것. 이 폴더를 `--add-dir`로 넘기면 agy가 규칙으로 읽어요 | antigravity.rs `AgyAgent::launch` |
| `…/canvases/<key>.json` | 캔버스 상태. `key`는 마에스트로 캔버스면 `overview`, 워크트리 캔버스면 워크트리 폴더 이름(영숫자·`-`·`_` 밖은 `_`) | main.rs `save_canvas_state` (`atomic_write`) |
| `~/.maestro/library/<분류>/*.md` | Maestro 공용 문서(분류 폴더마다). 예전 프로젝트별 `…/library.json`은 더 이상 읽지 않아요 | library.rs `seed_docs` / `create_doc` / `import_docs` |

`.settings.json`, `.brief.md`, `.notes.json`은 meta 파일 경로의 확장자를 바꿔서 만들어요(`meta.with_extension(...)`). 그래서 `maestro.json`이면 `maestro.settings.json`, `maestro.brief.md`, `maestro.notes.json`이 돼요.

## 저장소 안에 남기는 것 (git ref)

| ref | 내용 | 쓰는 곳 |
|---|---|---|
| `refs/maestro/trash/<브랜치>` (겹치면 `-2`, `-3` …) | 폐기에서 `버리기`를 고른 워크트리의 백업. 브랜치 끝 커밋이고, 커밋하지 않은 파일이 있었으면 그 위에 그 파일들을 담은 커밋 하나가 더 있어요. `<브랜치>`는 영숫자·`-` 밖을 `_`로 바꾼 이름. squash로 합쳐져 `git branch -d`가 거절한 브랜치를 지울 때도 남아요. 자동으로 지우지 않고, 기본 `git push`로 올라가지 않아요. 되살리기: `git branch <이름> refs/maestro/trash/<브랜치>` | worktree.rs `backup()` (`discard`) |

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

## 캔버스 파일 (`canvases/<key>.json`)

`Canvas.tsx`가 바뀐 뒤 500ms 지나서 저장해요. 필드는 `version: "1.0.0"`, `rootPath`, `activeSkillId`, `activeThemeId`, `viewport`, `tree: 1`, `nodes`(`id`, `type`, `position`, `width`, `height`, `data`), `edges`(`id`, `source`, `target`, `data`)예요. 마에스트로 계층 선(`data.kind === "orchestrates"`)은 저장하지 않고 meta의 `workers`로 다시 그려요.

## 채팅 런타임(ACP) 파일

- `~/.maestro/runtimes.json`: `{"chat":{"claude-code":"auto"|"pty"}}`. 설정의 "채팅 방식"(`set_chat_runtime`). 없으면 auto. 화면(localStorage)이 아니라 여기 두는 건 `maestro send`·자리 비움 재시작처럼 화면 없이 에이전트를 띄울 때도 읽어야 해서예요.
- `<app_local_data_dir>/acp/claude-agent-acp@0.102.0/`(Windows: `%LOCALAPPDATA%/dev.maestro.app/acp/…`, 앱 식별자 `dev.maestro.app`): 관리형 어댑터 설치. 설정의 "채팅 런타임 설치"만 만들어요(`node_modules/@agentclientprotocol/claude-agent-acp/…`). 영수증 `.install-complete.json` = `{ package, version, entry, node, omitOptional, installedAt }`가 있어야 설치된 걸로 쳐요. 설치 중에는 같은 폴더 옆 `.staging-<pid>-<nanos>/`를 써요(실패하면 지워요).
- `<app_local_data_dir>/acp/logs/<경로 id>/agent.log`: `MAESTRO_ACP_LOGS=1`일 때만 어댑터 로그.
- 개발용: `MAESTRO_ACP_CLAUDE_DIR`가 가리키는 폴더(읽기만 해요).

## Maestro가 읽기만 하는 외부 위치

| 경로 | 내용 | 읽는 곳 |
|---|---|---|
| `~/.claude/projects/<slug>/<세션 id>.jsonl` | Claude Code 대화 기록. `<slug>`는 워크트리 경로에서 ASCII 영숫자가 아닌 문자를 모두 `-`로 바꾼 것 | claude.rs `session_file()` → 채팅 뷰, `get_transcript`, 요약, 상태 tail |
| `~/.gemini/antigravity-cli/brain/<대화 id>/.system_generated/logs/transcript.jsonl` | Antigravity(`agy`) 대화 기록 | antigravity.rs `transcript()` |
| `~/.gemini/antigravity-cli/cache/last_conversations.json` | 작업 폴더 → 최근 대화 id 맵. 새 Antigravity 에이전트의 대화 id를 여기서 찾아 meta `cliSessionId`에 저장해요 | antigravity.rs `conversation_for()` |
| `<워크트리>/.claude/commands/**`, `<워크트리>/.claude/skills/*/SKILL.md` | 프로젝트 슬래시 명령·스킬 | commands.rs `list()` (`list_slash_commands`) |
| `~/.claude/commands/**`, `~/.claude/skills/*/SKILL.md` | 사용자 슬래시 명령·스킬 | commands.rs `list()` |
| 페르소나·문서 폴더의 `*.md` | 깊이 4까지, 최대 500개. `.`으로 시작하는 폴더, `node_modules`, `target`은 건너뛰어요 | library.rs `scan()` |

## 제약과 알려진 문제

- 프로젝트 폴더 해시는 저장소 루트 문자열을 그대로 해시해요. 그래서 `add_repo`는 이미 등록된 루트와 같은 저장소(구분자 `\`/`/`, 끝 `/`, Windows·macOS에서는 대소문자 무시)면 처음 등록한 표기를 그대로 써요(`same_root`). 같은 저장소가 두 번 등록되거나 프로젝트 폴더가 둘로 갈리지 않아요.
- `discard_worktree`는 `forget_agent`로 meta(`.json`), `.settings.json`, `.brief.md`, `.notes.json`, `.brief/` 폴더를 모두 지워요. 워크트리 폴더 삭제(`worktree::discard`)가 실패하면 이 파일들은 남겨요.
- Claude가 아주 긴 slug(200자 초과)를 해시로 줄이는 경우는 처리하지 않아요(claude.rs의 `shortcut:` 주석).
- meta의 `cliSessionId`는 실행 중에도 바뀔 수 있어요: 대화 없이 시작한 Antigravity 에이전트가 agy가 만든 대화를 가져올 때(R3 전과 같아요). `FOLLOW_SESSIONS`를 켜면 Claude `/clear`(SessionStart 훅)와 Antigravity의 더 최근 대화를 따라갈 때도 바뀌어요(`LiveTail::follow`, 읽고-고쳐-쓰기). R3에서는 꺼져 있어요.
- Antigravity 에이전트는 `.settings.json`과 `.brief.md` 대신 `.brief/AGENTS.md`를 써요. 승인 훅 설정은 없어요.
- meta 파일(`write_meta`)은 `atomic_write`가 아니라 `std::fs::write`로 바로 써요.
