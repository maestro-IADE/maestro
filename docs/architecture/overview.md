---
title: 전체 구조
area: architecture
status: done
files: [src-tauri/src/main.rs, src/App.tsx, src/store.ts, src/ipc.ts, src-tauri/tauri.conf.json, src-tauri/Cargo.toml, src-tauri/src/agent.rs, src-tauri/src/runtime.rs, src-tauri/src/pty.rs, src-tauri/src/events.rs, src-tauri/src/stream.rs, src-tauri/src/transcript.rs]
related: [architecture/agent-runtime.md, architecture/local-server.md, reference/storage-layout.md, reference/tauri-commands.md, architecture/runtime-events.md]
updated: 2026-11-10
---

# 전체 구조

## 요약

Maestro는 Tauri v2 앱이에요. Rust 백엔드가 에이전트 CLI를 PTY(가상 터미널)로 띄워서 관리하고, React 프런트엔드가 그 에이전트들을 사이드바, 캔버스, 전체 화면 뷰로 보여 줘요. 에이전트 하나는 git 워크트리 하나에서 돌고, 프로젝트(저장소)마다 Lead(조율자) 에이전트를 하나 둘 수 있어요.

## 동작

- **기본 단위:** 저장소(프로젝트) → 워크트리 → 에이전트예요. 워크트리 하나에 에이전트 하나가 붙어요.
- **화면 구성:** 왼쪽 사이드바(저장소·워크트리 목록), 가운데 스테이지, 오른쪽 패널(파일·변경사항)로 나뉘어요.
- **스테이지:** 무엇을 골랐는지에 따라 보여 주는 게 달라요.
  - 프로젝트만 골랐을 때: 워크트리 카드가 놓인 프로젝트 캔버스(`ProjectMap`)
  - 워크트리를 골랐을 때: 그 에이전트의 전체 화면(`AgentFull`) 또는 워크트리 캔버스(`Canvas`)
  - Lead를 골랐을 때: 항상 캔버스
- **편집기:** 파일을 열면 Monaco 편집기(`CodeView`)가 스테이지 위에 덮여 열려요. 아래 화면은 그대로 살아 있어요.

## 구현

**기술 스택**
- 백엔드: Tauri v2(Rust), `portable-pty` 0.9(Windows는 ConPTY), `base64`
- 프런트엔드: React 19, TypeScript, Vite, `@xyflow/react` v12(캔버스), `zustand` v5(상태), `@xterm/xterm`(CLI 뷰), `react-markdown` + `remark-gfm`(채팅 마크다운), `monaco-editor`(편집기)
- 창: `decorations: false`로 기본 제목 표시줄을 없애고 직접 그려요. → [창 프레임](../features/ui/window-frame.md)

**프로세스 구성**
- Maestro 앱 프로세스 하나 안에 다음이 들어 있어요.
  - 에이전트마다 PTY 하나와 그 안의 CLI 프로세스(Windows에서는 `cmd.exe /c claude …`)
  - 로컬 HTTP 서버 하나(127.0.0.1, 실행마다 바뀌는 포트와 토큰). 승인 훅과 `maestro` CLI가 여기로 요청해요. → [로컬 서버](local-server.md)
  - 자리 비움 정리 스레드(30초마다). → [자리 비움](../features/agents/away-stop.md)
- 에이전트 실행은 런타임 인터페이스 뒤에 있어요(2026-11-10 PRD R3). → [에이전트 실행 방식](agent-runtime.md)
  - `agent.rs`: `Agent` 파사드, meta, 상태, 브리프
  - `runtime.rs`: `Runtime`·`Terminal`·`PtyAgent` 트레이트, `history`·`snapshot`
  - `pty.rs`: PTY 런타임
  - `transcript.rs`: 트랜스크립트 읽기(`Normalize`, `TurnTracker`, `Tail`, `LiveTail`)
  - `events.rs`: 공통 이벤트 → [공통 이벤트](runtime-events.md)
  - `stream.rs`: 워크트리별 이벤트 스트림(epoch, seq, 델타 묶기)
  - `claude.rs`, `antigravity.rs`: CLI별 부분
  - 그 밖: `approval.rs`(로컬 서버, 승인), `control.rs`(maestro CLI), `catalog.rs`, `cli.rs`, `commands.rs`, `library.rs`, `workspace.rs`, `worktree.rs`
- 실행 중인 에이전트는 `AppState.agents`(`HashMap<소문자 워크트리 경로, Arc<Agent>>`)에 들어 있어요. Windows 경로는 대소문자를 구분하지 않아서 키를 소문자로 맞춰요.
- `ensure()`(main.rs)는 에이전트가 없거나 꺼졌으면 저장된 세션을 이어서(`--resume`) 다시 띄워요. 화면, 채팅 전송, `maestro send`가 모두 이 함수를 거쳐요.

**프런트엔드 상태**
- `useStore`(store.ts): 저장소, 워크트리, 선택, 승인 대기, CLI 설치 상태, 열린 편집기 파일
- `useSettings`(settings.ts): 사용자 설정. localStorage에 저장돼요. → [설정 키](../reference/settings-keys.md)
- 워크스페이스(창·분할·탭 묶음): 백엔드 `workspace.rs`가 한 벌을 가지고, 창마다 `useStore`가 그걸 보여 줘요. → [워크스페이스](../features/ui/split-and-windows.md)
- 백엔드 호출은 모두 `ipc.*`(ipc.ts)를 거쳐요. → [Tauri 명령 목록](../reference/tauri-commands.md)

**개발 실행**
- `npm run tauri dev`로 실행해요.
- `beforeDevCommand`가 먼저 `maestro-cli` 바이너리를 빌드한 뒤 Vite를 띄워요. 바이너리가 두 개라 Cargo.toml에 `default-run = "maestro"`가 있어요.

## 결정 기록

- **진짜 CLI를 띄우는 구조:** 처음에는 채팅 UI만 따로 만들었다가, 실제 CLI와 동작이 달라서 버렸어요. 지금은 실제 TUI 하나를 띄우고 채팅 뷰가 그 위에 얹히는 구조예요. → [에이전트 실행 방식](agent-runtime.md)
- **Electron 대신 Tauri:** 참고한 Orca는 Electron이에요. Maestro는 Tauri라서 앱이 더 가벼워요.
