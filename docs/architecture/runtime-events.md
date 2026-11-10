---
title: 공통 이벤트 (RuntimeEvent)
area: architecture
status: done
files: [src-tauri/src/events.rs, src-tauri/src/transcript.rs, src-tauri/src/claude.rs, src-tauri/src/antigravity.rs, src-tauri/src/stream.rs, src-tauri/src/runtime.rs, src/types.ts, src/chatFold.ts, scripts/chat-fold-parity.ts, src-tauri/tests/fixtures/chat-parity.json]
related: [architecture/agent-runtime.md, reference/events.md, features/agents/chat-view.md, features/agents/approvals.md, features/orchestration/maestro-cli.md]
updated: 2026-11-10
---

# 공통 이벤트 (RuntimeEvent)

## 요약

에이전트 대화의 한 걸음을 런타임과 상관없이 같은 모양으로 나타낸 타입이에요(PRD R3). PTY 런타임은 CLI 트랜스크립트에서, ACP 런타임(R4)은 `session/update`에서 이 이벤트를 만들어요. 채팅 뷰, `maestro read`, 워커 완료 메모, 카드 요약이 모두 이것만 읽어요. Rust 정의는 `src-tauri/src/events.rs`의 `pub enum RuntimeEvent`, TS 정의는 `src/types.ts`의 `RuntimeEvent`예요.

## 구현

### 모양

- JSON은 `{ "type": "<camelCase 이름>", …필드(camelCase) }`예요(`#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]`).
- `agent://event` 페이로드는 `Envelope { worktreePath, epoch, seq, event }`, `get_transcript` 결과는 `Transcript { epoch, seq, events }`예요.
- 턴 안의 이벤트는 모두 `turnId`가 있어요. 세션 수준 이벤트(`exited`, `reset`, 열린 턴이 없을 때의 승인·오류, 턴 밖의 `unknown`)만 `null`일 수 있어요.

### durable(기록으로 다시 만들 수 있음, `seq` 있음)

| type | 필드 | 뜻 |
|---|---|---|
| `turnStarted` | `turnId`, `initiator: "user" \| "agent"` | 턴 시작. 프롬프트 없이 나온 출력은 `agent` |
| `userMessage` | `turnId`, `itemId`, `kind: "prompt" \| "command" \| "shell"`, `text`, `files` | 사용자 입력. 슬래시 명령은 `command`, `!`는 `shell` |
| `agentText` | `turnId`, `itemId`, `messageId`, `text` | 글 블록 하나. 같은 `itemId`면 덮어써요(upsert) |
| `thinking` | `turnId`, `itemId`, `messageId`, `text` | 생각 과정 |
| `toolStarted` | `turnId`, `toolCallId`, `messageId`, `name`, `input` | 도구 호출 시작 |
| `toolUpdated` | `turnId`, `toolCallId`, `status?`, `title?`, `input?`, `output?` | ACP `tool_call_update`(없는 필드는 그대로). PTY는 안 보내요 |
| `toolFinished` | `turnId`, `toolCallId`, `output`, `isError` | 도구 결과 |
| `notice` | `turnId`, `itemId`, `kind: "localCommand" \| "interrupted"`, `text` | 흐린 안내 줄. 글은 최종본("중단됨"도 Rust가 만들어요) |
| `error` | `turnId?`, `kind`, `message` | 치명적이지 않은 오류(Claude `system/api_error` 재시도 알림, `kind: "apiRetry"`) |
| `turnEnded` | `turnId`, `outcome` | 턴 끝 |
| `unknown` | `turnId?`, `source`, `raw` | 모르는 내용을 버리지 않고 담아요. `raw`가 2KB를 넘으면 `{ type, truncated: true }` |

### live-only(`seq: null`, 기록에 없음)

