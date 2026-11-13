---
title: ACP 런타임 사전 조사 (R0)
area: architecture
status: decided
files: [src-tauri/src/agent.rs, src-tauri/src/antigravity.rs, src-tauri/src/cli.rs, src-tauri/src/approval.rs, src-tauri/src/acp.rs, src-tauri/src/acp_runtime.rs, src-tauri/src/acp_setup.rs]
related: [architecture/agent-runtime.md, features/agents/chat-runtime.md, architecture/agent-adapter-spec.md, features/agents/agent-adapters.md, features/agents/cli-install.md, features/agents/chat-view.md, features/agents/cli-view.md]
updated: 2026-11-13
---

# ACP 런타임 사전 조사 (R0)

## 요약

ACP 런타임을 만들기 전에 다섯 가지를 확인한 조사 문서예요. 코드는 바꾸지 않았어요. 결론은 이래요.
- Claude Code·Codex·Antigravity 모두 ACP 런타임을 **조건부로** 지원해요.
- 채팅↔CLI 전환은 **같은 세션 재개**가 기본이고, 단일 쓰기 잠금을 지킬 수 없을 때만 포크로 넘어가요.
- Rust ACP 클라이언트는 공식 크레이트 대신 **직접 작성**해요(`acp.rs`, std 스레드 + mpsc).
- Node가 필요한 어댑터는 **버전 고정 관리형 설치**로 두고, Node가 없으면 PTY 런타임으로 폴백해요.

사용자가 확인했고(아래 "사용자 확인 결과"), R4가 이대로 만들었어요. R4에서 실제로 확인한 것은 "R4에서 확인한 것"에 있어요.

## 조사 결과

### 에이전트별 결론

| | Claude Code | Codex | Antigravity |
|---|---|---|---|
| **ACP 런타임 지원** | **조건부** | **조건부** (실행 검증 안 됨) | **조건부** |
| 어댑터 | `@agentclientprotocol/claude-agent-acp` (Node 패키지, Apache-2.0) | `@agentclientprotocol/codex-acp` (Node 패키지, Apache-2.0). 내부에서 `codex app-server`를 띄우는 번역기 | 없음. `agy acp`가 CLI 1.5.0부터 내장 |
| 조건 | Node ≥22(0.102.0의 `engines`), 어댑터 설치, CLI 로그인 | Node(최소 버전 확인 안 됨), 어댑터 설치, codex ≥0.160 정도(Paradev 하한), 로그인 | agy ≥1.5.4 권장, 로그인. Node 불필요 |
| 실행 | `node.exe <prefix>/node_modules/@agentclientprotocol/claude-agent-acp/dist/index.js`, env `CLAUDE_CODE_EXECUTABLE=<사용자 claude>` | `node.exe <prefix>/.../codex-acp/dist/index.js`, env `CODEX_PATH=<사용자 codex>` | `agy acp [--add-dir <브리프 폴더>] [--dangerously-skip-permissions]` |
| 세션 id | ACP sessionId = Claude 세션 id = `~/.claude/projects/<cwd>/<id>.jsonl` 파일명 (**실험 확인**) | ACP sessionId = Codex thread id (어댑터 코드로 확인, 실행 안 함) | ACP sessionId = TUI conversation id, 같은 저장소 `~/.gemini/antigravity-cli` (**`session/list`로 확인**) |
| 채팅↔CLI 전환 | **재개**: 채팅 → `claude --resume <id>`, CLI → `session/load` (양방향 `-p`로 실험 확인). 폴백은 포크(`session/fork`, `--resume <id> --fork-session`) | **재개** 예상: `session/load` ↔ `codex resume <id>` (확인 안 됨). 첫 턴 전에는 롤아웃이 없어 재개 불가 | **재개** 예상: `session/resume` ↔ `agy --conversation <id>` (id·저장소 같음만 확인, 왕복은 확인 안 됨). 포크 capability 없음 |

### 1. 세션 공유 (Claude Code)

**환경.**
- 사용자 CLI는 `claude --version` = 2.1.302예요.
- 어댑터 `claude-agent-acp@0.102.0`은 `%TEMP%\maestro-acp-r0\pkg`에만 설치했어요(전역 설치 없음).
- 어댑터는 사용자 CLI가 아니라 SDK에 번들된 `@anthropic-ai/claude-agent-sdk-win32-x64@0.3.321`의 `claude.exe`(**2.1.321**)를 띄워요. 고르는 순서는 `CLAUDE_CODE_EXECUTABLE` → 번들 옆 → SDK 플랫폼 패키지예요(`dist/claude-cli.js:16-21`).
- `initialize` 응답: `loadSession: true`, `sessionCapabilities` = additionalDirectories, close, delete, fork, list, resume, systemPrompt.
- 드라이버는 `%TEMP%\maestro-acp-r0\driver.mjs`예요. `protocolVersion: 1`, `fs`/`terminal` false로 보냈고(`driver.mjs:55-56`) 정상 동작했어요.
- 실행은 모두 `clean.sh`로 감쌌어요. 호스트가 넣은 `CLAUDE_*`/`ANTHROPIC_*`(예: `CLAUDE_CODE_ENTRYPOINT=claude-desktop`, `ANTHROPIC_BASE_URL`)를 지우는 스크립트예요. 정리하지 않고 돌린 첫 세션은 entrypoint가 `claude-desktop`으로 찍혀서 결과에서 뺐어요.

