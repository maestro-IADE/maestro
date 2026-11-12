---
title: 공통 이벤트 (RuntimeEvent)
area: architecture
status: done
files: [src-tauri/src/events.rs, src-tauri/src/transcript.rs, src-tauri/src/acp_runtime.rs, src-tauri/src/claude.rs, src-tauri/src/antigravity.rs, src-tauri/src/stream.rs, src-tauri/src/runtime.rs, src/types.ts, src/chatFold.ts, scripts/chat-fold-parity.ts, scripts/chat-reduce-check.ts, src-tauri/tests/fixtures/chat-parity.json]
related: [architecture/agent-runtime.md, reference/events.md, features/agents/chat-view.md, features/agents/approvals.md, features/orchestration/maestro-cli.md]
updated: 2026-11-13
---

# 공통 이벤트 (RuntimeEvent)

## 요약

에이전트 대화의 한 걸음을 런타임과 상관없이 같은 모양으로 나타낸 타입이에요(PRD R3). 두 런타임 모두 durable 이벤트는 CLI 트랜스크립트에서 만들어요. ACP 런타임(R4)은 여기에 `session/update`의 실시간 글(`delta`)과, 트랜스크립트가 말하지 않은 턴 끝을 더해요. 채팅 뷰, `maestro read`, 워커 완료 메모, 카드 요약이 모두 이것만 읽어요. Rust 정의는 `src-tauri/src/events.rs`의 `pub enum RuntimeEvent`, TS 정의는 `src/types.ts`의 `RuntimeEvent`예요.

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
| `notice` | `turnId`, `itemId`, `kind: "localCommand" \| "interrupted" \| "failed"`, `text` | 안내 줄. 글은 최종본("중단됨"도 Rust가 만들어요). `failed`는 런타임이 실패로 끝낸 턴의 이유(ACP만)라 빨간 줄이고, 상태 점을 바꾸지 않아요 |
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
| `reset` | `reason` | epoch가 바뀜: 가진 것을 버리고 다시 받으세요. `restart` / `source` / `truncated` / `switch`(채팅↔CLI 전환) / `fallback`(ACP → PTY) |

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

**ACP(R4):** durable 이벤트는 트랜스크립트에서 나와서 id가 위 Claude 규칙과 같아요. 런타임이 직접 만드는 것만 다른 id를 써요.
- 실시간 글 항목: `live:<n>:<k>`(n = 프롬프트 번호, k = 조각 묶음 번호). 기록에 남지 않아요.
- 트랜스크립트에 닿지 못한 프롬프트: 턴 `p:<n>`, 말풍선 `p:<n>:u`.
- 런타임이 만든 턴 끝의 알림: `<turnId>:acp`("중단됨", 실패 이유 `notice(failed)`), 어댑터만 보여 준 출력의 알림: `<turnId>:acp-out`.

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
| ACP | epoch가 시작될 때 트랜스크립트 전체로 채운 기록(`Emitter::reset_seeded`, `LiveTail::start_seeded`), 그 뒤 tail이 읽은 이벤트와 런타임이 만든 이벤트(트랜스크립트에 없는 프롬프트, 턴 끝, 알림)를 붙인 것. `Runtime::history()` = `Some((epoch, events))`, 스트림의 epoch와 같을 때만 써요. `Emitter::with_record`가 seq를 매기는 잠금 안에서 쌓아요 | 기록 길이 = seq. 채운 이벤트는 observer(상태 점, 워커 완료)를 지나지 않고 `reset` 하나만 나가요 |

실행 중인 런타임이 없으면 트랜스크립트를 읽어요. ACP 에이전트가 꺼져 있을 때도 같아요: 트랜스크립트가 기록이에요. 런타임이 만든 이벤트는 다음 시작부터 보이지 않아요(PTY의 TUI 전용 명령이 기록에 안 남는 것과 같아요).

### 읽는 쪽의 upsert

`agentText`가 같은 `itemId`로 다시 오면(ACP: 빈 글로 열고 경계에서 전체 글) 블록 하나예요. 처음 자리에 마지막 글을 둬요. chatFold.ts(`reduceChat`), `events::last_answer`(`maestro read`), `events::turn_answer`(워커 완료/실패 메모, 그 턴만), `events::recent_turn_text`(카드 요약)가 모두 이렇게 읽어요. Claude·agy는 `itemId`를 되풀이하지 않아 결과가 R3 이전과 같아요.

