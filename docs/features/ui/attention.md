---
title: 확인할 일 (상단 막대 팝오버, 창 제목, 작업 표시줄)
area: ui
status: done
files: [src/attention.ts, src/nodes/Attention.tsx, src/nodes/ApprovalCards.tsx, src/agentActions.ts, src/nodes/PrCard.tsx, src/store.ts, src/App.tsx, src/Workspace.tsx, src/Sidebar.tsx, src/sidebarModel.ts, src/status.tsx, src/nodes/WorktreeCard.tsx, src/shortcuts.ts, src/ui/useShortcuts.ts, src/styles.css, src-tauri/capabilities/default.json, scripts/attention-check.ts]
related: [features/agents/approvals.md, features/orchestration/merge-request.md, features/orchestration/pr-mode.md, features/ui/window-frame.md, features/ui/side-panels.md, features/ui/shortcuts.md, features/ui/terms.md, features/ui/quick-switch.md]
updated: 2026-11-13
---

# 확인할 일

## 요약

사용자를 기다리는 것(도구 승인, 터미널 질문, 마에스트로의 병합 요청, 손이 필요한 PR)을 한 목록으로 모아요. 상단 막대의 종 버튼에 개수가 뜨고, 누르면 400px 팝오버에서 프로젝트별로 보고 그 자리에서 승인하거나 그 에이전트로 가요. 메인 창과 분리 창 모두에 있어서 프로젝트를 접거나 사이드바를 숨겨도 놓치지 않아요. 창 밖에서는 창 제목 앞 "(N) "과 작업 표시줄 깜박임으로 알아요. Ctrl+J는 다음 항목으로 가요.

## 동작

**목록에 들어가는 것**

| 종류 | 언제 | 누구의 것(열기) |
| --- | --- | --- |
| 도구 승인 | 에이전트가 도구 권한을 물을 때(채팅 카드와 같은 것) | 물은 에이전트 |
| 터미널 질문 | 에이전트의 터미널이 답을 기다릴 때(카드를 터미널로 넘김, 시간 초과, 화면의 선택지) | 그 에이전트 |
| 병합 요청 | 마에스트로가 `maestro merge-request`로 병합을 요청했을 때 | 요청한 마에스트로 |
| PR | PR 모드에서 워커의 열린 PR이 CI 실패했거나 지금 병합할 수 있을 때(초안·충돌·대기열·리뷰 필요·병합 진행 중·숨긴 PR은 빼요) | 마에스트로(없어졌으면 그 워커) |

- 새 응답(읽지 않음)은 넣지 않아요. 사이드바와 탭의 상태 글리프가 맡아요.
- 순서: 프로젝트(사이드바 순서) → 종류(승인, 터미널 질문, 병합 요청, PR) → 들어온 순서.

**상단 막대 버튼**
- `Bell` 16 + 개수 알약(경고색 글자, 같은 색 16% 면). 0이면 종만 흐려요. 툴팁 "확인할 일 (Ctrl+J 다음)".
- 개수는 팝오버에 있는 항목 수 전부예요(PR 포함).

**팝오버**(버튼 아래 오른쪽 정렬, 폭 400, 최대 높이 `min(70vh, 640px)`)
- 프로젝트마다 24px 머리(11px 흐린 글, 오른쪽 개수). 한 건은 세 줄이에요: 아이콘 + 이름(13/500), 내용(12px), 버튼(높이 24).

| 종류 | 1줄 | 2줄 | 버튼 |
| --- | --- | --- | --- |
| 도구 승인 | `MessageCircleQuestion` + 에이전트 이름 | 쉐브론 + 도구 이름 + 한 줄 요약(명령·파일·주소, 모노). 쉐브론을 누르면 채팅 카드와 같은 미리보기(파일 변경은 diff, 최대 240px 스크롤) | [허용] · [항상 허용](제안이 있을 때) · [거부] · 오른쪽 [열기] |
| 터미널 질문 | `SquareTerminal` + 이름 | "터미널에서 답을 기다려요"(도구를 알면 " · Bash") | [터미널에서 답하기] · [열기] |
| 병합 요청 | `GitMerge` + 워커 이름 | "<브랜치> → <기준> · 파일 N · 검증 있음/없음" | [변경 보기] · [열기] |
| PR | `GitPullRequest`(CI 실패면 빨강) + 워커 이름 | "PR #12 · CI 실패" 또는 "PR #12 · 병합할 수 있어요" | [열기] |

