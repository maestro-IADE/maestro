import { CANVAS_BGS, CANVAS_GAPS, EDGE_SHAPES, GUIDE_MODES, LINE_LABELS, useSettings } from "../settings";
import { shapeOf } from "../edgePath";
import { Group, Row, Seg, Switch } from "./parts";

/** A Seg's options from one of the settings' step tables (id → { label }). */
const opts = <K extends string>(table: Record<K, { label: string }>) => (Object.keys(table) as K[]).map((id) => [id, table[id].label] as const);

/**
 * 캔버스: what only the canvases have. How what an agent puts on its canvas comes in (shown at once or let in by
 * hand, how long its line stays, whether the view follows), the lines, and the pattern behind everything.
 * (docs/features/ui/settings-dialog.md; the look shared by the whole app is 모양.)
 */
export function CanvasSection() {
  const s = useSettings();
  return (
    <>
      <Group title="에이전트의 시각화">
        <Row label="추천 그림" desc={(GUIDE_MODES[s.guideMode] ?? GUIDE_MODES.auto).hint}>
          <Seg value={s.guideMode ?? "auto"} options={opts(GUIDE_MODES)} onChange={(v) => s.set({ guideMode: v })} />
        </Row>
        <Row label="화면 따라가기">
          <Switch on={s.canvasFollow ?? true} label="화면 따라가기" onChange={(on) => s.set({ canvasFollow: on })} />
        </Row>
      </Group>

      <Group title="선">
        <Row label="연결선">
          <Seg
            value={shapeOf(s.edgeShape)}
            options={Object.entries(EDGE_SHAPES).map(([id, e]) => [id as keyof typeof EDGE_SHAPES, e.label] as const)}
            onChange={(v) => s.set({ edgeShape: v })}
          />
        </Row>
        <Row label="선 라벨">
          <Seg
            value={s.lineLabels ?? "always"}
            options={Object.entries(LINE_LABELS).map(([id, l]) => [id as keyof typeof LINE_LABELS, l.label] as const)}
            onChange={(v) => s.set({ lineLabels: v })}
          />
        </Row>
      </Group>

      <Group title="바탕">
        <Row label="무늬">
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
    </>
  );
}
