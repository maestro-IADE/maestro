---
title: 승인 (권한 확인을 채팅에서 답하기)
area: agents
status: partial
files: [src-tauri/src/approval.rs, src-tauri/src/guard.rs, src-tauri/src/tui_wait.rs, src-tauri/src/pty.rs, src-tauri/src/agent.rs, src/nodes/ApprovalCards.tsx, src/nodes/Attention.tsx, src/attention.ts, src/nodes/ToolDiff.tsx, src/toolPreview.ts, scripts/tool-preview-check.ts, src/nodes/AgentNode.tsx, src/nodes/MergeCard.tsx, src/store.ts, src/Sidebar.tsx, src/Workspace.tsx, src/App.tsx, src-tauri/src/transcript.rs, src-tauri/src/claude.rs, src-tauri/src/events.rs, src/agents.tsx, src/status.tsx]
related: [features/ui/attention.md, features/orchestration/merge-request.md, features/agents/chat-runtime.md, architecture/local-server.md, features/agents/chat-view.md, architecture/runtime-events.md, architecture/agent-runtime.md, features/ui/terms.md]
updated: 2026-11-13
---

# 승인

## 요약

Claude가 도구를 쓰려고 권한을 물으면, 터미널 대신 채팅 뷰에 승인 카드가 떠요. 허용 / 항상 허용 / 거부 / 터미널에서 답하기 중에 고를 수 있어요. 같은 승인은 상단 막대의 확인할 일 팝오버에도 모여서, 어느 창·어느 화면에서든 허용·거부할 수 있어요. 터미널(PTY)에서는 Claude의 `PermissionRequest` 훅이 Maestro 로컬 서버에 요청을 보내고, 사용자가 답할 때까지 연결을 붙잡아 두는 방식이에요. 채팅 런타임(ACP)에서는 어댑터의 `session/request_permission`이 같은 카드가 되고, "터미널에서 답하기"는 없어요.

## 동작

- 카드에는 도구 이름이 보여요. 그 아래 내용은 도구에 따라 달라요.
  - **파일 수정(Edit, MultiEdit):** 바꾸기 전 → 후의 줄 비교(diff)예요. 파일 경로(워크트리 기준 상대 경로), `+추가 −삭제` 줄 수, 바뀐 줄 앞뒤로 같은 줄 3줄씩이 보이고, 더 긴 같은 줄은 "⋯ N줄 같음"으로 접혀요. `replace_all`이면 "일치하는 곳을 모두 바꿔요"가 붙어요. MultiEdit는 "편집 1/3"처럼 편집마다 나눠 보여요.
  - **파일 쓰기(Write), 노트북 셀(NotebookEdit):** 쓸 내용의 앞부분과 전체 줄 수예요("파일 전체를 이 내용으로 써요", "새 셀", "셀 삭제" 등).
  - 처음에는 8줄만 보이고 "N줄 더 보기"로 펼쳐요("접기"로 다시 접어요). 펼쳐도 상자 높이는 정해져 있어 그 안에서 스크롤돼요(입력 바의 카드는 더 낮아요). 허용/거부 버튼은 상자 밖이라 늘 보여요.
  - **그 밖의 도구(Bash 등):** 예전처럼 한 줄 요약(명령, 파일 경로, URL 등)이에요.
  - 설명(`description`)이 있으면 같이 보여요.
