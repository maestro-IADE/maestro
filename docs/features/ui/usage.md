---
title: 사용량
area: ui
status: done
files: [src/UsageButton.tsx, src/Sidebar.tsx, src/styles.css, src/ipc.ts, src/types.ts, src-tauri/src/usage.rs, src-tauri/src/main.rs]
related: [features/ui/side-panels.md]
updated: 2026-11-18
---

# 사용량

## 요약

사이드바 바닥 줄 왼쪽 끝, 설정 톱니 반대편에 계기판 아이콘(`Gauge`) 버튼이 있어요. 누르면 바닥 줄 위로 작은 창이 떠서 Claude와 Codex 구독의 사용 한도(5시간, 7일 등)가 몇 % 찼는지와 언제 초기화되는지 보여줘요.

## 동작

- 창을 열 때마다 새로 읽어요. 머리의 ↻ 버튼은 다시 읽어요. Esc나 창 바깥을 누르면 닫혀요.
- 한 줄: 이름(5시간, 7일, 7일 · Opus, 7일 · Sonnet), 오른쪽에 %, 그 아래 4px 막대, 그 아래 "2시간 10분 후 초기화". 막대는 70% 이상이면 경고색, 90% 이상이면 빨간색(%도 빨간색)이에요.
- Claude: Claude Code의 `/usage`와 같은 곳(`GET https://api.anthropic.com/api/oauth/usage`)을 Claude 로그인 토큰으로 물어봐요. 플랜에 없는 창(null)은 빼요.
- Codex: 네트워크를 쓰지 않아요. Codex CLI가 대화마다 세션 기록(`~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`, `CODEX_HOME`이 있으면 그 아래)의 `token_count` 이벤트에 적는 `rate_limits`(primary·secondary)를 가장 최근 것부터 30개 파일까지 찾아 읽어요. 그래서 마지막 Codex 대화 시점의 값이고, 머리에 "3시간 전 기준"처럼 나와요.
- 값이 없을 때는 이유를 한 줄로 보여요: 구독 로그인이 없음(API 키 사용 등), 토큰 만료("Claude 에이전트를 한 번 실행하면 갱신돼요"), 네트워크 오류, Codex 기록에 사용량이 아직 없음.

## 구현

- `usage.rs` `report(home)` → `Report { claude, codex }`, 각각 `Source { windows, note, as_of }`, 창은 `Window { label, percent, resets_at(RFC 3339, Claude), resets_unix(유닉스 초, Codex) }`. 명령 `usage_report`(main.rs, `spawn_blocking`).
- Claude 토큰: `claude::config_dir(home)/.credentials.json`의 `claudeAiOauth.accessToken`. `expiresAt`(ms)이 지났으면 부르지 않아요. 토큰을 새로 고치지는 않아요(사용자의 로그인 파일을 다시 쓰게 되니까요).
- 요청은 `curl -s -m 10 -H @- …`로 보내요. 헤더(`Authorization: Bearer …`, `anthropic-beta: oauth-2025-04-20`)는 표준 입력으로 넘겨서 토큰이 프로세스 명령줄에 보이지 않아요. Rust HTTP 크레이트를 새로 넣지 않으려고 OS의 curl을 써요(Windows 10 1803+, macOS, 대부분의 Linux에 있어요).
- Codex 리셋 시각: `resets_at`(유닉스 초)이 있으면 그대로, 없으면 줄의 `timestamp` + `resets_in_seconds`. `window_minutes` 300 → "5시간", 10080 → "7일".
- 화면: `UsageButton.tsx`. 버튼은 `.sb-gear`와 같은 28×28, 창은 `.usage-pop`(바닥 줄 기준 `position: absolute`, 좌우 8px, 사이드바 안에 들어가 잘리지 않아요). `.sb-foot`은 `justify-content: space-between`이에요.
- 테스트: `usage::tests`(Claude 응답 파싱, Codex 두 리셋 형식, RFC 3339 변환).

## 제약과 알려진 문제

- `api/oauth/usage`는 공개 문서가 없는 엔드포인트예요. 응답 모양이 바뀌면 "이 계정의 사용량 정보가 없어요"가 나와요.
- 실제 계정으로 응답을 확인하지 못했어요(개발 중 토큰 사용이 막혔어요). 필드 이름은 Claude Code `/usage`가 쓰는 것을 따랐어요.
- Codex는 마지막 대화 이후의 사용(다른 기기, 웹)을 몰라요.

## 결정 기록

- 2026-11-18: 사용자 요청("사용량도 표기해주고 왼쪽 사이드 설정 반대편에 버튼 눌러서")으로 추가했어요. 항상 보이는 막대 대신 버튼 뒤에 두어 사이드바를 조용하게 유지했어요.