| type | 필드 | 뜻 |
|---|---|---|
| `delta` | `turnId`, `itemId`, `channel: "text" \| "thinking" \| "toolOutput"`, `offset`, `text` | 글 조각. Rust가 ~50ms씩 묶어요. `offset` = 이 조각 앞까지 그 항목 글의 UTF-16 길이(같은 조각이 두 번 와도 결과가 같아요). 이어지는 조각(`offset` = 앞 조각 끝)만 하나로 묶어요 |
| `approvalRequested` | `turnId?`, `toolCallId?`, `approvalId`, `toolName`, `toolInput`, `canAlways` | 승인 요청 |
| `approvalResolved` | `turnId?`, `toolCallId?`, `approvalId`, `resolution` | `allow` / `always` / `deny` / `cli` / `timeout` / `withdrawn` |
| `exited` | `turnId?` | 프로세스가 끝남. 열려 있던 턴도 끝난 것으로 봐요 |
| `reset` | `reason` | epoch가 바뀜: 가진 것을 버리고 다시 받으세요. `restart` / `source` / `truncated` / `switch` / `fallback` / `replay` |

### TurnOutcome(`outcome.kind`)

| kind | 필드 | 언제 |
|---|---|---|
| `done` | `stopReason` | 모델이 끝냄(`end_turn`, `max_tokens`, …; ACP `stopReason`) |
| `failed` | `error`, `message` | API 오류로 끝남(Claude `isApiErrorMessage`, `error: "rate_limit"` 등; ACP prompt 오류) |
| `cancelled` | | Esc, `[Request interrupted`, ACP `cancelled` |
| `command` | | 슬래시·`!` 명령의 출력이 끝남 |
| `replaced` | | 턴이 닫히기 전에 새 프롬프트가 열림 |

### id 규칙(결정적: 파일 내용과 줄 번호에서만)

**Claude**

| id | 규칙 |
|---|---|
| turnId | 여는 user 줄의 `promptId`, 없으면 `uuid`, 없으면 `l<줄 번호>`. 에이전트가 연 턴은 `a:<uuid>`. 이미 쓴 id가 다시 오면 `~1`, `~2`… |
| userMessage itemId | 줄 `uuid` |
| agentText / thinking itemId | `<uuid>#<블록 번호>` |
| messageId | 줄 `uuid`(Claude는 블록마다 한 줄을 쓰고, 예전 `maestro read`·요약도 줄 단위였어요) |
| toolCallId | `tool_use.id` |
| notice itemId | 줄 `uuid` |

**Antigravity**

| id | 규칙 |
|---|---|
| turnId | `agy-t<step>`, 에이전트가 연 턴은 `agy-a<step>` |
| userMessage itemId | `agy-<step>` |
| agentText itemId | `agy-<step>#text`(생각은 `#thinking`) |
| messageId | `agy-<step>` |
| toolCallId | `agy-<step>-<i>` |

**ACP(R4 예정):** turnId는 `prompt()`에서 `p:<n>`, 다시 읽은(replay) 턴은 `r:<번호>`, toolCallId = `toolCallId`, messageId = `messageId`.

### 변환 규칙

**Claude(`ClaudeNormalizer`)**: `user`, `assistant`, `system` 줄이 대화예요. 실제 기록에서 본 세션 관리 줄(`BOOKKEEPING`: `attachment`, `queue-operation`, `file-history-snapshot`/`-delta`, `summary`, `custom-title`, `ai-title`, `agent-name`, `last-prompt`, `progress`, `mode`, `permission-mode`, `cost-state`, `tools-used` 등)과 `isMeta` user 줄은 건너뛰어요. 그 밖의 줄 type(나중 Claude가 새로 넣는 것 포함)은 버리지 않고 `unknown`이에요(PRD R3-2).
- `user`(`isMeta`가 아닌 것), 문자열 내용:
  - `<command-name>` → 턴 열기 + `userMessage(command, "이름 인자")`
  - `<local-command-stdout/stderr>` → `notice(localCommand)` + `turnEnded(command)`. 출력이 비면 아무것도 없어요(턴은 열린 채)
  - `<bash-input>` → 턴 열기 + `userMessage(shell, "!명령")`
  - `<bash-stdout/stderr>` → `notice(localCommand, 출력 또는 "(출력 없음)")` + `turnEnded(command)`
  - 그 밖 → 턴 열기 + `userMessage(prompt)`. `[Request interrupted`로 시작해도 문자열이면 프롬프트예요
