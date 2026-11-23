// The commands of the developer tests' 캔버스 CLI 시험 (DevTools.tsx, docs/features/ui/dev-tools.md): what is run, in
// order, and what each should do. Pure data, so scripts/dev-steps-check.ts can check the list against the rules the
// commands themselves enforce (a step that isn't marked `fails` must be one the backend accepts).

/**
 * One command. `args`: what comes after `maestro`. `file`: the JSON a `--file` flag points at (shown under the name
 * `fileName`). `text`: a `--file` that isn't JSON (a picture or a page for `maestro show`), written under
 * `fileName` as it is. `fails`: the command must be refused (the step passes when it is). `see`: what to look for on the
 * canvas or in the answer. `settle`: how long the canvas gets before the next command (it takes the drawing, then
 * saves half a second later, and `erase` and `canvas` read that saved file).
 */
export type Step = { title: string; args: string[]; file?: unknown; text?: string; fileName?: string; fails?: boolean; see: string; settle?: number };

const SAMPLE_SVG = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 320 200\"><rect width=\"320\" height=\"200\" fill=\"#f6f7f9\"/><text x=\"16\" y=\"28\" font-family=\"sans-serif\" font-size=\"14\" fill=\"#222\">응답 시간 (ms)</text><g fill=\"#4f7cff\"><rect x=\"32\" y=\"120\" width=\"40\" height=\"56\"/><rect x=\"96\" y=\"90\" width=\"40\" height=\"86\"/><rect x=\"160\" y=\"60\" width=\"40\" height=\"116\"/><rect x=\"224\" y=\"104\" width=\"40\" height=\"72\"/></g><line x1=\"16\" y1=\"176\" x2=\"304\" y2=\"176\" stroke=\"#888\"/></svg>";
const SAMPLE_HTML = "<!doctype html><html lang=\"ko\"><meta charset=\"utf-8\"><title>시험 페이지</title><style>body{font:14px sans-serif;margin:16px;color:#222}button{padding:6px 12px}</style><h3>시험 페이지</h3><p>보기 블록 안의 HTML이에요. 버튼을 누르면 숫자가 올라가요.</p><button id=\"b\">눌러 보기</button> <b id=\"n\">0</b><script>let n=0;document.getElementById(\"b\").onclick=()=>{document.getElementById(\"n\").textContent=++n}</script></html>";

/** The relation the test declares: dev.rs takes relations named 시험-… away when the test starts over or ends. */
export const REL = "시험-의존";

