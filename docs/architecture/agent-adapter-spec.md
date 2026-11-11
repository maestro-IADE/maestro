---
title: 에이전트 어댑터 명세 (Claude 기준, Antigravity·Codex 대조)
area: architecture
status: partial
files: [src-tauri/src/catalog.rs, src-tauri/src/agent.rs, src-tauri/src/antigravity.rs, src-tauri/src/approval.rs, src-tauri/src/cli.rs, src-tauri/src/commands.rs, src/nodes/AgentNode.tsx, src-tauri/src/runtime.rs, src-tauri/src/pty.rs, src-tauri/src/claude.rs, src-tauri/src/transcript.rs, src-tauri/src/events.rs]
related: [architecture/agent-runtime.md, features/agents/agent-adapters.md, features/agents/cli-install.md, features/agents/chat-view.md, features/agents/approvals.md, architecture/runtime-events.md]
updated: 2026-11-10
---

# 에이전트 어댑터 명세

## 요약

Maestro가 에이전트 CLI 하나를 "채팅 뷰와 CLI 뷰가 매끄럽게 이어지는" 수준으로 붙이려면 해야 하는 일을 항목별로 정리한 명세예요. 기준은 Claude Code 연동이에요. Antigravity가 어디까지 맞췄고 무엇이 빠졌는지, Codex를 붙일 때 무엇을 채워야 하는지를 같은 표로 봐요. 새 CLI를 붙일 때는 먼저 카탈로그(`catalog.rs` `AGENTS`)에 항목을 하나 넣고, 이 표의 칸을 채워요.

## 동작

어댑터 하나가 채워야 하는 항목과 지금 상태예요. ✅ 됨 · ⚠️ 일부·불안정 · ❌ 없음 · ❓ 확인 안 됨.

