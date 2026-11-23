import { AWAY_CHOICES, useSettings } from "../settings";
import { Group, Row, Seg, Switch } from "./parts";

/**
 * 일반: how agents show (chat or terminal, the first screen of a new tab by role), the sidebar's rows, OS
 * notifications, the away stop and the editor's auto save. Every value is `useSettings` (`maestro.settings`), under
 * its old key.
 */
export function GeneralSection() {
  const s = useSettings();
  return (
    <>
      <Group title="화면">
        <Row label="에이전트 보기" desc="다음에 여는 대화부터 적용돼요">
          <Seg value={s.agentView} options={[["chat", "채팅"], ["cli", "터미널"]]} onChange={(v) => s.set({ agentView: v })} />
        </Row>
        <Row label="열 때 화면 — 에이전트">
          <Seg value={s.agentLayout} options={[["full", "전체 화면"], ["canvas", "캔버스"]]} onChange={(v) => s.set({ agentLayout: v })} />
        </Row>
        <Row label="열 때 화면 — Lead">
          <Seg value={s.maestroLayout} options={[["canvas", "캔버스"], ["full", "전체 화면"]]} onChange={(v) => s.set({ maestroLayout: v })} />
        </Row>
        <Row label="사이드바 줄">
          <Seg value={s.sidebarDensity} options={[["two", "두 줄"], ["one", "한 줄"]]} onChange={(v) => s.set({ sidebarDensity: v })} />
        </Row>
        <Row label="에이전트 없는 워크트리">
          <Seg value={s.showBareWorktrees ? "show" : "hide"} options={[["hide", "숨기기"], ["show", "보이기"]]} onChange={(v) => s.set({ showBareWorktrees: v === "show" })} />
        </Row>
      </Group>

      <Group title="알림">
        <Row label="OS 알림">
          <Seg value={s.osNotify} options={[["all", "모두"], ["waiting", "기다릴 때만"], ["off", "끔"]]} onChange={(v) => s.set({ osNotify: v })} />
        </Row>
      </Group>

      <Group title="자리 비움">
        <Row
          label="자리 비운 프로젝트 정리"
          desc="쉬는 에이전트를 꺼요. 돌아오면 대화를 이어서 켜요"
        >
          <select aria-label="자리 비운 프로젝트 정리" value={s.awayMinutes} onChange={(e) => s.set({ awayMinutes: Number(e.target.value) })}>
            {AWAY_CHOICES.map((c) => <option key={c.minutes} value={c.minutes}>{c.label}</option>)}
          </select>
        </Row>
      </Group>

      <Group title="파일 편집">
        <Row label="자동 저장">
          <Switch label="자동 저장" on={s.autoSave} onChange={(v) => s.set({ autoSave: v })} />
        </Row>
      </Group>
    </>
  );
}