- `user`, 배열 내용:
  - `tool_result`가 있으면 블록마다 `toolFinished`(턴은 그 도구의 주인 턴). 결과는 문자열, 또는 `content[].text`를 `\n`으로 이은 것. 옆에 붙은 다른 블록(글, 이미지)은 제자리에서 `unknown`
  - 글을 이은 것이 `[Request interrupted`로 시작 → `notice(interrupted, "중단됨")` + `turnEnded(cancelled)`
  - 글이 있으면 턴 열기 + `userMessage(prompt)`
  - 글이 있든 없든 글 없는 블록(이미지 등)마다 `unknown`. 이미지만 보낸 프롬프트는 `unknown`만 나와요(R3 이전처럼 말풍선은 없어요)
- `assistant`: 열린 턴(없으면 straggler 규칙, 아니면 `a:<uuid>` 턴 열기). 블록마다 text → `agentText`(빈 글도), thinking → `thinking`, tool_use → `toolStarted`, 그 밖 → `unknown`. `stop_reason`이 null도 `tool_use`도 아니고 턴이 열려 있으면 닫아요: `isApiErrorMessage`면 `failed`, 아니면 `done`.
- `system`: `local_command` → stdout 줄과 같아요. `api_error` → `error(apiRetry)`. `stop_hook_summary`, `turn_duration` → 건너뜀. 그 밖 → `unknown`.
- 열린 턴 없이 온 `notice`는 에이전트 턴을 하나 열고 닫아요. 그래서 모든 `notice`에 턴이 있어요.

**Antigravity(`AgyNormalizer`)**
- `USER_INPUT` → `<USER_REQUEST>` 안의 글로 턴 열기 + `userMessage`. 대기 중 도구 목록을 비워요.
- `PLANNER_RESPONSE`, 글도 도구도 없음 → `thinking`이 있으면 `thinking`만(턴을 열지도 닫지도 않아요).
- `PLANNER_RESPONSE`, 그 밖 → 턴 보장, `thinking`, `agentText`, 도구마다 `toolStarted`. 도구가 없으면 `turnEnded(done, "end_turn")`.
- `GENERIC` → 대기 중인 도구의 `toolFinished`(`status == "ERROR"`면 오류). 대기 중인 도구가 없으면(사용자가 도구 실행 중에 입력해 목록이 비었을 때 등) 버리지 않고 `unknown`이에요.
- `SYSTEM_MESSAGE` → 없음. 그 밖 → `unknown`.

### TurnTracker(transcript.rs)

- `open_user(key)`: 열린 턴과 같은 key(Claude `promptId` 반복)면 아무것도 안 해요. 아니면 열린 턴을 `replaced`로 닫고 새 턴을 열어요.
- `ensure(msg_id, synth)`: 열린 턴, 아니면 마지막 턴을 닫은 메시지의 블록이면 그 턴(**straggler**: 다시 열지도 두 번 닫지도 않아요), 아니면 `turnStarted(agent)`.
- `end(outcome, closing_msg)`: 열려 있을 때만. 열린 도구 목록을 비우고 닫은 메시지를 기억해요.
- `locate(name, input) -> (turn_id, tool_call_id, exact)`: 열린 도구 중 이름·입력이 같은 가장 최근 것(`exact`), 없으면 이름만 같은 가장 최근 것(짐작). `LiveTail::locate`는 짐작이면 그 턴만 쓰고 `tool_call_id`는 `None`으로 줘요. 승인 이벤트의 `toolCallId`는 확실할 때만 있어요.
- `owner(tool_call_id)`: 그 호출이 시작된 턴(시작 줄을 읽은 뒤에만). 프롬프트가 id를 가지고 오면(ACP `request_permission`) 이름으로 찾지 않고 이걸 써요.

### seq의 출처

`seq` n은 `runtime::history(live, …)`가 주는 목록의 n번째(1부터)예요. 그 목록은 런타임이 정해요.

