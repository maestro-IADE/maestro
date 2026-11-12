---
title: 이벤트 목록
area: reference
status: done
files: [src-tauri/src/agent.rs, src-tauri/src/tui_wait.rs, src-tauri/src/acp_runtime.rs, src-tauri/src/approval.rs, src-tauri/src/control.rs, src/App.tsx, src/Workspace.tsx, src/store.ts, src/settings/AgentsSection.tsx, src/nodes/AgentNode.tsx, src/nodes/Attention.tsx, src/nodes/XTerm.tsx, src/types.ts, src-tauri/src/events.rs, src-tauri/src/stream.rs, src-tauri/src/pty.rs, src-tauri/src/transcript.rs, src-tauri/src/merge.rs, src-tauri/src/pr.rs]
related: [reference/tauri-commands.md, architecture/overview.md, architecture/runtime-events.md]
updated: 2026-11-13
---

# 이벤트 목록

## 요약

Rust 백엔드가 `app.emit(...)`으로 보내고 프런트엔드가 `listen<...>`으로 받는 Tauri 이벤트 전체 목록이에요. 모두 앱 전체(전역)로 보내요. 채팅 이벤트는 런타임과 상관없는 공통 이벤트예요. → [공통 이벤트](../architecture/runtime-events.md) 페이로드 필드는 `#[serde(rename_all = "camelCase")]`라서 JS에서 camelCase로 받아요.

## 이벤트