**실험** (프롬프트는 모두 1~2단어).

| # | 실험 | 결과 |
|---|---|---|
| 1 | ACP `session/new` → CLI `claude -p --resume <id>` | 성공. 같은 jsonl에 이어 쓰고 문맥 유지. sessionId·Claude 세션 id·파일명이 같음(어댑터가 SDK `options.sessionId`를 넘김, `acp-agent.js:9457-9459`) |
| 2 | CLI로 만든 세션 → ACP `session/load` | 성공. 기존 대화를 `user_message_chunk`/`agent_message_chunk`로 리플레이한 뒤 이어 답함("PELICAN") |
| 3 | ACP+CLI 턴이 섞인 세션 → 새 어댑터에서 load / resume / fork | load: CLI 턴 포함 4개 메시지 리플레이. resume: 리플레이 0건, 같은 id. fork: 새 id, 메시지마다 `forkedFrom` |
| 4 | CLI `--resume <id> --fork-session` | 새 id, 새 파일. 원본은 그대로 |
| 5 | **어댑터를 살려 둔 채** CLI가 턴을 씀 | 어댑터는 CLI 턴을 못 봄("APPLE"만 앎). `parentUuid` 체인으로 보면 **한 jsonl 안에서 대화가 두 갈래로 갈라짐** |
| 6 | 다른 cwd에서 `--resume` / `session/load` | 둘 다 성공. 원본 파일에 이어 쓰지만 레코드의 `cwd`가 바뀜 |
| 7 | 설정 영향 | `~/.claude.json`의 `projects`는 그대로. 테스트 세션 파일만 남음 |

**결론.** 같은 세션 재개는 양방향 모두 돼요. 버전이 달라도(2.1.302 ↔ 2.1.321) 동작했어요. 다만 실험 5 때문에 아래 조건을 반드시 지켜야 해요.
1. 단일 쓰기 잠금: 전환할 때 원본 쪽(PTY의 claude 또는 ACP 세션)을 턴이 끝난 뒤 닫아요.
2. 채팅으로 돌아올 때는 살아 있던 어댑터 세션을 쓰지 않고 `session/load`로 다시 읽어요. 리플레이로 CLI 턴이 채팅에 들어와요.
3. 원래 워크트리 cwd를 강제해요.
4. 어댑터 spawn env를 정리해요.

**레퍼런스.**
- Paradev는 **양방향 포크**예요.
  - `packages/shared/src/builtin-terminal-agents.ts:66-68`: `resumeCommand`, `forkCommand`(`--resume {sessionId} --fork-session`)
  - `useSurfaceFork.ts`: 채팅→CLI는 새 탭에서 `--fork-session`, CLI→채팅은 숨은 로더 채팅에 resume 후 ACP `session/fork`
- Orca는 **비대칭**이에요. 채팅→CLI는 포크, CLI→채팅은 재개와 소유권 claim이에요.
  - `tab-session-history-switch.ts:183, 240`
  - 두 writer 방지 주석: `sleeping-agent-pane-ownership.ts:178` "would put two --resume writers on one transcript"

포크는 잠금이 필요 없지만 전환할 때마다 id가 늘어나요. "에이전트 하나 = 대화 하나"인 Maestro 모델과 맞지 않아서 폴백으로만 써요.

### 2. Rust ACP 클라이언트

crates.io 현황(2026-11-10, `cargo search`, `cargo info`, crates.io API, `cargo tree`로 확인):

| 크레이트 | 버전 | 비고 |
|---|---|---|
| `agent-client-protocol` (공식 SDK) | 5.1.0 (2026-11-09) | Apache-2.0, edition 2024, MSRV 1.88. 클라이언트 쪽 지원(`examples/yolo_one_shot_client.rs`) |
| `agent-client-protocol-schema` | 1.16.0 | 와이어 타입만(serde). 1.x 유지, 의존 크레이트 40개 |
| `agent-client-protocol-tokio` | 0.11.1 | 옛 SDK 0.11용. 5.x와 안 맞음 |