- [열기]: 그 에이전트를 고정 탭으로 열고 팝오버를 닫아요(열리는 모습은 그 탭의 전체 화면/캔버스).
- 허용·거부를 누르면 그 줄이 바로 빠지고 팝오버는 열린 채로 남아요. 같은 승인은 채팅 카드에서도 사라져요.
- [터미널에서 답하기]: 팝오버를 닫고 그 에이전트를 전체 화면 터미널 보기로 열어요. 채팅/터미널 보기가 에이전트마다 되기 전(UI 구조 재구성 단계 12)까지는 전체 화면의 보기 설정을 터미널로 바꾸는 지금 방식이에요(캔버스 입력 바의 버튼과 같아요).
- [변경 보기]: 팝오버를 닫고 병합 요청의 비교 창(병합 요청 카드의 "변경 보기"와 같은 창)을 열어요. 툴팁은 "파일별 변경 · 병합과 거부는 마에스트로의 병합 요청 카드에서 해요"예요. 병합과 거부는 팝오버에서 하지 않아요. 마에스트로의 병합 요청 카드에서 해요. → [병합 요청](../orchestration/merge-request.md)
- 빈 상태: "확인할 일이 없어요" + "에이전트가 승인을 묻거나 마에스트로가 병합을 요청하면 여기에 모여요".
- 바깥을 누르거나 Esc로 닫혀요.

**같은 목록을 쓰는 곳**
- 상단 막대 개수, 창 제목 "(N) ".
- 사이드바 접힌 프로젝트 머리 "승인 대기 N": 에이전트를 멈춰 세우는 것(승인·터미널 질문·병합 요청)만 세요. PR은 빼요. → [사이드바](side-panels.md)
- 사이드바 세션 행 둘째 줄 "승인 대기 · Bash", "터미널 질문", "병합 요청 N".
- 상태 글리프의 "승인 대기"(물음표 말풍선): 그 에이전트(병합 요청은 마에스트로)에 멈춰 세우는 항목이 있을 때.
- Ctrl+J: 지금 보는 에이전트의 마지막 항목 다음 것을 고정 탭으로 열어요. 지금 에이전트에 항목이 없으면 처음 것, 끝이면 처음으로 돌아요. 입력칸·편집기·터미널 안이나 대화상자가 떠 있으면 동작하지 않아요. → [단축키](shortcuts.md)

**창 밖**
- 창 제목: 항목이 N개면 "(N) Maestro", 없으면 "Maestro". 개수가 바뀔 때만 바꿔요. 창마다 자기 제목을 바꿔요(분리 창도 같은 개수).
- 작업 표시줄: 메인 창만, 에이전트를 멈춰 세우는 항목 수가 늘었고 창에 포커스가 없을 때 한 번 깜박여요(`Informational`: 포커스를 빼앗지 않아요). PR 상태가 바뀐 것만으로는 깜박이지 않아요.

## 구현

