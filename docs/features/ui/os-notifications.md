---
title: OS 알림 (앱이 뒤에 있을 때)
area: ui
status: done
files: [src/osNotify.ts, src/ui/useOsNotify.ts, src/Workspace.tsx, src/store.ts, src/settings.ts, src/settings/GeneralSection.tsx, src/sidebarModel.ts, src/attention.ts, src-tauri/src/main.rs, src-tauri/Cargo.toml, src-tauri/capabilities/default.json, package.json, scripts/os-notify-check.ts]
related: [features/ui/attention.md, features/ui/window-frame.md, features/ui/settings-dialog.md, reference/settings-keys.md, features/ui/side-panels.md]
updated: 2026-11-24
---

# OS 알림

## 요약

Maestro 창에 포커스가 하나도 없을 때, 에이전트가 사용자를 기다리기 시작하거나(승인, 터미널 질문, 병합 요청) 턴을 끝내면 운영체제 알림(Windows 알림 센터 등)을 띄워요. 제목은 "에이전트 · 프로젝트", 본문은 새로 온 것만 사이드바 행 둘째 줄의 말로 쓴 것("승인 대기 · Bash")이나 "작업이 끝났어요"예요. 2초 안에 여러 에이전트 일이 몰리면 "에이전트 N개가 기다려요" 하나로 묶어요. 앱을 켜며 불러오는, 이미 기다리던 일은 알리지 않아요. 설정 → 일반 → "OS 알림" [모두 | 기다릴 때만 | 끔], 기본 모두. 작업 표시줄 깜박임은 그대로예요.

## 동작

**언제 뜨나**
- Maestro의 어느 창(메인 창, 분리 창)에도 포커스가 없을 때만이에요. 바뀌는 순간 메인 창에 포커스가 있으면 모으지 않고, 보내기 직전에 모든 창을 다시 보고 하나라도 포커스가 있으면 버려요.
- 메인 창만 보내요. 분리 창은 보내지 않아서 같은 일로 알림이 두 번 뜨지 않아요.

| 일 | 설정 | 제목 | 본문 |
| --- | --- | --- | --- |
| 에이전트가 기다리기 시작함: 새 도구 승인, 새 터미널 질문, 새 병합 요청 | 모두, 기다릴 때만 | 그 에이전트 이름 · 프로젝트 이름(병합 요청은 요청한 Lead) | 새로 온 것만 사이드바 행 둘째 줄의 말로: "승인 대기 · Bash", "터미널 질문", "병합 요청 1". 행과 다를 수 있어요(Bash 승인이 기다리는 중에 Edit 승인이 오면 행은 "승인 대기 · Bash", 알림은 "승인 대기 · Edit". 병합 요청이 하나 있는데 하나 더 오면 행은 "병합 요청 2", 알림은 "병합 요청 1") |
| 턴이 끝남(RUNNING → IDLE) | 모두 | 그 에이전트 이름 · 프로젝트 이름 | "작업이 끝났어요" |
| 위 일이 2초 안에 둘 이상의 에이전트에서 | | "Maestro" | "에이전트 N개가 기다려요" |

- 에이전트 하나는 한 번의 변화에 알림 하나예요. 한 번에 승인 두 건이 와도 하나, 턴이 질문으로 끝나면(턴 끝 + 터미널 질문) 기다림 하나만 보내요.
- PR(CI 실패, 병합할 수 있음)은 알리지 않아요. 에이전트를 멈춰 세우지 않아서예요(작업 표시줄 깜박임과 같은 기준). → [확인할 일](attention.md)
- 꺼짐(TERMINATED)은 턴 끝이 아니에요. 앱을 켜며 목록을 읽어 처음 나타난 에이전트의 상태도 턴 끝으로 보지 않아요.
- 앱을 켜거나 화면을 새로 고친 뒤 다 불러오기 전(프로젝트 목록, 프로젝트마다 에이전트, 이미 기다리던 승인·터미널 질문·병합 요청·PR)에 들어온 것은 알리지 않아요. 다시 켜도 남아 있는 병합 요청처럼 이미 기다리던 일이라서예요. 다 불러온 뒤로는 나중에 프로젝트를 더해도 계속 알려요.
- 첫 알림은 일이 생기고 2초 뒤에 떠요. 그 2초 사이에 그 알림이 말하는 항목에 답했거나(그 에이전트의 다른 오래된 항목이 남아 있어도), 에이전트가 다음 턴을 시작했거나(Lead가 워커 보고를 받고 바로 이어감), 설정을 바꿨거나, 창으로 돌아왔으면 그 알림은 보내지 않아요.
- 프로젝트 이름을 모르면 제목은 에이전트 이름만이에요.
- 알림을 눌렀을 때 Maestro가 그 에이전트를 여는 동작은 없어요. 운영체제마다 지원이 고르지 않아서 약속하지 않아요.