### 화면 모델(chatFold.ts)

화면은 공통 이벤트를 리듀서 `reduceChat(상태, { epoch, seq, event, at })`로 하나씩 접어요(인터페이스 PRD I1). 기록은 `loadChat`이 같은 규칙으로 한 번에 접어요. 항목(`ChatItem`)은 user, text, thinking, tool, approval, error, notice, turnStart·turnEnd, unknown이에요.

| 이벤트 | 항목 |
|---|---|
| `turnStarted` | `turnStart`(턴 경계). 그 전 턴의 항목은 `turns`의 배열 하나로 굳어요 |
| `userMessage` | `user` |
| `agentText`, `thinking` | `itemId`로 넣거나 바꾸기(`text`, `thinking`) |
| `delta`(글, 생각) | 실시간 항목(`live: true`)의 `offset` 자리에 이어 붙이기. 항목은 `turnId`와 상관없이 모든 턴에서 `itemId`로 찾아요(시작한 턴에 남아요). 빈 `offset: 0` 델타는 그 항목을 빼요. `toolOutput`은 아직 버려요 |
| `toolStarted` | `tool` |
| `toolUpdated`, `toolFinished` | `toolCallId`로 찾아 입력·제목·상태·결과·오류 고치기(짝이 없으면 버림) |
| `notice`, `error`, `unknown` | 같은 이름의 항목(`unknown`은 `raw` 그대로) |
| `turnEnded` | `turnEnd`. 시작과 끝을 둘 다 실시간으로 받았으면 `ms`(받은 시각의 차) |
| `approvalRequested`, `approvalResolved` | `approvalId`로 `approval` 넣기, `resolution` 달기 |
| `exited`, `reset` | 없음 |
| 모르는 type | `unknown`(`source: "event:<type>"`, 이벤트 통째로) |

- epoch가 더 작으면 그대로, 더 크면 빈 상태에서 다시 시작해요. `seq`가 받은 것 이하인 durable 이벤트는 건너뛰어요. live-only 이벤트(`seq: null`)는 `seq`를 옮기지 않아요. 아무것도 바뀌지 않으면 같은 상태 객체를 돌려줘요(다시 그리지 않아요).
- 찾기는 끝에서부터예요. 이벤트의 `turnId`가 마지막 턴이면 그 턴(`tail`)만 보고, 아니면(지난 턴의 straggler, `turnId` 없음) 지난 턴까지 봐요. 실시간 글·생각 항목은 늘 지난 턴까지 봐요: 프롬프트 없이 에이전트가 시작한 ACP 턴의 실시간 글은 앞 턴(`p:<n>`)으로 나가고, 그 턴의 `turnStarted`(트랜스크립트에서 읽음)가 같은 항목의 델타 사이에 올 수 있어요. 마지막 턴만 보면 뒤 델타가 같은 id의 둘째 항목을 만들고, 지울 때 앞 조각이 남아요.
- 지난 턴은 닫힐 때 한 번 만든 표시(`marks`: 턴마다 항목의 종류 + id 집합)로 걸러서, 찾는 항목이 있을 수 있는 턴만 열어요. 바뀐 배열만 복사해요: 마지막 턴의 이벤트는 지난 턴의 배열도, 그 목록도 건드리지 않아요. 그래서 이벤트 하나의 일은 마지막 턴의 크기에, 지난 턴까지 볼 때는 턴 수(표시 확인)와 그 항목이 있는 턴 하나의 크기를 더한 만큼이에요. 전체 항목을 훑는 경우는 없어요. `turnStarted`는 턴 목록을 복사해요(턴 수만큼).
- 화면이 그리는 것은 I2 전까지 R3 이전과 같아요(`isDrawn`: 사용자, 빈 글이 아닌 text, 도구, notice. `failed` notice는 빨간 줄). `turnEnded`는 그리지 않아요: Claude의 API 오류는 이미 그 턴의 `agentText`예요.
- `npm test`가 Rust 테스트가 적은 픽스처(`src-tauri/tests/fixtures/chat-parity.json`: 샘플 기록의 공통 이벤트와 R3 이전 fold의 항목)를 한 번에(`loadChat`)와 하나씩(`reduceChat`) 접어, 그리는 항목이 픽스처와 같고 두 길의 항목이 같은지 봐요(`scripts/chat-fold-parity.ts`). 리듀서 자체는 `scripts/chat-reduce-check.ts`가 봐요(델타 이어 붙이기, `turnStarted`를 사이에 둔 실시간 항목, 지난 턴 표시, 도구 갱신, epoch 초기화, 중복 seq, 모르는 이벤트 보존, 지난 턴 공유, 입력 상태 불변).

