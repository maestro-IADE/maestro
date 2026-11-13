---
title: 프롬프트 블록
area: canvas
status: done
files: [src/nodes/PromptNode.tsx, src/Canvas.tsx, src/types.ts]
related: [features/canvas/two-level-canvas.md, features/editor/drag-attach.md, features/agents/chat-view.md]
updated: 2026-11-13
---

# 프롬프트 블록

## 요약

프롬프트 블록은 캔버스에 따로 떠 있는 입력창이에요. 선(`command_pipe`)으로 이은 에이전트들에 같은 지시를 한 번에 보내요. 파일을 끌어 놓으면 첨부로 함께 보내요.

## 동작

- 캔버스 왼쪽 위 도구 막대의 "프롬프트"(`Plus`)로 만들어요. 이름은 "프롬프트 N"이에요.
- 프롬프트 블록에서 에이전트 블록으로 선을 이어요.
- 헤더에 연결 상태가 보여요: "연결 없음" / "보낼 곳 N"
- Ctrl+Enter 또는 [보내기]로 보내요.
- 보내면 이어진 모든 에이전트에 동시에 보내요. 꺼진 에이전트는 백엔드가 이어서 켜고 보내요.
- 연결이 없으면 "에이전트에 선을 이어 주세요"라고 알려 줘요.
- 마지막으로 보낸 프롬프트는 `lastSentPrompt`에 남아요.
- 파일 탐색기에서 파일을 끌어 놓으면 첨부 목록(`attachedContextPaths`)에 들어가요. → [드래그 첨부](../editor/drag-attach.md)

## 구현

- `PromptNode`(PromptNode.tsx): 보낼 대상은 이 블록에서 나가는 선을 따라 찾아요(PRD 4.2).
- 보내기: 대상마다 `ipc.send(root, path, text, attached)`
- 선 판정: `isPipe`(Canvas.tsx)는 프롬프트 → 에이전트 블록인 연결만 파이프로 받아요. 파이프 선은 캔버스 상태와 함께 저장돼요.

## 제약과 알려진 문제

- 워크트리 캔버스에서 주로 써요. Lead 캔버스의 워크트리 카드에는 파이프를 이을 수 없어요(카드는 `worktree_card`).

## 결정 기록

- 2026-11-13 (UI 구조 재구성 단계 11): 용어 표대로 "전송"을 "보내기"로 바꿨어요. 헤더의 "단독 전송"/"동시 전송 · N"은 "보낼 곳 N" 하나로 줄였어요(한 곳이든 여럿이든 하는 일은 같아요). 도구 막대 버튼은 "+ 프롬프트 블록"에서 `Plus` 아이콘 + "프롬프트"로 짧게 했어요.