공식 SDK가 Maestro에 맞지 않는 이유:
1. **변경이 너무 잦아요.** 약 5개월 동안 0.15 → 1.0 → 2.0 → 3.0 → 5.0으로 바뀌었어요. 5.0에만 `[breaking]` 항목이 약 20개예요(CHANGELOG.md:15-54).
2. **자체 비동기 런타임을 들여와요.** `futures`/`async-io` 기반이고 의존 크레이트가 70~94개예요. 지금 백엔드는 `std::thread` + `mpsc` 스타일이에요(`agent.rs`, `approval.rs`).
3. **Windows 프로세스 정리가 부족해요.** `ChildGuard::terminate`(acp_agent.rs:368-377)는 Unix에서만 프로세스 그룹을 죽여요. Windows에서는 직계 자식만 죽이므로 node 고아 프로세스가 남을 수 있어요(코드 읽기로 추정, 실행 확인 안 됨). Maestro는 이미 `taskkill /T /F`를 써요(`agent.rs:843` `kill`).
4. **spawn을 통제할 수 없어요.** `AcpAgentConfig`(acp_agent.rs:82-130)에는 cwd 지정이나 env 제거가 없어요.
5. **스키마가 엄격해요.** `SessionUpdate`가 `#[serde(tag="sessionUpdate")]` enum이고 `#[serde(other)]`가 없어요. 모르는 update가 오면 알림 전체가 역직렬화에 실패해요. Paradev는 일부러 느슨한 타입을 써요(`wire.ts:4-8`).

Paradev(주 레퍼런스)도 SDK 대신 290줄짜리 직접 구현을 써요(`rpcClient/rpcClient.ts`).

### 3. 어댑터 실행 환경 (Node)

- Paradev·Emdash는 Electron 실행 파일을 `ELECTRON_RUN_AS_NODE=1`로 띄워서 Node 없이 어댑터를 돌려요(Paradev `acpHarnesses.ts:91-99`, Emdash `claude/index.ts:146-157`). **Tauri에는 이 방법이 없어요.**
- 관리형 설치 패턴:
  - Orca: `managed-runtime-release-install.ts:47-128`(staging 폴더 → sha256 검증 → receipt → 원자적 rename)
  - T3 Code: `AcpRegistrySupport.ts:1252-1283`(`npm_config_prefix` + 정확한 버전으로 `npm install`, npx 안 씀)
- 어느 레퍼런스도 로컬 채팅용으로 Node를 직접 내려받지 않아요. Orca·Emdash는 원격 서버용으로만 Node를 받아요.
- **PRD 배경 36행 정정:** Orca와 T3 Code는 Claude Agent SDK와 `codex app-server`를 쓰고 ACP 어댑터를 쓰지 않아요. Nimbalyst는 Zed의 네이티브 `@zed-industries/codex-acp`(deprecated)를 실험 토글로만 둬요. ACP 어댑터를 쓰는 곳은 Paradev와 Emdash예요.
- 이 PC에서 실제로 설치해 보고 확인한 것:
  - `claude-agent-acp@0.102.0`의 `engines.node`는 `>=22`, SDK 0.3.321은 `>=18.14.0`이에요.
  - SDK의 optional 의존성 `@anthropic-ai/claude-agent-sdk-win32-x64`가 **약 331MB**예요(`du -sh`). `CLAUDE_CODE_EXECUTABLE`로 사용자 CLI를 지정하면 이 패키지가 필요 없을 수 있어요(`--omit=optional`로 빼도 되는지는 확인 안 됨).
- 지금 Maestro는 CLI를 `npm install -g`로 설치하고(`cli.rs:7-11`, `setup_argv`) npm이 있는지 보고해요(`cli.rs:51`). npm으로 CLI를 깐 사용자에게는 Node가 이미 있어요.
- Windows에서는 .cmd shim이나 `cmd.exe /c`를 거치지 않고 `node.exe <entry>`를 바로 띄워야 해요. 지금 PTY 경로는 `cmd.exe /c`를 앞에 붙여요(`agent.rs:505`). 이 방식은 따옴표 처리, 고아 프로세스, 콘솔 프로세스가 끼는 문제가 있어요.

### 4. Antigravity

- `agy --version` = **1.5.4**예요. `agy acp --help`에 `--add-dir`, `--dangerously-skip-permissions`, `--process-per-session`, `--login` 등이 있어요. `--mode`, `--model`, `--conversation`은 **없어요**. 모드·모델·재개는 ACP 메서드로 정해요.
- 프로브로 `initialize`와 `session/list`만 보냈어요(`session/new`·`prompt`는 대화 기록이 생겨서 안 보냄).
  - `agentInfo` = "Antigravity ACP Server" 1.5.4, `protocolVersion: 1`, `loadSession: true`
  - `sessionCapabilities` = resume, close, load, list, additionalDirectories (**fork 없음**)
  - `authMethods` 4종
  - `session/list`가 인증 오류 없이 39개를 돌려줬어요. 그중 31개가 `~/.gemini/antigravity-cli/brain/<id>` 폴더 이름과 같고, `cache/last_conversations.json`의 값 5개가 모두 들어 있어요. 즉 **ACP sessionId = TUI conversation id**예요.
