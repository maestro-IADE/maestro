import { AWAY_CHOICES, useSettings } from "../settings";
import { Group, Row, Seg, Switch } from "./parts";

/**
 * 일반: how agents show (chat or terminal, the first screen of a new tab by role), the sidebar's rows, the away stop
 * and the editor's auto save. Every value is `useSettings` (`maestro.settings`), under its old key.
 */
export function GeneralSection() {
  const s = useSettings();
  return (
    <>
      <Group title="화면">
        <Row label="에이전트 보기" desc="다음에 여는 대화부터 이 보기로 열려요. 진행 중인 대화는 터미널 위 막대로 넘겨요">
          <Seg value={s.agentView} options={[["chat", "채팅"], ["cli", "터미널"]]} onChange={(v) => s.set({ agentView: v })} />
        </Row>
        <Row label="열 때 화면 — 에이전트" desc="에이전트를 새 탭으로 열 때 처음 보이는 화면이에요. 탭 막대의 전환 버튼은 그 탭만 바꿔요">
          <Seg value={s.agentLayout} options={[["full", "전체 화면"], ["canvas", "캔버스"]]} onChange={(v) => s.set({ agentLayout: v })} />
        </Row>
        <Row label="열 때 화면 — Lead" desc="Lead를 새 탭으로 열 때 처음 보이는 화면이에요. 캔버스에서 워커가 보여요">
          <Seg value={s.maestroLayout} options={[["canvas", "캔버스"], ["full", "전체 화면"]]} onChange={(v) => s.set({ maestroLayout: v })} />
        </Row>
        <Row label="사이드바 줄" desc="두 줄은 이름 아래에 브랜치·PR이나 기다리는 일을 보여요. 한 줄은 한 화면에 더 많이 보여요">
          <Seg value={s.sidebarDensity} options={[["two", "두 줄"], ["one", "한 줄"]]} onChange={(v) => s.set({ sidebarDensity: v })} />
        </Row>
      </Group>

      <Group title="자리 비움">
        <Row
          label="자리 비운 프로젝트 정리"
          desc="다른 프로젝트로 옮긴 뒤 이만큼 지나면 쉬는 중인 에이전트를 꺼요. 돌아오면 대화를 이어서 다시 켜요"
        >
          <select aria-label="자리 비운 프로젝트 정리" value={s.awayMinutes} onChange={(e) => s.set({ awayMinutes: Number(e.target.value) })}>
            {AWAY_CHOICES.map((c) => <option key={c.minutes} value={c.minutes}>{c.label}</option>)}
          </select>
        </Row>
      </Group>

      <Group title="파일 편집">
        <Row label="자동 저장" desc="입력을 멈추고 1초 뒤에 저장해요. 그 사이 에이전트가 같은 파일을 바꿨으면 덮어쓰지 않고 알려 줘요">
          <Switch label="자동 저장" on={s.autoSave} onChange={(v) => s.set({ autoSave: v })} />
        </Row>
      </Group>
    </>
  );
}
