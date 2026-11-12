---
title: 승인 (권한 확인을 채팅에서 답하기)
area: agents
status: partial
files: [src-tauri/src/approval.rs, src-tauri/src/guard.rs, src/nodes/ApprovalCards.tsx, src/nodes/MergeCard.tsx, src/store.ts, src/Sidebar.tsx, src/App.tsx, src-tauri/src/transcript.rs, src-tauri/src/claude.rs, src-tauri/src/events.rs]
related: [features/orchestration/merge-request.md, features/agents/chat-runtime.md, architecture/local-server.md, features/agents/chat-view.md, architecture/runtime-events.md, architecture/agent-runtime.md]
updated: 2026-11-11
---

# 승인

## 요약

Claude가 도구를 쓰려고 권한을 물으면, 터미널 대신 채팅 뷰에 승인 카드가 떠요. 허용 / 항상 허용 / 거부 / CLI에서 답하기 중에 고를 수 있어요. 터미널(PTY)에서는 Claude의 `PermissionRequest` 훅이 Maestro 로컬 서버에 요청을 보내고, 사용자가 답할 때까지 연결을 붙잡아 두는 방식이에요. 채팅 런타임(ACP)에서는 어댑터의 `session/request_permission`이 같은 카드가 되고, "CLI에서 답하기"는 없어요.

## 동작

- 카드에는 도구 이름과 한 줄 요약(명령, 파일 경로, URL 등)이 보이고, 설명이 있으면 같이 보여요.
- **허용:** 이번 한 번만 허용해요.
- **항상 허용:** Claude가 제안한 규칙(`permission_suggestions`)을 그대로 적용해요. 제안이 있을 때만 버튼이 보여요.
- **거부:** "사용자가 Maestro에서 거부했어요."라는 이유와 함께 거부해요.
- **CLI에서 답하기:** 결정을 넘기지 않아요. 그러면 Claude가 터미널의 원래 승인 화면을 띄워요. 채팅 런타임(ACP)의 질문에는 이 버튼이 없어요(넘길 터미널이 없어요).
- 승인 대기 중인 워크트리는 사이드바에서 "승인 대기"로 표시되고, 상태 점도 바뀌어요.
- 터미널에서 먼저 답하거나 턴을 멈추면 카드가 저절로 사라져요.
- 마에스트로의 블록에는 같은 자리에 `병합 요청` 카드도 떠요(도구 승인보다 위). 도구 승인과는 따로예요: 런타임 스트림에 들어가지 않고(`approvalRequested` 없음), 채팅↔CLI 전환의 "바쁨"(`SwitchResult.busy`)에도 세지 않아요. 사이드바의 "승인 대기"는 둘 중 하나라도 있으면 보여요. → [병합 요청](../orchestration/merge-request.md)
- 명령 가드가 막은 명령(병합·푸시·파괴적 git)은 카드 없이 거부돼요.

## 구현

**훅 설정**
- `Approvals::write_settings`가 에이전트마다 `<meta>.settings.json`을 써요. 포트가 실행마다 바뀌어서 시작할 때마다 다시 써요.
- 이 파일은 `--settings`로 넘겨요. 사용자의 원래 Claude 설정은 건드리지 않아요.
- 훅 명령:
  `curl -s --max-time 600 -X POST … --data-binary @- "http://127.0.0.1:<port>/permission/<sid>?t=<token>"`
- 같은 파일에 `permissions.allow: ["Bash(maestro:*)"]`도 들어 있어요. maestro CLI는 삭제 기능이 없어서 매번 묻지 않게 했어요.
- 같은 파일에 신호용 `Stop`·`SessionStart` 훅도 있어요(`/hook/<sid>`). 승인과는 상관없어요. → [에이전트 실행 방식](../../architecture/agent-runtime.md)
- 훅 파일은 카탈로그 `features.approval_hook`이 켜진 에이전트(Claude)만 써요(`ClaudeAgent::launch`). 꺼져 있으면 `{}`만 써요.
- 같은 파일에 명령 가드도 있어요(R5): `PreToolUse` 훅(matcher `Bash|PowerShell|Monitor`, `/pretool/<sid>`, curl이 실패하면 `exit 2`로 막는 fail-closed)과 `permissions.deny`(`Bash(git push:*)`, `Bash(git reset --hard:*)`, `Bash(git branch -D:*)`, `Bash(git clean -f:*)` 등). `PreToolUse`는 권한 확인보다 먼저 돌아서 `bypassPermissions`나 자동 허용 규칙으로 통과할 호출도 막아요. → [병합 요청](../orchestration/merge-request.md)