- **모델** `src/attention.ts`(순수, `import type`만):
  - `attentionItems({ approvals, tuiWaits, merges, prs, worktrees, repos })` → `AttentionItem[]`. 항목은 `{ id, kind, root, path, name, title, detail?, tool?, approvalId?, mergeId?, worker?, files?, failed?, at? }`. `path`는 [열기]가 여는 에이전트(병합 요청·PR은 마에스트로)이고 사이드바 행도 이 경로로 항목을 찾아요. `root`는 워크트리 목록에서, 없으면 경로가 프로젝트 폴더인 프로젝트에서 찾아요(마에스트로). 못 찾으면 `""`이고 맨 뒤 "프로젝트 밖" 묶음이에요.
  - 정렬은 `sort`(안정 정렬)로 프로젝트 순서 → `KIND_ORDER` 순. 같은 종류 안은 스토어 배열 순서(들어온 순서)예요.
  - `samePath`, `normPath`(대소문자·슬래시·끝 슬래시 무시). `store.ts`가 `samePath`를 다시 내보내요.
  - `approvalSummary(a)`: 한 줄 요약(`command` → `file_path` → `url` → `pattern` → `path` → JSON). 채팅 카드의 대체 글도 이걸 써요.
  - `ciOf(pr)`: 보이는 머리 커밋의 CI 상태(다른 커밋의 결과면 `pending`). PR 카드·칩·사이드바·워커 카드가 써요(예전 `PrCard.tsx` `ciState`).
  - `prMergeable(pr)`, `prNeedsHand(pr)`: 열려 있고 숨기지 않았고 (CI 실패 또는 병합할 수 있음). 병합할 수 있음 = 초안·대기열·병합 진행 중·충돌이 아니고 CI 통과, 변경 요청·리뷰 필요·`BLOCKED`가 아님.
  - `isBlocking`(PR이 아님), `blockingCount`, `countByRoot`, `waitsAt(items, path)`, `nextItem(items, currentPath)`, `groupByRoot(items)`.
  - `cardItem(items, path)`: 워커 카드 확인 줄에 보일 한 건. 목록 순서로 처음 만나는 것 중 그 워커의 도구 승인·터미널 질문(`path`가 그 워커), 또는 그 워커 브랜치의 병합 요청(`worker`가 그 워커, `path`는 마에스트로)이에요. PR은 넣지 않아요(카드의 PR 칩이 보여 줘요).
- **스토어**(`store.ts`): `selectAttention(s)`가 목록을 만들어 두고, 읽는 것(`approvals`, `tuiWaits`, `merges`, `prs`, `repos`의 참조와 워크트리의 root·path·name)이 그대로면 같은 배열을 돌려줘요. 워크트리는 상태가 바뀔 때마다 새 배열이라 이름까지만 비교해요. 그 비교용 문자열(root·path·name)은 워크트리 배열 하나에 한 번만 만들어요(`worktreesKey`). 세션 행·상태 글리프·프로젝트 요약이 스토어가 바뀔 때마다 모두 부르므로, 같은 갱신 안의 두 번째 호출부터는 배열 참조만 보고 넘어가요. 승인·터미널 질문·병합 요청·PR이 하나도 없으면 워크트리를 보지 않고 늘 같은 빈 배열을 돌려줘요. `useAttention()`, `useAttentionCount()`(`{ all, blocking }`, `useShallow`), `useWaiting(path)`(= `waitsAt`).
- **화면**(`src/nodes/Attention.tsx`):
  - `AttentionButton`: 버튼과 팝오버. 메인 창(`App.tsx`)과 분리 창(`Workspace.tsx` `WorkspaceWindow`) 막대에 있어요. 열림 상태는 창마다 스토어 `attentionOpen` / `setAttentionOpen`이라 빠른 이동(Ctrl+K)의 "확인할 일"도 같은 팝오버를 열어요(→ [빠른 이동](quick-switch.md)). 바깥 누름·Esc로 닫는 방식은 업데이트 팝오버와 같아요. 비교 창(`MergeDiffDialog`, 필요할 때 불러옴)은 팝오버 밖에서 `document.body` 포털로 그려서 팝오버가 닫혀도 남아요. 요청이 끝나면(`merges`에서 빠지면) 비교 창도 닫혀요.
  - `AttentionRow({ item, size: "full" | "compact", onOpen, onDiff, onTerminal? })`: 한 건. `onTerminal`은 [터미널에서 답하기]이고 기본값이 `answerInTerminal`이에요(팝오버는 닫고 나서 부르도록 넘겨요). `compact`(한 줄 32px, 12px)는 워커 카드 확인 줄이에요: 승인 = "도구 · 요약" + [허용] [거부] [열기](항상 허용은 뺌), 터미널 질문 = "터미널 질문" + [터미널에서 답하기] [열기], 병합 요청 = "병합 요청 · 파일 N" + [변경 보기] [열기].
  - `MergeDiff({ id, onClose })`: 병합 요청 하나의 비교 창(팝오버와 워커 카드가 같이 써요).
  - `ApprovalItem({ a, onOpenCli })`: 채팅 카드 한 장(`ApprovalCards`가 승인마다 그려요). `answerApproval(a, choice)`: `dropApproval` 먼저, 그다음 `ipc.answerApproval`. 백엔드 `Approvals::answer`는 이미 끝난 id(`pending.remove`가 없음)를 그냥 무시해서, 두 창에서 같은 승인을 눌러도 두 번 답하지 않아요.
  - `openAgent(path)` = `select(path, true)`. `answerInTerminal(path)` = `showAgentAs(path, "cli")`(agentActions.ts): 그 에이전트만 터미널(`setViewOverride`) → `select(path, true)` → 그 묶음의 탭을 전체 화면으로(`setTabLayout`). 설정 `agentView`는 쓰지 않아요.
