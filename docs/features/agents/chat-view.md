---
title: 채팅 뷰
area: agents
status: done
files: [src/nodes/AgentNode.tsx, src/settingsMigrate.ts, scripts/view-check.ts, src/settings.ts, src/agents.tsx, src/AgentSettingsDialog.tsx, src/agentActions.ts, src/ime.ts, src/nodes/ApprovalCards.tsx, src/nodes/ToolDiff.tsx, src/toolPreview.ts, scripts/tool-preview-check.ts, src/AgentFull.tsx, src/store.ts, src-tauri/src/tui_wait.rs, src-tauri/src/agent.rs, src/chatFold.ts, scripts/chat-fold-parity.ts, scripts/chat-reduce-check.ts, src/nodes/WorktreeCard.tsx, src-tauri/src/events.rs, src-tauri/src/runtime.rs, src-tauri/src/transcript.rs]
related: [features/agents/agent-settings.md, features/worktrees/create-worktree.md, features/agents/chat-runtime.md, architecture/agent-runtime.md, features/agents/cli-view.md, features/agents/approvals.md, features/agents/slash-commands.md, features/editor/drag-attach.md, architecture/runtime-events.md]
updated: 2026-11-13
---

# 채팅 뷰

## 요약

채팅 뷰는 실제 CLI 위에 얹힌 대화 화면이에요. CLI가 쓰는 트랜스크립트를 읽어 말풍선과 도구 실행 줄로 보여 주고, 입력은 CLI에 붙여넣기로 보내요. 같은 에이전트를 CLI 뷰로 바꿔도 대화와 프로세스는 그대로예요.

## 동작

**헤더(32px).** 에이전트 보기 맨 위 한 줄이에요. 탭이 이미 보여 주는 것(이름, 상태)은 되풀이하지 않아요.
| 부분 | 전체 화면(탭 안) | 캔버스 블록 |
|---|---|---|
| 상태 글리프 + 이름(13px, 굵게) | 없음(탭에 있어요) | 있음 |
| 에이전트 | 로고 14 + 이름 12px | 로고만(툴팁에 이름) |
| 브랜치 | `GitBranch` + 브랜치(12px 고정폭, 최대 200px 말줄임, 툴팁은 폴더 경로) | 없음 |
| 칩 | 권한, 모델, (마에스트로) 병합, (넘어갔을 때) "터미널로 실행 중" | 같음. 블록 폭이 420px보다 좁으면 칩을 모두 숨겨요 |
| 오른쪽 | (전환 중이면 그 글) · 중단 · 보기 전환 아이콘 · ⋯ | 같음, ⋯ 앞에 "작게 접기" |
- **칩**(높이 20, 11px, 반경 4, 흐린 테두리)
  - 권한: 짧은 이름 "매번 묻기 / 수정 자동 / 자동 판단 / 모두 허용 / 계획만". "모두 허용"만 경고색이에요. 툴팁에 전체 이름과 뜻이 있어요. 누르면 에이전트 설정 창이 권한 칸에 포커스를 두고 열려요. → [에이전트 설정](agent-settings.md)
  - 모델: 고정폭 글자로 모델 이름(최대 160px 말줄임), 값이 없으면 "기본 모델"(CLI 기본값). 누르면 에이전트 설정 창이 모델 칸에 포커스를 두고 열려요.
  - 병합(마에스트로만): "병합 · 로컬" 또는 "병합 · PR". 툴팁에 뜻. 누르면 설정의 프로젝트 섹션이 열려요.
  - "터미널로 실행 중"(경고색 글자와 테두리): 채팅이 정말 터미널 방식으로 넘어갔을 때만(설정에서 터미널을 고른 것도, CLI 뷰도 아닐 때). 툴팁은 런타임이 준 이유예요. 누르면 설정의 에이전트 섹션이 열려요.
