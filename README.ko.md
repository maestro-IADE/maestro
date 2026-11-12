<p align="center">
  <img src="docs/assets/logo.gif" width="140" alt="Maestro 로고" />
</p>

<h1 align="center">Maestro</h1>

<p align="center">
  <a href="README.md">English</a> · <strong>한국어</strong>
</p>

<p align="center">
  <strong>Interactive Agentic Development Environment</strong><br/>
  코딩 에이전트를 지휘하는 상호작용형 개발 환경
</p>

<p align="center">
  <a href="https://www.maestro-iade.com/">공식 사이트</a> ·
  <a href="https://github.com/maestro-IADE/maestro/releases/latest">다운로드</a> ·
  <a href="https://github.com/maestro-IADE/maestro/issues">의견 보내기</a>
</p>

<p align="center">
  <a href="https://github.com/maestro-IADE/maestro/releases/latest"><img src="https://img.shields.io/github/v/release/maestro-IADE/maestro?style=flat&include_prereleases" alt="최신 릴리스" /></a>
  <img src="https://img.shields.io/badge/status-early%20preview-orange?style=flat" alt="초기 미리보기" />
  <a href="LICENSE.md"><img src="https://img.shields.io/badge/license-FSL--1.1--MIT-555?style=flat" alt="라이선스 FSL-1.1-MIT" /></a>
</p>

<!-- [자리: 히어로 이미지] UI 공사 뒤 캔버스 화면 한 장(가로 960px, docs/assets/readme-hero.png). 준비되면 아래 주석을 풀어요.
<p align="center"><img src="docs/assets/readme-hero.png" alt="Maestro 캔버스: 마에스트로가 워커 에이전트들을 나눠 맡긴 화면" width="960" /></p>
-->

> [!WARNING]
> **Maestro는 아직 상용 수준이 아니에요.** 지금은 방향을 보여 드리는 초기 미리보기예요. 기능이 빠져 있거나 바뀌고, 화면도 크게 다시 만들고 있어요. 중요한 저장소에 쓰기 전에는 꼭 백업하고, 결과를 직접 확인해 주세요.

## 왜 Maestro인가

코딩 에이전트는 이제 혼자서도 꽤 많은 일을 해요. 그런데 에이전트가 많아질수록 사람의 일은 줄지 않아요. 터미널 탭을 오가고, 브랜치를 손으로 나누고, 누가 무엇을 바꿨는지 따라가느라 바빠져요.

Maestro는 이 질문에서 시작했어요.

> 사람이 연주하지 않고 지휘한다면, 개발 환경은 어떤 모습이어야 할까?

오케스트라의 지휘자는 악기를 직접 연주하지 않아요. 곡을 나누고, 연주자에게 맡기고, 흐름을 짚어 주고, 소리를 하나로 모아요. Maestro는 개발자가 그 자리에 서도록 돕는 도구예요. 에이전트와 함께 작업을 설계하고, 여러 에이전트를 나란히 일하게 하고, 모든 변경은 사람이 직접 통제해요.

## 생각하는 방식

### 1. 지휘하는 에이전트
프로젝트마다 **마에스트로**가 있어요. 요청을 작업으로 나눠 워커 에이전트를 만들고, 워커가 끝나면 결과를 받아 기준 브랜치에 합쳐요. 사람은 모든 줄을 쓰는 대신 방향을 정하고 결과를 판단해요.

### 2. 짚어서 지시하기
말로만 설명하기 어려운 것은 그려서 보여 줘요. 캔버스에 영역·블록·화살표로 구조를 그리고, 블록을 골라 바로 지시해요. 블록에는 마크다운으로 명세를 담고, 마에스트로가 구조를 직접 그리기도 해요. 대화창 하나보다 사람의 직관에 가까운 방식이라고 믿어요.

### 3. 문서로 정하는 절차
기획자·구현 담당·리뷰어 같은 **페르소나**와, 작업을 나누는 순서를 적은 **플레이북**으로 팀을 꾸려요. 일하는 방식을 바꾸고 싶으면 코드가 아니라 문서를 고쳐요.

## 지키는 원칙