- **창 밖**(`Workspace.tsx` `useWindowAttention(main)`): `App`은 `true`, `WorkspaceWindow`는 `false`로 불러요. 마지막으로 쓴 개수와 다를 때만 `getCurrentWindow().setTitle()`. 멈춰 세우는 개수가 이전보다 늘고 `document.hasFocus()`가 거짓이면 메인 창만 `requestUserAttention(UserAttentionType.Informational)`.
- **권한**(`src-tauri/capabilities/default.json`): `core:window:allow-set-title`, `core:window:allow-request-user-attention`. 없으면 실행 중에 거부돼요(오류는 조용히 무시해요).
- **워커 카드**(`WorktreeCard.tsx`): `cardItem(selectAttention(s), path)`가 있으면 아래 바로 지시 입력칸 자리(48px)에 `.wt-card-attn` 안 `AttentionRow size="compact"`를 그려요. 입력칸은 `hidden`으로 남겨 두어 쓰던 지시가 지워지지 않아요. 카드의 [열기]는 그 워커를 열어요(병합 요청이어도 마에스트로가 아니라 워커: 마에스트로는 이미 그 캔버스예요). 병합 요청이 있는 카드도 경고색 테두리(`data-waiting`)예요.
- **사이드바**: `rowMeta(wt, items)`(sidebarModel.ts)와 `useProjectSummary`가 `selectAttention`을 읽어요. → [사이드바](side-panels.md)
- **단축키**: `useShortcuts`의 `nextAttention` = `nextItem(selectAttention(s), selectedPath)` → `select(path, true)`.
- **시험**: `scripts/attention-check.ts`(`npm test`): 종류별 수집과 줄 글, 정렬(프로젝트 → 종류 → 순서), 경로 대소문자·슬래시(`C:\a\b` = `c:/a/b/`), `blockingCount`, `countByRoot`, `prNeedsHand`(열림+CI 실패 = 참, 열림+CI 진행 중 = 거짓, 병합됨 = 거짓 등), `nextItem`(현재 경로 다음, 끝에서 처음으로), `cardItem`(승인 → 터미널 질문 → 병합 요청, PR은 아님, 마에스트로 카드에는 없음). 행 둘째 줄의 우선순위는 `scripts/sidebar-check.ts`가 같은 목록으로 확인해요.

## 제약과 알려진 문제

- 앱을 켜고 끄며 실제로 눌러 보지 않았어요. 타입 검사, 빌드, 순수 함수 시험만 했어요(작업 표시줄 깜박임과 제목은 오너가 확인).
- 앱을 켤 때 이미 기다리던 승인을 불러오면서, 창에 포커스가 없으면 작업 표시줄이 한 번 깜박일 수 있어요.
- [터미널에서 답하기]는 그 에이전트 하나만 터미널로 보여요(앱을 켜 둔 동안). 다른 에이전트와 설정의 에이전트 보기는 그대로예요.
- PR 항목은 PR이 열려 있고 병합할 수 있는 동안 계속 남아요(리뷰를 기다리는 중이어도). 숨기려면 PR 카드의 "닫기"를 써요.
- 팝오버 안에서 키보드로 항목 사이를 옮기는 기능은 없어요(Tab으로 버튼을 오가요).
- 작업 표시줄 배지 아이콘과 OS 알림(알림 센터)은 없어요.

