---
title: 캔버스 읽기와 그림 검사 (maestro canvas, lint, 완료 보고 표시)
area: canvas
status: done
files: [src-tauri/src/canvas.rs, src-tauri/src/control.rs, src-tauri/src/main.rs, src/report.ts, src/nodes/WorktreeCard.tsx, src/styles.css, scripts/report-check.ts]
related: [features/canvas/structure-drawing.md, features/canvas/relations.md, features/canvas/two-level-canvas.md, features/orchestration/maestro-cli.md, features/orchestration/worker-done-notes.md]
updated: 2026-11-16
---

# 캔버스 읽기와 그림 검사

## 요약

에이전트가 자기 캔버스를 구조로 읽고(`maestro canvas`), 그린 것에 대한 검사 결과(`lint`)를 돌려받아요. 워커의 상태와 완료 보고는 런타임 데이터에서 앱이 파생해서 보여 줘요: CLI에서는 `maestro canvas`의 `workers`, 화면에서는 Lead 캔버스 워커 카드의 `보고` 줄이에요. 에이전트는 그림을 그릴 수 있지만 상태·검증 같은 판정 표시는 칠할 수 없어요.

## 동작

**`maestro canvas [--brief true]`**: 내 캔버스(Lead는 Lead 캔버스, 워커는 자기 캔버스)를 JSON으로 읽어요.

| 칸 | 내용 |
|---|---|
| `areas` | `id`, `title`, `in`(바깥 영역 이름), `holds`(안에 든 것 이름), `box` |
| `blocks` | 구조 블록: `id`, `title`, `body`, `area`, `box` |
| `others` | 그 밖의 블록: `kind`(`agent` · `worker` · `doc` · `prompt`), `name`(에이전트는 워크트리 이름), `path`, `area`, `box` |
| `arrows` | 그린 화살표: `id`, `from`, `to`(이름), `relation`(관계 이름, 없으면 `기본 관계`), `label`, `body` |
| `links` | 동작 있는 선(지휘·참조·지시): `kind`, `from`, `to`, `relation`, `label`, `body` |
| `workers` | 내 워커들: `name`, `path`, `phase`, `report` |
| `lint` | 캔버스 검사 결과(아래) |

- `box`는 캔버스 절대 좌표 `{x, y, w, h}`예요(영역 안의 것도 영역 위치를 더한 값).
- `--brief true`면 본문(`body`)은 첫 줄만 와요.
- 마지막으로 **저장된** 캔버스를 읽어요. 방금 그린 것은 캔버스가 받아 자동 저장한 뒤에 보여요. 캔버스를 한 번도 연 적이 없으면 빈 목록과 `note`가 와요.
- `phase`는 런타임에서 파생해요(그림이나 글에서 읽지 않아요): `asking`(승인이나 터미널 질문을 기다림) > `running` > `failed`(마지막 턴이 실패로 끝남) > `merge-pending`(병합 요청이 승인을 기다림) > `idle` / `off`.
- `report`는 그 워커의 마지막 답변에서 첫 `## 보고` 블록을 읽은 것이에요(확정 형식은 답의 맨 앞에 둬요). 없거나 `상태`가 정확히 완료·부분·막힘이 아니면 `null`(추측하지 않아요).
  - `status`, `commit`(짧은 SHA, `없음`이면 빠짐), `verified`(`[{text, weak?}]`), `unverified`, `changed`, `remaining`
  - `weak`: 통과로 치지 않는 검증. `결과 없음`(첫 `→` 뒤가 비었거나 `→`가 없음), `테스트 0개`(`통과 0`, `0 passed` 등), `종료 코드만`(테스트를 돌렸는데 결과가 종료 코드뿐). 빌드의 `→ 종료 0`은 결과로 쳐요.

**그림 검사(`lint`)**: `[{ level, code, message }]`. `warn`은 뜻한 대로 안 그려졌을 가능성, `info`는 더 읽기 좋게 고칠 거리예요. 그리기는 막지 않아요(이름 오타 같은 오류는 지금처럼 그리기 전에 거절해요).

`maestro draw` 응답의 `lint`(그릴 JSON과 저장된 캔버스, 이 그림 전의 관계 목록으로 검사):