- **중단**: 작업 중일 때만 보여요(`Square` + "중단"). 입력창 아래 중단 버튼도 그대로 있어요.
- **보기 전환 아이콘**(24×24): 지금 채팅이면 `SquareTerminal` "이 에이전트만 터미널로 보기", 이 에이전트만 터미널로 보고 있으면 `MessageSquare` "채팅으로 돌아가기"예요. 에이전트 보기가 터미널이면 반대로 "이 에이전트만 채팅으로 보기" / "터미널로 돌아가기". 아래 "보기"를 보세요.
- **⋯ 메뉴**: 에이전트 설정… · 워커 추가 · 터미널로 보기 / 채팅으로 돌아가기 · 분할로 열기 · 새 창으로 열기 · 브랜치 이름 복사 · 폴더 경로 복사 · ─ · 폐기…(빨강, 마에스트로는 "마에스트로 없애기…"). 헤더의 휴지통 버튼은 없앴어요. 폐기는 사이드바 행 메뉴와 같은 흐름이에요. → [워크트리 폐기](../worktrees/discard-worktree.md)
- 전체 화면 ↔ 캔버스 버튼은 헤더가 아니라 탭 줄 오른쪽에 있어요. → [가운데 화면 탭](../ui/stage-tabs.md)

- **준비 전:** CLI가 입력을 받을 수 있을 때까지 "Claude Code 준비 중…" 로딩이 보여요. 처음 여는 폴더면 "이 폴더를 신뢰할까요?" 카드가 떠요.
- **메시지:** 사용자 입력, 답변, 도구 호출(접을 수 있는 줄)로 보여요.
  - 도구 줄은 접혀 있어요. 펼치면 파일 수정·쓰기(Edit, MultiEdit, Write, NotebookEdit)는 승인 카드와 같은 diff 미리보기(+/− 줄 수, 처음 40줄, "N줄 더 보기")와 결과가, 그 밖의 도구는 입력 JSON과 결과(4,000자까지)가 보여요. → [승인](approvals.md)
  - 답변은 마크다운으로 그려요. 표, 목록, 코드 블록이 다 돼요.
  - 링크를 눌러도 앱 안에서 이동하지 않아요.
- **입력창(Composer)**
  - Enter로 보내요.
  - **첫 글**: 새 에이전트 창에서 할 일을 적고 만들면, 그 에이전트의 채팅 입력창이 처음 열릴 때 할 일이 들어 있고 포커스가 가요(커서는 끝). 보내지는 않아요: Enter 한 번이면 가요. 에이전트가 준비된 뒤(입력창이 보일 때) 들어가고, 터미널 보기로 열렸으면 채팅 보기를 처음 열 때까지 기다려요. 한 번 넣으면 스토어에서 지워져서 다시 열어도 또 들어가지 않아요. → [에이전트 만들기](../worktrees/create-worktree.md)
  - `/`를 치면 슬래시 명령 메뉴가 떠요. `!`로 시작하면 bash 모드가 돼요. → [슬래시 명령](slash-commands.md)
  - Claude는 모델 선택기가 있어요.
  - 파일을 끌어 놓으면 첨부 칩이 생겨요. → [드래그 첨부](../editor/drag-attach.md)
  - 작업 중에는 멈춤(Esc)을 보낼 수 있어요.
  - 기록에 아무것도 남기지 않는 명령(`/help` 같은 TUI 대화창)을 보내면 턴이 시작되지 않아 "생각 중"으로 남아요. 이때 멈춤을 누르면 Esc로 대화창이 닫히고, 2초 뒤에도 보낸 뒤로 기록에 새 이벤트가 없으면 상태를 대기로 돌려요(`Agent::interrupt`).
