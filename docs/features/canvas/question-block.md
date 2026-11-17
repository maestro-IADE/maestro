---
title: 질문 블록 (maestro ask, 객관식·주관식으로 사람에게 묻기)
area: canvas
status: in-progress
files: [src/nodes/QuestionBlock.tsx, src/drawing.ts, src/Canvas.tsx, src/types.ts, src/ipc.ts, src/icons.tsx, src/styles.css, src-tauri/src/control.rs, src-tauri/src/canvas.rs, src-tauri/src/main.rs, src-tauri/src/agent.rs]
related: [features/canvas/structure-drawing.md, features/canvas/canvas-reading.md, features/orchestration/maestro-cli.md, features/orchestration/worker-done-notes.md, features/orchestration/maestro-agent.md, features/canvas/canvas-controls.md]
updated: 2026-11-17
---

# 질문 블록

## 요약

에이전트가 사람의 결정이나 입력이 필요할 때 `maestro ask`로 자기 캔버스에 질문 블록을 놓아요. 보기가 있으면 객관식, 없으면 주관식이에요. 사람이 블록에서 답하면 그 답이 `[maestro] 질문 … 답했어요` 메모로 그 에이전트의 입력에 들어가요. 답한 블록은 고른 것과 고르지 않은 보기를 함께 남겨서 그대로 결정 기록이 돼요. 결정 게이트를 따로 만들지 않고, 결정은 "객관식 질문에 답한 기록"으로 다뤄요. 사람도 질문 블록을 직접 만들 수 있어요: 질문과 보기를 정해 두고 골라 보내면, 이 캔버스의 에이전트에게 질문·보기·고른 것이 함께 가요(구조가 있는 입력).

## 동작

**에이전트가 묻기**: `maestro ask --title 질문 [--choices "A|B|C"] [--multi true] [--other true] [--body 설명]`

| 옵션 | 뜻 |
|---|---|
| `--title` | 질문(필수) |
| `--choices` | `\|`로 나눈 보기 2~8개. 있으면 객관식, 없으면 주관식. 같은 보기 두 번, 보기 하나, 9개 이상은 오류 |
| `--multi true` | 여러 개 고르기(객관식만. 보기 없이 쓰면 오류) |
| `--other true` | 보기 옆에 "기타" 글칸(객관식만. 주관식에서는 무시) |
| `--body` | 질문 아래 작은 설명 |

- 답: `{ asked: <질문 id>, kind: "객관식"|"주관식", note }`. 질문은 그리기처럼 열린 캔버스에 바로 놓이고, 닫혀 있으면 캔버스를 열 때 놓여요.
- 브리프(`MAESTRO_HINT`)가 모든 에이전트에게 "결정이나 입력이 필요하면 긴 답 속에 묻지 말고 캔버스에 물어라, 물었다고 답에 말하고 그 답이 필요한 일은 기다려라"라고 알려요.

**사람이 만들기**: 도구 막대 `블록` 패널의 "질문", 또는 빈 곳 우클릭 → "질문 블록 추가"(우클릭한 자리에).
- 만들면 편집 창이 바로 열려요: 질문(Enter로 닫기), 설명(선택), 보기(한 줄에 하나, 비우면 주관식, 같은 줄·빈 줄은 빠지고 8개까지), "여러 개 고르기"와 "기타 글칸"(보기가 있을 때만). 적는 대로 블록에 반영돼요.
- 답하기 전에는 더블클릭이나 우클릭 → "질문 편집…"으로 다시 열어요. 에이전트가 물은 질문은 고칠 수 없어요.
- 블록 머리는 "내 질문"이고, 상태는 "에이전트에게 보내요"(조용한 색)예요. 기다리는 에이전트가 없어서 경고색 테두리는 없어요. 질문이 비어 있으면 보내기가 눌리지 않아요.
- 보낸 메모에는 에이전트가 처음 보는 질문이라 질문·설명·모든 보기가 같이 가요:
```
[maestro] 사용자가 캔버스에 질문을 만들어 답했어요: '어느 DB?'
설명: 로컬 개발용
보기: Postgres | SQLite
고른 것: SQLite
```

**블록 모양**(폭 13칸 = 312px, 높이는 내용만큼)
- 머리: 물음표 아이콘, "질문", 오른쪽에 상태. 답을 기다리는 동안은 테두리와 상태 글이 경고색("답을 기다려요"), 답하면 조용한 "✓ 답함"(마우스를 올리면 답한 시각).
- 몸통: 질문(굵게), 설명(흐리게), 보기 버튼(하나 고르기는 동그라미, 여러 개는 네모 표시), 주관식이면 3줄 글칸, `--other`면 1줄 "기타" 글칸.
- 아래: "보내기"(고른 것이나 적은 글이 있어야 눌려요). 글칸에서 Enter는 보내기, Shift+Enter는 줄바꿈. 여러 개 고르기면 "여러 개 고를 수 있어요"가 보여요. 보내지 못하면 오류가 그 자리에 떠요.
- 하나 고르기에서 고른 보기를 다시 누르면 고르기가 풀려요.
- 답한 뒤: 버튼과 글칸이 잠기고, 고른 보기는 강조, 고르지 않은 보기는 흐리게 남아요. 적은 글은 "기타: …"(주관식은 그대로)로 보여요. 다시 답하기는 없어요.