**권한**
- 처음 알림을 보내려 할 때 한 번 플러그인에 권한을 물어요(`isPermissionGranted`). 없다고 하면 한 번 요청하고(`requestPermission`), 거절되면 그 실행 동안 조용히 있어요(오류 토스트도, 다시 묻기도 없어요). 설정이 끔이면 묻지도 않아요.
- Windows·Linux 데스크톱에서는 플러그인이 늘 허용으로 답해요. 운영체제 쪽에서 앱 알림을 꺼 두면 뜨지 않아요.

## 구현

- **순수 모델** `src/osNotify.ts`(`import type`만):
  - `notices(before, after, level, reason)`: 두 번의 스토어 모습(`NotifySnap = { items, agents, projects }`) 사이의 알림거리 `Notice[]`(`{ path, kind: "wait" | "done", title, body, ids }`). 기다림은 `before.items`에 없던 id의 확인할 일 중 PR이 아닌 것을 `item.path`(물은 에이전트, 병합 요청은 Lead)별로 묶어 하나씩, `ids`는 그 새 항목들의 id(턴 끝은 빈 목록). 본문은 `reason(agent, 새 항목들)`이 주고, 없으면 첫 항목의 `title`. 턴 끝은 `level === "all"`일 때 `before`에서 RUNNING이고 지금 IDLE인 에이전트 중 이번에 기다림 알림이 없는 것. 경로는 대소문자·슬래시·끝 슬래시를 무시해요(attention.ts `samePath`와 같은 규칙, 복사).
  - `live(pending, now, level)`: 모아 둔 알림 중 보낼 때도 맞는 것만. 기다림은 자기 `ids` 중 하나가 `now.items`에 아직 있을 때(같은 에이전트의 오래된 항목은 세지 않아요), 턴 끝은 `level === "all"`이고 그 에이전트가 아직 IDLE일 때. `off`면 모두 버려요.
  - `loaded(s)`: 앱을 켠 뒤 다 불러왔는지. `s.ready`(프로젝트 목록), `s.listed`(이미 기다리던 것), 모든 `s.repos`의 `root`가 `s.scanned`에 있음(프로젝트마다 에이전트).
  - `coalesce(pending)`: 에이전트마다 하나(기다림이 턴 끝을 이기고, 같은 종류면 나중 것), 에이전트가 둘 이상이면 `{ title: SUMMARY_TITLE, body: summaryText(n) }` 하나.
  - 상수: `BURST_MS = 2000`, `DONE_TEXT = "작업이 끝났어요"`, `SUMMARY_TITLE = "Maestro"`, `summaryText(n) = "에이전트 N개가 기다려요"`. 형 `OsNotify = "all" | "waiting" | "off"`.
