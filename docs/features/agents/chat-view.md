---
title: 채팅 뷰
area: agents
status: done
files: [src/nodes/AgentNode.tsx, src/ime.ts, src/nodes/ApprovalCards.tsx, src/nodes/ToolDiff.tsx, src/toolPreview.ts, scripts/tool-preview-check.ts, src/AgentFull.tsx, src/store.ts, src-tauri/src/tui_wait.rs, src-tauri/src/agent.rs, src/chatFold.ts, scripts/chat-fold-parity.ts, src-tauri/src/events.rs, src-tauri/src/runtime.rs, src-tauri/src/transcript.rs]
related: [features/agents/chat-runtime.md, architecture/agent-runtime.md, features/agents/cli-view.md, features/agents/approvals.md, features/agents/slash-commands.md, features/editor/drag-attach.md, architecture/runtime-events.md]
updated: 2026-11-13
---

# 채팅 뷰

## 요약

채팅 뷰는 실제 CLI 위에 얹힌 대화 화면이에요. CLI가 쓰는 트랜스크립트를 읽어 말풍선과 도구 실행 줄로 보여 주고, 입력은 CLI에 붙여넣기로 보내요. 같은 에이전트를 CLI 뷰로 바꿔도 대화와 프로세스는 그대로예요.

## 동작

- **준비 전:** CLI가 입력을 받을 수 있을 때까지 "Claude Code 준비 중…" 로딩이 보여요. 처음 여는 폴더면 "이 폴더를 신뢰할까요?" 카드가 떠요.
- **메시지:** 사용자 입력, 답변, 도구 호출(접을 수 있는 줄)로 보여요.
  - 도구 줄은 접혀 있어요. 펼치면 파일 수정·쓰기(Edit, MultiEdit, Write, NotebookEdit)는 승인 카드와 같은 diff 미리보기(+/− 줄 수, 처음 40줄, "N줄 더 보기")와 결과가, 그 밖의 도구는 입력 JSON과 결과(4,000자까지)가 보여요. → [승인](approvals.md)
  - 답변은 마크다운으로 그려요. 표, 목록, 코드 블록이 다 돼요.
  - 링크를 눌러도 앱 안에서 이동하지 않아요.
- **입력창(Composer)**
  - Enter로 보내요.
  - `/`를 치면 슬래시 명령 메뉴가 떠요. `!`로 시작하면 bash 모드가 돼요. → [슬래시 명령](slash-commands.md)
  - Claude는 모델 선택기가 있어요.
  - 파일을 끌어 놓으면 첨부 칩이 생겨요. → [드래그 첨부](../editor/drag-attach.md)
  - 작업 중에는 멈춤(Esc)을 보낼 수 있어요.
  - 기록에 아무것도 남기지 않는 명령(`/help` 같은 TUI 대화창)을 보내면 턴이 시작되지 않아 "생각 중"으로 남아요. 이때 멈춤을 누르면 Esc로 대화창이 닫히고, 2초 뒤에도 보낸 뒤로 기록에 새 이벤트가 없으면 상태를 대기로 돌려요(`Agent::interrupt`).
- **승인:** 권한 확인이 필요하면 입력창 위에 승인 카드가 떠요. → [승인](approvals.md)
- **터미널이 답을 기다릴 때:** 에이전트의 터미널이 자기 선택지(권한 질문 등)를 띄우고 있으면 입력창 위에 "…이(가) 터미널에서 답을 기다려요" 띠와 "터미널에서 답하기" 버튼이 떠요(누르면 터미널 보기로 바뀌어요). 입력창은 잠겨요: Enter와 보내기가 안 되고 안내가 "터미널에서 먼저 답해 주세요 · 지금 보내면 그 선택지에 들어가요"로 바뀌어요. 쓰던 글은 남아요. 중단 버튼은 그대로예요(Esc가 "아니요"로 답해요). → [승인](approvals.md)
- **다시 시작·대화 전환:** 에이전트가 다시 시작되거나, 트랜스크립트가 짧아지거나, 다른 대화를 읽게 되면(새 Antigravity 대화를 가져올 때. `/clear`·더 최근 agy 대화 따라가기는 R3에서 꺼져 있어요) 목록을 새로 받아 그려요. 보이는 모습은 같아요.
- **보기 전환:** 블록 헤더의 Chat/CLI 버튼으로 바꿔요.
  - 채팅 런타임이 있는 에이전트(Claude Code)가 돌고 있으면(ACP든 터미널이든) 먼저 대화를 옮기고(`switch_runtime`) 보기가 바뀌어요("전환 중…", 턴 중이면 "턴이 끝나면 전환돼요 · 지금 중단하고 전환"). 에이전트가 꺼져 있을 때만 보기가 바로 바뀌고, 새 보기가 뜨면서 에이전트를 그 보기의 런타임으로 띄워요(시작 오류는 그 보기의 "다시 시도"로). 전환을 기다리는 사이 그 패널이 닫히면(다른 워크트리를 전체 화면으로 열었을 때 등) 늦게 끝난 전환은 보기를 바꾸지도, 무엇을 묻지도 않아요. 고른 보기는 백엔드가 이미 기억해 다음 시작에 써요. → [채팅 런타임](chat-runtime.md)
  - 전체 화면에서 고른 뷰는 설정 `fullView`에 저장돼요.
  - 캔버스 블록은 블록마다 따로 저장돼요.

