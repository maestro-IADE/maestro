---
title: 코드 편집기와 diff 뷰 (Monaco)
area: editor
status: done
files: [src/CodeView.tsx, src/ui/Confirm.tsx, src/monaco.ts, src/App.tsx, src/store.ts, src/RightPanel.tsx, src-tauri/src/main.rs, src-tauri/src/worktree.rs, src/styles.css]
related: [features/editor/file-explorer.md, features/editor/drag-attach.md, features/ui/stage-tabs.md, features/orchestration/merge-request.md]
updated: 2026-11-13
---

# 코드 편집기와 diff 뷰

## 요약

오른쪽 패널에서 파일을 누르면 VS Code의 편집기 엔진인 Monaco가 가운데 화면(스테이지) 위에 덮여 열려요. "변경 비교"로 바꾸면 HEAD(마지막 커밋)와 지금 파일을 나란히 보는 diff 뷰가 돼요. 에이전트가 같은 파일을 고치면 알아서 새로 불러오거나 충돌을 알려 줘요.

## 동작

- **열기:** 파일 탭에서 파일을 누르면 편집 모드로, 변경사항 탭에서 파일을 누르면 변경 비교 모드로 열려요. 한 번 클릭은 미리보기 탭(다음 클릭이 그 자리를 다시 씀), 더블클릭이나 편집은 고정 탭이에요.
- **화면:** 편집기는 스테이지를 덮어요. 아래의 캔버스나 에이전트는 계속 돌고 상태도 그대로예요.
- **모드 전환:** 위쪽 "편집 / 변경 비교"로 바꿔요. 저장 안 한 편집은 전환해도 남아요.
- **마크다운 미리보기:** `.md`/`.markdown`/`.mdx`는 "미리보기" 모드가 더 있고, 처음 열면 미리보기로 열려요(변경사항 탭에서 열면 변경 비교). 채팅 답변과 같은 렌더러로 표, 목록, 코드 블록을 그려요. 저장 안 한 편집이 있으면 그 내용으로 보여 줘요. Monaco에는 마크다운 뷰어가 없어서 따로 붙였어요.
- **변경 비교:** 왼쪽이 HEAD, 오른쪽이 지금 파일이에요. 오른쪽은 고칠 수 있어요.
  - 새 파일은 왼쪽이 비어 보여요.
  - 지워진 파일은 오른쪽이 비고 읽기 전용이에요.
- **저장:** Ctrl+S로 해요(저장 버튼은 없어요). 저장 안 했으면 파일 이름 옆에 ●가 떠요.
- **자동 저장:** 설정 → 파일에서 켜면 입력을 멈추고 1초 뒤에 저장해요. 그 사이 에이전트가 파일을 바꿨으면 덮어쓰지 않고 배너로 알려 줘요. 켜는 순간 저장 안 한 편집이 있으면 바로 저장해요.
- **에이전트와 같은 파일을 고칠 때**
  - 2초마다 디스크의 수정 시각을 확인해요.
  - 내 편집이 없으면 자동으로 다시 불러와요.
  - 내 편집이 있으면 "에이전트가 이 파일을 바꿨어요" 배너가 떠요. 디스크 내용 불러오기와 내 편집으로 덮어쓰기 중에 골라요.
  - 저장할 때 그 사이에 파일이 바뀌었으면 덮어쓰지 않고 같은 배너를 띄워요.
- **탭:** 연 파일마다 가운데 화면 위에 탭이 생겨요. 여러 파일을 열어 두고 오갈 수 있고, 다른 탭에 갔다 와도 저장 안 한 편집이 남아요. → [가운데 화면 탭](../ui/stage-tabs.md)
- **닫기:** 편집기의 ×나 탭의 ×로 닫아요(탭도 함께 닫혀요). 저장 안 한 편집이 있으면 확인 창("저장하지 않은 변경이 있어요", 그 파일 경로, 빨간 [저장하지 않고 닫기])으로 물어요. → [메뉴와 확인 창](../ui/menus-and-dialogs.md) 다른 워크트리나 프로젝트로 옮기면 편집기는 화면에서 내려가고 탭은 남아요.
- **보기:** 테마는 앱의 밝음/어두움을 따라요. 미니맵은 껐어요. 스크롤할 때 위에 고정되는 범위 줄(sticky scroll)은 불투명한 배경이에요.
- **언어 지원:** TS/JS/JSON/CSS/HTML은 자동완성이 돼요. TS/JS는 문법 오류만 표시해요. 나머지 언어는 색 입히기만 돼요.

## 구현