- `agy changelog`:
  - 1.5.0: `agy acp` 추가
  - 1.5.1: 모든 ACP 클라이언트에 전체 모델 제공
  - 1.5.3: 연결이 닫힐 때 대화 저장이 끊기던 문제 수정
  - 1.5.4: `auto` 모드 추가, 재개 시 모드 초기화 버그 수정, 두 번째 승인에서 턴이 멈추던 문제 수정
- Orca의 호스트 `agy acp` 호환 조건(`antigravity-host-acp.ts:22-43`, 최소 1.5.2, protocol 1, loadSession, resume, list)을 1.5.4는 모두 만족해요. Orca는 상속받은 `GEMINI_API_KEY`/`GOOGLE_API_KEY`를 지워요(`antigravity-acp-launch-spec.ts:89-120`).
- 관리형 `agy_acp_server`(Orca 1.4.1, T3 1.4.5 고정)는 계정과 기록이 따로라서 TUI와 전환이 안 돼요(Orca `antigravity-acp-launch-spec.ts:35-40`). **호스트 `agy acp`를 써요.**
- 지금 `antigravity.rs`와의 대응:

| 지금 (PTY) | ACP 런타임 |
|---|---|
| `argv`의 `--add-dir <rules>` (`antigravity.rs:37`) | `agy acp --add-dir` 또는 `session/new`의 additionalDirectories |
| `--mode plan/auto/accept-edits` | `session/set_mode` (default/accept-edits/plan/auto, changelog 기준) |
| `--model` | 세션 config option `model` |
| `--conversation <id>` | `session/resume {sessionId}` |
| `conversation_for` 폴링 (`antigravity.rs:24`) | `session/new`가 id를 바로 돌려줌. PTY 폴백에서는 계속 필요 |
| `to_events` (`antigravity.rs:99`) | `session/update` + `session/prompt`의 `stopReason` |
| `ask`, `models` (`antigravity.rs:55, 76`) | 그대로 |

명세(`agent-adapter-spec.md`)의 #2, #5, #6, #7, #8, #9, #12, #13 ⚠️·❌가 ACP 경로에서 대부분 해소돼요.

### 5. Codex: codex-acp 대 `codex app-server`

- `codex-acp@2.6.0`(2026-11-08)을 `npm pack`으로 받아 읽었어요. README와 `dist/index.js:28132` `startCodexConnection`을 보면 `codex app-server`를 띄워 ACP로 번역하는 층이에요. `CODEX_PATH`가 있으면 그 실행 파일을 써요.
- 7개월에 158개 버전이 나왔어요. 단독 실행 바이너리는 배포하지 않아요(GitHub 릴리스 assets가 비어 있음). 따라서 **Node가 필요해요.**
- 승인(`availableDecisions` 반영), load/resume/fork/list, 이미지, steering이 ACP로 노출돼요(`dist/index.js:51070-51100`). sessionId는 `response.thread.id`예요(`:43200`).
- app-server 직접 연결은 공식적으로 experimental이고(`experimentalApi` opt-in) 서버 capability 협상이 없어요. 직접 붙인 Orca·T3·Paseo는 2.6k~8k줄을 들였어요.
- Paradev는 직접 연결에서 codex-acp로 옮겼어요(`packages/chat/src/core/harness/harness.ts:12, 61`).
- **이 PC에는 codex가 없어서 실행 검증은 하나도 못 했어요.**

## 제안 설계

R3·R4 범위를 정하기 위한 제안이에요. Claude Code 부분은 R4가 구현했어요(무엇이 됐고 무엇이 남았는지는 "R4가 구현한 것"). Codex와 Antigravity의 ACP 실행은 아직이에요.

### Rust ACP 클라이언트 (`src-tauri/src/acp.rs`, 신규)

약 250~350줄, `approval.rs`와 같은 스타일로 만들어요.
- spawn: `std::process::Command`, CREATE_NO_WINDOW(`cli.rs:30` `quiet`와 같은 방식), `current_dir`를 워크트리로 고정, env 정리
- stdout 읽기 스레드: `BufRead::lines` → `serde_json::Value` → 분기
  - `method`만 있음 = 알림
  - `method`+`id` = 에이전트의 요청
  - `id`만 있음 = 응답
- stderr tail 스레드, `Mutex<ChildStdin>` 쓰기
- 요청 대기: `Mutex<HashMap<u64, mpsc::Sender<..>>>` + `recv_timeout`. 프로세스가 끝나면 대기 중인 요청을 모두 실패 처리
- 종료: `taskkill /T /F`