**채팅 런타임(ACP, Claude Code):**
- 조건이 갖춰지면 채팅 뷰는 ACP 어댑터로 돌아요. 답이 쓰이는 대로 글이 흘러나오고(실시간 글), 블록이 끝나면 완성본으로 바뀌어요.
- 터미널 방식으로 도는 이유는 헤더의 에이전트 이름표 툴팁에 있어요(예: "Node.js 22 이상이 없어 터미널 방식으로 실행 중이에요"). ACP로 돌면 "채팅: Claude Agent(ACP)"예요. 정말 넘어간 경우(설정에서 터미널을 고른 것도, CLI 뷰도 아닐 때)는 이름표 옆에 작게 "· 터미널"이 붙어요.
- `!` 셸 명령과 채팅에서 안 되는 슬래시 명령은 보내지 않고 "CLI 보기에서" 안내를 돌려줘요. 입력창 안내도 "… · 셸 명령(!)은 CLI 보기에서"로 바뀌고, 중단 버튼 툴팁에서 "(터미널의 Esc)"가 빠져요. Stop은 `session/cancel`이에요. → [채팅 런타임](chat-runtime.md)
- 채팅 뷰가 뜰 때는 에이전트가 꺼져 있을 때만 ACP로 띄워요(`start_view`). 이미 돌고 있는 대화는 터미널에서 쉬고 있어도 그대로 두고, 사용자가 Chat을 눌러야 ACP로 옮겨요. 그때까지는 터미널의 트랜스크립트로 채팅해요.

**한글 입력**
- 한글을 조합하는 중에 누른 Enter는 보내지 않아요. 조합이 끝난 뒤 다시 누르면 보내요.
- 채팅 입력창뿐 아니라 Enter로 동작하는 모든 곳(문서 이름, 구조 블록 제목, 병합 거부 사유, 프롬프트 블록, 카드 입력창)에 같은 규칙을 써요.

## 구현

- `AgentPanel`(AgentNode.tsx): 헤더(로고, 이름, Chat/CLI 전환, 폐기 버튼)와 본문이에요. 캔버스 블록(`AgentNode`)과 전체 화면(`AgentFull`)이 같이 써요.
- `useAgentBoot(root, path, enabled, view)`: `view`("chat", 채팅 런타임이 있는 에이전트의 `ChatBody`만)가 있으면 먼저 `start_view(.., view)`를 부르고(꺼져 있으면 그 보기의 런타임으로 띄우고, 돌고 있으면 그대로 둬요. 결과·오류는 무시) `ipc.agentReady`를 불러 `starting` → `ready` / `trust` / `{error}` 상태를 만들어요. `failure`(로그인 필요, 시작하다 종료, 시간 초과)나 시작 오류(실행 파일 없음)는 `{error}`로 보여 주고, 채팅 뷰에는 "다시 시도"와 "CLI에서 보기" 버튼이 있어요. 로딩 문구와 신뢰 확인 문구에는 에이전트 이름(`agentLabel`)이 들어가요. 결과의 `runtime`/`runtimeNote`는 스토어의 `Worktree`로 가요. → [에이전트 실행 방식](../../architecture/agent-runtime.md)
- `useTranscript`: 리스너를 먼저 붙이고 `get_transcript`로 `{ epoch, seq, events }`를 받아요. 그 뒤 `agent://event`를 이렇게 다뤄요.
  - 받는 중이면 줄 세워 뒀다가 받은 뒤 처리해요.
  - epoch가 더 작으면 버려요(이미 다시 시작한 스트림).
  - epoch가 더 크면(`reset` 또는 새 스트림) 줄 세우고 `get_transcript`를 다시 불러요.
  - `seq: null` 중 `delta`(채팅 런타임의 실시간 글, 항목 `live:<n>:<k>`)만 목록에 덧붙여요. 승인, `exited`, `reset`은 넣지 않아요. 실시간 글은 완성 블록이 오기 직전에 빈 `offset: 0` 델타로 지워져요. 그래서 `events.length`는 델타마다 바뀌어요. 카드 요약(WorktreeCard.tsx)은 `delta`를 뺀 수만 계기로 써요 → [답변 요약](answer-summaries.md)
  - `seq`가 받은 것보다 크면 덧붙여요. 실시간 글 델타는 `addDelta`(chatFold.ts)로 붙여요. 글 채널만 남기고(생각·도구 출력 델타는 그리지 않아요), 항목의 퇴장 델타(`offset: 0`, 빈 글)가 오면 그 항목의 델타를 모두 빼요. durable `turnEnded`로는 빼지 않아요. Claude는 `end_turn`을 생각 줄에 먼저 쓰고 답 글 줄을 그 뒤에 써서, 턴 끝이 답이 아직 흐르는 중에 올 수 있어요.
  - 보기 전환은 새 epoch(`reset switch`)를 보내서 목록을 다시 받아요. 전과 같은 트랜스크립트라 항목이 같아요.
