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

사이드바 바닥 줄 왼쪽 끝, 설정 톱니 반대편에 계기판 아이콘(`Gauge`)과 가장 많이 찬 한도의 %가 있어요. 누르면 바닥 줄 위로 작은 창이 떠요. CLI마다 한 줄(5시간·7일 막대 둘)로 간추려 보이고, CLI 이름을 누르면 모든 한도와 초기화 시각이 펼쳐져요.

## 동작

- 앱이 뜰 때, 5분마다, 창을 열 때마다 읽어요. 머리의 ↻ 버튼은 다시 읽어요(읽는 동안 돌아요). Esc나 창 바깥을 누르면 닫혀요.
- 버튼: 아이콘 옆 % 는 Claude·Codex의 모든 한도 중 가장 많이 찬 값이에요(툴팁 "사용량 · 가장 많이 찬 한도 N%"). 70% 이상이면 경고색, 90% 이상이면 빨간색이에요. 아직 값이 없으면 아이콘만 보여요.
- 간추린 보기(기본): CLI 머리(16px 로고 `AgentLogo`, 이름, Claude는 플랜 "Max"·"Pro", Codex는 값의 나이 "3시간 전", 오른쪽 ›) 아래에 5시간·7일 칸 둘이 나란히 있고, 칸마다 이름·%·4px 막대예요. 값이 없으면 그 자리에 이유 한 줄이 나오고 머리는 눌리지 않아요.
- 펼친 보기: 머리를 누르면(› 가 90° 돌아요) 한도마다 이름(5시간, 7일, 7일 · Opus 등), %, 막대, "2시간 10분 후 초기화"가 줄로 나와요. 한 번에 한 CLI만 펼쳐져요.
- 막대 색은 버튼과 같은 기준(70% 경고색, 90% 빨간색, 빨간색일 때 %도 빨간색)이에요.
- Claude: Claude Code의 `/usage`와 같은 곳(`GET https://api.anthropic.com/api/oauth/usage`)을 Claude 로그인 토큰으로 물어봐요. 플랜에 없는 창(null)은 빼요. 모델별 주간 한도는 `model_scoped`(`display_name`)가 있으면 그것을, 없으면 `seven_day_opus`·`seven_day_sonnet`을 써요(emdash `claude/usage.ts`와 같은 규칙).
- Codex: 네트워크를 쓰지 않아요. Codex CLI가 대화마다 세션 기록(`~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`, `CODEX_HOME`이 있으면 그 아래)의 `token_count` 이벤트에 적는 `rate_limits`(primary·secondary)를 가장 최근 것부터 30개 파일까지 찾아 읽어요. 그래서 마지막 Codex 대화 시점의 값이고, 머리에 "3시간 전 기준"처럼 나와요.
- 값이 없을 때는 이유를 짧게 한 줄로 보여요: "구독 로그인 없음"(API 키 사용 등), "로그인 만료 · Claude를 한 번 실행하면 갱신돼요", "불러오지 못함 · …", "아직 기록 없음 · Codex로 대화하면 보여요".
- 테두리는 창 바깥 하나뿐이에요. CLI 사이는 선 없이 간격으로 나눠요.

## 구현

- `usage.rs` `report(home)` → `Report { claude, codex }`, 각각 `Source { windows, note, as_of }`, 창은 `Window { label, percent, resets_at(RFC 3339, Claude), resets_unix(유닉스 초, Codex) }`. 명령 `usage_report`(main.rs, `spawn_blocking`).
- Claude 토큰: `claude::config_dir(home)/.credentials.json`의 `claudeAiOauth.accessToken`. `expiresAt`(ms)이 지났으면 부르지 않아요. 토큰을 새로 고치지는 않아요(사용자의 로그인 파일을 다시 쓰게 되니까요).
- 요청은 `curl -s -m 10 -H @- …`로 보내요. 헤더(`Authorization: Bearer …`, `anthropic-beta: oauth-2025-04-20`)는 표준 입력으로 넘겨서 토큰이 프로세스 명령줄에 보이지 않아요. Rust HTTP 크레이트를 새로 넣지 않으려고 OS의 curl을 써요(Windows 10 1803+, macOS, 대부분의 Linux에 있어요).
- Codex 리셋 시각: `resets_at`(유닉스 초)이 있으면 그대로, 없으면 줄의 `timestamp` + `resets_in_seconds`. `window_minutes` 300 → "5시간", 10080 → "7일".
- 플랜: 로그인 파일의 `claudeAiOauth.subscriptionType`(네트워크 없음) → `Source.plan`.
- 화면: `UsageButton.tsx`(`Provider`가 간추린·펼친 보기, `Row`가 펼친 줄). 버튼 `.sb-usage-btn`은 높이 28, 창은 `.usage-pop`(바닥 줄 기준 `position: absolute`, 좌우 8px, 사이드바 안에 들어가 잘리지 않아요). `.sb-foot`은 `justify-content: space-between`이에요.
- 테스트: `usage::tests`(Claude 응답 파싱, Codex 두 리셋 형식, RFC 3339 변환).

## 제약과 알려진 문제

- `api/oauth/usage`는 공개 문서가 없는 엔드포인트예요. 응답 모양이 바뀌면 "이 계정의 사용량 정보가 없어요"가 나와요.
- 실제 계정으로 응답을 확인하지 못했어요(개발 중 토큰 사용이 막혔어요). 필드 이름은 Claude Code `/usage`가 쓰는 것을 따랐어요.
- Codex는 마지막 대화 이후의 사용(다른 기기, 웹)을 몰라요.
- Antigravity는 보여주지 않아요. Orca는 `agy -p /usage --output-format json`(agy 1.1.11+)의 `command.data.groups[].buckets[].remaining_fraction`을 읽지만, 2026-11-18에 agy 1.5.10으로 해 보니 `/usage`가 사용량 명령이 아니라 모델 프롬프트로 처리됐어요(`num_turns: 1`, 약 1만 토큰 사용). 그래서 넣지 않았어요. agy가 `/usage`를 다시 명령으로 받으면 붙일 수 있어요(Orca처럼 `num_turns`·`conversation_id`가 보이면 다시 부르지 않게 막아야 해요).

## 결정 기록

- 2026-11-18: 사용자 요청("사용량도 표기해주고 왼쪽 사이드 설정 반대편에 버튼 눌러서")으로 추가했어요. 항상 보이는 막대 대신 버튼 뒤에 두어 사이드바를 조용하게 유지했어요.
- 2026-11-18: 로고를 넣고, 설명 문장을 줄이고(Codex 나이는 머리의 "3시간 전"으로), CLI 사이 구분선을 뺐어요(사용자: "테두리 너무 무분별하게 사용한 거 아닐까?").
- 2026-11-18: "간추리고 디테일로 보고" 요청으로 간추린 보기와 펼친 보기를 나눴어요. 다른 ADE(ade-analysis `data/raw`)를 참고했어요. Orca는 상태 막대에 5시간·주간 막대를 짧게 두고 누르면 자세히 보여주고, Nimbalyst는 아이콘 + 팝오버예요. 버튼 옆 % 하나는 Orca의 "가장 빠듯한 한도"(most constrained window)를 따른 거예요.