- **허용:** 이번 한 번만 허용해요.
- **항상 허용:** Claude가 제안한 규칙(`permission_suggestions`)을 그대로 적용해요. 제안이 있을 때만 버튼이 보여요.
- **거부:** "사용자가 Maestro에서 거부했어요."라는 이유와 함께 거부해요.
- **터미널에서 답하기:** 결정을 넘기지 않아요. 그러면 Claude가 터미널의 원래 승인 화면을 띄워요. 버튼을 누르면 터미널 보기도 같이 열려요(캔버스 아래 입력 바에서는 전체 화면의 터미널 보기). 채팅 런타임(ACP)의 질문에는 이 버튼이 없어요(넘길 터미널이 없어요). 예전 이름은 "CLI에서 답하기"예요.
- **답할 수 있는 곳:** 그 에이전트의 채팅 뷰와 캔버스 아래 입력 바(카드 전체), 상단 막대의 확인할 일 팝오버(허용 · 항상 허용 · 거부 · 열기, 펼치면 같은 미리보기). 어느 쪽에서 눌러도 그 승인은 바로 모든 곳에서 사라져요. 창 밖에서는 창 제목 "(N) Maestro"와 작업 표시줄 깜박임으로 알아요. → [확인할 일](../ui/attention.md)
- 승인 대기 중인 워크트리는 사이드바에서 "승인 대기"로 표시되고, 상태 글리프가 물음표 말풍선(경고색)으로 바뀌어요. → [테마와 모션](../ui/themes-and-motion.md)
- **권한 모드와 승인 카드:** 카드가 뜨는 것은 권한 모드가 묻게 한 도구뿐이에요. `acceptEdits`(파일 수정만 자동 허용)는 파일 수정은 묻지 않고 허용하고, 명령 실행 등 다른 도구는 승인 카드로 물어요(거부하지 않아요). `bypassPermissions`는 묻지 않고, `plan`은 파일을 바꾸지 않아요. 새 에이전트 창과 설정의 권한 설명도 이렇게 말해요(`PERMISSIONS`, src/agents.tsx). 예전 설명 "나머지 도구는 거부돼요"는 틀린 말이었어요. → [용어](../ui/terms.md)
- **터미널이 답을 기다리는 상태:** 아래 경우에는 에이전트의 터미널(TUI)이 자기 선택지를 띄우고 답을 기다려요. 이때 채팅에서 보낸 글이 그 선택지에 들어가 Enter가 "Yes"를 고를 수 있어서, 이 상태 동안은 아무것도 보내지 않아요.
  - 카드에서 "터미널에서 답하기"를 눌렀을 때
  - 카드가 590초 안에 답을 못 받았을 때(시간 초과 → Claude가 터미널에서 다시 물어요)
  - 터미널 출력에 번호 선택지가 보일 때(훅이 없는 Antigravity의 권한 질문, Claude의 다른 선택지)
  - 이 동안: "승인 대기" 표시(사이드바, 워크트리 카드, 에이전트 칩)가 남아 있고, 입력창 위에 "…이(가) 터미널에서 답을 기다려요" 띠와 "터미널에서 답하기" 버튼이 떠요. 확인할 일 팝오버에도 "터미널 질문"으로 떠요([터미널에서 답하기]는 그 에이전트를 전체 화면 터미널 보기로 열어요). 입력창은 잠겨요(쓰던 글은 남아요). `maestro send`는 오류를 받고, 워커 완료 노트는 디스크에 남았다가 상태가 풀리면 전달돼요.
  - 풀리는 때: CLI 보기에서 Enter, 숫자(1–9), Esc, Ctrl+C를 입력했을 때, 채팅의 중단(= Esc), 그 도구 호출이 끝나거나 트랜스크립트에 새 진행(답변, 생각, 새 도구)이 보일 때, 턴이 끝나거나 중단됐을 때, 프로세스가 끝나거나 바뀌었을 때.
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
- `ApprovalCards`: 승인 한 건마다 `ApprovalItem`(nodes/Attention.tsx)을 그려요. 확인할 일 팝오버의 `AttentionRow`도 같은 `answerApproval(a, choice)`를 써요: 스토어에서 먼저 빼고(`dropApproval`) `ipc.answerApproval(id, choice)`를 불러요. 그래서 채팅 카드와 팝오버에서 같은 승인을 두 번 누를 수 없고, 두 창에서 거의 동시에 눌러도 백엔드 `Approvals::answer`가 이미 끝난 id를 조용히 무시해요(`pending.remove`가 없으면 그냥 돌아가요). "터미널에서 답하기"는 `a.cliAnswer !== false`일 때만 그려요(ACP 카드는 `false`, 필드가 없던 예전 이벤트는 보여요). `onOpenCli`를 받으면 "터미널에서 답하기" 뒤에 그것도 불러요(채팅 뷰: 터미널 보기로 전환, 입력 바: 전체 화면을 터미널 보기로).
- 한 줄 요약(`approvalSummary`)은 순수 모듈 `src/attention.ts`에 있어요(카드의 대체 글과 팝오버 둘째 줄이 같이 써요).
- 앱 시작 때 `ipc.listApprovals()`로 이미 대기 중인 요청을 다시 불러와요(App.tsx).
- **파일 변경 미리보기:** `ToolInputPreview`(ToolDiff.tsx)가 `toolPreview(toolName, toolInput)`(toolPreview.ts)을 그려요. null이면 원래의 한 줄 요약(`summary`)을 그대로 그려요. 채팅의 도구 줄(`ToolRow`)도 같은 `ToolPreviewView`를 써요. → [채팅 뷰](chat-view.md)
  - `toolPreview`는 이름이 아니라 입력 모양으로 골라요(채팅 런타임은 도구 이름이 없으면 호출 제목을 이름으로 써서). `old_string`/`new_string` 문자열 → Edit, 그런 편집의 배열 `edits` → MultiEdit, `content` 문자열 + `file_path`(또는 이름이 Write) → Write, `new_source` → NotebookEdit. 그 밖(빈 입력 `{}`, `null` 포함)은 null이에요.
  - `lineDiff(a, b)`: CRLF는 LF로 보고 줄 단위로 비교해요. 같은 앞·뒤 줄을 먼저 떼고, 가운데가 작으면(줄 수 곱 250,000칸 이하, 글자 400,000자 이하) LCS로, 크면 옛 줄 전부 − 다음 새 줄 전부 +로 보여요(최소는 아니지만 맞아요). 그래서 큰 Write나 Edit에도 화면이 멈추지 않아요. 줄이 같고 줄바꿈만 다르면 "줄바꿈만 달라요"예요.
  - 한도: 미리보기는 400줄까지(넘은 줄 수는 펼쳤을 때 "N줄은 너무 길어 생략했어요"), 한 줄은 500자까지(넘으면 `…`). `+`/`−`·전체 줄 수는 한도 전의 수예요.
  - 경로는 `relPath(path, worktree)`로 워크트리 안이면 상대 경로예요(슬래시 방향 무시, 드라이브 경로는 대소문자 무시).
  - 확인: `npm test`의 `scripts/tool-preview-check.ts`(diff, 접기, 한도, 모양 판별, 상대 경로).