- `fold(events)`(chatFold.ts): 공통 이벤트 → 화면 항목(`Item`). AgentNode가 `fold`, `lastText`를 다시 내보내서 WorktreeCard의 import는 그대로예요. → [공통 이벤트](../../architecture/runtime-events.md)
  - `userMessage` → 말풍선, `agentText` → `itemId`로 넣거나 바꾸기, `toolStarted`/`toolFinished` → `toolCallId`로 짝 맞춘 도구 줄, `notice` → 흐린 줄("중단됨" 포함), `notice(failed)` → 빨간 줄(채팅 런타임이 실패로 끝낸 턴의 이유: 로그인 필요, 어댑터 오류, 꺼져서 전달 안 됨). `turnEnded`는 그리지 않아요(터미널 방식의 API 오류는 이미 그 턴의 `agentText`라 두 번 보이지 않게). 빈 글은 빼요. R3 이전 fold와 결과가 같다는 걸 두 단계로 확인해요: Rust 테스트(`events::oracle`)가 R3 이전 fold와 chatFold.ts의 Rust 사본을 비교하고, `npm test`(`scripts/chat-fold-parity.ts`)가 chatFold.ts `fold` 자체를 Rust 테스트가 적은 픽스처(`src-tauri/tests/fixtures/chat-parity.json`, R3 이전 fold의 항목)에 대 봐요. 픽스처의 `acp-failed` 경우(실패·중단으로 끝난 ACP 턴)는 R3 이전에 없던 것이라 Rust 사본(`oracle::items`)의 항목과 비교해요. 그 항목은 Rust 테스트가 글자 그대로 고정해 둬요. 같은 스크립트가 실시간 델타도 확인해요: 흐르는 동안은 글이 보이고(UTF-16 오프셋), 지운 뒤에는 완성 블록만 남고, 생각 델타는 그리지 않아요. `useTranscript`의 epoch·seq 처리는 테스트가 없어요.
- `ChatBody`, `Messages`, `RichText`(react-markdown + remark-gfm), `ToolRow`, `Composer`가 화면을 나눠 맡아요.
  - `ToolRow`는 펼쳤을 때만 `toolPreview(name, input)`(toolPreview.ts)을 계산해요. 결과가 있으면 `ToolPreviewView`(ToolDiff.tsx, 승인 카드와 같은 것)로 그리고 입력 JSON은 빼요. null이면 예전처럼 입력 JSON이에요. `npm test`가 `scripts/tool-preview-check.ts`로 이 미리보기도 확인해요.
- 보내기: `ipc.send(root, path, text, files)` → `send_message`(main.rs) → `wait_ready()` 뒤 `Agent::send`예요. 신뢰 확인 중이거나 시작에 실패했으면 그 이유로 에러를 돌려줘요.

- 한글 조합 판별: `isImeComposing(e)`(src/ime.ts). `isComposing`이 참이거나, `keyCode`가 229(IME 처리 중)이거나, `compositionend` 직후 20ms 안의 Enter면 조합 중으로 봐요. 일부 WebView는 조합을 끝내는 Enter를 `isComposing=false`, `keyCode=229`로 보내서 `isComposing`만으로는 덜 조합된 글자가 전송됐어요(Cline도 같은 두 조건을 봐요). Enter로 동작하는 핸들러는 모두 이 함수를 먼저 불러요.

## 제약과 알려진 문제

- 트랜스크립트에 남지 않는 TUI 화면(일부 대화상자 등)은 채팅 뷰에 안 보여요. CLI 뷰로 봐야 해요.
- 에이전트를 띄울 때마다(`reset restart`)와 채팅↔CLI 전환 때마다(`reset switch`) 열린 채팅 뷰가 기록을 한 번 더 받아요.
- 리스너가 붙기 전에 시작된 실시간 글은 앞부분 없이 보일 수 있어요. 완성 블록이 오면 바로잡혀요.

## 결정 기록

- 2026-11-13 (UI 구조 재구성 단계 5): 터미널 질문 띠의 버튼 "CLI 보기에서 답하기"와 잠긴 입력창 안내 "CLI 보기에서 먼저 답해 주세요"를 용어 표대로 "터미널에서 답하기", "터미널에서 먼저 답해 주세요"로 바꿨어요. 승인 카드와 확인할 일 팝오버의 버튼 이름과 맞췄어요. 셸 명령(!) 안내의 "CLI 보기에서"는 보기 이름을 바꾸는 단계 12에서 같이 바꿔요.
- 채팅 화면을 먼저 띄우고 나중에 CLI가 뜨면 그 사이 입력이 사라졌어요. 그래서 CLI가 준비될 때까지 입력창을 보여 주지 않아요.
