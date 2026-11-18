import { useState } from "react";
import { EASINGS, PANEL_STYLES, SPEEDS, useSettings } from "../settings";
import { Group, Row, Seg, Switch } from "./parts";
import { ThemePicker } from "./ThemePicker";

/**
 * 모양: the look the whole app shares. The theme (theme cards and 내 팔레트, ThemePicker.tsx), how the panels sit in
 * the window, and the motion. What only the canvases or only the chat have is in 캔버스 (CanvasSection.tsx) and 채팅
 * (ChatSection.tsx).
 */

export function LookSection() {
  const s = useSettings();
  const [flip, setFlip] = useState(false);
  return (
    <>
      <ThemePicker />

      <Group title="패널 구분">
        <Row label="패널" desc={PANEL_STYLES[s.panelStyle]?.hint}>
          <Seg
            value={s.panelStyle}
            options={Object.entries(PANEL_STYLES).map(([id, p]) => [id as keyof typeof PANEL_STYLES, p.label] as const)}
            onChange={(v) => s.set({ panelStyle: v })}
          />
        </Row>
      </Group>

      <Group title="모션">
        <Row label="모션 사용" desc="패널 접기·펴기, 미니맵 확대, 창 열기, 블록으로 이동">
          <Switch label="모션 사용" on={s.motion} onChange={(v) => s.set({ motion: v })} />
        </Row>
        <Row label="이징">
          <Seg disabled={!s.motion} value={s.easing} options={Object.entries(EASINGS).map(([id, e]) => [id as keyof typeof EASINGS, e.label] as const)} onChange={(v) => s.set({ easing: v })} />
        </Row>
        <Row label="속도">
          <Seg disabled={!s.motion} value={s.speed} options={Object.entries(SPEEDS).map(([id, sp]) => [id as keyof typeof SPEEDS, sp.label] as const)} onChange={(v) => s.set({ speed: v })} />
        </Row>
        <Row label="미리보기">
          <button className="motion-preview" onClick={() => setFlip(!flip)} title="눌러서 미리보기">
            <span className="knob" data-flip={flip || undefined} />
          </button>
        </Row>
      </Group>
    </>
  );
}
