---
title: CI와 릴리스
area: reference
status: done
files: [.github/workflows/ci.yml, .github/workflows/release.yml, scripts/version.mjs, scripts/updater-manifest.mjs, cliff.toml, CONTRIBUTING.md, .github/pull_request_template.md]
related: [reference/storage-layout.md, features/ui/single-instance.md, features/ui/auto-update.md]
updated: 2026-11-12
---

# CI와 릴리스

## 요약

PR과 `main` 푸시마다 Windows·macOS·Linux에서 빌드와 테스트가 돌고(`ci.yml`), PR 제목이 Conventional Commits 형식인지 검사해요. `vX.Y.Z` 태그를 푸시하면 네 가지 환경에서 설치 파일을 만들어 버전 없는 고정 이름으로 릴리스에 올리고, 모두 올라간 뒤에만 그 릴리스를 "latest"로 공개해요(`release.yml`). 그래서 README와 공식 사이트는 `releases/latest/download/<고정 이름>` 주소 하나로 늘 최신 설치 파일을 줘요.

## 동작

**커밋·브랜치·PR 규칙** (`CONTRIBUTING.md`)
- 커밋 메시지와 PR 제목은 `<type>(<scope>): <요약>` 형식이에요. type은 `feat`, `fix`, `docs`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`.
- 브랜치는 `<type>/<짧은-이름>`.
- PR은 스쿼시로 합쳐요. 그래서 PR 제목이 곧 `main`의 커밋이에요.

**CI** (`.github/workflows/ci.yml`)
- `test`: `windows-latest`, `macos-latest`, `ubuntu-22.04`에서 `npm ci` → `npm run build`(tsc + vite, Rust 빌드가 `dist/`를 넣기 때문에 먼저) → `npm test` → `cargo test`. 한 OS가 실패해도 나머지는 끝까지 돌아요(`fail-fast: false`).
- 테스트가 임시 저장소에 커밋을 만들어서 git 사용자 이름을 `ci`로 정해 둬요.
- PR 제목만 고친 경우(`edited`)에는 `test`를 건너뛰어요.
- `pr-title`: PR 제목이 위 형식인지, 요약이 소문자로 시작하고 마침표로 끝나지 않는지 검사해요.

**릴리스** (`.github/workflows/release.yml`)
1. 버전 올리기: `node scripts/version.mjs 0.2.0`이 다섯 파일(`package.json`, `package-lock.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `src-tauri/tauri.conf.json`)의 버전을 한 번에 바꿔요. 이걸 `chore(release): v0.2.0` 커밋으로 `main`에 넣어요.
2. 태그 푸시: `git tag v0.2.0 && git push origin v0.2.0`.
3. `draft` 작업: 태그와 다섯 파일의 버전이 같은지 확인하고(`version.mjs --check`), 이전 태그부터의 커밋으로 릴리스 노트를 만들고(git-cliff, `cliff.toml`), 초안 릴리스를 만들어요. 같은 태그의 릴리스가 이미 있으면 그대로 써요(다시 돌릴 때).
4. `build` 작업: 네 환경에서 `npm run tauri build`를 업데이트 서명을 켜고(`createUpdaterArtifacts`, 비밀값 `TAURI_SIGNING_PRIVATE_KEY[_PASSWORD]`) 돌리고 결과물을 고정 이름으로 올려요(`--clobber`라 다시 올리면 덮어써요). 설치 파일마다 `<이름>.sig`도 올리고, macOS는 업데이트용 `Maestro-macos-<arch>.app.tar.gz`(+`.sig`)도 올려요. 자세한 건 [자동 업데이트](../features/ui/auto-update.md).

   | 환경 | 올리는 파일 |
   |---|---|
   | Windows x64 | `Maestro-windows-x64-setup.exe`, `Maestro-windows-x64.msi` |
   | macOS Apple silicon | `Maestro-macos-arm64.dmg` |
   | macOS Intel | `Maestro-macos-x64.dmg` |
   | Linux x64 | `Maestro-linux-x86_64.AppImage`, `Maestro-linux-amd64.deb`, `Maestro-linux-x86_64.rpm` |

5. `publish` 작업: 네 빌드가 모두 성공해야 돌아요. 올라온 `.sig`로 `latest.json`을 만들어 올리고(`scripts/updater-manifest.mjs`, 필수 서명이 빠지면 실패), 초안을 공개하고 "latest"로 표시해요. 하나라도 실패하면 초안으로 남고, 이전 릴리스가 계속 latest예요.
- 실패한 릴리스를 다시 돌리려면 Actions 탭에서 `release`를 수동 실행(`workflow_dispatch`)하고 태그를 넣어요.

**다운로드 주소**
- `https://github.com/maestro-IADE/maestro/releases/latest/download/<고정 이름>`은 GitHub가 늘 최신 공개 릴리스의 그 파일로 보내 줘요.
- 공식 사이트(`maestro-web`)의 다운로드 버튼과 README가 이 주소를 써서, 새 버전을 내도 사이트를 다시 배포하지 않아요.

## 제약과 알려진 문제

- 코드 서명이 없어요. Windows SmartScreen과 macOS Gatekeeper가 경고를 띄워요.
- 업데이트 서명 비밀값이 없으면 `build`가 실패해요. 키는 [자동 업데이트](../features/ui/auto-update.md) 참고.
- v0.1.0 릴리스는 이 워크플로 전에 만들어져서 파일 이름에 버전이 붙어 있어요(`Maestro_0.1.0_x64-setup.exe` 등).
- 규칙 전의 커밋은 릴리스 노트에서 "Other"로 모여요.

## 결정 기록

- **고정 이름 + `latest/download`**: 릴리스마다 사이트를 다시 배포하거나, 앱 저장소에서 사이트 저장소로 신호를 보내는 방법(저장소 사이 개인 토큰 필요)보다 움직이는 부분이 적어요.
- **초안 → 모두 성공 후 공개**: tauri-action처럼 빌드마다 바로 릴리스에 올리면, 한 OS가 실패했을 때 latest가 파일이 빠진 릴리스를 가리켜요.
- **스쿼시 + PR 제목 검사**: 기여자의 커밋 하나하나까지 규칙을 강제하지 않아도 `main` 기록이 규칙을 따라요.