| code | 수준 | 뜻 |
|---|---|---|
| `unknown-field` | warn | 모르는 칸이라 무시됨. 흔한 실수는 고칠 칸을 알려 줘요(`meaning` → `description`, 블록의 `text`·`description` → `body`, 화살표의 `relation` → `as`, `name` → `title`) |
| `same-name` | warn | 같은 이름이 영역과 블록에 함께 쓰였거나, 캔버스에 이미 다른 종류로 있음. 화살표는 하나에만 이어져요 |
| `self-arrow` | warn | 자기 자신으로 가는 화살표(그려지지 않아요) |
| `duplicate-arrow` | warn | 같은 방향 화살표가 두 번(뒤의 것이 앞의 것을 고쳐 써요) |
| `empty-area` | warn | 든 것이 없는 영역 |
| `undeclared-relation` | warn | 선언 안 한 `as`가 기본 모양·뜻 없음으로 자동 선언됨 |
| `orphan` | info | 어느 선에도 이어지지 않은 블록(이 그림과 캔버스의 화살표 모두 봐요. 화살표가 하나도 없는 그림에는 말하지 않아요) |
| `no-label` | info | 라벨도 관계(`as`)도 없는 화살표: 문장으로 읽히지 않아요 |
| `long-label` | info | 라벨이 24자 초과: 짧은 동사구로, 긴 설명은 `body`에 |
| `relation-no-meaning` | info | 선언한 관계에 `description`이 없음 |

`maestro canvas`의 `lint`(저장된 캔버스의 위치로 검사): `overlap`(겹친 블록, 서로를 담지 않는 영역끼리), `loose-in-area`(영역 위에 있지만 그 영역에 속하지 않은 블록), `same-name`(같은 이름이 둘 이상: 이름으로 가리키면 첫 것만), `orphan`, `empty-area`, `no-label`.

**워커 카드의 `보고` 줄**(Lead 캔버스): 워커가 일하는 중이 아니고 마지막 답변이 있으면, 상태 줄 아래에 `보고` 태그와 한 줄이 보여요.
- `보고 없음`(흐림): 마지막 답변 끝에 `## 보고` 블록이 없거나 형식이 안 맞음
- `완료 · 검증 2 · 3f2a9c1`(초록): 완료이고, 검증이 있고, 통과 아닌 검증과 미검증이 없음. 커밋 SHA가 있으면 끝에 붙어요
- `부분 · 검증 1 · 통과 아님 1`, `완료 · 검증 없음`, `완료 · 검증 2 · 미검증 1`(경고색): 확인이 필요함
- 마우스를 올리면 블록 전체(상태, 커밋, 검증 줄마다 통과 아님 이유, 미검증, 변경, 남은 일)가 보여요.
- `완료`인데 미검증이 있으면 형식 규칙(미검증이 있으면 `부분`)에 어긋나서 경고색이에요.
- 사용자가 새 지시를 보내면(그 뒤 답이 아직 없으면) 줄이 사라져요. 일하는 중에도 숨겨요.

## 구현

- 순수 함수(src-tauri/src/canvas.rs)
  - `parse_report(text)`: 공유 메모의 확정 규칙 그대로예요. 줄 전체가 `## 보고`인 첫 줄부터 첫 빈 줄·`#` 줄·글 끝까지. `키: 값`(첫 `:`)만 읽고 모르는 키와 다른 줄은 건너뛰어요. 아는 키 `상태`·`커밋`·`검증`·`미검증`·`변경`·`남은 일`. `검증`·`미검증`은 모으고 나머지는 첫 값. `없음`은 빈 값(`검증: 없음` = 검증 0개). `상태`가 정확히 세 값 중 하나가 아니면 None. `weak_check(v)`가 통과 아님 이유를 붙여요(`zero_count`: 앞뒤에 숫자가 없는 0만 셈, `10 passed`는 아님).
  - `nodes_of(saved, name_of)`: 캔버스 파일의 노드를 종류·이름·절대 상자로. 크기가 없으면 노드 종류의 기본 크기(격자 24 기준).
  - `read(saved, rel, name_of, brief)`: `maestro canvas`의 본문. 선의 라벨·관계는 `edge.data`, 없으면 파일의 `lines`(지휘·참조 선의 따로 기억한 글)에서 읽어요. 관계 이름은 `relations.json`과 내장 관계로 풀어요.
  - `lint_drawing(spec, canvas_nodes, canvas_arrows, rel)`, `lint_canvas(nodes, saved)`, `check_layouts(spec)`(그리기 전 오류), `lint_json`.
  - 테스트: `report_parses_the_confirmed_block`, `report_takes_the_first_block_and_skips_what_it_doesnt_know`, `report_missing_or_broken_is_none`, `weak_checks_do_not_count_as_a_pass`, `read_gives_areas_blocks_lines_with_ids_and_boxes`, `canvas_lint_finds_overlap_orphans_and_bare_lines`, `drawing_lint_catches_ignored_fields_clashes_and_skipped_arrows`, `drawing_lint_is_quiet_for_a_clean_drawing`.