| 이름 | 페이로드 | 보내는 곳 | 받는 곳 |
|---|---|---|---|
| `agent://state` | `{ worktreePath, state: { kind: "RUNNING" \| "IDLE" \| "TERMINATED" } }` (`AgentStatePayload`) | agent.rs `emit_state()`: 채팅 전송 시 `Agent::send` → `RUNNING`, 스트림의 observer가 durable 이벤트마다 `events::status_after`(`userMessage` → `RUNNING`, `notice`·`turnEnded`(replaced 빼고) → `IDLE`), 프로세스 종료 감시(pty.rs) → `TERMINATED`. 그리고 `Agent::spawn` 직후 `IDLE`을 직접 보내요 | `Workspace.tsx` `useAgentEvents` → `useStore.onAgentState`(상태·읽지 않음 표시) |
| `agent://event` | `Envelope { worktreePath, epoch, seq, event: RuntimeEvent }` (`AgentEventPayload`). ACP 런타임은 실시간 글을 `delta`(항목 `live:<n>:<k>`, `seq: null`, 50ms로 묶음)로 보내고, 완성된 블록이 오면 `delta { offset: 0, text: "" }`로 지워요. 화면은 `seq: null`인 `delta`를 이벤트 목록에 붙여 그려요. `seq`는 durable 이벤트의 기록 속 번호(1부터), live-only 이벤트(`delta`, `approvalRequested`, `approvalResolved`, `exited`, `reset`)는 `null` | stream.rs `Emitter`: PTY 런타임의 `LiveTail`(250ms마다, Stop 훅이면 바로 트랜스크립트의 새 줄을 공통 이벤트로), `reset`(재시작·소스 전환·파일 잘림), `exited`(프로세스 종료), 승인(approval.rs `Approvals::request`) | `nodes/AgentNode.tsx` `useTranscript`: 경로가 같고 epoch가 같으면 `seq`가 더 큰 durable 이벤트를 추가하고, `seq: null` 중 글 `delta`만 `addDelta`로 덧붙여요(항목의 퇴장 델타가 오면 그 항목의 델타를 빼요. durable `turnEnded`로는 빼지 않아요). 더 새 epoch면(`reset switch`·`fallback` 포함) `ipc.transcript()`로 다시 받아요. 옛 epoch와 다른 live-only 이벤트는 버려요 |
| `pty://data` | `{ worktreePath, dataB64, seq, epoch }` (`PtyDataPayload`). `seq`는 지금까지의 총 바이트 수, `epoch`는 이 키의 몇 번째 PTY인지(시작마다 +1) | pty.rs `PtyRuntime::start`: 시작 직후 빈 배치(`dataB64: ""`, `seq: 0`) 한 번, 그 뒤 출력 스레드(16ms 또는 64KB 단위로 묶어서) | `nodes/XTerm.tsx`: 경로가 같고 epoch가 같으면 `seq` 기준으로 중복을 버리고 씀. 더 새 epoch면 화면을 지우고 `ptySnapshot`부터 다시 그려요(채팅→CLI 전환으로 새 TUI가 뜰 때도 이렇게 다시 그려요) |
| `pty://exit` | `{ worktreePath, code }` (`PtyExitPayload`). `code`는 프로세스의 종료 코드 | pty.rs `PtyRuntime::start`의 종료 감시 스레드: 프로세스가 끝났을 때 한 번, `on_exit`(→ `agent://state` `TERMINATED`)보다 먼저. 모든 PTY(에이전트, 설치 터미널)가 보내요 | `settings/AgentsSection.tsx` `SetupTerminal`(키 `setup:<id>`): 끝나면 `loadCli()`로 설치 상태를 다시 읽고, `code === 0`이면 `setup_stop` 후 터미널을 닫아 "끝났어요"를 남겨요. 아니면 열어 둔 채 종료 코드를 보여요 |
| `agent://approval` | `Approval`: `{ id, worktreePath, toolName, toolInput, canAlways, turnId, toolCallId, cliAnswer }` (`turnId`·`toolCallId`는 PTY 훅에서는 짐작이라 `null`일 수 있어요. `cliAnswer: false`면 "터미널에서 답하기"를 보이지 않아요) | approval.rs `Approvals::request`: 에이전트의 `PermissionRequest` 훅이 로컬 서버 `/permission/<session>`으로 요청했을 때, 또는 ACP `session/request_permission`. 같은 내용이 `agent://event`에 `approvalRequested`로도 가요 | `Workspace.tsx` `useAgentEvents` → `useStore.addApproval`. `ApprovalItem`(nodes/Attention.tsx, 채팅의 승인 카드)이 `cliAnswer !== false`일 때만 "터미널에서 답하기"를 그려요. 확인할 일 팝오버(`AttentionRow`)에도 같은 승인이 떠요 |
| `agent://approval-done` | `{ id, worktreePath }` | approval.rs `Approvals::request`: 사용자가 답했거나, 시간 초과(590초)거나, 훅 쪽 연결이 끊겼을 때. `agent://event`에는 `approvalResolved { resolution }`으로도 가요 | `Workspace.tsx` `useAgentEvents` → `useStore.dropApproval(id)` (`id`만 써요) |
| `agent://tui-wait` | `{ worktreePath, wait: TuiWait \| null }` (`TuiWait` = `{ worktreePath, source: "cli" \| "timeout" \| "screen", toolName, turnId, toolCallId }`) | tui_wait.rs `Watch::apply`: 에이전트의 터미널이 자기 선택지로 답을 기다리기 시작할 때(승인 카드를 CLI로 넘김·시간 초과, 출력에 선택지가 보임)와 그 상태가 풀릴 때(`wait: null`) | `Workspace.tsx` `useAgentEvents` → `useStore.setTuiWait`: "승인 대기" 표시, 채팅 뷰·입력 바의 `TuiWaitBand`와 입력창 잠금. 늦게 뜬 창은 `list_tui_waits`로 받아요. 목록은 이 이벤트 리스너가 등록된 뒤에 묻고, 그 사이 이벤트를 받은 경로는 목록 값으로 덮지 않아요(풀린 대기가 되살아나 입력창이 잠기지 않게) |
| `agent://runtime` | `{ worktreePath, runtime: "acp" \| "pty", note: FallbackReason \| null }` (`FallbackReason` = `{ kind, …필드, message }`) | agent.rs `Agent::swap`(시작, 전환, ACP → PTY 폴백) | `Workspace.tsx` `useAgentEvents` → `useStore.setRuntime`(`Worktree.runtime`/`runtimeNote`, 기다리던 전환 표시도 지움): 헤더 이름표 툴팁과 "· 터미널". `nodes/XTerm.tsx`(채팅 런타임 에이전트의 CLI 뷰): `"acp"`면 "CLI로 전환" 띠, `"pty"`면 띠를 내려요 |
| `agent://switch` | `{ worktreePath, pending: "chat" \| "cli" \| null }` | agent.rs `Agent::switch`: 턴이 끝나기를 기다리기 시작할 때 한 번(`"chat"`/`"cli"`), 그렇게 알린 전환이 끝날 때 한 번(`null`, 교체 여부와 상관없이: superseded·busy·stuck·오류도) | `Workspace.tsx` `useAgentEvents` → `useStore.setSwitchPending`: 그 워크트리 헤더의 "턴이 끝나면 전환돼요 · 지금 중단하고 전환", CLI 뷰(`XTerm`)의 "채팅의 턴이 끝나면 CLI로 전환돼요" 띠(`"cli"`일 때). `null`이 모든 창에서 지워요(`agent://runtime`과 시작한 창의 `switch_runtime` 결과도 지워요) |
| `agent://commands` | `{ worktreePath }` | acp_runtime.rs `update`: 어댑터의 `available_commands_update`(세션 응답 뒤에 오고, MCP 서버가 붙으며 다시 올 수 있어요) → agent.rs `start`가 넘긴 `on_commands` | `nodes/AgentNode.tsx` `Composer`: 그 워크트리면 `list_slash_commands`를 다시 불러 `/` 메뉴의 "채팅에서는 안 돼요" 표시와 어댑터 전용 명령을 맞춰요 |
| `acp://install` | `{ id, kind: "line" \| "done" \| "failed", text }` | acp_setup.rs `start_install`(npm 출력 한 줄씩, 끝, 실패) | `settings/AgentsSection.tsx` `useAcpInstall`(에이전트 행마다): 최근 6줄을 관리 안 `ChatRuntimeRow`에 보여 주고, `done`/`failed`면 `loadCli()`. `failed`의 `text`는 빨간 글로, 행이 저절로 펼쳐져요. 행의 한 줄(`ChatRuntimeLine`)은 설치 중 "설치하는 중…" + [취소] |
| `maestro://merge` | `MergeRequest` (`{ id, root, maestroPath, workerPath, workerName, verified, requestedAt, branch, base, baseSha, tipSha, mergeBase, commits, commitCount, files, fileCount, adds, dels, conflictsChecked, workerUntracked, projectDirty, mode, workerDirty, remote, cmpRef, existingPr, pr, prDraft }`) | merge.rs `Merges::insert`: 마에스트로가 `maestro merge-request`를 불렀을 때. pr.rs: PR 모드 승인의 단계(`pr.step`)·오류가 바뀔 때마다 같은 요청을 다시 보내요. "다시 확인"이나 재시작이 PR을 찾았지만 이 요청의 커밋이 아직 푸시 전이면 요청은 끝나지 않고(`merge-done` 없음) `existingPr`·`pr`(애매한 시도 지움, 안내 글)을 바꿔 다시 보내요 | `Workspace.tsx` `useAgentEvents` → `useStore.addMerge`(같은 id나 같은 워커의 이전 것을 바꿈). `ApprovalCards`가 마에스트로 블록에 `MergeCard`로 그려요 |
| `maestro://merge-done` | `{ id, maestroPath, outcome }` (`merged` / `rejected` / `stale` / `conflict` / `failed` / `pr` / `recovered` / `pr-updated` / `replaced` / `withdrawn`) | merge.rs: 승인·거부가 끝났을 때(`answer`), 같은 워커의 새 요청이 왔을 때(`replaced`), 워커나 그 요청을 한 마에스트로를 폐기했을 때, 병합 방식을 바꿨을 때(`withdrawn`). pr.rs: PR이 만들어졌거나(`pr`) 찾았거나(`recovered`) 갱신됐을 때(`pr-updated`) | `Workspace.tsx` → `useStore.dropMerge(id)` |
| `maestro://pr` | `PullRequest` (pr.rs `PrRecord`: `{ root, maestroPath, workerPath, workerName, branch, base, remote, host, owner, repo, number, url, appCreated, state, isDraft, headSha, reviewDecision, mergeable, mergeStateStatus, autoMerge, queued, checks, ciNotedSha, mergeAttempt, merged, mergedHeads, afterMerge, error, notice, checkedAt, hidden, noteOwed }`; `notice`는 승인 결과처럼 사라진 요청 카드 대신 PR 카드가 보여 줄 글이나 병합 클릭의 상태처럼 오류가 아닌 안내) | pr.rs: PR을 만들었거나 찾았을 때, 조회로 바뀐 것만, 병합·거절·기준 받기 뒤, 병합 클릭이 새 머리를 봤을 때 | `Workspace.tsx` → `useStore.addPr`(워커당 하나). `ApprovalCards`의 `PrCard`, `PrChip`(워커 카드·블록 머리·접은 블록), 사이드바 |
| `maestro://pr-done` | `{ workerPath }` | main.rs: 워커를 폐기했을 때, `dismiss_pr` | `Workspace.tsx` → `useStore.dropPr(workerPath)` |
| `maestro://pr-poll` | `{ host, pausedUntil, message }` (`PrPollPayload`, `pausedUntil` 0이면 다시 조회) | pr.rs 조회: 요청 한도에 가깝거나 걸렸을 때, gh 로그인 오류, 다시 조회할 때 | `Workspace.tsx` → `useStore.setPrPoll`. `PrCard`가 "… 상태 확인을 쉬어요 (HH:MM까지)"(그 시각이 지나면 카드가 스스로 숨겨요). 끝난 쉼은 조회 차례가 시작할 때 `pausedUntil: 0`으로 알려요(그 호스트 조회가 실패하거나 남은 PR이 없어도) |
| `maestro://worktrees` | `string` (프로젝트 루트) | control.rs: `maestro worktree create`(`worktree/create`)와 `maestro ref add/remove`(`ref/add`, `ref/remove`) 처리 후. merge.rs: 병합이 끝난 뒤. pr.rs: 병합 방식을 바꿨을 때, PR 병합·기준 받기 뒤 | `Workspace.tsx` `useAgentEvents` → `useStore.refreshWorktrees(root)` |

## 제약과 알려진 문제

- `emit_state()`는 상태가 같거나 이미 `TERMINATED`면 보내지 않아요. 다만 `Agent::spawn` 직후의 `IDLE`은 `emit_state()`를 거치지 않고 바로 보내요.
- `agent://approval-done` 리스너 타입은 `{ id: number }`라서 `worktreePath`는 프런트엔드에서 쓰지 않아요.
- 이벤트를 놓친 승인 대기는 `list_approvals` 명령으로 다시 받아요(`App.tsx` 시작 시).
- `agent://event`의 live-only 이벤트 중 그리는 건 `delta`뿐이에요(R4). 나머지(승인, `exited`, `reset`)는 `useTranscript`가 목록에 넣지 않아요.
- 설치 터미널(`setup:<id>`)은 `agent://event`를 보내지 않아요(트랜스크립트가 없어요). `pty://data`의 epoch는 같은 규칙으로 올라가요.