- **승인:** 권한 확인이 필요하면 입력창 위에 승인 카드가 떠요. → [승인](approvals.md)
- **터미널이 답을 기다릴 때:** 에이전트의 터미널이 자기 선택지(권한 질문 등)를 띄우고 있으면 입력창 위에 "…이(가) 터미널에서 답을 기다려요" 띠와 "터미널에서 답하기" 버튼이 떠요(누르면 터미널 보기로 바뀌어요). 입력창은 잠겨요: Enter와 보내기가 안 되고 안내가 "터미널에서 먼저 답해 주세요 · 지금 보내면 그 선택지에 들어가요"로 바뀌어요. 쓰던 글은 남아요. 중단 버튼은 그대로예요(Esc가 "아니요"로 답해요). → [승인](approvals.md)
- **다시 시작·대화 전환:** 에이전트가 다시 시작되거나, 트랜스크립트가 짧아지거나, 다른 대화를 읽게 되면(새 Antigravity 대화를 가져올 때. `/clear`·더 최근 agy 대화 따라가기는 R3에서 꺼져 있어요) 목록을 새로 받아 그려요. 보이는 모습은 같아요.
**보기(채팅 / 터미널).** 보기를 정하는 값은 둘뿐이에요.
- **에이전트 보기**(설정 → 일반, `agentView`): 앱 전체의 기본이에요. 전체 화면, 캔버스 블록, 분리 창이 모두 이 값을 따라요. 바꾸면 이미 열린 패널도 그 보기로 바뀌지만 돌고 있는 대화를 옮기지는 않아요: 채팅이 대화를 쥔 채 터미널로 바뀐 패널에는 터미널 위에 "지금은 채팅이 대화를 쓰고 있어요 · 터미널로 넘기기" 띠가 떠요(눌러야 옮겨요). 꺼져 있던 에이전트는 다음에 뜰 때 새 보기로 시작해요. 바꾸면 아래 예외도 모두 없어져요.
- **이 에이전트만 잠시**(스토어 `viewOverride`): 헤더 아이콘, 헤더·사이드바 행 ⋯의 "터미널로 보기 / 채팅으로 돌아가기", "터미널에서 답하기"(승인 카드, 터미널 질문 띠, 확인할 일 팝오버)가 그 에이전트 하나만 바꿔요. 메모리에만 있어서 앱을 다시 켜면 없어지고, 다른 에이전트로 새지 않아요. 앱의 창(메인, 분리 창)끼리는 같이 써요(`view://override`, 새로 뜬 창은 `view://hello`로 받아 와요). 에이전트 보기와 같은 쪽으로 돌아가면 예외가 지워져요.
- 예외로 터미널을 보는 동안(에이전트 보기는 채팅) 터미널 위에 28px 띠 "이 에이전트를 잠시 터미널로 보고 있어요 · 채팅으로 돌아가기"가 떠요.
- **헤더 아이콘으로 바꾸기:** 채팅 런타임이 있는 에이전트(Claude Code)가 돌고 있으면(ACP든 터미널이든) 먼저 대화를 옮기고(`switch_runtime`) 보기가 바뀌어요("전환 중…", 턴 중이면 "턴이 끝나면 전환돼요 · 지금 중단하고 전환"). 에이전트가 꺼져 있을 때만 보기가 바로 바뀌고, 새 보기가 뜨면서 에이전트를 그 보기의 런타임으로 띄워요(시작 오류는 그 보기의 "다시 시도"로). 전환을 기다리는 사이 그 패널이 닫히면(다른 워크트리를 전체 화면으로 열었을 때 등) 늦게 끝난 전환은 보기를 바꾸지도, 무엇을 묻지도 않아요(대화는 옮겨졌어요). → [채팅 런타임](chat-runtime.md)
- **메뉴와 "터미널에서 답하기"로 바꾸기:** 대화를 옮기지 않고 예외만 둔 뒤 그 에이전트를 고정 탭으로 열어요. 터미널이면 전체 화면으로 열어요(캔버스에는 그 에이전트 자신의 터미널이 없어요). 채팅이 대화를 쥐고 있으면 터미널 위 띠의 "터미널로 넘기기"로 옮겨요. 터미널 질문과 터미널에 넘긴 승인은 이미 터미널에서 돌고 있어서 띠가 없어요.
- 백엔드는 보기를 에이전트마다 기억하지 않아요. 화면 없이 시작하는 에이전트(마에스트로의 위임, 자리 비움 뒤 재개, 빠른 프롬프트 등)는 에이전트 보기를 따라요. → [채팅 런타임](chat-runtime.md)