프로토콜:
- **protocol v1**을 쓰고 메시지는 느슨하게(`Value` + 작은 serde 구조체) 다뤄요. 모르는 `sessionUpdate`는 원본을 담아 보존해요(R3 공통 이벤트 규칙).
- 구현할 메서드:
  - `initialize`(`fs`/`terminal` false 광고)
  - `session/new`
  - `session/load`(`loadSession`일 때만), `session/resume`(capability가 있을 때)
  - `session/prompt`(`stopReason`이 턴 종료 신호)
  - `session/cancel`
  - 받는 쪽: `session/update`, `session/request_permission`
  - 그 밖의 메서드에는 -32601
- `session/request_permission`은 `ApprovalCards`로 보내고, 응답이 없으면 거부해요(fail-closed).
- `authenticate`는 부르지 않아요. -32000(AuthRequired)이 오면 "로그인 필요"로 분류하고 PTY로 폴백해요.
- 나중에 타입이 필요하면 `agent-client-protocol-schema`만 가져와요.

### 어댑터 설치와 실행 (Node)

1. 카탈로그(R2)에 어댑터 정보를 둬요: 패키지, 정확한 버전, entry(`dist/index.js`), 실행 파일 env(`CLAUDE_CODE_EXECUTABLE`/`CODEX_PATH`), CLI 최소 버전.
   - 시작 후보: claude-agent-acp **0.102.0**(이번에 실험한 버전, Node ≥22)
   - Paradev의 검증 조합: claude-agent-acp 0.85.0 + claude ≥2.1.226, codex-acp 2.1.0 + codex ≥0.160.0
2. 관리형 설치는 npx 없이 해요.
   - 사용자의 Node로 `<app_data>/acp/<pkg>@<ver>/` 아래 staging 폴더에 `npm install --prefix`해요. 앱에 lockfile을 넣어 `npm ci`로 하위 의존성까지 고정하는 안이 더 좋아요.
   - 설치 후 이름·버전 확인 → `initialize` 핸드셰이크로 검증 → `.install-complete.json` receipt 작성 → rename해요.
   - 실행할 때는 receipt만 믿어요.
3. 실행은 `node.exe <entry>`를 바로 띄워요. `node.exe`는 `.EXE`만 찾아요(volta/nvm의 `.cmd` 제외, T3 `nodeRuntime.ts:55-91`).
4. Node가 없거나 버전이 낮으면 설치 안내(`winget install OpenJS.NodeJS.LTS` 등)를 보여 줘요. 그 에이전트는 "조건부: Node 필요"로 표시하고 PTY + 트랜스크립트 채팅으로 폴백해요(D1, D3).
5. Node를 앱이 직접 받는 방식(고정 버전 zip + sha256)은 미뤄요.

### 에이전트별 실행

- **Claude Code**: 위 설치 경로를 써요. env를 정리해요.
  - 지금 `host_session_env`(`agent.rs:314`)는 `CLAUDECODE`가 있을 때만 동작하고 `ANTHROPIC_*`는 지우지 않아요. ACP spawn에서 기준을 넓힐지는 열린 질문이에요.
  - `CLAUDE_CODE_EXECUTABLE`에 사용자 claude의 실제 `.exe` 경로를 넣어 CLI와 어댑터의 버전을 맞춰요.
- **Codex**: codex-acp를 같은 설치 경로로 써요. `CODEX_PATH`에는 .cmd가 아닌 실제 실행 파일을 넣어요. app-server 직접 연결은 Node를 빼야 할 때 다시 검토해요.
- **Antigravity**: 호스트 `agy acp`를 바로 띄워요(Node 불필요). `GEMINI_API_KEY`/`GOOGLE_API_KEY`를 지워요. 준비 상태는 `initialize` + `session/list`로 확인해요(대화를 만들지 않음). PTY 경로(`antigravity.rs`)는 폴백으로 남겨요.

### 채팅↔CLI 전환 (재개 기본)

1. 에이전트마다 쓰기 소유자를 하나만 둬요(채팅 또는 CLI).
2. 전환 요청이 오면 진행 중인 턴이 끝나기를 기다리거나 취소해요. 그다음 원본 프로세스를 닫아요(ACP 어댑터 종료 또는 PTY의 CLI 종료).
3. 대상 쪽을 같은 id와 **원래 워크트리 cwd**로 새로 열어요.
   - CLI 쪽: `claude --resume <id>` / `agy --conversation <id>` / `codex resume <id>`
   - 채팅 쪽: `session/load`(기록 리플레이 필요) 또는 `session/resume`
