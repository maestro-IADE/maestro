---
title: 채팅 뷰
area: agents
status: done
files: [src/nodes/AgentNode.tsx, src/AgentFull.tsx, src-tauri/src/agent.rs, src/chatFold.ts, scripts/chat-fold-parity.ts, src-tauri/src/events.rs, src-tauri/src/runtime.rs, src-tauri/src/transcript.rs]
related: [architecture/agent-runtime.md, features/agents/cli-view.md, features/agents/approvals.md, features/agents/slash-commands.md, features/editor/drag-attach.md, architecture/runtime-events.md]
updated: 2026-11-10
---

# 채팅 뷰

## 요약

채팅 뷰는 실제 CLI 위에 얹힌 대화 화면이에요. CLI가 쓰는 트랜스크립트를 읽어 말풍선과 도구 실행 줄로 보여 주고, 입력은 CLI에 붙여넣기로 보내요. 같은 에이전트를 CLI 뷰로 바꿔도 대화와 프로세스는 그대로예요.

## 동작

- **준비 전:** CLI가 입력을 받을 수 있을 때까지 "Claude Code 준비 중…" 로딩이 보여요. 처음 여는 폴더면 "이 폴더를 신뢰할까요?" 카드가 떠요.
- **메시지:** 사용자 입력, 답변, 도구 호출(접을 수 있는 줄)로 보여요.
  - 답변은 마크다운으로 그려요. 표, 목록, 코드 블록이 다 돼요.
  - 링크를 눌러도 앱 안에서 이동하지 않아요.
- **입력창(Composer)**
  - Enter로 보내요.
  - `/`를 치면 슬래시 명령 메뉴가 떠요. `!`로 시작하면 bash 모드가 돼요. → [슬래시 명령](slash-commands.md)
  - Claude는 모델 선택기가 있어요.
  - 파일을 끌어 놓으면 첨부 칩이 생겨요. → [드래그 첨부](../editor/drag-attach.md)
  - 작업 중에는 멈춤(Esc)을 보낼 수 있어요.
- **승인:** 권한 확인이 필요하면 입력창 위에 승인 카드가 떠요. → [승인](approvals.md)
- **다시 시작·대화 전환:** 에이전트가 다시 시작되거나, 트랜스크립트가 짧아지거나, 다른 대화를 읽게 되면(새 Antigravity 대화를 가져올 때. `/clear`·더 최근 agy 대화 따라가기는 R3에서 꺼져 있어요) 목록을 새로 받아 그려요. 보이는 모습은 같아요.
- **보기 전환:** 블록 헤더의 Chat/CLI 버튼으로 바꿔요.
  - 전체 화면에서 고른 뷰는 설정 `fullView`에 저장돼요.
  - 캔버스 블록은 블록마다 따로 저장돼요.

## 구현

- `AgentPanel`(AgentNode.tsx): 헤더(로고, 이름, Chat/CLI 전환, 폐기 버튼)와 본문이에요. 캔버스 블록(`AgentNode`)과 전체 화면(`AgentFull`)이 같이 써요.
- `useAgentBoot(root, path)`: `ipc.agentReady`를 불러 `starting` → `ready` / `trust` / `{error}` 상태를 만들어요. `failure`(로그인 필요, 시작하다 종료, 시간 초과)나 시작 오류(실행 파일 없음)는 `{error}`로 보여 주고, 채팅 뷰에는 "다시 시도"와 "CLI에서 보기" 버튼이 있어요. 로딩 문구와 신뢰 확인 문구에는 에이전트 이름(`agentLabel`)이 들어가요. → [에이전트 실행 방식](../../architecture/agent-runtime.md)
- `useTranscript`: 리스너를 먼저 붙이고 `get_transcript`로 `{ epoch, seq, events }`를 받아요. 그 뒤 `agent://event`를 이렇게 다뤄요.
  - 받는 중이면 줄 세워 뒀다가 받은 뒤 처리해요.
  - epoch가 더 작으면 버려요(이미 다시 시작한 스트림).
  - epoch가 더 크면(`reset` 또는 새 스트림) 줄 세우고 `get_transcript`를 다시 불러요.
  - `seq: null`(live-only: `delta`, 승인, `exited`, `reset`)은 R3에서 목록에 넣지 않아요. 넣으면 `events.length`가 바뀌어 카드 요약이 다시 불려요.
  - `seq`가 받은 것보다 크면 덧붙여요.
- `fold(events)`(chatFold.ts): 공통 이벤트 → 화면 항목(`Item`). AgentNode가 `fold`, `lastText`를 다시 내보내서 WorktreeCard의 import는 그대로예요. → [공통 이벤트](../../architecture/runtime-events.md)
  - `userMessage` → 말풍선, `agentText` → `itemId`로 넣거나 바꾸기, `toolStarted`/`toolFinished` → `toolCallId`로 짝 맞춘 도구 줄, `notice` → 흐린 줄("중단됨" 포함). 빈 글은 빼요. R3 이전 fold와 결과가 같다는 걸 두 단계로 확인해요: Rust 테스트(`events::oracle`)가 R3 이전 fold와 chatFold.ts의 Rust 사본을 비교하고, `npm test`(`scripts/chat-fold-parity.ts`)가 chatFold.ts `fold` 자체를 Rust 테스트가 적은 픽스처(`src-tauri/tests/fixtures/chat-parity.json`, R3 이전 fold의 항목)에 대 봐요. `useTranscript`의 epoch·seq 처리는 테스트가 없어요.
- `ChatBody`, `Messages`, `RichText`(react-markdown + remark-gfm), `ToolRow`, `Composer`가 화면을 나눠 맡아요.
- 보내기: `ipc.send(root, path, text, files)` → `send_message`(main.rs) → `wait_ready()` 뒤 `Agent::send`예요. 신뢰 확인 중이거나 시작에 실패했으면 그 이유로 에러를 돌려줘요.

## 제약과 알려진 문제

- 트랜스크립트에 남지 않는 TUI 화면(일부 대화상자 등)은 채팅 뷰에 안 보여요. CLI 뷰로 봐야 해요.
- 에이전트를 띄울 때마다(`reset restart`) 열린 채팅 뷰가 기록을 한 번 더 받아요.

## 결정 기록

- 채팅 화면을 먼저 띄우고 나중에 CLI가 뜨면 그 사이 입력이 사라졌어요. 그래서 CLI가 준비될 때까지 입력창을 보여 주지 않아요.
