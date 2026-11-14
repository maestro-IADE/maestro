import { useState } from "react";
import { CANVAS_BGS, CANVAS_GAPS, EASINGS, EDGE_SHAPES, LINE_LABELS, PANEL_STYLES, SPEEDS, useSettings } from "../settings";
import { Group, Row, Seg, Switch } from "./parts";
import { ThemePicker } from "./ThemePicker";

/** 모양: the theme (theme cards and 내 팔레트, ThemePicker.tsx), how the panels sit in the window, the canvas pattern and the motion (the old 스타일 section). */
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

      <Group title="캔버스">
        <Row label="연결선" desc={(s.edgeShape === "curve" || !s.edgeShape ? EDGE_SHAPES.curve : EDGE_SHAPES.straight).hint}>
          <Seg
            value={s.edgeShape === "curve" || !s.edgeShape ? "curve" : "straight"}
            options={Object.entries(EDGE_SHAPES).map(([id, e]) => [id as keyof typeof EDGE_SHAPES, e.label] as const)}
            onChange={(v) => s.set({ edgeShape: v })}
          />
        </Row>
        <Row label="선 라벨" desc={(LINE_LABELS[s.lineLabels] ?? LINE_LABELS.always).hint}>
          <Seg
            value={s.lineLabels ?? "always"}
            options={Object.entries(LINE_LABELS).map(([id, l]) => [id as keyof typeof LINE_LABELS, l.label] as const)}
            onChange={(v) => s.set({ lineLabels: v })}
          />
        </Row>
        <Row label="무늬" desc={(CANVAS_BGS[s.canvasBg] ?? CANVAS_BGS.lines).hint}>
          <Seg
            value={s.canvasBg ?? "lines"}
            options={Object.entries(CANVAS_BGS).map(([id, b]) => [id as keyof typeof CANVAS_BGS, b.label] as const)}
            onChange={(v) => s.set({ canvasBg: v })}
          />
        </Row>
        <Row label="간격">
          <div className="seg">
            {CANVAS_GAPS.map((g) => (
              <button key={g.px} disabled={s.canvasBg === "none"} data-on={g.px === (s.canvasGap ?? 24) || undefined} title={`${g.px}px`} onClick={() => s.set({ canvasGap: g.px })}>{g.label}</button>
            ))}
          </div>
        </Row>
        <Row label="진하기">
          <div className="set-range">
            <input
              type="range"
              aria-label="무늬 진하기"
              min={10}
              max={100}
              step={5}
              disabled={s.canvasBg === "none"}
              value={s.canvasAlpha ?? 100}
              onChange={(e) => s.set({ canvasAlpha: Number(e.target.value) })}
            />
            <span className="muted small">{s.canvasAlpha ?? 100}%</span>
          </div>
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