| # | 항목 | 무엇을 해야 하나 | Claude | Antigravity | Codex |
|---|---|---|---|---|---|
| 0 | 카탈로그 항목 | id, 이름, 로고, 실행 파일, 설치, 런타임, ACP 어댑터, 최소 버전, 로그인 확인, 슬래시, 모델, 기능 플래그 | ✅ | ✅ | ✅ (런타임 없음) |
| 1 | 실행 명령 | 찾은 실행 파일 경로 + 권한 모드, 모델, 설정 파일, 브리프를 넣은 argv | ✅ `claude::argv()` | ✅ `antigravity::argv` | ❌ |
| 2 | 세션 고정·이어 하기 | 처음부터 세션 id를 정하거나 찾아서, 다시 켤 때 같은 대화로 | ✅ `--session-id` / `--resume` | ⚠️ id를 미리 못 정해서 `last_conversations.json`에서 나중에 찾음(`conversation_for`). 찾기 전까지 채팅이 비어 보임 | ❌ |
| 3 | 준비 판정 | TUI가 입력을 받을 수 있는 순간을 알아내기. 안 되면 이유(로그인 필요, 종료, 시간 초과) | ✅ `ESC[?2004h` + 800ms 조용, `login_hints` | ✅ 같은 신호를 보냄(2026-11-02 PTY로 확인). `login_hints`는 ❓ | ❓ |
| 4 | 신뢰 확인 | 처음 여는 폴더의 "믿을까요?" 화면을 알아보고 수락 | ✅ "trust" 글자 + Enter | ✅ "Do you trust the contents of this project?"(같은 방식으로 잡힘) | ❓ |
| 5 | 메시지 보내기 | 붙여넣기 + Enter, `!` bash 모드, `/명령` 자동완성 회피, `@파일` 첨부 | ✅ `PtyRuntime::prompt` | ⚠️ 같은 코드를 씀. agy가 켜는 kitty 키보드 프로토콜(`ESC[>1u`)에서 붙여넣기·Enter·`!`가 똑같이 먹는지 확인 안 됨 | ❌ |
| 6 | 멈추기(Esc) | 진행 중인 턴만 중단 | ✅ `\x1b` | ⚠️ kitty 프로토콜이 켜지면 Esc를 `ESC[27u`로 보내야 할 수 있음. 지금은 맨 `\x1b` | ❌ |
| 7 | 대화 기록 읽기 | CLI가 쓰는 기록 파일을 따라 읽어 공통 이벤트(`RuntimeEvent`)로 바꾸기 | ✅ `ClaudeNormalizer`(jsonl) | ⚠️ `AgyNormalizer`. 사고(thinking)만 있는 응답은 `thinking`으로 남김(턴은 안 바뀜). 도구 결과는 순서로 짝지음(`pending`) | ❌ |
| 8 | 작업 중/대기 판정 | 턴 시작·끝을 이벤트로 알아내기(`turnStarted`/`turnEnded`, `events::status_after`) | ✅ `stop_reason`, API 오류는 `failed`, Stop 훅이 바로 읽게 함 | ⚠️ "도구 호출 없는 응답 = 턴 끝"으로 추정. 사고 후 바로 끝나는 턴, 오류로 끝난 턴은 대기로 안 돌아올 수 있음 | ❌ |
| 9 | 승인 | 권한 확인을 채팅 카드로 받기 | ✅ `PermissionRequest` 훅(`approval.rs`) | ❌ 훅 없음. CLI 뷰에서 답해야 함 | ❌ |
| 10 | 시스템 브리프 | maestro CLI 안내, 역할, 페르소나, 참조 문서 | ✅ `--append-system-prompt-file` | ✅ `--add-dir` 규칙 폴더(`AGENTS.md`) | ❌ |
| 11 | maestro 명령 자동 허용 | `maestro …` 셸 명령을 매번 묻지 않기 | ✅ `Bash(maestro:*)` | ❌ 설정 키 모름 | ❌ |
| 12 | 슬래시 명령 목록 | 채팅의 `/` 메뉴 | ✅ 기본 + `.claude/commands`·스킬 | ⚠️ 빈 목록(Claude 목록은 안 보임). agy 자체 명령은 안 읽음 | ❌ |
| 13 | 모델 선택 | 기본 모델, 채팅에서 바꾸기 | ✅ `--model`, `/model` 목록(`Models::Fixed`) | ⚠️ 기본 모델만(`Models::Command`, `agy models`). 채팅 모델 선택기 없음 | ❌ |
| 14 | 요약(보조 모델) | 한 번짜리 질문 실행 | ✅ `claude -p` | ✅ `agy -p`(`antigravity::ask`) | ❌ |
| 15 | 설치·로그인 | 설정의 설치 터미널 | ✅ npm + 첫 실행 | ⚠️ npm 없음(앱 설치 안내) | ✅ npm + `codex login` |
| 15a | 탐지 | 경로(Windows shim), 버전·최소 버전, 로그인 상태 → 미설치/버전 낮음/로그인 필요/준비됨 | ✅ `.credentials.json`·`.claude.json` | ✅ 자격 증명 관리자(Windows) | ✅ `auth.json` (실행 확인 안 됨) |
| 16 | 백그라운드 셸 판단 | 자리 비움 정리 전 셸이 도는지 | ✅ bash 자식 확인 | ❓ agy가 쓰는 셸 이름 확인 안 됨 | ❌ |

## 구현

**정의는 카탈로그 한 곳**(`catalog.rs`, 2026-11-10 PRD R2): 위 표의 0, 12, 13, 15, 15a와 `features`(승인 훅, 이미지 입력, 세션 재개, `/model`, 보조 모델)는 데이터로 있어요. 탐지는 `cli.rs`, 슬래시 목록은 `commands.rs`, 모델은 `cli::models`가 카탈로그를 읽어요. 프런트는 `agent_catalog`로 받아요.