**터미널이 답을 기다리는 상태(`tui_wait.rs`)**
- 에이전트마다 `Watch` 하나(`Agent.tui`)가 `Option<TuiWait { worktreePath, source: cli | timeout | screen, toolName, turnId, toolCallId }>`를 들고 있어요. 바뀌면 `agent://tui-wait { worktreePath, wait }`를 보내고, 늦게 뜬 화면은 `list_tui_waits`로 가져와요(Workspace.tsx `useAgentEvents`). 백엔드는 바뀔 때만 보내서, 목록이 이미 풀린 대기를 다시 넣으면 그 창의 입력창이 다음 대기까지 잠겨요. 그래서 리스너를 먼저 등록한 뒤 목록을 묻고, 그 뒤 이벤트를 받은 경로는 목록 값을 무시해요.
- 상태 변화는 순수 함수 `tui_wait::step(path, cur, running, Signal)`이 정해요. 신호:
  - `Asked`: `Approvals::handle`이 결과가 `Resolution::Cli`/`Timeout`이면(`tui_source`) 빈 응답을 돌려주기 **전에** 걸어요. 그래서 Claude가 선택지를 그리기 전에 막혀 있어요.
  - `Screen`: PTY 런타임의 트랜스크립트 틱 스레드가 250ms마다 새 출력이 있으면 `pty::picker_shown`으로 봐요. 턴 중(`Running`)일 때만 세요. 이미 훅으로 걸려 있으면 출처를 바꾸지 않아요.
  - `Event`: 이벤트 관찰자(`Agent::observer`)가 넘겨요. `TurnEnded`, `UserMessage`, `Notice(interrupted)`는 풀고, `ToolFinished`는 같은 `toolCallId`일 때만 풀어요. `AgentText`/`Thinking`/`ToolStarted`는 출처가 `screen`이거나 `toolCallId`가 없을 때 풀어요(선택지에 막힌 TUI는 트랜스크립트에 새로 쓰지 않아요).
  - `Keys`: `Agent::write`(CLI 보기의 입력)와 `Agent::interrupt`(Esc). Enter가 들어 있거나, Esc 하나, Ctrl+C, 숫자 1–9면 풀어요. 화살표·포커스·커서 응답 같은 이스케이프 시퀀스는 그대로 둬요.
  - `Gone`: 프로세스 종료(`exit_cb`)와 런타임 교체(`swap`).
