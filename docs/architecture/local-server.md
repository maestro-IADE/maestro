---
title: 로컬 서버 (승인 훅 + maestro CLI)
area: architecture
status: done
files: [src-tauri/src/approval.rs, src-tauri/src/control.rs, src-tauri/src/transcript.rs]
related: [features/agents/approvals.md, features/orchestration/maestro-cli.md, architecture/agent-runtime.md]
updated: 2026-11-10
---

# 로컬 서버

## 요약

Maestro는 실행할 때마다 `127.0.0.1`의 무작위 포트에 작은 HTTP 서버를 띄우고, 실행마다 새로 만든 토큰으로 보호해요. 경로는 세 가지예요. `/permission/…`은 Claude의 권한 확인 훅이, `/hook/…`은 Claude의 신호 훅(Stop, SessionStart)이, `/api/…`는 에이전트가 쓰는 `maestro` CLI가 써요.

## 구현

**시작:** `Approvals::start`(approval.rs)
- `127.0.0.1:0`에 바인딩하고, 요청마다 스레드 하나로 처리해요.
- 토큰은 `RandomState` 해시 두 개를 이어 붙인 32자리 hex예요.
- 개발 빌드에서는 시작할 때 `[maestro] MAESTRO_URL=… MAESTRO_TOKEN=…`을 출력해요. 터미널에서 CLI를 직접 시험할 때 써요.

**`/permission/<세션 id>?t=<토큰>`**
- 쿼리의 토큰이 맞아야 해요.
- 세션 id로 워크트리를 찾아요(`register`). URL에 id를 넣는 이유는 훅 입력의 세션 id가 `/clear` 뒤에 바뀌기 때문이에요.
- 자세한 흐름 → [승인](../features/agents/approvals.md)

**`/hook/<세션 id>?t=<토큰>`**
- 토큰이 맞으면 곧바로 빈 본문 200으로 답해요(SessionStart 출력은 Claude 컨텍스트에 들어가고, Stop 훅은 Claude를 붙잡으면 안 돼요). 그다음 처리해요(`signal()`).
- `hook_event_name`이 `Stop`이면 `agent.sync()`: 트랜스크립트를 지금 읽어요(`STOP_HOOK`으로 끌 수 있어요).
- `SessionStart`이고 `source == "clear"`면 `agent.follow(session_id, transcript_path)`: 채팅이 새 세션을 따라가요. 단 `FOLLOW_SESSIONS`가 켜져 있을 때만이고, R3에서는 꺼져 있어 아무것도 안 해요.
- 훅은 이벤트를 만들지 않아요. 상태와 워커 완료는 트랜스크립트에서만 나와요. → [에이전트 실행 방식](agent-runtime.md)
- 모르는 세션 id면 아무것도 안 해요.

**`/api/<경로>`**
- 헤더 `X-Maestro-Token`이 맞아야 해요. 틀리면 403이에요.
- 헤더 `X-Maestro-Worktree`는 호출한 에이전트의 워크트리예요.
- 본문 JSON을 `control::handle(app, route, caller, input)`에 넘겨요. 성공은 200, 실패는 400과 `{ "error": … }`예요.
- 경로 목록 → [maestro CLI](../features/orchestration/maestro-cli.md)

**요청 읽기:** `read_request`
- 헤더 중 `x-maestro-*`만 소문자로 모아요.
- 본문은 최대 4MB예요.

## 제약과 알려진 문제

- 토큰은 에이전트 환경 변수와 설정 파일에 들어가요. 같은 사용자의 다른 프로세스는 이론상 읽을 수 있어요. 외부(다른 기기)에서는 접근할 수 없어요.