| 런타임 | `history()` 소스 | 비고 |
|---|---|---|
| PTY | CLI 트랜스크립트를 처음부터 다시 읽은 것(`transcript_history`, `Runtime::history()` = `None`) | 꼬리(`LiveTail`)가 같은 `Normalize`로 같은 순서를 만들고 `prime(n)`으로 이어 붙여요 |
| ACP(R4) | 이번 epoch에 내보낸 durable 이벤트를 쌓은 목록(`Runtime::history()` = `Some((epoch, events))`, 스트림의 epoch와 같을 때만 써요). 런타임이 따로 쌓지 않고 `Emitter::with_record`가 seq를 매기는 잠금 안에서 쌓아요(`Emitter::history()`) | `p:<n>`·messageId id, upsert 쌍, `session/load` 재생은 트랜스크립트로 다시 만들 수 없어요. ACP 전용 에이전트는 트랜스크립트가 없어요 |

실행 중인 런타임이 없으면 트랜스크립트를 읽어요. ACP 에이전트가 꺼져 있을 때의 기록은 R4에서 정해요.

### 읽는 쪽의 upsert

`agentText`가 같은 `itemId`로 다시 오면(ACP: 빈 글로 열고 경계에서 전체 글) 블록 하나예요. 처음 자리에 마지막 글을 둬요. chatFold.ts, `events::last_answer`(`maestro read`, 워커 완료 메모), `events::recent_turn_text`(카드 요약)가 모두 이렇게 읽어요. Claude·agy는 `itemId`를 되풀이하지 않아 결과가 R3 이전과 같아요.

### 화면 fold(chatFold.ts)

`userMessage` → 사용자 말풍선, `agentText` → `itemId`로 넣거나 바꾸기, `delta(text)` → `offset` 자리에 이어 붙이기, `toolStarted` → 도구 줄(`toolCallId`로 기억), `toolUpdated` → 입력·결과 고치기, `toolFinished` → 결과·오류 표시(짝이 없으면 버림), `notice` → 흐린 줄. 마지막에 빈 글 항목을 빼요. `Item` 타입은 R3 이전과 같아요. `npm test`가 이 `fold`를 Rust 테스트가 적은 픽스처(`src-tauri/tests/fixtures/chat-parity.json`: 샘플 기록의 공통 이벤트와 R3 이전 fold의 항목)에 대 봐요.

### ACP 대응(R4 참고)

| ACP | 공통 이벤트 |
|---|---|
| `session/prompt` 시작 | `turnStarted(user)` + `userMessage` |
| `agent_message_chunk` | `delta(text)`, 경계에서 `agentText`(같은 `itemId`) |
| `agent_thought_chunk` | `delta(thinking)` / `thinking` |
| `tool_call` | `toolStarted` |
| `tool_call_update` | `toolUpdated`, 완료면 `toolFinished` |
| `session/request_permission` | `Approvals::request` → `approvalRequested` / `approvalResolved` |
| prompt 응답 `stopReason` | `turnEnded(done)`, `cancelled`면 `turnEnded(cancelled)`, 오류면 `failed` |
| `session/load` 재생 | `reset(replay)` 뒤 `r:<번호>` 턴들 |

## 제약과 알려진 문제

- 받는 쪽(인터페이스 마이그레이션의 리듀서)은 닫힌 턴에 붙는 항목(straggler 블록), `turnStarted(agent)`가 Running 없이 오는 것, `exited`가 열린 턴을 닫는 것을 받아들여야 해요.
- `turnStarted`, `thinking`, `unknown`, `error`도 durable이라 채팅 이벤트 수(`events.length`)에 들어가요. 화면에는 안 그려요.
- TS 유니온에는 "모르는 type" 갈래를 두지 않았어요(넣으면 `switch`의 타입 좁히기가 안 돼요). 새 type은 fold의 `switch`가 그냥 지나쳐요.

## 결정 기록

- Emdash의 결과 enum·도구 주인 턴, Paradev의 델타 병합·flush-before-durable·upsert 모델에서 생각을 가져왔어요. 코드는 베끼지 않았어요(THIRD_PARTY_NOTICES.md 변경 없음).
- `messageId`를 Claude 줄 `uuid`로 둔 이유: 예전 `maestro read`(`last_answer`)와 요약(`recent_turn_text`)이 줄 단위로 동작했어요. `message.id`로 묶으면 바이트가 달라져 요약 캐시가 다 다시 만들어져요.