**서버 처리:** `Approvals::handle` → `Approvals::request`
- `request(app, worktree, ApprovalReq, still_waiting)`는 런타임과 상관없는 승인 창구예요. R4의 ACP `request_permission`도 같은 함수를 불러요. 결과는 `(Resolution, Option<Choice>)`이에요.
- 대기 목록에 넣고 `agent://approval` 이벤트를 보내요. 그 워크트리의 스트림에도 `approvalRequested`(live-only)를 보내요.
- **어느 도구 호출인지(turnId, toolCallId):** Claude 2.1.302의 `PermissionRequest` 입력에는 `tool_use_id`가 없어요. 그래서 `correlate()`가
  1. `agent.locate_tool(tool_name, tool_input, tool_use_id)`가 트랜스크립트를 지금 읽고(`sync`) `ToolMatch { turnId, toolCallId, exact }`를 줘요.
     - 입력에 `tool_use_id`가 있으면 그 id를 쓰고, `turnId`는 그 호출의 주인 턴이에요(아직 안 읽혔으면 열린 턴). 이름이 같은 다른 호출의 턴을 쓰지 않아요.
     - 없으면 열린 도구 중 이름·입력이 같은 가장 최근 것(`exact`), 없으면 이름만 같은 가장 최근 것(짐작).
  2. `exact`가 아니면(호출이 아직 디스크에 안 왔을 수 있어서) 100ms씩 3번 다시 봐요. 예: 자동 허용된 `maestro …` Bash가 실행 중일 때 다른 Bash가 권한을 물으면, 첫 번째 읽기는 실행 중인 Bash를 이름으로만 찾아요. 그걸 바로 쓰지 않고 새 호출이 읽히기를 기다려요.
  3. 다시 봐도 `exact`가 없으면 짐작한 호출의 `turnId`만 쓰고 `toolCallId`는 `null`이에요. 이름만 같은 다른 호출(병렬 Bash 등)일 수 있어서, 화면이 이 id로 엉뚱한 도구 카드를 찾지 않게 해요. 아무것도 없으면 `toolCallId: null`, `turnId`는 열린 턴이에요.
- 1초 간격으로 답을 기다려요. 다음 경우에는 결정 없이(빈 응답) 끝내요.
  - 590초가 지났을 때
  - curl 연결이 끊겼을 때(`peer_closed`)
  - 터미널에서 먼저 답했을 때: Claude Code 2.1.302는 TUI에서 답해도 훅의 curl을 끊지 않아서, 그 승인의 `toolCallId`가 `toolFinished`로 끝나거나(호출을 못 찾았으면 `turnId`의 턴이 `turnEnded`로 끝나면) `Approvals::settle`이 대기 항목을 지워요. 결과는 `withdrawn`이에요.
- 끝나면 `agent://approval-done`과 `approvalResolved { resolution }`(`allow`/`always`/`deny`/`cli`/`timeout`/`withdrawn`)을 보내요.
- 답은 `Choice { Allow, Always, Deny, Cli }`예요. `answer(id, "allow" | "always" | "deny" | 그 밖)`이 문자열을 바꿔요(그 밖은 `Cli`).
- 응답 JSON은 `decision(Choice, suggestions)`가 만들어요: `{ hookSpecificOutput: { hookEventName: "PermissionRequest", decision } }`. `Cli`는 빈 응답이에요.

