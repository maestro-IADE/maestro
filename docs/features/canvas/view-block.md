---
title: 보기 블록 (maestro show, 이미지·HTML 파일을 캔버스에 띄우기)
area: canvas
status: in-progress
files: [src/nodes/ViewBlock.tsx, src/drawing.ts, src/Canvas.tsx, src/RightPanel.tsx, src/types.ts, src/ipc.ts, src/icons.tsx, src/styles.css, src-tauri/src/control.rs, src-tauri/src/canvas.rs, src-tauri/src/main.rs, src-tauri/src/agent.rs]
related: [features/canvas/structure-drawing.md, features/canvas/question-block.md, features/canvas/zones.md, features/canvas/canvas-reading.md, features/orchestration/maestro-cli.md, features/editor/file-explorer.md, features/ui/dev-tools.md]
updated: 2026-11-18
---

# 보기 블록

## 요약

보기 블록은 파일 하나를 있는 그대로 캔버스에 띄우는 블록이에요. 구조 블록·영역·화살표·질문 블록으로 그릴 수 없는 것(차트, 화면 캡처, 에이전트가 만든 페이지)을 보여 줄 때 써요. 이미지(png, jpg, gif, webp, svg)는 그림으로, HTML은 앱에 닿지 못하는 틀 안의 페이지로 보여요. 에이전트는 `maestro show --file out/chart.png`로 놓고, 사람은 파일 트리에서 이미지나 HTML 파일을 캔버스로 끌어 놓아요. 블록은 파일의 내용이 아니라 경로를 갖고 있어서, 파일을 다시 쓰면 블록이 스스로 다시 읽어요.

## 동작

**에이전트가 띄우기**: `maestro show --file 파일 [--title 제목]`

| 옵션 | 뜻 |
|---|---|
| `--file` | 보여 줄 파일(필수). 상대 경로는 에이전트 자기 폴더(워크트리) 기준, 절대 경로도 돼요 |
| `--title` | 블록 머리에 보일 제목(선택). 없으면 파일 이름 |

- 보여 줄 수 있는 파일: `png`, `jpg`, `jpeg`, `gif`, `webp`, `svg`(이미지), `html`, `htm`(페이지). 10MB까지.
- 다른 확장자, 없는 파일, 폴더, 10MB 초과는 오류이고 캔버스는 그대로예요.
- 답: `{ shown: <경로>, kind: "image" | "html", note }`.
- 블록은 새 그림과 같은 곳(캔버스의 첫 빈 자리: 위부터, 왼쪽부터. → [구조 그리기](structure-drawing.md) "배치 규칙")에 놓여요. 캔버스가 닫혀 있으면 열 때 놓여요.
- 같은 파일을 다시 `show` 하면 블록이 하나 더 생기지 않아요. 있는 블록이 제목을 바꾸고 파일을 다시 읽어요.
- 치우기: `maestro erase --view <파일 또는 제목>`. `show`에 준 경로 그대로, 답의 `shown` 경로, 또는 제목으로 찾아요. `erase --all true`는 보기 블록도 모두 지워요(파일은 그대로 있어요).
- 읽기: `maestro canvas`의 `others`에 `{ id, kind: "view", name, path, area, box }`로 나와요(`name`은 제목, 없으면 파일 이름).

**사람이 띄우기**: 오른쪽 파일 트리에서 이미지나 HTML 파일을 캔버스로 끌어 놓으면 놓은 자리에 보기 블록이 생겨요. 끄는 동안 캔버스가 강조돼요(문서 파일을 끌 때와 같아요).

**블록 모양** (처음 크기 16×12칸 = 384×288px)
- 머리: 그림 아이콘, 제목(없으면 파일 이름), 제목이 있으면 그 옆에 흐린 파일 이름, 오른쪽에 "다시 읽기" 버튼. 블록에 마우스를 올리면 전체 경로가 보여요.
- 몸통: 이미지는 비율을 지켜 몸통에 맞춰요. HTML은 틀(iframe) 안에서 돌아요.
- 크기: 블록을 선택하면 나오는 손잡이로 바꿔요(최소 8×6칸). 바꾼 크기는 캔버스와 함께 저장돼요.
- 옮기기는 머리를 잡고 해요. 몸통 안의 마우스와 휠은 그림·페이지의 것이에요.
- 영역 안에 끌어 놓으면 그 영역에 속하고, 단계·상태 영역에서는 놓은 때에만 보여요(→ [영역](zones.md)).
- 우클릭 메뉴는 "지우기"예요. 블록을 지워도 파일은 그대로예요. 연결점은 없어요(선으로 잇지 않아요).