4. 원본이 응답하지 않아 잠금을 얻을 수 없으면, 사용자가 "분기"를 고른 경우에만 포크로 열어요. 포크 방법은 Claude `--fork-session`/`session/fork`, Codex `session/fork`예요. Antigravity는 포크가 없어서 이 경우 전환을 막아요.

## 제약과 알려진 문제

확인 안 된 항목이에요. 구현 전에 실측이 필요해요.
- **Claude**
  - 대화형 TUI(`claude --resume <id>`, `-p` 없음)로는 열어 보지 않았어요(새 폴더의 trust 대화상자를 수락하면 `~/.claude.json`이 바뀌어서). 사용자 확인 명령: `cd %TEMP%\maestro-acp-r0\work2 && claude --resume 084de826-f414-43cf-afd5-ba9098fb2fb0`
  - TUI `/resume` 목록에 entrypoint `sdk-ts` 세션이 보이는지 확인 안 됨
  - 갈래가 생긴 jsonl에서 CLI가 어느 leaf를 고르는지 규칙 확인 안 됨
  - 도구 호출·권한 요청·서브에이전트가 들어간 긴 대화의 상호 재개는 시험 안 함
  - `CLAUDE_CODE_EXECUTABLE`로 사용자 CLI(2.1.302)를 지정한 실행, `--omit=optional` 설치는 시험 안 함
  - `elicitation`을 광고하지 않으면 claude-agent-acp가 AskUserQuestion을 버린다는 Paradev 주석(`acpAdapter.ts:972-973`)은 확인 안 됨
- **Codex**: 실행 검증이 전혀 없어요. `codex resume <thread-id>` 왕복, protocol v1 수용 여부, Node 최소 버전, Windows에서 `CODEX_PATH` 지정 시 보조 바이너리 PATH 모두 확인 안 됨.
- **Antigravity**
  - `session/new`·`prompt`를 보내지 않아서 실제 모드 id와 config option 구조는 changelog·레퍼런스 기준이에요.
  - ACP↔TUI 왕복과 동시 접근 시 동작은 확인 안 됨
  - `session/new`가 워크트리에 `.gemini/skills` 같은 파일을 만드는지 확인 안 됨
  - ACP 세션도 `brain/<id>/.../transcript.jsonl`을 남기는지 확인 안 됨
- **Rust 공식 SDK의 Windows 고아 프로세스 위험**은 코드 읽기로 추정한 것이에요.
- **실험 흔적**: `~/.claude/projects/C--Users-user-AppData-Local-Temp-maestro-acp-r0-work`(-work2, -work3, -work4)와 `%TEMP%\maestro-acp-r0` 설치 폴더가 남아 있어요. 지우지 않았어요.

## 결정 기록

- **전환 방식 = 재개(기본) + 포크(폴백).** 같은 id 재개가 양방향으로 동작함을 확인했고, Maestro의 "에이전트 하나 = 대화 하나" 모델에 맞아요. 포크만 쓰는 방식(Paradev)은 전환마다 id가 늘어서 버렸어요. 대신 실험 5의 갈래 문제 때문에 단일 쓰기 잠금이 필수예요(Orca의 소유권 claim과 같은 취지).
- **Rust 클라이언트 = 직접 작성.** 공식 크레이트는 존재하고 유지도 활발하지만, 빠른 breaking change, 별도 async 런타임, Windows 트리 kill·cwd·env 통제 부재, 엄격한 enum 때문에 버렸어요. Paradev도 직접 구현을 써요.
- **Node = 버전 고정 관리형 설치 + 안내 + PTY 폴백.** npx는 실행마다 느리고, 오프라인에서 실패하고, 프롬프트로 stdio가 오염될 수 있고, 공급망 위험도 있어요. 쓰는 레퍼런스도 없어요. Electron식 우회는 Tauri에서 안 돼요. Node 직접 배포는 미뤄요.
- **Codex = codex-acp.** Claude용 ACP 런타임을 그대로 다시 쓸 수 있고, 주 레퍼런스(Paradev)도 같은 방향으로 옮겼어요. app-server 직접 연결은 두 번째 프로토콜 클라이언트와 버전 하한·fixture 유지비가 들어서 후순위로 둬요.
- **Antigravity = 호스트 `agy acp`.** 관리형 서버는 기록이 따로라 전환이 안 돼요.

## 사용자 확인 결과 (2026-11-10)

사용자가 "R0 그대로"로 확인했어요. 아래 질문들은 제안한 기본값으로 정해졌어요.