**실행 동작은 `PtyAgent` 트레이트**(runtime.rs, 2026-11-10 PRD R3)예요. `runtime::pty_agent(id)`가 `ClaudeAgent`(claude.rs) / `AgyAgent`(antigravity.rs)를 골라요. `antigravity::ID`로 나누는 곳은 여기뿐이에요.
- `launch(ctx, meta, files, exe)`: argv, 세션, 브리프, Claude 승인 훅(카탈로그 `features.approval_hook`을 Rust가 읽어요)
- `conversation_id`, `transcript`, `current_conversation`(agy 대화 찾기·따라가기)
- `normalizer()`: 트랜스크립트 → 공통 이벤트(`Normalize`) → [공통 이벤트](runtime-events.md)
- `preview`: 설정 타일의 실행 명령
- `summarize`: 요약 명령(카탈로그 `features.assistant`로 거른 뒤)
- PTY·배치·커서 응답·준비 판정·붙여넣기·Esc·종료 감시는 CLI와 상관없이 `PtyRuntime`(pty.rs) 하나예요. ACP 런타임(R4)은 같은 `Runtime` 트레이트를 따로 구현해요.

**더 정리할 것:** 표의 나머지(준비 신호, 신뢰 문구, Esc 인코딩, 셸 이름 등)는 아직 `PtyRuntime`의 공통 코드예요. CLI마다 달라지면 `PtyAgent` 메서드로 옮겨요. 처음 생각한 목록:
- `argv(meta, session, resume, files)`
- `session_after_start(home, cwd)`
- `ready_marker()`
- `trust_text()`
- `encode_send(text, files)`
- `encode_interrupt()`
- `transcript(home, cwd, id)`
- `to_events(line, state)`
- `turn_state(event)`
- `install_approvals(...)`
- `write_brief(...)`
- `slash_commands(...)`
- `models()`
- `ask(prompt, model)`
- `shell_names()`

`PtyRuntime`(PTY·배치·커서 응답·종료 감시)은 이미 CLI와 상관없이 공통이에요.

**Antigravity를 매끄럽게 하려면** 위 표의 ⚠️·❌부터 해결해야 해요.
1. **(6) Esc:** kitty 키보드 프로토콜이 켜졌는지 출력에서 보고(`ESC[>1u` / `ESC[<u`), 켜져 있으면 키를 프로토콜 형식으로 보내기
2. **(8) 턴 끝 판정:** 기록의 마지막 단계·상태 필드를 실제 대화로 확인해서 정확히 잡기
3. **(2) 새 대화의 기록 찾기:** 시작 직후 바로 잡히게 하기(찾을 때까지 짧게 다시 확인)
4. **(12)·(13)** Antigravity용 슬래시 명령 출처(카탈로그 `Slash`에 새 값)와 채팅 모델 목록
5. **(9)·(11)** 승인 훅과 자동 허용: agy 설정(`--settings` JSON)의 키를 찾아야 해요. 지금은 모름.

**확인 방법:** `cargo test --bin maestro tui_startup_dump -- --ignored --nocapture`에 `TUI_BIN=agy`(또는 `codex`), `TUI_CWD=<폴더>`를 주면 첫 15초 출력을 볼 수 있어요.

## 제약과 알려진 문제

- 표의 Antigravity 칸 중 (3)·(4)만 실제로 확인했어요. 나머지 ⚠️는 코드와 출력에서 추정한 거예요.
- 어느 부분이 "매끄럽지 않은지"(채팅 늦게 뜸, 상태 안 바뀜, 입력 안 먹음 등) 사용자 증상을 구체적으로 받아서 위 항목과 맞춰 봐야 해요.

## 결정 기록

- Claude 연동을 기준 명세로 삼았어요. 기능이 가장 많이 맞춰져 있고 확인도 가장 많이 했어요.
- 어댑터를 먼저 trait로 뽑지 않고 명세 표부터 썼어요. CLI마다 다른 점(세션 id를 정할 수 있는지, 키 프로토콜, 훅 유무)이 아직 다 드러나지 않았기 때문이에요.