**프런트엔드**
- `ApprovalCards`: 답하면 `ipc.answerApproval(id, choice)`를 불러요. "CLI에서 답하기"는 `a.cliAnswer !== false`일 때만 그려요(ACP 카드는 `false`, 필드가 없던 예전 이벤트는 보여요).
- 앱 시작 때 `ipc.listApprovals()`로 이미 대기 중인 요청을 다시 불러와요(App.tsx).

### ACP 런타임의 권한 질문

- `session/request_permission`이 같은 `Approvals::request`로 카드가 돼요. 카드의 `toolCallId`는 요청이 가진 id(= 트랜스크립트의 `tool_use.id`)라 짐작이 없어요.
- `Approval.cliAnswer`가 `false`예요. 돌려줄 터미널이 없어서 `ApprovalCards`가 "CLI에서 답하기"를 그리지 않아요. PTY 훅의 카드는 `true`. 터미널에서 답하고 싶으면 CLI 보기로 전환하면 되는데, 승인 카드가 떠 있는 동안은 턴이 끝날 때까지 전환을 기다려요.
- 시간 제한이 없어요(`ApprovalReq.waitMax: None`). 에이전트가 계속 기다리고, 답 없음은 허용이 되면 안 돼서예요. 철회(`$/cancel_request`), 취소, 어댑터 종료로 끝나요.
- **fail-closed:** 답이 없거나 오류·패닉·앱 종료면 `reject_once`(드롭 가드 `Reply`). `cancelled`는 사용자가 Stop을 눌렀거나 어댑터가 철회했을 때만이에요.
- **명령 가드(R5):** 카드를 띄우기 전에 `agent.rs`가 감싼 `Ask`가 `guard::live_check`를 불러, 거부할 명령이면 카드 없이 `Deny`(→ `reject_once`)예요. 묻지 않고 자동 허용되는 호출(`acceptEdits`·`bypassPermissions`, 허용 규칙)은 이 길로 오지 않아서, 세션 `_meta.claudeCode.options.settings`로 넘긴 `PreToolUse` 훅(`/pretool/acp-<path_id>`)과 deny 목록이 막아요(`Approvals::acp_settings`: 가드만, `PermissionRequest`·`Stop`·`SessionStart` 없음). 어댑터 0.102.0에서 `bypassPermissions`로 확인했어요(`acp_live_pretool_blocks_push`).

| 답 | 어댑터에 보내는 것 |
|---|---|
| 허용 | 첫 `allow_once`(없으면 거부) |
| 항상 허용 | `allow-with-updates` → `allow-skill-exact` → `exit-plan-accept-edits` → `exit-plan-`이 아닌 `allow_always` → `allow_once` |
| 거부 / CLI | `reject_once`(없으면 `reject_always`) |
| 답 없음(취소·철회 뒤) | `cancelled` |
| 답 없음(그 밖) | `reject_once` |

`canAlways`는 "항상 허용"이 고르는 선택지가 "허용"과 다를 때만 true예요. → [채팅 런타임](chat-runtime.md)

## 제약과 알려진 문제

- Claude Code에서만 동작해요. Antigravity는 이 훅이 없어서 CLI에서 답해야 해요. 명령 가드도 없어서 병합·푸시 금지는 브리프 지시뿐이에요.
- 끝까지 실제로 확인하지 않았어요(훅 → 카드 → 답 → Claude 진행).
- `turnId`는 짐작일 수 있어요. `toolCallId`는 이름·입력이 같은 호출을 찾았을 때만 있어요. 같은 이름·입력의 호출이 동시에 여럿이면 가장 최근 것을 골라요. R3의 화면(`ApprovalCards`)은 이 값을 쓰지 않아요.
- Maestro가 꺼져 있으면 훅이 빈 응답을 받아요. 그러면 Claude가 원래 터미널 프롬프트로 돌아가요. 말없이 승인되는 경우는 없어요.

## 결정 기록

- 승인하려고 매번 CLI 뷰로 가야 하는 게 불편하다는 요청에서 시작했어요.
- 터미널 화면을 파싱하지 않고 공식 훅을 쓴 이유: 화면 모양에 기대지 않아서 안정적이에요.