**다시 읽기** (파일이 바뀌면 블록도 바뀌어요)
- 블록이 나타날 때 파일을 읽어요.
- 2.5초마다 파일의 수정 시각을 보고, 바뀌었으면 다시 읽어요. 창이 가려져 있는 동안은 보지 않고, 창이 다시 앞으로 오면 바로 한 번 봐요.
- 에이전트가 같은 파일을 다시 `show` 하거나, 사람이 "다시 읽기"를 누르면 바로 읽어요.
- 파일이 없어지면 마지막으로 읽은 모습을 그대로 두고 아래에 "파일이 없어졌어요"를 띄워요. 처음부터 없으면 오류 글만 보여요.

**HTML의 제한** (안전을 위해)
- 틀은 `sandbox="allow-scripts"`예요. 페이지의 스크립트는 돌지만, 앱과 같은 출처가 아니라서 Maestro의 기능이나 데이터에 닿지 못해요. 창 전체를 다른 곳으로 보내기, 새 창 열기, 폼 보내기는 막혀 있어요.
- 확인한 것(2026-11-18, 개발 빌드에서 틀 안에 스크립트를 넣어 시험): 틀의 출처는 `null`이고, 바깥 문서 읽기·창 이동·저장소(localStorage)는 `SecurityError`로 막혀요. 틀 안에도 Tauri의 호출 객체(`__TAURI_INTERNALS__`)는 보이지만, 앱 명령 호출은 백엔드에 도달하지 않아요: 보통 길(IPC 프로토콜)은 "Origin header is not a valid URL"로 거절되고, 그 길을 일부러 막아 메시지 길(postMessage)로 돌리거나 메시지를 직접 보내도 명령이 실행되지 않았어요(같은 명령을 앱 화면에서 부르면 실행됨).
- 페이지는 파일 한 개로 완결돼야 해요. 내용을 글로 넣어 보여 주기 때문에 옆 파일을 상대 경로로 불러오지 못해요(`./style.css`, `./data.json`, `img/a.png`). 스타일·스크립트·데이터를 파일 안에 넣거나, 인터넷 주소(CDN)로 불러와요.
- SVG는 이미지로 보여서 그 안의 스크립트가 돌지 않아요.

## 구현

- CLI: `control.rs` `"show"` 경로 → `showing(caller, file, title, id)`가 경로를 풀고(상대면 호출자 폴더 기준, `worktree::norm`으로 `/` 통일) 종류·있음·크기를 검사해 `{ id: "view_<ms>_<순번>", path, kind, title? }`를 만들어요. 그리기 큐(`state.drawings`)에 `{ show }`로 넣고 `maestro://draw`를 보내요. 종류 판정은 `canvas.rs` `view_kind`(확장자), 상한은 `VIEW_MAX`(10MB).
- 캔버스: 그리기 큐를 받는 곳(`Canvas.tsx`)이 `{ show }`를 `applyShow(v, nodes)`(drawing.ts)로 놓아요. 같은 경로(대소문자 무시)의 블록이 있으면 새로 만들지 않고 `title`과 `at`(지금 시각)을 바꿔요.
- 노드: `view_block`(`ViewNode`, types.ts: `path`(절대 경로, `/`), `kind`(`image` | `html`), `title?`, `at?`, `when?`). 컴포넌트 `nodes/ViewBlock.tsx`, 처음 크기 `VIEW_SIZE`, 스타일 styles.css `.node-view`, `.view-body`. `Canvas.tsx`의 `nodeTypes`와 `nestable`에 들어 있고, `normalize`가 저장된 크기를 격자에 맞춰 지켜요.
- 파일 읽기: `ipc.viewFile(path)` → `main.rs` `view_file`이 이미지는 base64와 미디어 타입(`canvas::view_mime`), HTML은 글을 돌려줘요(그 밖의 확장자와 10MB 초과는 거절). 이미지는 `data:` 주소로 `<img>`, HTML은 `<iframe sandbox="allow-scripts" srcDoc>`. 수정 시각은 `ipc.viewMtime(path)` → `view_mtime`(없으면 0).
- 다시 읽기: `ViewBlock`의 효과 둘. 하나는 `data.path`·`data.at`이 바뀔 때 읽고, 하나는 `POLL_MS`(2500)마다와 창 `focus` 때 수정 시각을 봐서 달라졌으면 읽어요(`document.hidden`이면 건너뜀).
- 끌어 놓기: `RightPanel.tsx`의 파일 트리가 `viewKind(rel)`이면 `VIEW_MIME`(`application/x-maestro-view`, `{ path }`)을 같이 실어요. `Canvas.tsx` `onDragOver`/`onDrop`이 그것을 받아 놓은 자리에 블록을 만들어요.
- 읽기: `canvas.rs` `nodes_of`가 `view_block`을 kind `view`로 읽고(이름은 제목, 없으면 파일 이름) `read`의 `others`에 `path`와 함께 넣어요.
- 지우기: `control.rs` `erasing`이 `--view`(와 `--file`의 `views`)를 저장된 캔버스의 보기 블록과 맞춰(경로 그대로, 호출자 폴더 기준 경로, 제목) 노드 id 목록 `views`로 바꾸고, `drawing.ts` `applyErase`가 지워요.
- 브리프: `agent.rs` `MAESTRO_HINT`에 `maestro show`와 `erase --view` 문장. 도움말: `control.rs` `ROUTES`의 `show`.
- 시험: `control.rs` `showing_checks_the_file`, `canvas.rs` `view_blocks_are_read_with_their_file`, `dev.rs` `test_files_stay_in_the_test_folder`, `scripts/dev-steps-check.ts`(시험 목록의 show 단계), 개발용 시험의 "보기 블록" 단계(→ [개발용 시험](../ui/dev-tools.md)).