- **보내기** `src/ui/useOsNotify.ts` `useOsNotify(main)`: `useWindowAttention(main)`(Workspace.tsx)이 불러요. `main`이 거짓(분리 창)이면 아무것도 하지 않아요.
  - `useStore.subscribe`로 스토어가 바뀔 때마다 `selectAttention(s)`, `s.worktrees`, `s.repos`의 참조를 비교해 하나라도 바뀌었을 때만 새 `NotifySnap`을 만들어요(채팅 스트리밍 같은 다른 변화는 그냥 넘겨요). React 렌더는 없어요.
  - `loaded`가 한 번 참이 되기 전에는 비교 기준(`last`)만 옮기고 알림거리를 만들지 않아요. 한 번 참이 되면 그대로예요(나중에 더한 프로젝트를 읽는 동안도 알려요).
  - 스토어 `listed`(store.ts): `useAgentEvents`(Workspace.tsx)가 `listApprovals`, `listTuiWaits`, `listMerges`, `listPrs`를 모두 마친 뒤(실패해도) 참으로 둬요.
  - 그 뒤 `document.hasFocus()`가 거짓이고 설정이 끔이 아니면 `notices(...)`를 모아 두고, 첫 알림에서 `BURST_MS` 타이머를 한 번 걸어요(늘리지 않아요). 타이머가 끝나면 `coalesce(live(모아 둔 것, 지금 모습, 지금 설정))` → `appFocused()`(이 창의 `document.hasFocus()` 또는 `getAllWindows()`의 `isFocused()`)가 거짓이고 `permitted()`가 참이면 `sendNotification({ title, body })`.
  - `reason`: `reasonText(rowMeta({ path, isMaestro, adapterId: "asking" }, items))`(sidebarModel.ts). 묻는 에이전트에는 늘 에이전트가 있어서 "에이전트 연결 필요"가 나오지 않게 `adapterId`를 채워 넘겨요.
  - `permitted()`: 실행마다 한 번만 만드는 약속(`isPermissionGranted()` → 아니면 `requestPermission() === "granted"`, 실패는 거짓).
- **플러그인**: `src-tauri/Cargo.toml` `tauri-plugin-notification = "2"`(지금 2.8.1), `main.rs` 빌더에 `.plugin(tauri_plugin_notification::init())`(process 다음). npm `@tauri-apps/plugin-notification` `^2.8.1`. 권한(`capabilities/default.json`)은 쓰는 셋만: `notification:allow-is-permission-granted`, `notification:allow-request-permission`, `notification:allow-notify`.
- **설정** `osNotify`(`src/settings.ts`, 기본 `"all"`, 저장 형식 버전은 그대로). 설정 → 일반 → "OS 알림" `Seg` [모두 | 기다릴 때만 | 끔](`GeneralSection.tsx`). 제목 없는 카드 하나에 들어 있어요(묶음 이름 "알림"은 확인할 일과 섞여서 쓰지 않아요, [용어](terms.md)). → [설정 키](../../reference/settings-keys.md)
- **시험** `scripts/os-notify-check.ts`(`npm test`): 승인·터미널 질문·병합 요청의 제목과 본문(실제 `rowMeta` + `reasonText`로), 이미 있던 것은 알리지 않음, 한 에이전트의 두 건은 하나, PR은 알리지 않음, 턴 끝(모두만, TERMINATED·IDLE → IDLE·새로 나타난 에이전트는 아님, 다른 철자의 같은 경로), 질문으로 끝난 턴은 기다림 하나, 프로젝트를 모를 때 제목, `live`(답함·오래된 병합 요청만 남은 Lead·다음 턴·설정 낮춤·끔), `coalesce`(한 에이전트 하나, 둘 이상이면 요약), `loaded`(목록·대기·프로젝트마다 에이전트, 지운 프로젝트가 남긴 `scanned`, 프로젝트 없음).

## 제약과 알려진 문제

- 앱을 켜서 실제 알림을 띄워 보지 않았어요. 타입 검사, 빌드, 순수 함수 시험, `cargo test`만 했어요. 알림 모양, 알림 센터의 앱 이름·아이콘, 포커스 판정은 확인 안 됨이에요.
- 개발 빌드(`npm run tauri dev`)에서는 Windows 알림의 앱 이름·아이콘이 Maestro가 아닐 수 있어요(설치된 앱이 아니라서). 확인 안 됨.
- 알림을 눌렀을 때 어떻게 되는지(창이 앞으로 오는지)는 운영체제에 맡겨요. 확인 안 됨.
- 앱을 켜고 다 불러오기 전(보통 몇 초, 프로젝트가 많으면 더)에 정말 새로 생긴 일도 알리지 않아요. 이미 기다리던 일과 구별할 수 없어서예요. 작업 표시줄 깜박임은 이 구별이 없어서 그때 깜박일 수 있어요.
- 모든 알림이 2초 늦게 떠요(묶기 위해서).
- 권한을 거절하면 앱을 다시 켤 때까지 다시 묻지 않아요. 운영체제가 거절을 기억하면 다시 켜도 묻지 않을 수 있어요.