- 연결(control.rs)
  - `canvas` 경로: `saved_canvas()`(`canvas_path` + `canvas_key`, 못 읽으면 `{}`) → `canvas::read` + `lint_canvas` + `workers`(`me.workers`마다 `worker_view`).
  - `worker_view()`: 승인 대기(`Approvals::pending`) 또는 터미널 질문(`Agent::tui_wait`), `AgentState`, 기록의 마지막 `TurnEnded`가 `Failed`인지, `Merges::of_worker` → `worker_phase()`(테스트 `worker_phase_puts_a_question_first`). 보고는 `events::last_answer` → `parse_report`.
  - `draw` 경로: `drawing()`이 `(캔버스로 보낼 spec, 쓴 그대로의 JSON)`을 돌려줘요. 관계를 선언하기 전에 `lint_drawing`을 돌려 응답에 `lint`로 붙여요.
- 화면(src/report.ts, WorktreeCard.tsx): `lastAnswer(chat)`(마지막 글 항목 전체, 그 뒤에 사용자 메시지가 있으면 null), `parseReport`(Rust와 같은 규칙), `reportSummary`(한 줄, `tone` = `ok` · `warn` · `none`, 툴팁 `detail`). 카드는 `!running && !isMaestro`일 때 `.wt-card-report[data-tone]`를 그려요. 색은 `--mae-rel-green` · `--mae-warn` · 흐림. 테스트: `scripts/report-check.ts`(Rust 테스트와 같은 예).

## 제약과 알려진 문제

- 보고 형식의 주인은 플레이북 규칙 작업이에요(2026-11-16 확정본을 따름). 형식이 바뀌면 `parse_report`와 `parseReport`를 함께 고쳐요. 브리프와 페르소나에 형식이 들어가기 전에 만든 답은 `보고 없음`으로 보여요.
- 카드는 마지막 글 항목 하나만 읽어요. 보고 블록 뒤에 도구를 쓰고 글을 또 쓰면 보고가 다른 글 항목에 남아 `보고 없음`으로 보일 수 있어요(CLI의 `events::last_answer`도 마지막 메시지 단위예요).
- 보고 파싱이 Rust와 TS에 따로 있어요(CLI와 카드가 각자 읽음). 같은 예로 테스트해서 맞춰요.
- `maestro canvas`는 저장된 파일을 읽어서, 사용자가 방금 옮긴 것은 자동 저장 뒤에 반영돼요. 화면 밖·안 구분(tldraw의 단계별 인식)은 아직 없고 전부 줘요. 지금 캔버스 크기(블록 10개 안팎)에서는 충분하다고 봤어요.
- 워커의 `phase`는 `maestro canvas`에만 있어요. 카드는 원래 있던 상태 글리프와 확인 줄로 같은 것을 보여 줘요.
- 의존(ready/blocked) 파생은 아직 없어요. 블록과 워커를 잇는 방법이 먼저 필요해요.

## 결정 기록

- 2026-11-16: 처음 만듦. 근거는 `docs/prd/canvas-visualization-ideas.md`(A1 캔버스 읽기, A4 린트, C2 주장 vs 확인의 첫 단계)와 4차 분석의 플레이북 그림 49개(`ade-analysis/canvas4`).
  - 표본 49개 모두 관계 뜻을 `meaning`에 적어서 `description`이 조용히 버려졌어요. 그래서 모르는 칸 경고(`unknown-field`)와 고칠 칸 제안을 먼저 넣었어요.
  - 표본 3개가 영역과 블록에 같은 이름을 썼어요(화살표가 블록으로만 감). 오류로 막지 않고 `same-name` 경고로 했어요: 지금까지 그려지던 그림이 갑자기 거절되지 않게요.
  - 린트는 그리기를 막지 않아요. 한 번에 몇 가지만 고치는 피드백 루프가 목적이에요(제안서 원칙 4).
  - 처음엔 공유 메모의 초안(블록을 답 끝에)으로 만들었다가 같은 날 확정본(답 맨 앞, `커밋` 줄, 첫 블록, 빈 줄에서 끝, `- `·`**` 꾸밈 안 받음)으로 맞췄어요. 확정본의 예 `npm run build → 종료 0`이 통과 아님으로 보이지 않게 `종료 코드만`은 테스트 실행에만 붙여요.
  - 판정 표시(상태·검증)는 앱만 파생해요. 에이전트가 칠한 완료 표시는 워커 주장과 같은 등급이라서예요(제안서 원칙 2). 보고가 없거나 깨지면 추측하지 않고 `보고 없음`이에요.
  - 버린 것: tldraw식 3단계 인식(화면 안 요약 / 선택 전체 / 화면 밖 묶음). 지금 캔버스는 작아서 전부 주는 쪽이 단순하고 정확해요.