**프런트엔드**
- `CodeView({ base, rel, diff })`(CodeView.tsx)
  - App.tsx에서 `lazy()`로 불러요. Monaco(약 3.8MB와 언어 워커들)는 첫 파일을 열 때만 받아요.
  - 파일 탭마다 하나씩, 키 `fileKey(base, rel)`로 떠 있어요. 화면에 없는 것은 `hidden`이라 편집 상태가 유지되고, 모드를 바꿔도 같은 컴포넌트예요.
  - 저장 안 한 상태를 `setFileDirty`로 스토어에 알려서 탭에 ●가 보여요.
- Monaco 설정: `src/monaco.ts`(R5에 CodeView에서 옮김. 병합 요청의 비교 창 `MergeDiffDialog`도 같이 써요. 둘 다 lazy라 Monaco는 처음 열 때만 로드돼요)
  - `loader.config({ monaco })`로 CDN 대신 번들의 Monaco를 써요. 오프라인에서도 열려요.
  - 언어 워커는 Vite `?worker`로 불러와요(`self.MonacoEnvironment.getWorker`).
  - `monaco.typescript.{typescript,javascript}Defaults.setDiagnosticsOptions({ noSemanticValidation: true })`
  - 테마 `maestro-dark` / `maestro-light`: 편집기와 여백 배경을 투명하게 해서 패널 배경이 보이게 해요.
- diff 편집기는 `language`를 직접 넘겨야 해요(`languageOf(rel)`, 확장자로 찾아요. 편집기 옵션 `OPTIONS`와 함께 `monaco.ts`에 있어요). 안 넘기면 `@monaco-editor/react`가 plain text로 열어서 색이 안 나와요.
- 열린 파일 상태: `useStore().editor`(`{ base, rel, diff } | null`), `openEditor()`

**백엔드**(main.rs, "code editor" 구역)
- `inside(base, rel)`: 경로를 실제 경로로 풀어서 워크트리 밖(`..`, 절대 경로, 밖을 가리키는 링크)이면 거부해요. 아직 없는 파일은 부모 폴더로 확인해요.
- `read_text_file`: 5MB(`EDIT_MAX`)를 넘거나 NUL 바이트가 있으면(바이너리) 거부해요. 수정 시각(`mtime`, ms)과 함께 돌려줘요.
- `write_text_file(base, rel, text, expected, force)`: 디스크의 수정 시각이 `expected`와 다르면 `"CHANGED"` 에러를 내요(`force`면 무시).
- `file_mtime`: 2초 폴링용이에요.
- `file_at_head`: `worktree::show_head` → `git show HEAD:./<rel>`. 끝 줄바꿈이 diff에 잡히지 않게 자르지 않은 출력(`git_raw`)을 써요.
- 테스트: `editor_paths_stay_inside_the_worktree`

**키 입력과 캔버스**
- `.code-view`에 `nokey` 클래스가 있어요. 아래에 살아 있는 React Flow는 `.nokey` 안의 키를 무시해요(`isInputDOMNode`).
- Monaco 0.55는 textarea 대신 EditContext로 입력을 받아요. 그래서 이 클래스가 없으면 React Flow가 입력 중인 줄 모르고, 스페이스를 캔버스 이동용으로 가져가 버렸어요(스페이스가 안 눌리던 버그). Backspace/Delete로 선택된 캔버스 블록을 지울 위험도 있었어요.
- 미리보기는 `RichText`(AgentNode.tsx, `className="cv-md"`)로 그려요. 미리보기 중에는 편집기가 내려가 있어서 저장은 `draft`에서 읽어요.

**스타일**(styles.css, "code editor" 구역)
- `.code-view`는 스테이지 위 `position: absolute; inset: 0; z-index: 30`이에요.
- 배경은 `--cv-bg`(테마 색 아래 검정)예요. 비치는 테마(네온)에서 캔버스가 뒤로 보이지 않게 해요.

## 제약과 알려진 문제

- 열린 파일마다 Monaco 편집기가 하나씩 살아 있어서, 파일을 아주 많이 열어 두면 메모리를 더 써요.
- 프로젝트 설정(tsconfig, node_modules)을 모르니 타입 오류는 꺼 뒀어요.
- 실제 앱에서 열고 저장하는 흐름은 확인하지 않았어요.

## 결정 기록

- **VS Code 전체 대신 Monaco:** Orca도 `monaco-editor`와 `@monaco-editor/react`를 써요. code-server 같은 VS Code 전체는 수백 MB에 Node 서버까지 필요해서 버렸어요.
- **CodeMirror 6 대신 Monaco:** 더 가볍지만, 에이전트 작업 검토에 핵심인 diff 뷰가 기본으로 들어 있어서 Monaco를 골랐어요.
- **미니맵 끔:** 에이전트 작업을 검토하는 용도라 코드를 훑어 다닐 필요가 적어요.
- **저장 버튼 없앰:** Ctrl+S만 남겼어요.
- 원래 오른쪽 패널 안에 있던 글자 diff(`git_diff_file`)는 이 diff 뷰로 대체하고 지웠어요.