## 결정 기록

- 2026-11-24: OS 알림을 더했어요. 창 밖 신호가 작업 표시줄 깜박임 하나라서, 다른 일을 하다 보면 어느 에이전트가 왜 기다리는지 창을 열어 봐야 알았어요. 내놓을 준비를 하며 백그라운드에서 돌려 두는 쓰임을 채우려고 Tauri 공식 알림 플러그인을 썼어요.
- 알림 조건은 "메인 창에 포커스가 없을 때"보다 한 걸음 좁게, Maestro 창이 하나도 포커스를 갖지 않을 때로 했어요. 분리 창에서 일하는 동안 메인 창 몫의 알림이 뜨면 소음이라서예요. 보내는 쪽은 메인 창 하나라 중복이 없어요.
- 본문은 새 글을 만들지 않고 사이드바 행 둘째 줄의 말(`rowMeta` + `reasonText`)을 쓰되, 새로 온 항목만 넣어요. 알림은 무엇이 새로 왔는지를 말하는 것이라서, 이미 보이던 Bash 승인보다 새 Edit 승인을 말하는 편이 맞아요. 그래서 행과 다를 수 있어요("병합 요청 N"은 새 요청 수). 순수 모듈끼리는 서로 불러올 수 없어서 `notices`가 `reason` 함수를 받아요. 병합 요청은 확인할 일 목록과 같이 요청한 Lead의 것으로 세요(제목 Lead).
- 묶기는 첫 알림부터 고정 2초 창이에요(새 알림이 와도 늘리지 않아요). 계속 바쁜 날에도 알림이 2초마다 많아야 하나라서, 쉬는 틈을 기다리는 방식보다 예측하기 쉬워요. 요약 문구는 턴을 끝낸 에이전트가 섞여 있어도 "기다려요"예요. 끝낸 에이전트도 다음 지시를 기다리니까요.
- 보내기 직전에 다시 확인해요(`live`). Lead는 워커의 보고를 받으면 턴이 끝나자마자 다시 시작하고, 승인은 다른 곳에서 곧바로 답해질 수 있어서, 이미 지난 일을 알리는 걸 막아요.
- 알림을 누르면 그 에이전트를 여는 기능은 넣지 않았어요. 데스크톱 플러그인의 클릭 처리가 운영체제마다 고르지 않아서 약속할 수 없어요.
- 권한은 `notification:default` 대신 쓰는 명령 셋만 열었어요(예약·채널·액션은 쓰지 않아요).
- 설정은 켜기/끄기 대신 [모두 | 기다릴 때만 | 끔]이에요. 턴 끝 알림은 에이전트가 많으면 잦아서, 기다림만 받고 싶은 경우를 따로 뒀어요.
- 2026-11-24 (리뷰 반영): 앱을 켜는 동안 불러오는 것은 알리지 않아요(`loaded`, 스토어 `listed`). 병합 요청은 다시 켜도 남아서, 창이 뒤에 있는 채로 켜지거나 업데이트로 다시 켜지면 이미 알던 요청이 "병합 요청 N"이나 "에이전트 N개가 기다려요"로 또 떴고, 워크트리를 읽기 전이라 제목이 Lead 대신 워커 이름이었어요. 짧은 유예 시간 대신 실제로 다 불러왔는지(프로젝트 목록, 프로젝트마다 에이전트, 이미 기다리던 목록)를 봐요. 프로젝트가 많아 오래 걸려도 맞고, 빨리 끝나면 바로 알리기 시작해서예요. 한 번 다 불러오면 그대로라, 나중에 프로젝트를 더하는 동안 다른 에이전트 알림이 묻히지 않아요.
- 2026-11-24 (리뷰 반영): 보내기 직전 확인(`live`)을 에이전트가 아니라 알림이 말하는 항목으로 해요(`Notice.ids`). 오래된 병합 요청이 있는 Lead에 새 승인이 왔다가 2초 안에 정리되면(턴 취소 등), 에이전트로만 보면 병합 요청 때문에 "승인 대기 · Bash"가 그대로 나갔어요.