- 막는 곳은 `Agent::send_prompt` 하나예요(`sending` 잠금 안). 채팅, `maestro send`, 워커 노트, 페르소나 알림이 다 여기로 와요. 풀리면(관찰자·`write`·`interrupt`) `deliver_kept_notes`로 남은 노트를 보내요.
- **화면 확인(`pty::picker_shown`):** 출력의 마지막 4KB(`Scrollback::picker_tail`, `picker_mark` 이후)를 글자로 바꿔, 테두리를 뗀 마지막 12줄에서 찾아요.
  - 선택지 줄: `[표시] N. 글` 또는 `N) 글`(N = 1–9). 1과 2가 이어서 있어야 해요.
  - 하나 이상에 선택 표시가 있어야 해요. Claude는 `❯`만, 다른 에이전트는 `❯ › ● ◉ ▸ ▶ →`(`picker_glyphs`). Windows의 Claude는 답변 앞에 `●`를 붙여서 Claude에서는 빼요. `>`는 지난 프롬프트 표시라 세지 않아요.
  - 선택지에 답 단어(yes, no, allow, deny, reject, approve, proceed, don't ask, cancel)가 있거나, 1번 바로 윗줄이 `?`로 끝나야 해요.
  - `picker_mark`는 프롬프트를 붙여넣은 뒤, 시작 화면(신뢰 확인)이 끝난 뒤, 상태가 풀린 뒤(`take_rearm`)에 지금 위치로 옮겨요. 이미 본 선택지를 다시 세지 않아요.
- 프런트엔드: `store.tuiWaits`, `setTuiWait`, `useTuiWait`. 확인할 일 목록(`attentionItems`, `kind: "tui"`)이 이것도 넣어서 상태 글리프(`useWaiting`), 사이드바 둘째 줄 "터미널 질문", 접힌 프로젝트의 "승인 대기 N"이 유지돼요. `TuiWaitBand`(AgentNode.tsx)가 채팅 뷰와 캔버스 입력 바에서 승인 카드 아래, 입력창 위에 떠요. `Composer`는 이 상태면 보내지 않고 버튼을 끄고 안내 문구를 바꿔요.

### ACP 런타임의 권한 질문

- `session/request_permission`이 같은 `Approvals::request`로 카드가 돼요. 카드의 `toolCallId`는 요청이 가진 id(= 트랜스크립트의 `tool_use.id`)라 짐작이 없어요.
- `Approval.cliAnswer`가 `false`예요. 돌려줄 터미널이 없어서 `ApprovalItem`이 "터미널에서 답하기"를 그리지 않아요. PTY 훅의 카드는 `true`. 터미널에서 답하고 싶으면 CLI 보기로 전환하면 되는데, 승인 카드가 떠 있는 동안은 턴이 끝날 때까지 전환을 기다려요.
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

- 승인 카드는 Claude Code에서만 떠요. Antigravity는 이 훅이 없어서 CLI에서 답해야 해요. 대신 터미널 출력의 선택지를 찾아 "터미널이 답을 기다리는 상태"로 잠가요. 명령 가드도 없어서 병합·푸시 금지는 브리프 지시뿐이에요.
- 화면 확인은 렌더링된 화면이 아니라 출력 바이트의 글자 비교예요(터미널 에뮬레이터가 없어요).
  - 놓칠 수 있어요: 모르는 표시나 문구, 조금씩 다시 그려진 선택지. Antigravity의 실제 선택지 모양은 확인하지 않았어요(`pty::tests::tui_picker_dump`로 받아 볼 수 있어요).
  - 잘못 볼 수 있어요: 선택 표시가 붙은 번호 목록이 모델 답변에 있을 때. 다음 트랜스크립트 진행이나 턴 끝에서 풀려요.
  - 선택지가 그려진 직후 250ms 안에 보낸 글은 막지 못할 수 있어요. 훅으로 거는 경우(Claude)는 빈 응답 전에 걸어서 이 틈이 없어요.
- CLI 보기에서 Enter를 다른 데 눌러도 상태가 풀려요(키로는 선택지에 답했는지 알 수 없어요).
- 끝까지 실제로 확인하지 않았어요(훅 → 카드 → 답 → Claude 진행).
- `turnId`는 짐작일 수 있어요. `toolCallId`는 이름·입력이 같은 호출을 찾았을 때만 있어요. 같은 이름·입력의 호출이 동시에 여럿이면 가장 최근 것을 골라요. R3의 화면(`ApprovalCards`)은 이 값을 쓰지 않아요.
- 파일 변경 미리보기는 에이전트가 **요청한 내용**이지 디스크와 맞춰 본 패치가 아니에요.
  - Edit diff는 `old_string` → `new_string`만 보여 줘요. 파일의 몇째 줄인지, 앞뒤 내용은 없어요. `old_string`이 파일과 안 맞으면 허용해도 도구가 실패해요. `replace_all`은 여러 곳을 바꿀 수 있어요.
  - Write는 디스크를 읽지 않아서 새 파일인지 덮어쓰기인지 몰라요. 쓸 내용 전체가 + 줄로 보여요.
  - Antigravity 같은 다른 모양의 도구 입력은 미리보기 없이 원래 모습이에요.
- Maestro가 꺼져 있으면 훅이 빈 응답을 받아요. 그러면 Claude가 원래 터미널 프롬프트로 돌아가요. 말없이 승인되는 경우는 없어요.

## 결정 기록

- 승인하려고 매번 CLI 뷰로 가야 하는 게 불편하다는 요청에서 시작했어요.
- 터미널 화면을 파싱하지 않고 공식 훅을 쓴 이유: 화면 모양에 기대지 않아서 안정적이에요.
- 2026-11-13 (UI 구조 재구성 단계 5): 승인을 상단 막대의 확인할 일 팝오버에서도 답하게 했어요. 예전에는 신호가 사이드바 점뿐이라 프로젝트를 접거나 사이드바를 숨기면 사라졌어요. 팝오버에 새 백엔드 명령은 없고 카드와 같은 `answerApproval`을 써요. 버튼 이름 "CLI에서 답하기"는 용어 표대로 "터미널에서 답하기"로 바꿨어요.
- 2026-11-13: `acceptEdits` 설명을 "명령 실행 등 나머지 도구는 거부돼요"에서 "승인 카드로 물어봐요"로 고쳤어요. 실제로는 거부하지 않고 이 승인 카드로 물어요.
