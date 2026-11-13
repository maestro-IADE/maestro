---
title: 답변 요약 (보조 모델)
area: agents
status: done
files: [src-tauri/src/agent.rs, src-tauri/src/catalog.rs, src-tauri/src/main.rs, src/nodes/WorktreeCard.tsx, src/chatFold.ts, src/settings/AgentsSection.tsx, src/settings.ts, src-tauri/src/events.rs, src-tauri/src/runtime.rs, src-tauri/src/claude.rs]
related: [features/canvas/two-level-canvas.md, features/ui/settings-dialog.md, features/agents/agent-adapters.md]
updated: 2026-11-13
---

# 답변 요약

## 요약

마에스트로 캔버스의 워크트리 카드(`WorktreeCard`)는 에이전트의 가장 최근 답변을 1~2문장(90자 이내)으로 요약해서 보여 줘요. 요약은 설정의 "보조 모델"이 만들어요. 기본값은 Claude Haiku예요. 답변 하나당 한 번만 만들고 메타 파일에 저장해서, 카드를 다시 볼 때는 저장된 요약이 바로 떠요.

## 동작

- 에이전트가 작업 중이면 요약 대신 진행 중인 최근 텍스트를 보여 줘요.
- 턴이 끝나고 요약이 아직 없으면 "답변 요약 중…"(로딩 원)이 보이고, 곧 요약으로 바뀌어요.
- 요약 문장은 "~에 대해 설명했다" 같은 말 대신 내용 자체를 써요(예: "로그인은 JWT로 처리하고 만료는 1시간이다").
- 마우스를 올리면 요약 전의 원래 텍스트가 보여요.
- 보조 모델을 "끔"으로 하면 요약하지 않고 원문 끝부분을 보여 줘요.

## 구현

**요약할 내용:** `events::recent_turn_text(events, 4000)`
- `events`는 `runtime::history()`가 읽은 공통 이벤트예요(실행 중인 런타임이 자기 기록을 가지면 그것, 아니면 트랜스크립트). 채팅 뷰와 같은 변환 경로예요.
- 같은 `itemId`로 다시 온 `agentText`(ACP upsert)는 "결과:" 한 줄로, 처음 자리에 마지막 글을 써요.
- 빈 글이 아닌 마지막 `agentText`를 찾고, 같은 단위(`messageId`: Claude는 트랜스크립트 한 줄, Antigravity는 한 단계)의 뒤 블록까지 넓혀요(agy는 글 뒤에 도구 호출이 같은 단계에 있어요).
- 그 앞의 마지막 `userMessage`(종류 상관없이)부터 "요청:" / "결과:"로 묶고, `toolStarted` 이름을 "(사용한 도구: …)"로 붙여요.
- 출력은 R3 이전과 바이트까지 같아요(요약 캐시 키가 이 글이라, 달라지면 모든 카드가 다시 요약돼요). `events::oracle` 테스트와 로컬 기록 비교로 확인해요.

**모델 호출:** `summarize(conversation, agent, model)`
- 카탈로그 `features.assistant`가 있는 에이전트만 돼요(Codex는 안 됨). 모델이 비면 그 값이 기본 모델이에요(Claude `haiku`, Antigravity는 CLI 기본값). 설정의 보조 모델 목록도 이 값으로 걸러요.
- 실행 파일은 `cli::command`가 찾은 경로로 실행해요(`cmd.exe /c claude`가 아님).
- Claude:
  `claude -p --model <m> --no-session-persistence --setting-sources local --strict-mcp-config --disable-slash-commands --tools "" --output-format json --system-prompt SUMMARY_PROMPT`
  - 입력은 stdin으로 넣어요.
  - 한 번에 약 $0.00015예요.
- Antigravity: `antigravity::ask` → `agy -p … --output-format json --disable-slash-commands`
- 에이전트별 호출은 `PtyAgent::summarize`가 해요(`runtime::pty_agent(agent)`).

**캐시:** `summarize_agent`(main.rs)
- `(SUMMARY_PROMPT, 보조 에이전트, 보조 모델, 텍스트)`의 해시를 `summary_key`로 메타에 저장해요. 키가 같으면 모델을 다시 부르지 않아요.
- 전역 `Mutex`로 한 번에 하나씩만 요약해요. 카드 두 개나 React StrictMode가 동시에 불러도 문장이 두 번 바뀌지 않게 하려는 거예요.
- 모델 호출(수 초)이 끝나면 meta를 **다시 읽어** `summary`·`summary_key`만 바꿔 써요. 그 사이 `cliSessionId`가 바뀌었을 수 있어서(agy 대화 가져오기, 켜면 `/clear`·더 최근 agy 대화 따라가기), 처음 읽은 meta를 그대로 쓰면 옛 세션 id가 되살아나요. 턴이 끝나자마자 카드가 요약을 시작하므로 그 직후 `/clear`가 흔한 경우예요.

**프런트엔드:** WorktreeCard.tsx
- 작업 중이 아니고 이벤트가 있을 때만 `ipc.summarize(root, path, assistantAgent, assistantModel)`를 불러요.
- 다시 부르는 계기는 durable 이벤트 수(채팅 상태의 `durable`, 실시간 델타는 세지 않아요)예요. 채팅 런타임의 실시간 글 델타는 ~50ms마다 오고, Claude가 `end_turn`을 답 글 줄보다 먼저 써서 턴이 끝난(작업 중 아님) 뒤에도 이어질 수 있어요. 델타마다 바뀌는 값을 계기로 쓰면 델타마다 요약을 다시 부르고, 마지막 답 블록이 오기 전 글을 요약해요. 완성 블록(`agentText`)이 오면 durable 수가 바뀌어 그때 요약해요.

## 결정 기록

- **"~를 설명한다"식 요약:** 메타 설명이 나와서 프롬프트를 고쳤어요. 주어는 내용으로 하고, "사용자, 에이전트, 설명했다" 같은 단어를 금지했어요. 입력 이름표도 "요청/결과"로 바꿨어요.
- **방문할 때마다 요약이 바뀌던 문제:** StrictMode 이중 호출과 대상 턴이 어긋나서 생겼어요. 가장 최근 답변만 대상으로 하고, 해시 캐시와 직렬화를 넣어 고쳤어요.