## 제약과 알려진 문제

- 화면 확인(2026-11-18, 개발 빌드를 스크립트로 조작): `show`로 그림과 페이지가 뜨는 것, 페이지의 스크립트가 도는 것, 파일을 다시 쓰면 블록이 5초 안에 바뀌는 것, 파일을 지우면 마지막 모습과 "파일이 없어졌어요"가 남는 것, `erase --view`를 확인했어요. 확인하지 못한 것: 파일 트리에서 끌어 놓기, 크기 손잡이, 영역 안에 넣기.

- HTML은 파일 한 개로 완결돼야 해요(위 "HTML의 제한"). 여러 파일로 된 사이트는 보이지 않아요.
- 페이지의 스크립트는 인터넷에 요청을 보낼 수 있어요(틀이 막는 것은 앱에 닿는 것이지 네트워크가 아니에요). 에이전트가 만든 HTML을 브라우저로 여는 것과 같은 수준이에요.
- 블록은 절대 경로를 저장해요. 워크트리를 옮기거나 지우면 "파일이 없어요"가 돼요.
- 다시 읽기는 수정 시각을 2.5초마다 보는 방식이에요. 에이전트의 턴이 끝나는 순간을 따로 알지는 않아요.
- 영상, PDF, 마크다운은 보여 주지 않아요. 큰 이미지(10MB 초과)도요.
- 보기 블록은 선으로 이을 수 없고, 선택해도 `maestro selection`에 들어가지 않아요.
- 캔버스를 열지 않으면 볼 곳이 없어요. 열리지 않은 캔버스의 보기 블록은 앱 메모리의 그리기 큐에 있어서, 열기 전에 앱을 끄면 사라져요(그리기·질문과 같음).

## 결정 기록

- 2026-11-18 (6단계, 사용자 결정 "지금 도구로 못 그리면 보기 블럭 혹은 html블럭으로 가는 거고"): 내장 블록(구조, 영역, 화살표, 질문)으로 그릴 수 없는 것의 출구로 만들었어요. 블록 종류를 늘리는 대신 "파일을 그대로 보여 준다" 하나로 했어요: 에이전트는 이미 파일을 만들 줄 알고(차트 그리기, HTML 쓰기), 캔버스는 그것을 보여 주기만 하면 돼요.
- 내용이 아니라 경로를 저장해요. 에이전트가 같은 파일을 다시 쓰는 것이 "갱신"이 되게 하려는 것이에요(다시 `show` 하지 않아도 됨). 캔버스 파일도 작게 남아요.
- 이미지 블록과 HTML 블록을 따로 두지 않았어요. 둘 다 "파일을 보여 준다"이고 확장자로 갈려요.
- HTML은 스크립트를 허용했어요(차트 라이브러리가 스크립트라서). 대신 같은 출처를 주지 않아 앱에 닿지 못하게 했어요. 스크립트를 아예 막는 것은 쓸모를 너무 줄여서 택하지 않았어요.
- 경로는 에이전트 폴더 밖도 받아요. Lead가 워커의 결과 파일을 띄우는 쓰임이 있어서예요.
