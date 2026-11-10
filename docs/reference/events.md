---
title: 이벤트 목록
area: reference
status: done
files: [src-tauri/src/agent.rs, src-tauri/src/approval.rs, src-tauri/src/control.rs, src/App.tsx, src/SettingsDialog.tsx, src/nodes/AgentNode.tsx, src/nodes/XTerm.tsx, src/types.ts, src-tauri/src/events.rs, src-tauri/src/stream.rs, src-tauri/src/pty.rs, src-tauri/src/transcript.rs]
related: [reference/tauri-commands.md, architecture/overview.md, architecture/runtime-events.md]
updated: 2026-11-10
---

# 이벤트 목록

## 요약

Rust 백엔드가 `app.emit(...)`으로 보내고 프런트엔드가 `listen<...>`으로 받는 Tauri 이벤트 전체 목록이에요. 이벤트는 6개이고 모두 앱 전체(전역)로 보내요. 채팅 이벤트는 런타임과 상관없는 공통 이벤트예요. → [공통 이벤트](../architecture/runtime-events.md) 페이로드 필드는 `#[serde(rename_all = "camelCase")]`라서 JS에서 camelCase로 받아요.

## 이벤트

| 이름 | 페이로드 | 보내는 곳 | 받는 곳 |
|---|---|---|---|
| `agent://state` | `{ worktreePath, state: { kind: "RUNNING" \| "IDLE" \| "TERMINATED" } }` (`AgentStatePayload`) | agent.rs `emit_state()`: 채팅 전송 시 `Agent::send` → `RUNNING`, 스트림의 observer가 durable 이벤트마다 `events::status_after`(`userMessage` → `RUNNING`, `notice`·`turnEnded`(replaced 빼고) → `IDLE`), 프로세스 종료 감시(pty.rs) → `TERMINATED`. 그리고 `Agent::spawn` 직후 `IDLE`을 직접 보내요 | `App.tsx` → `useStore.onAgentState`(상태·읽지 않음 표시). `SettingsDialog.tsx` → 설치 터미널(`setup:<id>`)이 `TERMINATED`면 `loadCli()`로 설치 상태 재확인 |
| `agent://event` | `Envelope { worktreePath, epoch, seq, event: RuntimeEvent }` (`AgentEventPayload`). `seq`는 durable 이벤트의 기록 속 번호(1부터), live-only 이벤트(`delta`, `approvalRequested`, `approvalResolved`, `exited`, `reset`)는 `null` | stream.rs `Emitter`: PTY 런타임의 `LiveTail`(250ms마다, Stop 훅이면 바로 트랜스크립트의 새 줄을 공통 이벤트로), `reset`(재시작·소스 전환·파일 잘림), `exited`(프로세스 종료), 승인(approval.rs `Approvals::request`) | `nodes/AgentNode.tsx` `useTranscript`: 경로가 같고 epoch가 같으면 `seq`가 더 큰 durable 이벤트만 추가. 더 새 epoch면 `ipc.transcript()`로 다시 받아요. 옛 epoch와 `seq: null`은 버려요 |
| `pty://data` | `{ worktreePath, dataB64, seq, epoch }` (`PtyDataPayload`). `seq`는 지금까지의 총 바이트 수, `epoch`는 이 키의 몇 번째 PTY인지(시작마다 +1) | pty.rs `PtyRuntime::start`: 시작 직후 빈 배치(`dataB64: ""`, `seq: 0`) 한 번, 그 뒤 출력 스레드(16ms 또는 64KB 단위로 묶어서) | `nodes/XTerm.tsx`: 경로가 같고 epoch가 같으면 `seq` 기준으로 중복을 버리고 씀. 더 새 epoch면 화면을 지우고 `ptySnapshot`부터 다시 그려요 |
| `agent://approval` | `Approval`: `{ id, worktreePath, toolName, toolInput, canAlways, turnId, toolCallId }` (`turnId`·`toolCallId`는 짐작이라 `null`일 수 있어요) | approval.rs `Approvals::request`: 에이전트의 `PermissionRequest` 훅이 로컬 서버 `/permission/<session>`으로 요청했을 때. 같은 내용이 `agent://event`에 `approvalRequested`로도 가요 | `App.tsx` → `useStore.addApproval` |
| `agent://approval-done` | `{ id, worktreePath }` | approval.rs `Approvals::request`: 사용자가 답했거나, 시간 초과(590초)거나, 훅 쪽 연결이 끊겼을 때. `agent://event`에는 `approvalResolved { resolution }`으로도 가요 | `App.tsx` → `useStore.dropApproval(id)` (`id`만 써요) |
| `maestro://worktrees` | `string` (프로젝트 루트) | control.rs: `maestro worktree create`(`worktree/create`)와 `maestro ref add/remove`(`ref/add`, `ref/remove`) 처리 후 | `App.tsx` → `useStore.refreshWorktrees(root)` |

## 제약과 알려진 문제

- `emit_state()`는 상태가 같거나 이미 `TERMINATED`면 보내지 않아요. 다만 `Agent::spawn` 직후의 `IDLE`은 `emit_state()`를 거치지 않고 바로 보내요.
- `agent://approval-done` 리스너 타입은 `{ id: number }`라서 `worktreePath`는 프런트엔드에서 쓰지 않아요.
- 이벤트를 놓친 승인 대기는 `list_approvals` 명령으로 다시 받아요(`App.tsx` 시작 시).
- `agent://event`의 live-only 이벤트는 R3에서 그리는 곳이 없어요. `useTranscript`는 목록에 넣지 않아요(이벤트 수가 바뀌지 않게).
- 설치 터미널(`setup:<id>`)은 `agent://event`를 보내지 않아요(트랜스크립트가 없어요). `pty://data`의 epoch는 같은 규칙으로 올라가요.