### ACP 런타임 매핑

트랜스크립트와 ACP를 섞어요. 항목은 트랜스크립트가 정하고, ACP는 실시간 글과 턴 제어를 줘요.

| ACP | 공통 이벤트 |
|---|---|
| `session/prompt` 시작 | 없음. 트랜스크립트의 사용자 줄이 `turnStarted(user)` + `userMessage`를 만들어요 |
| `agent_message_chunk` | `delta(text)`, 항목 `live:<n>:<k>`. 50ms로 묶여 나가요(stream.rs). 트랜스크립트의 `agentText`가 오기 직전 `delta { offset: 0, text: "" }`로 지워요 |
| `agent_thought_chunk` | `delta(thinking)`(화면은 안 그려요). 트랜스크립트의 `thinking` 직전에 지워요 |
| `tool_call` | 없음(tail 깨우기). 트랜스크립트의 `toolStarted`가 와요 |
| `tool_call_update` | 없음(완료면 tail 깨우기). 트랜스크립트의 `toolFinished`가 와요. `toolUpdated`는 쓰지 않아요 |
| `session/request_permission` | `Approvals::request` → `approvalRequested` / `approvalResolved` |
| prompt 응답 `stopReason` | 트랜스크립트가 턴을 닫았으면 없음. 2초 안에 안 닫으면 `turnEnded(done)`, `cancelled`면 `notice(interrupted)` + `turnEnded(cancelled)`, 오류면 `notice(failed, 이유)` + `turnEnded(failed)`(이유: `transcript::failure_note`) |
| prompt가 트랜스크립트에 안 닿음 | `turnStarted(user)` + `userMessage(p:<n>:u)` + 위의 끝 |
| `session/load` 리플레이 | 버려요. 트랜스크립트가 이미 기록이에요(`reset(switch 또는 restart)` 뒤 다시 받기) |
| 어댑터 종료 | `exited { turnId }` |

**실시간 글 규칙:**
- 이 프롬프트의 `userMessage`가 트랜스크립트에서 나올 때까지 실시간 글을 쌓아만 둬요. 그래서 글이 말풍선 위에 그려지지 않아요.
- 지울 항목: 같은 채널에서 아직 살아 있는 가장 오래된 항목 중 앞뒤 공백을 뺀 글이 완성된 블록의 앞부분이거나 그 반대인 것(스트리밍이 파일보다 늦거나 빠를 수 있어요). 맞는 게 없으면 그대로 둬요.
- 프롬프트의 턴이 끝나면(응답 뒤 트랜스크립트를 다시 읽고 나서) 남은 항목을 모두 지워요. 트랜스크립트의 `turnEnded`에서는 지우지 않아요: Claude가 `end_turn`을 생각 줄에 달고 글 줄을 그 뒤에 쓰기 때문이에요(실측, straggler). 이 프롬프트에 트랜스크립트 글이 없었는데 남은 글이 있으면 `notice(localCommand)`로 남겨요(어댑터만 보여 준 명령 출력 등).
- 화면 쪽: 리듀서(chatFold.ts)가 `seq: null`인 `delta`를 실시간 항목(`live: true`, 글이나 생각)으로 그리고, 그 항목의 퇴장 델타가 오면 항목을 빼요. durable `turnEnded`로는 빼지 않아요: 생각 줄의 `end_turn`이 답 글 줄보다 먼저 읽히면 턴 끝이 답 중간에 오고, 그 뒤 델타는 자기 offset부터의 나머지만 실어요. `npm test`가 "흐르는 동안 보임 → 지운 뒤 완성 블록만 → 생각 델타는 안 그림 → 답 중간의 `turnEnded`는 글을 남김"을 확인해요.