**채팅 런타임(ACP, Claude Code):**
- 조건이 갖춰지면 채팅 뷰는 ACP 어댑터로 돌아요. 답이 쓰이는 대로 글이 흘러나오고(실시간 글), 블록이 끝나면 완성본으로 바뀌어요.
- 터미널 방식으로 넘어간 이유는 헤더의 "터미널로 실행 중" 칩 툴팁에 있어요(예: "Node.js 22 이상이 없어 터미널 방식으로 실행 중이에요"). ACP로 돌면 헤더 에이전트 툴팁에 "채팅: Claude Agent(ACP)"가 보여요. 캔버스 아래 입력창 머리에도 이름표 옆에 같은 칩이 붙어요(UI 구조 재구성 단계 11에서 예전의 작은 "· 터미널"을 바꿨어요).
- `!` 셸 명령과 채팅에서 안 되는 슬래시 명령은 보내지 않고 터미널에서 하라는 안내를 돌려줘요(백엔드 오류 글은 아직 "CLI 보기에서"예요). 입력창 안내도 "… · 셸 명령(!)은 터미널에서"로, `!`를 치면 "채팅에서는 셸 명령(!)을 쓸 수 없어요 · 터미널에서 실행"으로 바뀌고, 중단 버튼 툴팁에서 "(터미널의 Esc)"가 빠져요. Stop은 `session/cancel`이에요. → [채팅 런타임](chat-runtime.md)
- 채팅 뷰가 뜰 때는 에이전트가 꺼져 있을 때만 ACP로 띄워요(`start_view`). 이미 돌고 있는 대화는 터미널에서 쉬고 있어도 그대로 두고, 헤더의 채팅 아이콘을 눌러야 ACP로 옮겨요. 그때까지는 터미널의 트랜스크립트로 채팅해요.

**한글 입력**
- 한글을 조합하는 중에 누른 Enter는 보내지 않아요. 조합이 끝난 뒤 다시 누르면 보내요.
- 채팅 입력창뿐 아니라 Enter로 동작하는 모든 곳(문서 이름, 구조 블록 제목, 병합 거부 사유, 프롬프트 블록, 카드 입력창)에 같은 규칙을 써요.

## 구현

- `AgentPanel`(AgentNode.tsx): 헤더(`header.agent-head`)와 본문이에요. 캔버스 블록(`AgentNode`)과 전체 화면(`AgentFull`, `inTab`)이 같이 써요. 보기는 props로 받지 않고 `useAgentView(path)`로 정해요.
- 보기 모델(UI 구조 재구성 단계 12)
  - 순수 `src/settingsMigrate.ts`: `effectiveView(path, override, global)` = `override[viewKey(path)] ?? global`(`viewKey`는 슬래시·끝 슬래시·대소문자를 맞춘 경로), `withOverride(override, path, view | null, global)`(전역과 같거나 null이면 지움), `viewSwitch(view, global)` → `{ to, label, menu }`(헤더 아이콘 툴팁과 메뉴 글). `migrateSettings`(설정 v3)도 여기 있어요. `npm test`의 `scripts/view-check.ts`가 확인해요.
  - 스토어 `viewOverride`, `setViewOverride(path, view | null)`, 훅 `useAgentView(path)`. `setViewOverride`는 다른 창에 `view://override { from, path, view }`를 보내고, 창마다 모듈이 뜰 때 `view://hello`를 보내 다른 창의 `view://overrides` 전체를 받아요(자기 창 것은 `ME`로 걸러요). `useSettings.subscribe`가 `agentView`가 바뀌면 예외를 모두 지워요(다른 창에서 바꾼 것은 storage 이벤트의 rehydrate로 와요).
  - 헤더 아이콘과 ⋯ 항목은 `useRuntimeSwitch(wt, v => setViewOverride(path, v)).toggle(flip.to)`. 전환 로직은 그대로예요. 사이드바 행 메뉴와 "터미널에서 답하기"는 `agentActions.ts` `showAgentAs(path, view)`(예외 + `select(path, true)` + 터미널이면 그 탭 `setTabLayout(.., "full")`). 캔버스 아래 입력창(`AgentBar`)의 터미널 질문 버튼은 `setViewOverride(path, "cli")` 뒤 전체 화면으로 가요.
  - `ViewBand`(AgentNode.tsx, `.view-band` 28px): 보기가 `cli`이고 전역과 다를 때 터미널 위에 그려요. 버튼은 헤더 아이콘과 같은 `toggle("chat")`.
  - 노드 `data.view`는 읽지도 쓰지도 않아요(`types.ts`에서도 뺐어요). 예전 캔버스 파일에 남은 값은 무시돼요.
  - 메인 창 `App`이 시작할 때와 `agentView`가 바뀔 때 `ipc.setViewDefault(agentView)`(`set_view_default`)를 불러요. → [Tauri 명령](../../reference/tauri-commands.md)
  - `inTab`이면 글리프와 이름을 그리지 않고 `AgentKind`(로고 + 이름)와 브랜치를 그려요. 블록은 글리프, 이름, 로고만.
  - `HeadChips`: 권한(`Worktree.permissionMode`, 이름은 agents.tsx `permissionOf`, 누르면 `openAgentSettings({ path, focus: "permission" })`), 모델(`Worktree.model`, `focus: "model"`), 마에스트로 병합(`mergeMode`, `openSettings({ section: "projects", root })`), 폴백(`runtime === "pty"`이고 `runtimeNote.message`가 있고 `kind !== "forced"`). 폴백 칩은 `openSettings({ section: "agents", agent: adapterId })`.
  - 헤더가 CSS 컨테이너(`container-type: inline-size`)라 `@container (max-width: 419px)`에서 `.head-chips`를 숨겨요.
  - ⋯는 `openMenu`(ui/Menu.tsx). 분할·새 창·복사는 `agentActions.ts`(사이드바 행 메뉴와 같은 함수), 폐기는 `useDiscard`(discard.tsx).
  - `Worktree.permissionMode`는 `list_worktrees`가 이미 보내던 `WorktreeInfo.permissionMode`를 스토어로 옮긴 거예요(`fromInfo`).
