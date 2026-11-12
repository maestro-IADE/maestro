---
title: 자동 업데이트
area: ui
status: done
files: [src/UpdateNotice.tsx, src/SettingsDialog.tsx, src/App.tsx, src/styles.css, src-tauri/src/main.rs, src-tauri/Cargo.toml, src-tauri/tauri.conf.json, src-tauri/capabilities/default.json, scripts/updater-manifest.mjs, .github/workflows/release.yml]
related: [reference/release.md, features/ui/single-instance.md]
updated: 2026-11-12
---

# 자동 업데이트

## 요약

설치된 Maestro는 켜진 지 10초 뒤와 그 뒤 6시간마다 GitHub 최신 릴리스의 `latest.json`을 읽어요. 더 새 버전이 있으면 메인 창 오른쪽 아래에 "새 버전 vX.Y.Z이 나왔어요" 카드를 띄워요. 사용자가 "업데이트"를 눌러야 내려받고, 서명을 확인한 뒤 설치하고 앱을 다시 시작해요. 서명은 릴리스 워크플로가 업데이트 전용 키로 만들어요.

## 동작

**확인**
- 주소: `https://github.com/maestro-IADE/maestro/releases/latest/download/latest.json`. 설치 파일과 같은 고정 주소 방식이라, 새 릴리스가 공개되면 바로 그 릴리스의 `latest.json`을 가리켜요.
- 처음 확인은 켜진 지 10초 뒤, 그 뒤 6시간마다. 메인 창에서만 확인해요(팝아웃 창 `ws-*`에서는 안 해요).
- 오프라인이거나 릴리스에 `latest.json`이 없으면 조용히 넘어가고 다음 차례에 다시 해요.
- 개발 빌드(`npm run tauri dev`)에서는 저절로 확인하지 않아요.
- 손으로 확인: 설정 → 정보 → `업데이트 확인`. 새 버전이 있으면 카드를 바로 띄워요("나중에"로 숨긴 버전이어도). → [설정 창](settings-dialog.md)

**카드** (`UpdateNotice.tsx`)
- 새 버전 있음: "업데이트하면 앱이 다시 시작되고, 실행 중인 에이전트는 멈춰요" + `나중에` / `업데이트`.
- `나중에`: 그 버전은 이번 실행 동안 다시 띄우지 않아요. 다음에 켜면 다시 떠요. 더 새 버전이 나오면 바로 떠요.
- 내려받는 중: 진행률(%)과 막대. 받은 뒤 "설치하는 중… 곧 다시 시작해요."
- 실패: 오류 문장과 `닫기` / `다시 시도`.
- 카드는 대화상자(설정, 새 에이전트)보다 아래, 캔버스보다 위에 떠요.

**설치**
- Windows: NSIS로 설치했으면 NSIS 설치 파일, MSI로 설치했으면 MSI를 받아요. 설치 프로그램이 진행 막대만 보이는 모드(`passive`)로 돌고, 앱을 닫고 새 버전을 켜요.
- macOS: `.app.tar.gz`를 받아 앱 번들을 바꾸고 다시 시작해요(`relaunch`).
- Linux: AppImage는 파일을 바꾸고 다시 시작해요. deb·rpm은 서명이 있을 때만 `latest.json`에 들어가고, 설치에 관리자 권한을 물어요.

**어떤 파일을 받는지** (`latest.json`의 `platforms`)
- 업데이터는 `<os>-<arch>-<설치 방식>`(예: `windows-x86_64-msi`)을 먼저 찾고, 없으면 `<os>-<arch>`를 써요.

  | 키 | 파일 |
  |---|---|
  | `windows-x86_64-nsis`, `windows-x86_64` | `Maestro-windows-x64-setup.exe` |
  | `windows-x86_64-msi` | `Maestro-windows-x64.msi` |
  | `darwin-aarch64-app`, `darwin-aarch64` | `Maestro-macos-arm64.app.tar.gz` |
  | `darwin-x86_64-app`, `darwin-x86_64` | `Maestro-macos-x64.app.tar.gz` |
  | `linux-x86_64-appimage`, `linux-x86_64` | `Maestro-linux-x86_64.AppImage` |
  | `linux-x86_64-deb`, `linux-x86_64-rpm` | `.deb`, `.rpm` (서명이 있을 때만) |

