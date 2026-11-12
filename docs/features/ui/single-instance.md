---
title: 한 번에 하나만 실행
area: ui
status: done
files: [src-tauri/src/main.rs, src-tauri/Cargo.toml, src-tauri/src/durable.rs]
related: [reference/storage-layout.md, features/ui/window-frame.md, features/ui/split-and-windows.md]
updated: 2026-11-12
---

# 한 번에 하나만 실행

## 요약

릴리스 빌드의 Maestro는 한 번에 하나만 떠요. 이미 떠 있을 때 다시 실행하면 새 프로세스는 바로 끝나고, 떠 있던 Maestro의 메인 창이 앞으로 나와요. 개발 빌드는 기본으로 이 제한이 없고, `MAESTRO_SINGLE_INSTANCE`로 켜고 끌 수 있어요.

## 동작

- 두 번째 실행: 새 창이 뜨지 않아요. 떠 있던 메인 창(`main`)이 최소화돼 있으면 되살리고, 숨겨져 있으면 보이고, 포커스를 줘요. 미리 만들어 둔 숨은 팝아웃 창(`ws-*`)은 건드리지 않아요.
- 두 번째 프로세스는 setup 전에 끝나요. 그래서 로컬 서버(Approvals), 병합 요청·PR 기록 불러오기, PR 조회, `~/.maestro/bin`에 CLI 복사를 하지 않아요.
- `MAESTRO_SINGLE_INSTANCE=0`: 릴리스 빌드에서도 끄기. `MAESTRO_SINGLE_INSTANCE=1`(또는 0이 아닌 값): 개발 빌드에서도 켜기. 변수가 없으면 릴리스는 켜고 개발은 꺼요.

## 구현

- `src-tauri/Cargo.toml`: 데스크톱 대상(`cfg(any(target_os = "macos", windows, target_os = "linux"))`)에 `tauri-plugin-single-instance = "2"`. JS 명령이 없는 플러그인이라 capability 항목은 필요 없어요.
- `main.rs` `main()`: `single_instance()`가 참이면 `tauri_plugin_single_instance::init(|app, _argv, _cwd| focus_main(app))`을 다른 모든 플러그인보다 먼저(`tauri_plugin_dialog`보다 앞) 등록해요. 플러그인 문서가 첫 번째로 등록하라고 해요.
- `focus_main(app)`: `get_webview_window("main")` → `unminimize`, `show`, `set_focus`(실패는 무시).
- `single_instance()`: `MAESTRO_SINGLE_INSTANCE`가 있으면 `"0"`이 아닌지, 없으면 `!cfg!(debug_assertions)`.

## 제약과 알려진 문제

- 개발 빌드와 릴리스 빌드는 앱 식별자(`dev.maestro.app`)와 `~/.maestro`를 같이 써요. 기본값으로는 둘이 함께 뜰 수 있고, 그때 같은 파일(`merge-requests.json`, `pull-requests.json` 등)은 마지막에 쓴 쪽이 이겨요. 상태 파일은 형식 1로 써서(목록은 맨 배열) 버전을 모르는 예전 빌드(main에서 띄운 개발 빌드 등)도 읽어요. 형식 버전 검사([디스크 저장 위치](../../reference/storage-layout.md))는 이 빌드가 더 새 빌드의 파일을 덮어쓰지 않게 막을 뿐이에요. 예전 빌드에는 이 검사가 없어서, 예전 빌드가 쓰면 그대로 덮어써요.
- 메인 창이 없으면(닫혔으면) 두 번째 실행은 아무 창도 앞으로 가져오지 않고 끝나요.
- Linux에서는 플러그인이 D-Bus를 써요. 확인 안 됨(Windows에서만 빌드했어요).
- 자동 테스트는 없어요. 릴리스 빌드에서 두 번 실행해 손으로 확인해요.

## 결정 기록

- 개발 빌드를 기본으로 뺀 이유: 설치된 Maestro 안의 에이전트가 같은 식별자로 `tauri dev`를 자주 띄워요. 켜 두면 개발 빌드가 뜨자마자 끝나고 설치된 창만 앞으로 나와요.
- 개발용 저장 폴더를 따로 두는 것(`MAESTRO_HOME` 같은)은 이번에 하지 않았어요.