export const STEPS: Step[] = [
  { title: "명령 목록", args: ["--help"], see: "draw, erase, ask, canvas 같은 명령이 답에 나와요", settle: 200 },
  {
    title: "관계 선언",
    args: ["relation", "define", "--name", REL, "--description", "앞이 끝나야 뒤가 시작해요", "--pattern", "dashed", "--color", "amber"],
    see: `도구 막대의 관계 패널에 '${REL}'이 생겨요 (시험이 끝나면 지워져요)`,
    settle: 300,
  },
  {
    title: "그리기: 영역, 블록, 화살표, 배치",
    args: ["draw"],
    fileName: "1-흐름.json",
    file: {
      layout: "flow",
      areas: [{ title: "설계", layout: "grid", color: "blue" }, { title: "구현", layout: "flow", color: "green" }, { title: "검증", layout: "tree", color: "violet" }],
      blocks: [
        { title: "요구 정리", area: "설계", body: "## 요구 정리\n- 무엇을 만들지 적어요\n- 하지 않을 것도 적어요" },
        { title: "구조 잡기", area: "설계" },
        { title: "API", area: "구현" },
        { title: "화면", area: "구현" },
        { title: "연결", area: "구현", color: "amber" },
        { title: "시험", area: "검증" },
        { title: "검토", area: "검증" },
      ],
      arrows: [
        { from: "설계", to: "구현", label: "넘겨요" },
        { from: "구현", to: "검증", label: "넘겨요" },
        { from: "API", to: "연결", as: REL, label: "끝나면" },
        { from: "화면", to: "연결", as: REL, label: "끝나면" },
        { from: "시험", to: "검토", label: "통과하면" },
        { from: "검증", to: "구현", label: "되돌려요", body: "검토에서 걸리면 구현으로 돌아가요" },
      ],
    },
    see: "영역 셋(설계 → 구현 → 검증)이 왼쪽에서 오른쪽으로 놓여요. 영역 테두리는 파랑·초록·보라, '연결' 블록 테두리는 호박색이에요. 'API → 연결', '화면 → 연결'은 주황 점선이에요",
  },
  {
    title: "그리기: 있는 것 고치기, 영역 안에 더하기",
    args: ["draw"],
    fileName: "2-고치기.json",
    file: {
      areas: [{ title: "구현" }, { title: "배포 준비", in: "구현" }],
      blocks: [{ title: "API", body: "서버 쪽이에요. 인증을 먼저 붙여요" }, { title: "연결" }, { title: "빌드", area: "배포 준비" }],
      arrows: [{ from: "연결", to: "빌드", label: "되면" }],
    },
    see: "'API' 블록 모서리가 접혀요(내용이 생김). '구현' 아래쪽에 '배포 준비' 영역과 '빌드' 블록이 더해지고 '구현'이 커져요",
  },
  { title: "캔버스 읽기", args: ["canvas", "--brief", "true"], see: "답에 영역 4개, 블록 8개, 화살표, lint가 나와요", settle: 300 },
  // steps and states (zones.md): drawn, moved, read back
  {
    title: "그리기: 단계가 있는 영역, 상태가 있는 영역",
    args: ["draw"],
    fileName: "4-단계와-상태.json",
    file: {
      layout: "grid",
      areas: [
        { title: "배포 순서", layout: "flow", color: "teal", steps: ["빌드", "올리기", "확인"] },
        { title: "서버", layout: "grid", color: "red", states: ["정상", "장애"] },
      ],
      blocks: [
        { title: "이미지 만들기", area: "배포 순서", step: 1 },
        { title: "스테이징에 올리기", area: "배포 순서", step: 2 },
        { title: "스모크 테스트", area: "배포 순서", step: [2, 3] },
        { title: "배포 기록", area: "배포 순서" },
        { title: "로드밸런서", area: "서버" },
        { title: "주 서버", area: "서버", state: "정상" },
        { title: "예비 서버", area: "서버", state: "장애" },
      ],
      arrows: [
        { from: "이미지 만들기", to: "배포 기록", label: "남겨요" },
        { from: "로드밸런서", to: "주 서버", label: "보내요", state: "정상" },
        { from: "로드밸런서", to: "예비 서버", label: "돌려요", state: "장애" },
      ],
    },
    see: "'배포 순서' 머리에 ◀ 1 / 3 · 빌드 ▶가 있고 '이미지 만들기'와 '배포 기록'만 보여요. '서버'는 상태 목록이 '정상'이고 '주 서버'로 가는 선만 보여요",
  },
  { title: "단계 넘기기: 다음", args: ["step", "--area", "배포 순서"], see: "'배포 순서'가 2 / 3 · 올리기가 되고, '스테이징에 올리기'와 '스모크 테스트'가 보여요. '이미지 만들기'는 사라져요" },
  { title: "단계 넘기기: 번호로", args: ["step", "--area", "배포 순서", "--to", "3"], see: "3 / 3 · 확인이 되고 '스모크 테스트'와 '배포 기록'만 보여요" },
  { title: "상태 바꾸기", args: ["state", "--area", "서버", "--to", "장애"], see: "'서버'가 장애가 되고 '주 서버' 대신 '예비 서버'와 그쪽 선이 보여요" },
  { title: "거절: 없는 상태", args: ["state", "--area", "서버", "--to", "점검"], fails: true, see: "'서버'에 없는 상태 이름이라 거절돼요", settle: 200 },
  { title: "거절: 범위를 벗어난 단계의 블록", args: ["draw"], fileName: "5-없는-단계.json", file: { areas: [{ title: "배포 순서" }], blocks: [{ title: "되돌리기", area: "배포 순서", step: 4 }] }, fails: true, see: "'배포 순서'는 3단계까지라 거절돼요", settle: 200 },
  { title: "캔버스 읽기: 단계와 상태", args: ["canvas", "--brief", "true"], see: "'배포 순서'에 mode: step, current: 3, '서버'에 mode: state, current: 장애가 있고, 블록마다 in_steps / in_states가 나와요", settle: 300 },
  {
    title: "그리기: 묶음 (설명 한 줄과 함께)",
    args: ["draw", "--say", "배포 전에 하나씩 확인할 점검표예요"],
    fileName: "9-묶음.json",
    file: {
      areas: [{ title: "배포 점검표", stack: true, color: "teal" }],
      blocks: [
        { title: "점검 1 · 빌드 통과", area: "배포 점검표" },
        { title: "점검 2 · 마이그레이션", area: "배포 점검표", body: "되돌리는 스크립트도 같이 확인해요" },
        { title: "점검 3 · 알림 채널", area: "배포 점검표" },
      ],
    },
    see: "'배포 점검표' 묶음이 놓이고 블록 셋이 준 순서대로 세로 한 줄로 붙어요. 묶음은 든 것에 딱 맞는 크기예요 (블록을 위아래로 끌면 순서가 바뀌고, 밖으로 끌면 빠져요). 캔버스 위쪽 가운데에 설명 한 줄이 떴다가 사라지고, 화면이 묶음으로 가요 (설정 → 그림 넘기기가 '눌러서'면 그 설명을 눌러야 묶음이 나타나요)",
  },
  { title: "거절: 단계가 있는 묶음", args: ["draw"], fileName: "10-단계-묶음.json", file: { areas: [{ title: "안 되는 묶음", stack: true, steps: 2 }] }, fails: true, see: "묶음에는 단계나 상태를 줄 수 없어 거절돼요. 캔버스는 그대로예요", settle: 200 },
  { title: "캔버스 읽기: 묶음", args: ["canvas", "--brief", "true"], see: "'배포 점검표'에 stack: true가 있고 holds가 위에서 아래 순서로 나와요", settle: 300 },
  { title: "질문: 하나 고르기", args: ["ask", "--title", "어느 DB로 갈까요?", "--choices", "Postgres|SQLite|둘 다", "--body", "로컬 개발 기준이에요"], see: "노란 테두리의 질문 블록이 캔버스의 빈 자리에 놓여요: 옆에 자리가 있으면 옆, 폭이 넘치면 아래 줄 (동그라미 = 하나 고르기)" },
  { title: "질문: 여러 개 고르기와 기타", args: ["ask", "--title", "넣을 기능을 골라 주세요", "--choices", "로그인|검색|알림|결제", "--multi", "true", "--other", "true"], see: "네모 표시(여러 개 고르기)와 '기타' 글칸이 있는 질문 블록이 놓여요" },
  { title: "질문: 주관식", args: ["ask", "--title", "서비스 이름은 뭘로 할까요?"], see: "글칸만 있는 질문 블록이 놓여요" },
  { title: "거절: 보기가 하나뿐인 질문", args: ["ask", "--title", "보기 하나", "--choices", "A"], fails: true, see: "보기는 둘 이상이어야 해서 거절돼요. 캔버스는 그대로예요", settle: 200 },
  {
    title: "질문 묶음: 여러 개를 한 번에",
    args: ["ask"],
    fileName: "6-질문-묶음.json",
    file: {
      title: "시작 전에 정할 것",
      questions: [
        { title: "어떤 화면부터 만들까요?", choices: ["로그인", "대시보드", "설정"] },
        { title: "지원할 플랫폼을 골라 주세요", choices: ["Windows", "macOS", "Linux"], multi: true, other: true },
        { title: "첫 버전의 마감은 언제인가요?", body: "대략이면 돼요" },
      ],
    },
    see: "'시작 전에 정할 것' 영역이 놓이고 머리에 1 / 3이 보여요. 화살표로 답하면 다음 질문으로 넘어가고, 마지막 답에서 모두 보내요 (답 셋이 메모 하나로 나와요)",
  },
  {
    title: "질문 묶음: 곧 지울 것",
    args: ["ask"],
    fileName: "7-지울-묶음.json",
    file: { title: "곧 지울 묶음", questions: [{ title: "A안과 B안 중 어느 쪽인가요?", choices: ["A안", "B안"] }, { title: "고른 이유를 적어 주세요" }] },
    see: "두 번째 묶음 영역이 빈 자리에 놓여요 (뒤에서 묶음째 지워요)",
  },
  { title: "거절: 질문이 하나뿐인 묶음", args: ["ask"], fileName: "8-하나뿐인-묶음.json", file: { questions: [{ title: "혼자" }] }, fails: true, see: "묶음은 질문이 둘 이상일 때 써서 거절돼요. 캔버스는 그대로예요", settle: 200 },
  {
    title: "거절: 없는 이름으로 가는 화살표",
    args: ["draw"],
    fileName: "3-틀린-화살표.json",
    file: { blocks: [{ title: "혼자" }], arrows: [{ from: "혼자", to: "없는 블록" }] },
    fails: true,
    see: "화살표의 to가 파일에 없는 이름이라 거절돼요. 캔버스는 그대로예요",
    settle: 200,
  },
  { title: "보기 블록: 그림 파일", args: ["show", "--title", "응답 시간 (시험 그림)"], fileName: "시험-그림.svg", text: SAMPLE_SVG, see: "막대 그림이 든 보기 블록이 빈 자리에 놓여요 (머리에 제목과 파일 이름, 다시 읽기 버튼)" },
  { title: "보기 블록: HTML 파일", args: ["show", "--title", "시험 페이지"], fileName: "시험-페이지.html", text: SAMPLE_HTML, see: "작은 페이지가 든 보기 블록이 놓여요. 안의 버튼을 누르면 숫자가 올라가요 (페이지는 앱에 닿지 못하는 틀 안에서 돌아요)" },
  { title: "거절: 보여 줄 수 없는 파일", args: ["show", "--file", "notes.md"], fails: true, see: "이미지나 HTML이 아니라서 거절돼요. 캔버스는 그대로예요", settle: 200 },
  { title: "설명 한 줄: 그림 없이", args: ["say", "--text", "여기까지가 그리기와 묻기예요. 이제 화면을 옮기고 배치를 맞춰요"], see: "캔버스 위쪽 가운데에 그 한 줄이 떴다가 사라져요 (캔버스는 그대로예요)" },
  { title: "화면 옮기기: 이름으로", args: ["focus", "--title", "구현", "--say", "구현 영역이에요"], see: "화면이 '구현' 영역으로 가고 그 테두리가 잠깐 강조돼요. 설명도 함께 떠요" },
  { title: "거절: 너무 긴 설명", args: ["say", "--text", "가".repeat(121)], fails: true, see: "설명은 120자 안의 한 줄이라 거절돼요", settle: 200 },
  { title: "배치 정리", args: ["tidy"], see: "영역 안의 블록이 줄 맞춰지고(같은 줄은 같은 높이, 같은 폭), 그린 영역·질문·보기 블록이 빈틈 없이 다시 놓여요 (되돌리기: Ctrl+Z)" },
  { title: "선택 읽기", args: ["selection"], see: "캔버스에서 블록을 골라 두었으면 그 블록이 답에 나와요. 질문 블록도 돼요 (안 골랐으면 빈 목록)", settle: 200 },
  { title: "지우기: 화살표", args: ["erase", "--from", "검증", "--to", "구현"], see: "'검증 → 구현'(되돌려요) 화살표가 사라져요" },
  { title: "지우기: 영역", args: ["erase", "--title", "배포 준비"], see: "'배포 준비' 영역이 사라지고 안에 있던 '빌드'는 남아요" },
  { title: "지우기: 질문", args: ["erase", "--question", "서비스 이름은 뭘로 할까요?"], see: "주관식 질문 블록이 사라져요 (에이전트가 답을 적용한 뒤 치우는 것과 같은 명령이에요)" },
  { title: "지우기: 질문 묶음", args: ["erase", "--question", "곧 지울 묶음"], see: "'곧 지울 묶음' 영역이 안의 질문 둘과 함께 사라져요 (묶음은 id나 물을 때 준 이름으로 지워요)" },
  { title: "지우기: 보기 블록", args: ["erase", "--view", "응답 시간 (시험 그림)"], see: "그림 보기 블록이 사라져요 (파일 경로나 제목으로 지워요). HTML 보기 블록은 남아요" },
  { title: "거절: 없는 이름 지우기", args: ["erase", "--title", "없는-이름"], fails: true, see: "캔버스에 없는 이름이라 아무것도 지우지 않고 거절돼요", settle: 200 },
  { title: "캔버스 다시 읽기", args: ["canvas", "--brief", "true"], see: "지운 뒤의 캔버스예요. 남은 질문은 questions에 answer: null로 나오고, 묶음의 질문에는 set과 area가 붙어요. 남은 보기 블록은 others에 kind: view로 나와요 (남은 질문에 답해 보려면 지금 해요: 다음 단계가 모두 지워요)", settle: 300 },
  { title: "지우기: 전부", args: ["erase", "--all", "true"], see: "캔버스가 비어요: 그린 블록·영역·화살표, 남아 있던 질문 블록과 질문 묶음, HTML 보기 블록까지 모두 사라져요 (답의 erased에 지운 수와, 답 없이 지워진 질문 수 unanswered가 나와요)" },
  { title: "관계 지우기", args: ["relation", "remove", "--name", REL], see: `관계 패널에서 '${REL}'이 사라져요`, settle: 300 },
];