- `url`은 버전이 붙은 주소(`releases/download/vX.Y.Z/<파일>`)예요. 받는 도중 새 릴리스가 나와도 서명과 파일이 어긋나지 않아요.

## 구현

**앱**
- `Cargo.toml`: 데스크톱 대상에 `tauri-plugin-updater = "2"`, `tauri-plugin-process = "2"`. npm: `@tauri-apps/plugin-updater`, `@tauri-apps/plugin-process`.
- `main.rs`: `tauri_plugin_updater::Builder::new().build()`, `tauri_plugin_process::init()` 등록(single-instance 다음).
- `capabilities/default.json`: `updater:default`, `process:allow-restart`.
- `tauri.conf.json` `plugins.updater`: `pubkey`(공개 키), `endpoints`(위 주소), `windows.installMode: "passive"`.
- `UpdateNotice.tsx`: `check()` → `Update`; `downloadAndInstall(진행 콜백)` → `relaunch()`. `offerUpdate(u)`는 창 이벤트 `maestro:update-offer`로 카드에 `Update`를 넘겨요(설정의 `AboutSection`이 써요). `App.tsx`가 메인 창에 한 번 띄워요. 스타일은 `styles.css`의 `.update-card`.

**서명 키**
- 업데이트 파일은 minisign 키로 서명해요. 앱은 `pubkey`로 서명을 확인하고, 맞지 않으면 설치하지 않아요.
- 개인 키는 저장소에 없어요. 만든 사람 컴퓨터의 `~/.tauri/maestro-updater.key`(비밀번호 `~/.tauri/maestro-updater.key.password`)와 GitHub 저장소 비밀값 `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`에만 있어요.

**릴리스** (`.github/workflows/release.yml`)
- `build`: 비밀값을 환경 변수로 넘기고 `--config '{"bundle":{"createUpdaterArtifacts":true}}'`로 빌드해요. 설치 파일마다 `<파일>.sig`가 생기고, macOS는 `.app.tar.gz`(+`.sig`)도 생겨요. 이것들도 고정 이름으로 올려요.
- `publish`: 릴리스의 `*.sig`를 내려받아 `scripts/updater-manifest.mjs`로 `latest.json`을 만들고 올린 뒤 공개해요. 필수 서명(Windows 2개, macOS 2개, AppImage)이 하나라도 없으면 실패하고 릴리스는 초안으로 남아요.
- `createUpdaterArtifacts`를 설정 파일이 아니라 릴리스 빌드에서만 켜는 이유: 켜 두면 개인 키가 없는 로컬 `npm run tauri build`가 실패해요.

## 제약과 알려진 문제

- **v0.2.0 이하에는 업데이터가 없어요.** 그 버전 사용자는 이 기능이 들어간 버전을 한 번은 직접 받아 설치해야 해요.
- **개인 키를 잃으면** 이미 설치된 앱에 업데이트를 보낼 수 없어요(공개 키가 앱에 박혀 있어요). 새 키로 바꾸면 사용자가 새 버전을 한 번 직접 설치해야 해요. 키 파일 두 개를 따로 백업해 두세요.
- 코드 서명이 없어서, 업데이트 설치 때도 Windows SmartScreen이나 macOS Gatekeeper가 막을 수 있어요. 특히 macOS는 서명 안 된 앱의 자기 교체가 막힐 수 있어요. 확인 안 됨.
- 설치하면 앱이 꺼지면서 실행 중인 에이전트 프로세스도 끝나요. 카드 문구로만 알려요.
- 릴리스 노트는 카드에 보여 주지 않아요(`latest.json`의 `notes`는 릴리스 페이지 주소).
- macOS·Linux 업데이트는 실제 기기에서 확인 안 됨. Windows는 로컬 서명 빌드로 `.sig`가 생기는 것까지 확인했어요.

## 결정 기록

- **자동 설치 대신 묻기**: 에이전트가 돌고 있을 때 앱이 저절로 꺼지면 작업이 끊겨요.
- **`latest/download/latest.json`**: 설치 파일 다운로드와 같은 방식이라 사이트나 별도 서버가 필요 없어요.
- **확인 주기 6시간**: 앱을 며칠씩 켜 두는 사용 방식이라 켤 때 한 번으로는 부족하고, GitHub 요청 제한에 걸릴 만큼 잦지도 않아요.