1. 전환: 같은 세션 재개 + 단일 쓰기 잠금. 포크는 사용자가 분기를 고를 때와 잠금을 못 걸 때만.
2. Node가 없는 PC: 설치 안내 + PTY 폴백. 앱이 Node를 직접 받지는 않아요.
3. Claude 어댑터: 번들 CLI(약 331MB)를 빼고 사용자 `claude`를 `CLAUDE_CODE_EXECUTABLE`로 써요.
4. 환경 변수: 사용자가 직접 설정한 `ANTHROPIC_*`는 넘기고, 호스트(Claude Desktop, Maestro)가 넣은 `CLAUDE_*`·`CLAUDE_CODE_ENTRYPOINT` 같은 값은 지워요.
5. Codex: 설치되어 있지 않으니 R4 이후로 미뤄요.
6. R4의 첫 ACP 대상: Claude Code.

## R4에서 확인한 것

라이브 테스트(`acp_live_*`, `#[ignore]`, 2026-11-11)로 확인했어요. 환경: 어댑터 0.102.0(`MAESTRO_ACP_CLAUDE_DIR=%TEMP%\maestro-acp-r0\pkg`), Node 24.21.0, 사용자 claude 2.1.302(npm 패키지의 `bin\claude.exe`), 모델 haiku.

- **`--omit=optional` 설치:** 된다. `npm install --prefix <staging> --omit=optional --no-audit --no-fund --save-exact @agentclientprotocol/claude-agent-acp@0.102.0`로 패키지 107개(약 8초). SDK의 번들 CLI(`@anthropic-ai/claude-agent-sdk-win32-x64`)는 없고, `CLAUDE_CODE_EXECUTABLE`로 사용자 claude를 주면 `initialize`가 통과해요(`acp_live_install`).
- **`--ignore-scripts`:** 넣지 않았어요. R0 설치의 package-lock에 `hasInstallScript: true`인 패키지가 0개라 넣어도 달라지는 것은 없어요.
- **설치 rename:** 확인용 어댑터를 staging 폴더 안에서 띄우면 Windows가 폴더를 잠가 rename이 실패했어요(os error 32). 확인은 임시 폴더에서 하고, rename을 250ms씩 최대 20번 다시 해요.
- **사용자 CLI로 실행:** `CLAUDE_CODE_EXECUTABLE=<사용자 claude.exe>`로 세션이 돌고, 트랜스크립트는 같은 `~/.claude/projects/<슬러그>/<sessionId>.jsonl`에 쌓여요. 호스트의 `CLAUDE_CODE_ENTRYPOINT=claude-desktop`을 지우면 기록의 `entrypoint`는 `sdk-ts`예요.
- **트랜스크립트 줄 순서:** 생각 줄(빈 thinking)에 `stop_reason: end_turn`이 달리고 글 줄이 그 뒤에 와요. 그래서 실시간 글은 트랜스크립트의 `turnEnded`가 아니라 프롬프트 응답 뒤에 정리해요.
- **load 리플레이 순서:** `session/load`의 `user_message_chunk`·`agent_message_chunk`가 응답보다 먼저 와요. `available_commands_update`는 응답 뒤에 와요(목록은 스킬·사용자 명령 포함 수십 개).
- **권한:** default 모드에서 `mkdir`은 `session/request_permission`이 오고, 그 `toolCallId`는 앞서 온 `tool_call`·트랜스크립트 `tool_use.id`와 같아요. 답 없이 `reject_once`를 보내면 도구 결과가 `is_error`("User refused permission to run tool")이고 폴더는 안 만들어져요. `echo`는 읽기 전용이라 묻지 않았어요. `mkdir`에는 "항상 허용" 선택지가 없었어요(`canAlways = false`).
- **`reject_once`와 `cancelled`:** 답이 없으면 `reject_once`로 충분해요(턴이 이어져 끝나요). `cancelled`는 `session/cancel` 뒤에만 써요.
- **취소:** 글을 쓰는 중에 `session/cancel`을 보내면 응답이 `stopReason: cancelled`이고, CLI가 트랜스크립트에 "[Request interrupted" 줄을 써서 "중단됨"과 `turnEnded cancelled`가 트랜스크립트에서 나와요(런타임이 만들지 않아요, 중복 없음). 턴이 끝난 뒤(응답 뒤 마무리 중)에 보낸 cancel은 CLI가 빈 "중단됨" 턴을 하나 더 쓰게 해서, 열린 턴이 있을 때만 보내요(응답을 기다리는 프롬프트, 또는 백그라운드 작업이 끝나 Claude Code가 스스로 시작해 트랜스크립트에 열린 턴).
- **`allowedTools` / `systemPrompt.append`:** `_meta.claudeCode.options.allowedTools: ["Bash(maestro:*)"]`로 `maestro --version`이 묻지 않고 돌았고, `_meta.systemPrompt.append`의 문구를 모델이 답했어요. `extraArgs.settings` 대안은 필요 없었어요.
- **CLI로 이어 가기:** ACP로 만든 세션을 `claude -p --resume <id>`가 같은 파일에 이어 썼어요(`MAESTRO_ACP_LIVE_CLI=1`).
- **종료:** `session/close` 뒤 node 트리를 kill하면 node, claude와 그 아래가 모두 사라져요. stdin EOF로 node가 스스로 끝나게 두면 claude 아래에서 돌던 셸이 고아로 남을 수 있어서(실제로 남았어요) 트리 kill을 먼저 해요. 그 뒤 새 어댑터의 `session/load`가 그 파일을 열어요.
- **테스트 주의:** cargo가 테스트의 PATH에 `target\debug`(앱 실행 파일 `maestro.exe`)를 넣어서, 에이전트가 `maestro`를 실행하면 앱이 떠요. 라이브 테스트는 PATH 앞에 출력만 하는 `maestro` 심(shim)을 두고 `target/debug`를 빼요.