- `useAgentBoot(root, path, enabled, view)`: `view`("chat", 채팅 런타임이 있는 에이전트의 `ChatBody`만)가 있으면 먼저 `start_view(.., view)`를 부르고(꺼져 있으면 그 보기의 런타임으로 띄우고, 돌고 있으면 그대로 둬요. 결과·오류는 무시) `ipc.agentReady`를 불러 `starting` → `ready` / `trust` / `{error}` 상태를 만들어요. `failure`(로그인 필요, 시작하다 종료, 시간 초과)나 시작 오류(실행 파일 없음)는 `{error}`로 보여 주고, 채팅 뷰에는 "다시 시도"와 "터미널에서 보기" 버튼이 있어요(누르면 헤더 아이콘과 같은 전환). 로딩 문구와 신뢰 확인 문구에는 에이전트 이름(`agentLabel`)이 들어가요. 결과의 `runtime`/`runtimeNote`는 스토어의 `Worktree`로 가요. → [에이전트 실행 방식](../../architecture/agent-runtime.md)
- 화면 모델(인터페이스 PRD I1): 순수 `src/chatFold.ts`. 공통 이벤트를 하나씩 접는 리듀서 `reduceChat(상태, { epoch, seq, event, at })`와, 기록을 한 번에 접는 `loadChat({ epoch, seq, events })`예요. 이벤트 → 항목 표는 [공통 이벤트](../../architecture/runtime-events.md#화면-모델chatfoldts)에 있어요.
  - 항목(`ChatItem`): user, text, thinking, tool, approval, error, notice, turnStart·turnEnd(실시간으로 본 턴은 `ms`), unknown(원본 `raw` 그대로, 모르는 이벤트 type도 통째로). 항목마다 `key`(받은 순서로 매긴 번호, 같은 기록이면 같은 키)와 `turnId`가 있어요.
  - 상태(`ChatState`): `epoch`, `seq`, 지난 턴마다 배열 하나(`turns`), 마지막 턴(`tail`), 마지막 턴 id(`turn`), `durable`(접은 durable 이벤트 수).
  - 그리는 것은 지금까지와 같아요(`isDrawn`): 사용자 말풍선, 빈 글이 아닌 답 글, 도구 줄, notice 줄(`failed`면 빨간 줄: 채팅 런타임이 실패로 끝낸 턴의 이유). 생각, 승인, 오류, 턴 경계, unknown은 항목으로만 있고 I2 카드에서 그려요. `turnEnded`를 그리지 않는 이유는 그대로예요(터미널 방식의 API 오류는 이미 그 턴의 `agentText`).
  - `lastText(상태)`: 끝에서부터 빈 글이 아닌 답 글이나 사용자 글의 마지막 세 줄(작게 줄인 블록, 워커 카드). `hasDrawn(상태)`: 그릴 게 있는지(없으면 "무엇을 할지 아래에 입력하세요"). `chatItems(상태)`: 평평한 목록(테스트용).
- `useChat(root, path)`(AgentNode.tsx, 예전 `useTranscript`): 리스너를 먼저 붙이고 `get_transcript`로 `{ epoch, seq, events }`를 받아 `loadChat`으로 접어요. 그 뒤 `agent://event`를 이렇게 다뤄요.
  - 받는 중이면 받은 시각과 함께 줄 세워 뒀다가 받은 뒤 처리해요.
  - epoch가 더 크면(`reset` 또는 새 스트림) 줄 세우고 `get_transcript`를 다시 불러요.
  - 나머지는 `reduceChat`에 넣어요. 옛 epoch와 받은 것 이하의 `seq`는 리듀서가 버려요. 상태가 바뀌었을 때만 다시 그려요. 상태는 setState 갱신 함수 밖(효과의 변수)에 둬서 이벤트 하나를 한 번만 접어요.
  - `seq: null`인 글·생각 `delta`(채팅 런타임의 실시간 글, 항목 `live:<n>:<k>`)는 실시간 항목이 되고, 완성 블록이 오기 직전의 빈 `offset: 0` 델타로 빠져요. durable `turnEnded`로는 빼지 않아요. Claude는 `end_turn`을 생각 줄에 먼저 쓰고 답 글 줄을 그 뒤에 써서, 턴 끝이 답이 아직 흐르는 중에 올 수 있어요.
  - 카드 요약(WorktreeCard.tsx)은 `durable`을 계기로 써요. 실시간 델타로는 바뀌지 않아요 → [답변 요약](answer-summaries.md)
  - 보기 전환은 새 epoch(`reset switch`)를 보내서 목록을 다시 받아요. 전과 같은 트랜스크립트라 항목과 키가 같아요.
- `Messages`: 지난 턴은 `Turn`(memo) 하나씩, 마지막 턴은 그 뒤 `Turn` 하나로 그려요. 마지막 턴의 이벤트는 지난 턴 배열을 바꾸지 않아서 지난 턴은 다시 그리지 않아요. 마지막 턴의 `Turn` 키는 턴 번호라 다음 턴이 시작돼 지난 턴이 되어도 같은 컴포넌트로 남아요. 줄(`Row`, memo)은 항목 `key`로 그려서, 위의 실시간 글이 빠져도 아래 도구 줄의 펼침이 유지돼요(예전에는 순서 번호가 키였어요).
- 확인: `npm test`의 `scripts/chat-fold-parity.ts`가 Rust 테스트가 적은 픽스처(`src-tauri/tests/fixtures/chat-parity.json`, R3 이전 fold의 항목)를 `loadChat`과 `reduceChat`으로 접어 그리는 항목이 같은지, 두 길의 항목이 같은지 봐요. 픽스처의 `acp-failed` 경우(실패·중단으로 끝난 ACP 턴)는 R3 이전에 없던 것이라 Rust 사본(`oracle::items`)의 항목과 비교해요. 같은 스크립트가 실시간 델타도 봐요(흐르는 동안 보임, 지운 뒤 완성 블록만, 생각 델타는 안 그림, 답 중간의 `turnEnded`는 글을 남김). `scripts/chat-reduce-check.ts`가 리듀서를 봐요: 델타 이어 붙이기(UTF-16, 같은 델타 두 번은 그대로), 도구 갱신, epoch 초기화, 중복 seq, 모르는 이벤트 보존, 턴 소요 시간, 지난 턴 배열 공유, 승인, 입력 상태 불변. `useChat`의 다시 받기는 테스트가 없어요.
- `ChatBody`, `Messages`, `RichText`(react-markdown + remark-gfm), `ToolRow`, `Composer`가 화면을 나눠 맡아요.
  - `Composer` 첫 글: 마운트할 때 `useState(() => peekSeed(path))`로 스토어 `composerSeed`를 읽어 입력칸을 시작하고, 씨앗이 있었으면 마운트 효과에서 `dropSeed(path)` 후 입력칸에 포커스해요. 렌더 중에 스토어를 바꾸지 않으려고 읽기와 지우기를 나눴어요(계획의 `takeSeed`). 키는 경로를 대소문자·슬래시 무시로 맞춘 것이에요.
  - `ToolRow`는 펼쳤을 때만 `toolPreview(name, input)`(toolPreview.ts)을 계산해요. 결과가 있으면 `ToolPreviewView`(ToolDiff.tsx, 승인 카드와 같은 것)로 그리고 입력 JSON은 빼요. null이면 예전처럼 입력 JSON이에요. `npm test`가 `scripts/tool-preview-check.ts`로 이 미리보기도 확인해요.
- 보내기: `ipc.send(root, path, text, files)` → `send_message`(main.rs) → `wait_ready()` 뒤 `Agent::send`예요. 신뢰 확인 중이거나 시작에 실패했으면 그 이유로 에러를 돌려줘요.

- 한글 조합 판별: `isImeComposing(e)`(src/ime.ts). `isComposing`이 참이거나, `keyCode`가 229(IME 처리 중)이거나, `compositionend` 직후 20ms 안의 Enter면 조합 중으로 봐요. 일부 WebView는 조합을 끝내는 Enter를 `isComposing=false`, `keyCode=229`로 보내서 `isComposing`만으로는 덜 조합된 글자가 전송됐어요(Cline도 같은 두 조건을 봐요). Enter로 동작하는 핸들러는 모두 이 함수를 먼저 불러요.

## 제약과 알려진 문제

- 트랜스크립트에 남지 않는 TUI 화면(일부 대화상자 등)은 채팅 뷰에 안 보여요. CLI 뷰로 봐야 해요.
- 에이전트를 띄울 때마다(`reset restart`)와 채팅↔CLI 전환 때마다(`reset switch`) 열린 채팅 뷰가 기록을 한 번 더 받아요.
- 리스너가 붙기 전에 시작된 실시간 글은 앞부분 없이 보일 수 있어요. 완성 블록이 오면 바로잡혀요.
- 메인 창이 에이전트 보기를 알리기 전(앱이 막 켜진 순간)에 화면 없이 시작한 에이전트는 채팅으로 시작해요. 에이전트 보기를 터미널로 둔 사람은 그런 시작이 채팅으로 뜰 수 있어요.
- 같은 에이전트를 두 창에서 볼 때 예외는 같이 바뀌지만, 대화를 옮기는 전환은 누른 쪽 패널에서만 진행 표시가 보여요.
- 사이드바 메뉴로 터미널을 고르면 채팅이 대화를 쥐고 있을 때 "터미널로 넘기기"를 한 번 더 눌러야 해요(메뉴는 패널 밖이라 전환 진행을 보여 줄 곳이 없어요).
- 메시지 목록은 아직 가상화하지 않았어요(I4). 리듀서와 지난 턴의 그리기는 이벤트마다 반복되지 않지만, 마지막 턴이 아주 길면(도구 수백 개) 그 턴의 줄은 이벤트마다 비교해요(바뀐 줄만 다시 그려요).
- 승인 항목은 실시간으로 본 것만 있고, 턴 소요 시간은 시작과 끝을 둘 다 실시간으로 받은 턴에만 있어요(이벤트에 시각이 없어요). 둘 다 I2 전에는 그리지 않아요.

## 결정 기록

- 2026-11-13 (인터페이스 I1): 이벤트가 올 때마다 목록 전체를 다시 접던 `fold(events)`와 항목 4종(`Item`)을 순수 리듀서 `reduceChat`과 항목 10종(`ChatItem`)으로 바꿨어요. 화면은 I2 전까지 그대로예요. 항목을 턴마다 나눠 둔 이유는 불변 상태에서 이벤트 하나의 일을 마지막 턴 크기로 묶기 위해서예요(평평한 배열이면 이벤트마다 전체를 복사해요). 같은 이유로 화면도 턴마다 memo로 그려서 지난 턴은 다시 그리지 않아요. 실시간 델타를 이벤트 목록에 붙이던 `addDelta`는 리듀서의 실시간 항목(`live`)으로 바뀌어 없앴어요. 프런트 테스트 러너는 따로 들이지 않고 지금처럼 Node 타입 제거 실행 스크립트(`scripts/chat-reduce-check.ts`)를 `npm test`에 이었어요.
- 2026-11-13 (UI 구조 재구성 단계 12): 보기를 기억하는 곳이 셋(전역 `fullView`, 노드 `data.view`, 백엔드 `last_view`)이라 한 에이전트에서 CLI를 고르면 다른 에이전트나 다음 시작이 터미널로 떴어요. 이제 설정 `agentView` 하나와 메모리의 "이 에이전트만" 예외 하나예요. 헤더의 Chat/CLI 세그먼트는 아이콘 하나로 바꿨어요. 화면 버튼은 전역을 쓰지 않아요(`set({ agentView })`는 설정 일반 섹션 한 곳). 전역을 바꿀 때 열린 패널의 대화를 모두 옮기면 전환 확인 창이 여러 개 겹칠 수 있어 보기만 바꾸고 옮기기는 터미널 위 띠에 맡겼어요. 전역을 바꾸면 예외를 모두 지워요(값이 둘뿐이라 남은 예외는 새 전역과 같아지고, 남겨 두면 다시 돌아올 때 "왜 이것만"이 생겨요). 예외는 창끼리 이벤트로 나눠요(같은 에이전트를 분리 창으로 꺼내도 예외가 따라가요). 문구 "Chat", "CLI 보기에서", "CLI에서 보기"를 "채팅", "터미널에서", "터미널에서 보기"로 바꿨어요.
- 2026-11-13 (UI 구조 재구성 단계 6): 헤더를 32px로 고정하고, 탭 안에서는 이름과 상태를 뺐어요. 탭이 이미 보여 주는 정보를 두 번 그리지 않고, 탭이 못 보여 주는 브랜치·권한·병합 방식을 그 자리에 뒀어요. 휴지통 버튼은 레이아웃 토글 바로 옆에서 늘 보이던 파괴 동작이라 ⋯ 메뉴로 옮겼어요(폐기가 한 번 더 눌러야 하는 건 일부러예요). 메뉴에 "워커 추가"도 뒀어요(워커 캔버스의 `+ 워커`를 없애는 단계 11의 대신 입구). Chat/CLI 버튼은 보기 모델을 바꾸는 단계 12까지 그대로 둬요.
- 2026-11-13 (UI 구조 재구성 단계 5): 터미널 질문 띠의 버튼 "CLI 보기에서 답하기"와 잠긴 입력창 안내 "CLI 보기에서 먼저 답해 주세요"를 용어 표대로 "터미널에서 답하기", "터미널에서 먼저 답해 주세요"로 바꿨어요. 승인 카드와 확인할 일 팝오버의 버튼 이름과 맞췄어요. 셸 명령(!) 안내의 "CLI 보기에서"는 보기 이름을 바꾸는 단계 12에서 같이 바꿔요.
- 채팅 화면을 먼저 띄우고 나중에 CLI가 뜨면 그 사이 입력이 사라졌어요. 그래서 CLI가 준비될 때까지 입력창을 보여 주지 않아요.
- 2026-11-13 (UI 구조 재구성 단계 8): 권한·모델 칩을 버튼으로 바꿔 에이전트 설정 창을 열게 했어요(만든 뒤 권한 바꾸기 흐름 F10a: 칩 → 고르기 → 저장하고 다시 시작 = 3번). 모델 칩을 이제 그려요. 새 에이전트 창의 할 일은 이 입력창에 넣어 두기만 해요. 채팅 본문(메시지, 카드)은 이 단계에서 건드리지 않았어요.