## 제약과 알려진 문제

- 리듀서는 닫힌 턴에 붙는 항목(straggler 블록)을 받은 자리(지금 턴의 끝)에 두고, 지난 턴의 도구·글·실시간 항목은 id로 찾아 그 자리에서 고쳐요. `turnStarted(agent)`도 턴 경계예요. `exited`는 항목을 만들지 않아요(턴 끝 항목이 없는 턴으로 남아요).
- `turnStarted`, `thinking`, `unknown`, `error`도 durable이라 `ChatState.durable`에 세요. 항목은 있지만 I2 전에는 그리지 않아요.
- 실시간 글 `delta`는 `durable`을 바꾸지 않아요. 이벤트 수를 계기로 쓰는 곳은 이 수를 써요(카드 요약: WorktreeCard.tsx). 턴 끝 뒤에도 델타가 올 수 있어요.
- TS 유니온에는 "모르는 type" 갈래를 두지 않았어요(넣으면 `switch`의 타입 좁히기가 안 돼요). 새 type은 리듀서의 `default`가 `unknown` 항목으로 남겨요.
- 승인 항목은 실시간으로 본 것만 있어요(기록에 없어요). 다시 받으면(epoch, 다시 열기) 사라져요. 승인 카드는 지금처럼 스토어의 승인 목록으로 그려요.
- 턴 소요 시간(`ms`)은 시작과 끝을 둘 다 실시간으로 받은 턴에만 있어요. 이벤트에 시각이 없어서 기록으로 다시 받은 턴에는 없어요.
- 지난 턴의 항목을 고치는 이벤트(straggler)와 처음 오는 `p:<n>` 실시간 항목은 지난 턴까지 찾아서 그 한 번은 항목 수만큼 걸려요. 턴이 시작될 때 지난 턴 목록(턴 수만큼의 참조)을 한 번 복사해요.
- `toolUpdated`는 지금 어느 런타임도 보내지 않아요(ACP도 트랜스크립트의 `toolFinished`를 써요). 트랜스크립트가 없는 런타임(agy acp, Codex)을 위해 남겨 뒀어요.

## 결정 기록

- Emdash의 결과 enum·도구 주인 턴, Paradev의 델타 병합·flush-before-durable·upsert 모델에서 생각을 가져왔어요. 코드는 베끼지 않았어요(THIRD_PARTY_NOTICES.md 변경 없음).
- ACP에서도 트랜스크립트가 기록인 이유(R4): 실시간 항목과 다시 읽은 항목이 같아야 해요(채팅, `maestro read`, 요약 캐시 키). → [에이전트 실행 방식](agent-runtime.md#결정-기록)
- `ResetReason::Replay`를 없앤 이유(R4): load 리플레이를 버려서 쓰는 곳이 없어요.
- 2026-11-13 (인터페이스 I1): 화면이 이벤트마다 목록 전체를 다시 접던 `fold(events)`를 순수 리듀서 `reduceChat`으로 바꿨어요. 항목은 지난 턴마다 배열 하나(`turns`)와 마지막 턴(`tail`)으로 나눠 뒀어요. 평평한 배열 하나면 불변 상태라 이벤트마다 배열 전체를 복사해야 해서 항목 수에 비례해요. 턴으로 나누면 마지막 턴의 이벤트는 그 턴 배열만 복사하고, 화면도 지난 턴을 다시 그리지 않아요(`Turn` memo). 키 → 위치 색인(Map)은 두지 않았어요: 불변으로 두려면 새 항목마다 색인을 복사해야 하고, 찾는 대상은 거의 마지막 턴에 있어요. `addDelta`(델타를 이벤트 목록에 붙이던 것)는 리듀서의 실시간 항목으로 대체돼 없앴어요.
- `messageId`를 Claude 줄 `uuid`로 둔 이유: 예전 `maestro read`(`last_answer`)와 요약(`recent_turn_text`)이 줄 단위로 동작했어요. `message.id`로 묶으면 바이트가 달라져 요약 캐시가 다 다시 만들어져요.