## 결정 기록

- 2026-11-13 (UI 구조 재구성 단계 12): [터미널에서 답하기]가 전역 보기 설정을 바꾸던 것을 그 에이전트의 예외(`viewOverride`)로 바꿨어요. 답한 뒤 다른 에이전트가 터미널로 열리던 누수가 없어졌어요.
- 2026-11-13 (UI 구조 재구성 단계 5): 확인할 일을 순수 모델 하나(`attention.ts`)와 상단 막대 팝오버로 만들었어요. 오른쪽 패널 탭은 분리 창에 없고 오너가 파일·변경 용도로 열어 두는 자리라 쓰지 않았어요. 사이드바 "대기만 보기" 필터나 "다음으로 이동"만 하는 버튼도 만들지 않았어요. 팝오버는 모든 창에서 되고, 모델은 사이드바·창 제목·Ctrl+J가 같이 써서 숫자가 어긋나지 않아요.
- 팝오버에서 바로 하는 일은 승인(허용·항상 허용·거부), 터미널 질문 열기, 병합 요청 변경 보기까지예요. 병합은 diff를 보지 않고 누르지 않게 카드와 [변경 보기] 비교 창 바닥에서만 해요(로컬 방식은 바닥의 [거부…] [병합], PR 방식은 카드). → [병합 요청](../orchestration/merge-request.md)
- 상단 개수와 제목은 PR을 포함한 전부, 접힌 프로젝트 "승인 대기 N"과 작업 표시줄 깜박임은 에이전트를 멈춰 세우는 것만 세요. 팝오버를 열었을 때 보이는 수와 버튼 숫자가 같아야 하고, PR의 CI 상태가 바뀔 때마다 작업 표시줄이 깜박이면 소음이라서예요.
- `useAttention`은 `useShallow` 대신 스토어 쪽 메모(`selectAttention`)로 같은 배열을 돌려줘요. 항목 객체가 매번 새로 만들어지면 얕은 비교로는 같다고 볼 수 없어서예요.
- 순수 모듈끼리는 서로 불러올 수 없어서(Node가 번들러 없이 시험을 돌림) `sidebarModel.ts`는 경로 비교를 자기 안에 따로 두고, 둘째 줄은 확인할 일 항목을 받아 판정해요.
- 이 단계에서 버튼 이름 "CLI에서 답하기", "CLI 보기에서 답하기"를 "터미널에서 답하기"로 바꿨어요(용어 표). 터미널 질문 때 잠긴 입력창 안내도 "터미널에서 먼저 답해 주세요"로 맞췄어요.
- 단계 5 확인 뒤 고친 것: [터미널에서 답하기]가 전체 화면 터미널을 연 뒤에도 팝오버가 그 위에 떠 있었어요. [열기]·[변경 보기]처럼 닫아요. `selectAttention`이 부를 때마다 모든 워크트리로 문자열을 만들어 갱신 한 번에 O(에이전트²)였어요. 워크트리 배열마다 한 번으로 줄였어요.
- 인터페이스 PRD의 U3(캔버스 위 승인 배너)는 이 팝오버와 워커 카드 확인 줄(단계 11)로 대체해요. 채팅 안 승인 카드와 노드 배지는 그대로예요.
- 2026-11-13 (UI 구조 재구성 단계 11): 워커 카드 확인 줄을 붙였어요. 한 카드에 한 건만 보이고(다음 건은 앞 건을 답하면 올라와요), 줄 하나 높이라 여러 건을 쌓지 않아요. 카드 크기(312×264)가 늘면 캔버스 배치가 겹쳐서예요. 카드의 [열기]는 팝오버와 달리 늘 그 워커예요.