- **격리.** 에이전트마다 자기 브랜치와 git 워크트리에서 일해요. 원본 폴더는 건드리지 않아요.
- **병렬.** 여러 워커가 동시에 만들고, 끝날 때마다 마에스트로에게 알려요.
- **통제.** 에이전트가 명령을 실행하거나 파일을 바꾸려 하면 승인 카드가 떠요. 병합도 에이전트가 직접 하지 않고, 사람이 승인하면 앱이 해요.
- **쓰던 도구 그대로.** Maestro는 에이전트를 새로 만들지 않아요. 이미 쓰는 CLI 에이전트를 실제로 띄우고 그 위에 화면을 얹어요. 구독과 설정도 그대로 써요.
- **정직한 한계.** 워크트리는 파일을 나눌 뿐, 에이전트를 가두는 샌드박스가 아니에요. 위험한 명령을 막는 가드도 실수를 막는 장치이지 보안 경계는 아니에요.
- **내 컴퓨터에서.** Maestro 자체는 사용 기록을 어디에도 보내지 않아요. 상태는 내 컴퓨터의 `~/.maestro`에 저장돼요. 코드와 대화는 각 에이전트 CLI가 자기 공급자(Anthropic, Google 등)로 보내요.

## 지금 할 수 있는 것

| 에이전트 | 상태 |
|---|---|
| [Claude Code](https://docs.anthropic.com/en/docs/claude-code) | 채팅과 터미널 화면 |
| [Antigravity](https://antigravity.google/) | 터미널 화면 |
| [Codex](https://github.com/openai/codex) | 준비 중 |

자세한 기능은 [공식 사이트](https://www.maestro-iade.com/)에서 볼 수 있어요.

<!-- [자리: 기능 카드] 기능 공사가 끝나면 여기에 기능마다 한 칸씩 채워요. 칸 하나 = 제목 + 두 문장 + 그림.
<table>
<tr>
<td width="50%" valign="middle">

### 기능 이름
두 문장 설명.

</td>
<td width="50%"><img src="docs/assets/readme-features/기능.webp" alt="그림 설명" width="100%" /></td>
</tr>
</table>
-->

## 앞으로

<!-- [자리: 로드맵] 공개 전에 다듬어요. 한 줄씩, 완료되면 [x]. -->
- [ ] 화면(UI/UX) 다시 만들기
- [ ] 영어를 기본으로 한 다국어 지원
- [ ] Codex 연결
- [ ] 자동 업데이트와 코드 서명

## 설치해 보기

[최신 릴리스](https://github.com/maestro-IADE/maestro/releases/latest)에서 운영체제에 맞는 파일을 받아요(Windows `.exe`·`.msi`, macOS `.dmg`, Linux `.AppImage`·`.deb`·`.rpm`).

먼저 필요한 것:

- [Git](https://git-scm.com/)
- 쓸 에이전트 CLI. 설치하고 한 번 실행해 로그인해 두세요.
- [Node.js](https://nodejs.org/) 22 이상 — Claude Code를 채팅 화면으로 쓸 때
- [GitHub CLI(`gh`)](https://cli.github.com/) — PR로 합치는 모드를 쓸 때만

아직 코드 서명이 없어서 Windows와 macOS에서 "확인되지 않은 앱" 경고가 뜰 수 있어요.
<!-- TODO: macOS에서 서명 없는 앱을 여는 방법 한 줄 확인해서 넣기 -->

<!-- [자리: 자주 묻는 질문] 공식 사이트의 FAQ(무료인가요? / 어떤 에이전트? / 코드가 외부로 나가나요? / 지원 OS)를 옮겨 올 자리. <details>로 접어서 한 질문씩.
## 자주 묻는 질문
<details><summary>무료인가요?</summary>

답.
</details>
-->

## 함께 만들기

초기 단계라 쓰면서 느낀 점이 가장 큰 도움이 돼요. 버그, 불편한 점, "이렇게 지휘하고 싶다"는 생각을 [Issues](https://github.com/maestro-IADE/maestro/issues)에 남겨 주세요.

직접 빌드하려면 [Rust](https://rustup.rs/), [Node.js](https://nodejs.org/) 22 이상, [Tauri 사전 준비](https://tauri.app/start/prerequisites/)가 필요해요.

```bash
npm install
npm run tauri dev
```

구조와 구현 설명은 [docs/](docs/README.md)에 있어요(한국어).

## 라이선스

[Functional Source License 1.1, MIT Future License (FSL-1.1-MIT)](LICENSE.md)

개인이나 회사 안에서 쓰기, 고치기, 비상업적 교육·연구에는 자유롭게 쓸 수 있어요. 막히는 건 하나예요. Maestro를 대신하거나 같은 기능을 하는 상업용 제품·서비스로 남에게 제공하는 것이에요. 각 버전은 공개되고 2년이 지나면 MIT 라이선스로 바뀌어요.
