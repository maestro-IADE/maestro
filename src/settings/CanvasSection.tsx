import { CANVAS_BGS, CANVAS_GAPS, EDGE_SHAPES, GUIDE_MODES, GUIDE_SECONDS, LINE_LABELS, useSettings } from "../settings";
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
        <Row label="시각화 허용" desc={(GUIDE_MODES[s.guideMode] ?? GUIDE_MODES.auto).hint}>
          <Seg value={s.guideMode ?? "auto"} options={opts(GUIDE_MODES)} onChange={(v) => s.set({ guideMode: v })} />
        </Row>
        <Row label="안내 시간" desc="에이전트의 설명 한 줄이 캔버스에 머무는 시간이에요. 자동으로 허용할 때는 이 시간 뒤에 다음 그림이 나와요.">
          <div className="seg">
            {GUIDE_SECONDS.map((sec) => (
              <button key={sec} data-on={sec === (s.guideSeconds ?? 4) || undefined} onClick={() => s.set({ guideSeconds: sec })}>{sec}초</button>
            ))}
          </div>
        </Row>
        <Row label="화면 따라가기" desc="에이전트가 캔버스에 무엇을 놓으면 화면이 그리로 가요. 끄면 화면은 그대로 있고 위쪽 알림으로 알려 줘요.">
          <Switch on={s.canvasFollow ?? true} label="화면 따라가기" onChange={(on) => s.set({ canvasFollow: on })} />
        </Row>
      </Group>

      <Group title="선">
        <Row label="연결선" desc={EDGE_SHAPES[shapeOf(s.edgeShape)].hint}>
          <Seg
            value={shapeOf(s.edgeShape)}
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
      </Group>

      <Group title="바탕">
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
    </>
  );
}
