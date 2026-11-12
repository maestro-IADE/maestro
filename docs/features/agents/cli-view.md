---
title: CLI 뷰
area: agents
status: done
files: [src/nodes/XTerm.tsx, src/nodes/AgentNode.tsx, src-tauri/src/agent.rs, src-tauri/src/main.rs, src-tauri/src/pty.rs, src-tauri/src/stream.rs, src-tauri/src/tui_wait.rs]
related: [features/agents/chat-runtime.md, architecture/agent-runtime.md, features/agents/chat-view.md, features/agents/approvals.md]
updated: 2026-11-12
---

# CLI 뷰

## 요약

CLI 뷰는 에이전트의 실제 터미널 화면을 xterm.js로 그대로 보여 줘요. 채팅 뷰와 같은 프로세스예요. 키 입력은 PTY로 바로 가요. 색은 트루컬러로 나와요.

## 동작

- 실제 Claude Code/Antigravity TUI가 그대로 보이고, 직접 타이핑할 수 있어요.
- 블록이나 창 크기를 바꾸면 터미널 크기도 맞춰 바뀌어요.
- 에이전트가 다시 시작되면(종료 뒤 재개, 다시 시도) 터미널을 지우고 새 프로세스의 화면부터 그려요. 예전에는 새 프로세스의 `seq`가 0부터 다시 세어져서, 이전 출력 길이를 넘을 때까지 새 출력이 안 보였어요.
- 스크롤바는 앱 테마에 맞춰 얇게 다듬었어요.
- 터미널이 자기 선택지로 답을 기다리는 동안(채팅 뷰가 잠겨 있을 때) 여기서 Enter, 숫자(1–9), Esc, Ctrl+C를 치면 그 상태가 풀리고 채팅 입력창이 다시 열려요. 화살표로 고르는 건 상태를 바꾸지 않아요. → [승인](approvals.md)

- **채팅 런타임과 전환(Claude Code):** 채팅 뷰가 ACP로 돌고 있으면 터미널이 없어요(`pty_snapshot`의 `terminal: false`). 헤더의 CLI를 누르면 ACP를 끝내고 같은 세션을 `claude --resume <id>`로 띄운 뒤 보기가 바뀌어요(턴 중이면 헤더에 "턴이 끝나면 전환돼요 · 지금 중단하고 전환"). CLI 뷰가 그냥 뜰 때는(앱을 다시 켰을 때, 멀리서 다시 확대했을 때 등) 꺼져 있던 에이전트만 터미널로 띄우고(`start_view(.., "cli")`), 채팅이 쥐고 있는 대화는 가져가지 않아요: "지금은 채팅이 대화를 쓰고 있어요 · CLI로 전환" 띠가 떠요. 띠의 "CLI로 전환"이나 헤더의 CLI가 턴을 기다리는 동안(백엔드가 기다림을 알렸을 때만, `agent://switch`) 터미널 위에 "채팅의 턴이 끝나면 CLI로 전환돼요 · 지금 중단하고 전환" 띠가 보여요. 터미널이 늦게 뜨는 것만으로는 띠가 없어요(채팅 런타임 없이 터미널로만 도는 에이전트도 마찬가지). 기다리는 동안 채팅 런타임이 꺼지면 기다림을 그만두고 바로 터미널로 띄워요. 그때 터미널에 친 글은 가지 않아요(`pty_write`가 거절). 채팅으로 돌아가면 TUI를 끝내고 ACP `session/load`로 다시 열어요. 마지막에 본 보기를 기억해서(`lastView`) 다음 시작도 그 보기의 런타임으로 해요. → [채팅 런타임](chat-runtime.md)

## 구현

- `XTerm({ root?, path, start?, takeOver? })`(XTerm.tsx)
  - `start`: PTY가 있게 해요(기본 `ensure_agent`). 채팅 런타임이 있는 에이전트는 `AgentPanel`이 `start_view(.., "cli")`(꺼져 있을 때만 띄움, 돌고 있는 대화는 그대로)를 넘겨요. 기다리는 띠는 시간이 아니라 스토어의 `switchPending === "cli"`(`agent://switch`)로 보여요.
  - `takeOver`: 있으면 띠를 써요. `pty_snapshot.terminal === false`이거나 `agent://runtime`이 `"acp"`를 알리면 "CLI로 전환" 띠, `"pty"`면 띠를 내려요(새 터미널 출력은 epoch로 다시 그려요). 띠의 버튼은 헤더 토글과 같은 전환(`useRuntimeSwitch`)이고, "지금 중단하고 전환"은 `stop_agent`예요.
  - 처음에는 `pty_snapshot`으로 지금까지의 출력과 `epoch`를 받아 그려요.
  - 그 뒤로는 `pty://data` 이벤트를 같은 epoch면 `seq`로 걸러서 이어 그려요. 옛 epoch는 버려요.
  - 더 새 epoch가 오면 `term.reset()` 뒤 그 이벤트를 줄 세우고 `pty_snapshot`을 다시 받아요(`resync`). 새 프로세스가 아직 등록되기 전이면 100ms씩 최대 20번 다시 물어요. 그다음 줄 세운 이벤트를 다시 걸러 그리고 `pty_resize`를 보내요. 에이전트를 다시 띄우지는 않아요(`ensure` 안 함).
  - 새 PTY는 시작하자마자 빈 배치(`seq: 0`)를 보내서 이 과정이 바로 일어나요(pty.rs `PtyRuntime::start`).
- 입력은 `pty_write`로, 크기 변경은 `@xterm/addon-fit` 뒤 `pty_resize`로 보내요.
- xterm이 내는 커서 위치 응답(CPR)은 버려요. 백엔드가 이미 답하기 때문이에요. → [에이전트 실행 방식](../../architecture/agent-runtime.md)
- 글꼴은 `Cascadia Mono, Consolas, monospace`예요.
- 설정 창의 설치·로그인 터미널도 같은 컴포넌트를 써요. 이때 키는 `setup:<id>`예요. → [CLI 설치](cli-install.md)

## 결정 기록

- **색이 안 나오던 문제:** Maestro를 Claude Code 안에서 실행하면 `NO_COLOR`와 `CLAUDE_CODE_*` 환경 변수를 물려받아 흰 글자만 나왔어요. 이 변수들을 지우고 `COLORTERM=truecolor`를 넣어 고쳤어요.