## R4가 구현한 것

제안 설계 중 Claude Code 부분이에요(2026-11-11, 커밋 전 작업 트리). → [에이전트 실행 방식](agent-runtime.md), [채팅 런타임](../features/agents/chat-runtime.md)

| 제안 | R4 | 비고 |
|---|---|---|
| Rust ACP 클라이언트(acp.rs, std 스레드 + mpsc, v1) | 됨 | `session/resume`은 쓰지 않아요(load만). 모르는 업데이트는 버려요(트랜스크립트가 기록이라 보존할 필요가 없어요) |
| `session/request_permission` → 승인 카드, 답 없음 = 거부 | 됨 | 카드에 "터미널에서 답하기"(예전 "CLI에서 답하기") 없음(`cliAnswer: false`) |
| `authenticate` 안 부름, -32000 → 로그인 필요 → PTY | 됨 | |
| 카탈로그의 어댑터 정보(`entry`, `node_min`, `dev_dir_env`) | 됨 | Claude Code `runtimes: [Pty, Acp]` |
| 관리형 설치(npx 없음, staging → 확인 → 영수증 → rename) | 됨 | `npm ci` + 앱 lockfile 안은 안 했어요(`--save-exact` 최상위만 고정) |
| Node 없음·낮음 → 안내 + PTY | 됨 | 설정에 "Node.js 22 이상 필요 · winget install OpenJS.NodeJS.LTS" |
| `CLAUDE_CODE_EXECUTABLE` = 사용자 claude, 호스트 env 정리 | 됨 | `ANTHROPIC_*`는 남겨요 |
| 채팅↔CLI 같은 세션 재개 + 단일 쓰기 잠금, 포크는 동의 뒤에만 | 됨 | 화면: 헤더 Chat/CLI, "턴이 끝나면 전환돼요", CLI 뷰의 "CLI로 전환" 띠, `stuck` → "새 갈래로 열까요?" |
| 실시간 글(델타) | 됨 | 50ms 묶음, `live:` 항목, 완성 블록이 오면 지움 |
| 설정의 채팅 런타임 행(상태, 자동/터미널, 설치·진행·취소) | 됨 | |
| Antigravity `agy acp` | 안 함 | PTY 그대로 |
| Codex(codex-acp) | 안 함 | 미룸 |
| Node를 앱이 직접 받기 | 안 함 | 미룸 |

## 사용자 확인이 필요한 질문 (위에서 결정됨)

1. 채팅↔CLI 전환을 "같은 세션 재개 + 단일 쓰기 잠금"으로 하고, 포크는 사용자가 분기를 고를 때만 쓰는 것에 동의하나요?
2. Node가 없는 PC: 설치 안내 + PTY 폴백으로 충분한가요, 아니면 앱이 Node를 직접 받아 두는 방식(약 30MB)까지 필요한가요?
3. Claude 어댑터 설치 크기: SDK의 번들 CLI 패키지(약 331MB)를 빼고(`--omit=optional`) 사용자 claude를 `CLAUDE_CODE_EXECUTABLE`로 쓰는 쪽을 시험해도 될까요? 번들 CLI 대신 사용자 CLI를 쓰면 CLI와 어댑터의 버전이 맞아요.
4. 어댑터 spawn 때 `ANTHROPIC_BASE_URL` 같은 `ANTHROPIC_*` 변수를 지워야 하나요? 사용자가 일부러 설정한 프록시일 수도 있어요.
5. Codex는 이 PC에 설치되어 있지 않아요. 실측을 위해 codex를 설치하고 로그인해 줄 수 있나요? 아니면 Codex ACP는 R4 이후로 미룰까요?
6. 직접 확인해 줄 것: 위의 `claude --resume 084de826-...` TUI 재개, 그리고 테스트 흔적(`maestro-acp-r0` 폴더와 세션 파일)을 지워도 되는지.
7. R4의 첫 ACP 대상을 Claude Code로 할까요, Antigravity(Node 불필요, 이미 로그인됨)로 할까요?