/**
 * The commands of the 채팅 창구 시험 (docs/features/agents/chat-visuals.md): what an agent puts up for its canvas
 * while the user is in the chat. It starts with the canvas closed and 추천 그림 on 눌러서, so each thing waits in
 * the stand-in chat as a card with a small picture; `see` says what to press there. Ending the test takes
 * everything away, so it has no erasing steps of its own.
 */
export const CHAT_STEPS: Step[] = [
  {
    title: "그리기: 채팅에 먼저 보여요",
    args: ["draw", "--say", "설계에서 구현으로 가는 흐름이에요"],
    fileName: "1-흐름.json",
    file: {
      layout: "flow",
      areas: [{ title: "설계", layout: "grid", color: "blue" }, { title: "구현", layout: "flow", color: "green" }],
      blocks: [{ title: "요구 정리", area: "설계" }, { title: "구조 잡기", area: "설계" }, { title: "API", area: "구현" }, { title: "화면", area: "구현" }],
      arrows: [{ from: "설계", to: "구현", label: "넘겨요" }, { from: "API", to: "화면" }],
    },
    see: "채팅 대역에 '설계에서 구현으로 가는 흐름이에요' 카드가 뜨고, 그 안에 영역 둘과 블록 넷의 작은 그림이 보여요. 캔버스에는 아직 없어요. '캔버스에 올리기'를 누르면 카드가 '올린 것 1개' 줄로 바뀌어요",
    settle: 600,
  },
  {
    title: "그리기: 버려 보기",
    args: ["draw", "--say", "검증 단계를 더했어요"],
    fileName: "2-검증.json",
    file: { areas: [{ title: "검증", layout: "tree", color: "violet" }], blocks: [{ title: "시험", area: "검증" }, { title: "검토", area: "검증" }], arrows: [{ from: "시험", to: "검토", label: "통과하면" }] },
    see: "앞의 것을 올렸으면 이 카드가 떠요 (안 올렸으면 '뒤에 1개 더'). 이 카드에서 '버리기'를 누르면 카드가 사라지고, 왼쪽 '에이전트에게 갈 메모'에 버렸다는 메모가 떠요",
    settle: 600,
  },
  {
    title: "채팅 선택지와 함께",
    args: ["ask", "--in", "chat", "--title", "어느 쪽부터 볼까요?", "--choices", "설계|구현"],
    see: "그림 카드 아래에 선택지 카드가 따로 떠요 (1 설계, 2 구현, 3 직접 입력). 고르면 왼쪽에 답 메모가 떠요",
    settle: 400,
  },
  {
    title: "이미 있는 것 가리키기",
    args: ["focus", "--title", "설계", "--say", "설계 영역이에요"],
    see: "앞의 카드를 모두 넘기면 '설계 영역이에요' 카드가 떠요: 새로 그리지 않고 '설계' 영역의 그림만 보여요. '캔버스에서 보기'를 누르면 캔버스가 열리고 화면이 그 영역으로 가요",
    settle: 600,
  },
  {
    title: "보기 블록",
    args: ["show", "--title", "응답 시간 (시험 그림)", "--say", "응답 시간 그림이에요"],
    fileName: "시험-그림.svg",
    text: SAMPLE_SVG,
    see: "카드에 막대 그림이 그대로 보여요 (캔버스에 올리기 전에도). 캔버스가 열려 있으면 캔버스의 안내와 채팅 카드가 함께 기다려요: 어느 쪽을 눌러도 같아요",
    settle: 600,
  },
  {
    title: "보기 블록: HTML 페이지",
    args: ["show", "--title", "시험 페이지", "--say", "작은 페이지예요"],
    fileName: "시험-페이지.html",
    text: SAMPLE_HTML,
    see: "카드에 페이지가 작게 줄어든 모습으로 보여요 (카드 안에서는 누를 수 없어요. 캔버스에 올리면 그 블록 안에서 버튼이 눌려요)",
    settle: 600,
  },
  {
    title: "설명 한 줄: 그림 없이",
    args: ["say", "--text", "여기까지가 채팅 창구예요"],
    see: "그림 없는 카드가 떠요 ('넘기기'만 있어요). 위의 '추천 그림'을 바로로 바꾸면 기다리던 것이 모두 저절로 캔버스에 올라가고 카드는 사라져요",
    settle: 400,
  },
  {
    title: "바로일 때: 캔버스를 닫고 그리기",
    args: ["draw", "--say", "배포 단계예요"],
    fileName: "3-배포.json",
    file: { areas: [{ title: "배포", color: "amber" }], blocks: [{ title: "빌드", area: "배포" }, { title: "출시", area: "배포" }], arrows: [{ from: "빌드", to: "출시" }] },
    see: "'추천 그림'을 바로로, 캔버스를 닫아 두고 실행해요: 채팅 대역에 그림이 '캔버스를 열면 올라가요'로 떠요. '캔버스에서 보기'를 누르면 캔버스가 열리며 올라가고, ×는 채팅에서만 치워요",
    settle: 600,
  },
];