**답 전달**: 보내기 → `answer_question(owner, id, title, picked, text)` → 그 에이전트에게 메모. 메모는 워커 완료 메모와 같은 길(`control::tell`)이라, 에이전트가 꺼져 있거나 입력창이 아직 안 떴으면 보관했다가 뜰 때 붙여요(질문마다 하나, 키 `answer:<id>`).
```
[maestro] 질문 '어느 DB?'에 사용자가 답했어요.
고른 것: SQLite
덧붙인 말: 나중에 Postgres로 옮길 수 있게
```
주관식은 `답: …` 한 줄이에요. 고른 것은 보기 순서대로예요(누른 순서가 아니라).

**캔버스에서**: 질문 블록은 캔버스 파일에 저장돼요(질문, 보기, 답). 우클릭 메뉴는 "지우기"예요. 연결점은 없어요(선으로 잇지 않아요). `maestro canvas`의 `questions`에 `{id, title, choices, multi, answer, box}`로 나와요(`answer`는 답 전에는 `null`, 답한 뒤 `{picked, text?, at}`).

## 구현

- CLI: `control.rs` `"ask"` 경로. `question(id, title, body, choices, multi, other)`가 검사하고 `{id, title, body?, choices, multi, other}`를 만들어요. id는 `q_<ms>_<순번>`. 그리기 큐(`state.drawings`)에 `{ ask }`로 넣고 `maestro://draw`를 보내요.
- 캔버스: 그리기 큐를 받는 곳(`Canvas.tsx`)이 `{ ask }`를 `applyAsk(q, scope, nodes)`(drawing.ts)로 놓아요. 자리는 새 그림과 같은 곳(맨 위 단계의 모든 것 오른쪽, 맨 위 줄). 같은 id는 한 번만.
- 노드: `question_block`(`QuestionNode`, types.ts `QuestionData`: `qid`, `owner`(물은, 또는 사람이 만든 캔버스의 에이전트 폴더), `mine?`(사람이 만듦), `title`, `body?`, `choices?`, `multi?`, `other?`, `answer?`). 컴포넌트는 `nodes/QuestionBlock.tsx`, 스타일은 styles.css `.node-question`, `.q-*`.
- 사람이 만들기: `Canvas.tsx` `addQuestion(at?)`(id `q_<ms>`, `mine: true`, 편집 창 `QuestionEditor`(QuestionBlock.tsx)를 `qEditing`으로 열어요). 보기 줄 정리는 `parseChoices`.
- 답: `main.rs` `answer_question` 명령(`made`, `body`, `choices`를 함께 받아 사람이 만든 질문이면 `control::Made`로 넘겨요)(입력창을 기다릴 수 있어 `spawn_blocking`) → `control::answer_question` → `locate`로 프로젝트를 찾고 `tell`. 메모 글은 `answer_note`.
- 읽기: `canvas.rs` `nodes_of`가 `question_block`을 kind `question`으로 읽고, `read`가 `questions`에 모아요(`others`에는 넣지 않아요).
- 시험: `control.rs` `question_checks_its_choices`, `answer_note_says_what_was_picked`, `canvas.rs` `read_gives_questions_with_their_answers`, `agent.rs` 브리프 시험. 화면 쪽은 타입 검사만.

## 제약과 알려진 문제

- 화면에서 확인하지 않았어요(타입 검사와 백엔드 시험만).
- 캔버스를 열지 않으면 질문을 볼 곳이 없어요. "확인할 일"(위 막대의 알림)이나 사이드바에 기다리는 질문 표시는 아직 없어요. 에이전트가 답에 "캔버스에 물었다"고 말하는 것이 지금의 알림이에요.
- 열리지 않은 캔버스의 질문은 앱 메모리의 그리기 큐에 있어서, 캔버스를 열기 전에 앱을 끄면 사라져요(그리기와 같음).
- 답을 고치거나 다시 보낼 수 없어요. 답하기 전에 질문 블록을 지우면 에이전트에게 알리지 않아요.
- `maestro selection`에 질문 블록은 들어가지 않아요.

## 결정 기록

- 2026-11-17 (같은 날, 사용자 요청 "사람이 직접 만들 수도 있게"): 사람이 만든 질문은 사람이 답도 하므로, 이 캔버스의 에이전트에게 보내는 구조 있는 입력으로 했어요(사용자가 처음에 말한 "인풋"). 에이전트가 본 적 없는 질문이라 메모에 질문·설명·보기를 모두 담아요.
- 2026-11-17 (사용자 요청): 그릴 수 있는 요소를 늘리자는 이야기에서, 사용자가 "에이전트가 묻고 사람이 답하는 칸"을 객관식·주관식으로 원했어요. 결정 게이트(질문 + 선택지 + 버린 대안 기록, `docs/prd/canvas-visualization-ideas.md` B2)와 겹쳐서 하나로 만들었어요: 고르지 않은 보기가 남는 객관식 답이 곧 결정 기록이에요. 하류 일을 막는 게이트(의존 관계와 ready/blocked)는 의존 관계가 생긴 뒤에 이 블록에 이어 붙여요.
- 질문은 `draw` JSON의 한 종류가 아니라 따로 `ask` 명령으로 했어요. 답이 돌아오는 수명(기다림 → 답함 → 메모)이 있어서, 구조 그림과 섞으면 다시 그리기·지우기 규칙이 꼬여요.
