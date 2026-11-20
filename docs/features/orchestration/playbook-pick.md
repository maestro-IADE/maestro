---
title: 런북 고르기 (패널에서, 입력창에서 이름으로, 고르면 허용 없이)
area: orchestration
status: in-progress
files: [src/playbookPick.ts, src/nodes/AgentNode.tsx, src/Canvas.tsx, src/canvasGate.ts, src/visualSteps.ts, scripts/focus-check.ts]
related: [features/canvas/canvas-flows.md, features/orchestration/library-and-personas.md, features/agents/chat-visuals.md, features/agents/slash-commands.md]
updated: 2026-11-19
---

# 런북 고르기

## 요약

에이전트에게 "이 런북대로 해"를 붙여 보내는 방법이 셋이에요: 캔버스의 런북 패널에서 누르기, 입력창에서 `/`로 이름을 골라 스킬처럼 부르기, 플레이리스트의 "플레이 만들기" 버튼. 고르지 않으면 에이전트가 스스로 골라요(자동).

## 동작

- **패널에서**: 캔버스 도구 막대의 런북 패널에서 하나를 누르면 골라지고 아래 입력창이 열려요.
- **이름으로**(스킬처럼): 채팅 입력창에 `/`를 치면 명령어 목록에 런북도 나와요(오른쪽에 "런북"). 이름을 이어 치면 걸러지고, 고르면 입력칸은 비워지고 런북이 골라져요. 전체 화면 채팅, 캔버스의 에이전트 블록, 아래 입력창 어디서나 같아요.
- **골라진 표시**: 입력창의 메시지 위에 "런북 · 이름" 칩이 떠요. ×로 빼요(다시 자동).
- **보낼 때**: 메시지 앞에 `[런북: 이름] 경로 를 읽고 이 절차를 따라.`가 붙어요. 슬래시 명령과 셸 명령(`/`, `!`로 시작)에는 붙지 않아요. 고른 런북은 뺄 때까지 다음 메시지에도 붙어요.
- **Lead가 아니어도**: 전에는 Lead의 캔버스에서만 붙었어요. 이제 어느 에이전트에게나 붙일 수 있어요(플레이 만들기 때문에).
- **허용**: 런북을 골라 보낸 턴에는 에이전트가 캔버스에 보여 주는 것이 "시각화 허용 → 눌러서"여도 기다리지 않고 바로 나타나요. 자동(고르지 않음)일 때는 설정대로예요.

## 구현

- `playbookPick.ts`: `usePicked`(zustand, 에이전트 경로 → 고른 문서, 앱이 켜진 동안), `pickBook(path, doc)`, `pickedBook`, `bookLine(doc)`, `playbooks()`(`ipc.playbookList`를 30초까지 기억), `MAKE_PLAY`.
- `Composer`(AgentNode.tsx): `/` 목록에 런북을 더해요(`source: "런북"`). 고르면 `pickBook`. 칩과 앞줄 붙이기도 여기서 해요(전에는 `AgentBar`가 했어요). 보낼 때 `gate.loosen(path, true)`, 턴이 끝나면 `false`.
- Canvas.tsx의 런북 패널은 같은 저장소에 골라 넣어요.
- 허용: `canvasGate.ts` `loose`, `visualSteps.ts` `chatView`(시험: `scripts/focus-check.ts`).

## 제약과 알려진 문제

- 화면에서 확인하지 않았어요(2026-11-19, 타입 검사와 규칙 시험만).
- 고른 런북은 앱을 닫으면 잊어요.
- 일반 에이전트의 안내문(브리프)에는 런북 목록이 없어요. 붙여 보낸 한 줄의 경로를 읽고 따르는 거예요.
- 메시지 글 안에 런북 이름을 그냥 적는 것만으로는 골라지지 않아요. `/` 목록에서 골라야 해요.

## 결정 기록

- 2026-11-19 (사용자: "런북 스킬처럼 사용되는 거 지금 되어 있니? 안 되어 있으면 만들어 주고 … 런북 중 하나를 선택하거나 사용할 기능의 명시성이 높으면 느슨한 허용"): 그때까지는 패널에서 누르는 것만 있었어요. `/` 목록에서 이름으로 고르는 길을 더하고, 고른 턴의 느슨한 허용을 넣었어요. 사용자가 든 이유: 자동일 때 엄격한 것은, 자동 허용을 켜지 않은 사용자가 런북을 활발히 쓰고 싶지 않은(순수한 에이전트를 원하는) 사람일 가능성이 높아서예요. 직접 고른 것은 그 반대의 뜻이에요.