/** The command as it would be typed: a word with a space or a shell character in it is quoted. */
/**
 * 깊은 영역 시험 (zones.md "들어가기"): areas six deep, the innermost with steps, to go into one level after
 * another and see the path fold its front into a list, the Step line under it, and going out again. Its own canvas.
 */
export const DEEP_STEPS: Step[] = [
  {
    title: "그리기: 여섯 겹 영역",
    args: ["draw", "--say", "회사 › 제품 › 앱 › 화면 › 로그인 › 로그인 흐름, 여섯 겹이에요"],
    fileName: "1-다섯-겹.json",
    file: {
      areas: [
        { title: "회사", color: "blue" },
        { title: "제품", in: "회사" },
        { title: "앱", in: "제품", color: "green" },
        { title: "화면", in: "앱" },
        { title: "로그인", in: "화면", color: "violet" },
        { title: "로그인 흐름", in: "로그인", steps: ["입력", "검사", "결과"], layout: "flow" },
      ],
      blocks: [
        { title: "조직도", area: "회사" },
        { title: "로드맵", area: "제품" },
        { title: "설정", area: "앱" },
        { title: "홈", area: "화면" },
        { title: "아이디 입력", area: "로그인" },
        { title: "비밀번호", area: "로그인" },
        { title: "폼 채우기", area: "로그인 흐름", step: 1 },
        { title: "서버 검사", area: "로그인 흐름", step: 2 },
        { title: "환영 화면", area: "로그인 흐름", step: 3 },
      ],
      arrows: [{ from: "아이디 입력", to: "비밀번호" }, { from: "홈", to: "로그인" }],
    },
    see: "영역 여섯이 겹쳐 그려져요(맨 안 '로그인 흐름'은 Step 셋). 맨 안으로 들어가면 경로 아래 줄에 '◀ Step 1 · 입력 ▶'이 떠요. '회사'를 더블클릭하면 그 안만 보이고 위에 '↑ 캔버스 › 회사' 경로가 떠요. '제품' → '앱' → '화면'을 차례로 더블클릭하면 넷째부터 경로 앞이 '…▾'로 접혀요(넓을 때 '…▾ › 앱 › 화면'). …▾를 누르면 캔버스·회사·제품 중 골라 그곳으로 나가요. 시험 창을 좁히면(캔버스 760px 아래) 두 자리로 줄어요. 들어갈수록 바탕이 조금씩 짙어져요",
    settle: 800,
  },
  {
    title: "Step 넘기기",
    args: ["step", "--area", "로그인 흐름", "--to", "2", "--say", "검사 단계로 넘겼어요"],
    see: "'로그인 흐름'이 Step 2 · 검사가 되고 '서버 검사'가 보여요. 들어가 있으면 경로 아래 줄도 Step 2로 바뀌어요",
    settle: 600,
  },
  {
    title: "깊은 블록 가리키기",
    args: ["focus", "--title", "비밀번호", "--say", "맨 안쪽의 비밀번호 블록이에요"],
    see: "화면이 '비밀번호'로 가고 테두리가 밝아져요(맨 밖에서 보고 있을 때). 영역 안에 들어가 있으면 그 영역 밖은 숨어 있어서 화면이 움직이지 않을 수 있어요",
    settle: 600,
  },
  {
    title: "읽기: 겹친 영역",
    args: ["canvas"],
    see: "답의 areas에 '제품'의 in이 '회사', '로그인 흐름'의 in이 '로그인'처럼 다섯 겹이 차례로 보이고, '로그인 흐름'의 current가 2예요. 손으로 해 볼 것: 아무 영역을 우클릭 → 작게 접기 한 뒤 더블클릭하면 칩 없이 안으로 들어가요. 새 탭에서 열기는 시험 캔버스에서는 안 돼요(에이전트 캔버스가 아니라서)",
    settle: 300,
  },
];

export const typed = (s: Step) =>
  ["maestro", ...s.args.map((a) => (/[\s|&;<>()"'`$]/.test(a) ? `"${a.replace(/"/g, '\\"')}"` : a)), ...(s.fileName ? ["--file", s.fileName] : [])].join(" ");
