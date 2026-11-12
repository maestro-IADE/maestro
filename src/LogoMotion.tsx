import { useEffect, useRef } from "react";
import { logoLit, useSettings } from "./settings";

/**
 * The logo's three objects (B1), each two balls joined by a neck: [x, y, r] in a 48 × 40 box.
 * Left and right orchestra, and the conductor (one ball: its two balls share a spot).
 */
export const LOGO_SLOTS: Array<[[number, number, number], [number, number, number]]> = [
  [[16, 13, 5.2], [7, 23, 5.2]],
  [[32, 13, 5.2], [41, 23, 5.2]],
  [[24, 30.5, 4.2], [24, 30.5, 4.2]],
];
const CONDUCTOR = 2;
const O = [24, 22]; // what everything turns around
const TAU = Math.PI * 2;
const ease = (t: number) => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2);
const clamp01 = (x: number) => Math.min(1, Math.max(0, x));
const polar = ([x, y]: number[]) => [Math.atan2(y - O[1], x - O[0]), Math.hypot(x - O[0], y - O[1])];

// One loop step (each object goes all the way round, back to its own slot): the logo holds still, the conductor sets off and the
// orchestra follows, all three gather and melt into one ball that keeps turning for a good while, then spread
// out into the next slots, where the logo is exactly itself again.
const PERIOD = 6300; // ms per step
const HOLD = 0.18; // part of the step the logo rests
const LAG = 0; // how much later the orchestra sets off than the conductor (0: all gather at once)
const IN = 0.18, OUT = 0.2; // parts of the move spent gathering / spreading; the rest is the merged spin
const TURNS = 1; // extra turns while merged
const GATHER = 0.75; // how far into the centre (1 = all the way); short of it, so the merged core keeps its three lobes
const SWELL = 0.06; // the balls grow a little while merged
// while merged the balls don't sit still: they drift out and back in a few times, each a little out of step with
// the others (its phase follows where it started), so the core squirms rather than spins as a lump
const WIGGLES = 3; // out-and-back drifts while merged
const WIGGLE = 0.6; // how far out each drift goes (1 = all the way back to its slot)
const PULSES = 4; // and each ball swells and shrinks this many times while merged, on its own beat
const PULSE = 0.45; // by up to this share of its size
const CONDUCTOR_PULSE = 0.8; // the conductor swells more

function ball(a: number[], b: number[], u: number, pulse = PULSE): [number, number, number] {
  const [a0, d0] = polar(a), [a1, d1] = polar(b);
  let da = (a1 - a0) % TAU;
  if (da <= 0) da += TAU; // always clockwise (y points down)
  // all turn together at an even speed, setting off when nearly gathered and stopping just after they start to spread
  const ang = a0 + (da + TURNS * TAU) * clamp01((u - 0.7 * IN) / (1 - 0.7 * (IN + OUT)));
  let pull = u < IN ? ease(u / IN) : u > 1 - OUT ? 1 - ease((u - (1 - OUT)) / OUT) : 1;
  if (u >= IN && u <= 1 - OUT) {
    const m = (u - IN) / (1 - IN - OUT); // 0 → 1 through the merged part; the envelope keeps both ends at rest
    pull -= WIGGLE * Math.sin(Math.PI * m) * (1 - Math.cos(TAU * WIGGLES * m + a0)) / 2;
  }
  const d = (d0 + (d1 - d0) * u) * (1 - GATHER * pull);
  let r = (a[2] + (b[2] - a[2]) * u) * (1 + SWELL * pull);
  if (u >= IN && u <= 1 - OUT) {
    const m = (u - IN) / (1 - IN - OUT);
    // its own size beat: a different rhythm per ball (from where it started, and which of the pair it is)
    r *= 1 + pulse * Math.sin(Math.PI * m) * Math.sin(TAU * PULSES * m + a0 * 2.3 + d0);
  }
  return [O[0] + d * Math.cos(ang), O[1] + d * Math.sin(ang), r];
}

function pose(t: number) {
  const f = (t / PERIOD) % 1;
  // each object turns all the way round back into its own slot, so its colour stays where it was
  return [0, 1, 2].map((slot) => {
    const u = clamp01((f - HOLD - (slot === CONDUCTOR ? 0 : LAG)) / (1 - HOLD - LAG));
    const from = LOGO_SLOTS[slot], to = from;
    const pulse = slot === CONDUCTOR ? CONDUCTOR_PULSE : PULSE;
    return [ball(from[0], to[0], u, pulse), ball(from[1], to[1], u, pulse)] as const;
  });
}

/** The logo's shapes, drawn from the objects' balls (a neck between each pair, melted by the filter). */
function Shapes({ objs, colored }: { objs: ReadonlyArray<readonly (readonly number[])[]>; colored?: string }) {
  return (
    <>
      {objs.map(([[x0, y0, r0], [x1, y1, r1]], i) => {
        const [k0, k1] = colored ? BALL_COLORS[i] : ["currentColor", "currentColor"];
        return (
          // colored: each object adds its light onto the others where they overlap
          <g key={i} style={colored ? { mixBlendMode: "plus-lighter" } : undefined}>
            <line x1={x0} y1={y0} x2={x1} y2={y1} stroke={k0} strokeWidth={Math.min(r0, r1) * 0.7} strokeLinecap="round" />
            <circle cx={x0} cy={y0} r={r0} fill={colored ? `url(#${colored}-${i}-0)` : k0} />
            <circle cx={x1} cy={y1} r={r1} fill={colored ? `url(#${colored}-${i}-1)` : k1} />
          </g>
        );
      })}
    </>
  );
}

// the five balls' colours for the moving logo (the conductor's pair shares one spot, so one colour)
const BALL_COLORS = [["#ff7a9c", "#ffbe6a"], ["#6fd8ff", "#8492ff"], ["#86ffb5", "#86ffb5"]];

let filterSeq = 0;

/**
 * How far into the coloured light the logo is (0 → 1): plain at rest, fading into colour as it gathers
 * and back to plain as it spreads out at the end.
 */
function lit(t: number) {
  const u = clamp01(((t / PERIOD) % 1 - HOLD) / (1 - HOLD - LAG));
  // in while it gathers, out while it spreads back to the slots
  return Math.min(clamp01((u - 0.2 * IN) / (0.8 * IN)), clamp01((1 - u - 0.2 * OUT) / (0.8 * OUT)));
}

/**
 * Draws a pose into a moving logo (only attributes change; React doesn't render): its plain and coloured copies
 * get the same balls and cross-fade by `mix`.
 */
function paint(el: SVGGElement, objs: ReturnType<typeof pose>, mix: number) {
  Array.from(el.children).forEach((copy, n) => {
    // the plain copy stays solid under the coloured one until that one is fully in, so the cross-fade never thins out
    copy.setAttribute("opacity", `${n ? mix : mix < 1 ? 1 : 0}`);
    paintShapes(copy, objs);
  });
}
function paintShapes(el: Element, objs: ReturnType<typeof pose>) {
  objs.forEach(([[x0, y0, r0], [x1, y1, r1]], i) => {
    const part = el.children[i];
    if (!part) return;
    const [line, c0, c1] = Array.from(part.children);
    line.setAttribute("x1", `${x0}`); line.setAttribute("y1", `${y0}`); line.setAttribute("x2", `${x1}`); line.setAttribute("y2", `${y1}`);
    line.setAttribute("stroke-width", `${Math.min(r0, r1) * 0.7}`);
    c0.setAttribute("cx", `${x0}`); c0.setAttribute("cy", `${y0}`); c0.setAttribute("r", `${r0}`);
    c1.setAttribute("cx", `${x1}`); c1.setAttribute("cy", `${y1}`); c1.setAttribute("r", `${r1}`);
  });
}

/**
 * Every moving logo on screen, driven by one shared clock: each one joins when it's put on the page and leaves
 * when it's taken off (a ref callback, so it doesn't depend on when effects run), and they all move in step.
 * The loop runs only while there's at least one.
 */
const moving = new Set<SVGGElement>();
let raf = 0;
function frame(now: number) {
  // only Maestro's own motion switch stops it (the OS "reduce motion" would also hide the loading cue itself)
  const on = useSettings.getState().motion;
  const objs = on ? pose(now) : LOGO_SLOTS.map((o) => o);
  // the colour light is the chrome theme's; every other theme keeps the logo one colour while it moves
  const mix = on && logoLit() ? lit(now) : 0;
  moving.forEach((el) => (el.isConnected ? paint(el, objs, mix) : moving.delete(el)));
  raf = moving.size ? requestAnimationFrame(frame) : 0;
}
function track(el: SVGGElement | null, prev: { current: SVGGElement | null }) {
  if (prev.current) moving.delete(prev.current);
  prev.current = el;
  if (!el) return;
  moving.add(el);
  if (!raf) raf = requestAnimationFrame(frame);
}

/**
 * The logo, metaball-style: the shapes are blurred together, then cut back to a hard edge by an alpha
 * threshold, which rounds every join into a soft bridge. Static by default; `moving` plays the loading loop
 * (still when motion is turned off in settings). Drawn in the text colour, so it follows the theme.
 */
export function LogoMotion({ height = 24, moving: animate = false, className }: { height?: number; moving?: boolean; className?: string }) {
  // several logos on one page each need their own filter
  const idRef = useRef("");
  if (!idRef.current) idRef.current = `logo-goo-${++filterSeq}`;
  const joined = useRef<SVGGElement | null>(null);
  useEffect(() => () => void (joined.current && moving.delete(joined.current)), []);

  return (
    // square: the mark (x 2-46, y 8-35) centred in a 56 x 56 box with some air around it
    <svg className={className ?? "logo"} width={height} height={height} viewBox="-4 -6.5 56 56" aria-hidden>
      <defs>
        <filter id={idRef.current} x="-15%" y="-15%" width="130%" height="130%">
          <feGaussianBlur in="SourceGraphic" stdDeviation="1.6" />
          <feColorMatrix values="1 0 0 0 0  0 1 0 0 0  0 0 1 0 0  0 0 0 24 -11" />
        </filter>
        {animate && (
          <filter id={`${idRef.current}-light`} x="-40%" y="-40%" width="180%" height="180%">
            <feGaussianBlur in="SourceGraphic" stdDeviation="1.6" />
            <feColorMatrix values="1 0 0 0 0  0 1 0 0 0  0 0 1 0 0  0 0 0 24 -11" result="shape" />
            {/* a soft halo of the shape's own colours behind it, so the coloured balls read as light */}
            <>
              <feGaussianBlur in="shape" stdDeviation="1.6" />
              <feComponentTransfer result="glow">
                <feFuncA type="linear" slope="0.45" />
              </feComponentTransfer>
              {/* white towards the logo's centre, laid only over the shape */}
              <feImage href={`#${idRef.current}-white`} />
              <feComposite in2="shape" operator="in" result="white" />
              <feMerge>
                <feMergeNode in="glow" />
                <feMergeNode in="shape" />
                <feMergeNode in="white" />
              </feMerge>
            </>
          </filter>
        )}
        {animate && (
          <radialGradient id={`${idRef.current}-fade`} gradientUnits="userSpaceOnUse" cx={O[0]} cy={O[1]} r="24">
            <stop offset="0" stopColor="#fff" stopOpacity="0.85" />
            <stop offset="1" stopColor="#fff" stopOpacity="0" />
          </radialGradient>
        )}
        {animate &&
          // each ball whitens towards its own centre
          BALL_COLORS.flatMap((pair, i) =>
            pair.map((c, k) => (
              <radialGradient key={`${i}-${k}`} id={`${idRef.current}-${i}-${k}`}>
                <stop offset="0" stopColor="#fff" />
                <stop offset="0.7" stopColor={c} />
              </radialGradient>
            )),
          )}
        {animate && <rect id={`${idRef.current}-white`} x="-4" y="-6.5" width="56" height="56" fill={`url(#${idRef.current}-fade)`} />}
      </defs>
      {animate ? (
        // plain at rest; the coloured light copy fades in only while it turns
        <g ref={(el) => track(el, joined)}>
          <g filter={`url(#${idRef.current})`} fill="currentColor">
            <Shapes objs={LOGO_SLOTS} />
          </g>
          <g filter={`url(#${idRef.current}-light)`} opacity={0}>
            <Shapes objs={LOGO_SLOTS} colored={idRef.current} />
          </g>
        </g>
      ) : (
        <g filter={`url(#${idRef.current})`} fill="currentColor">
          <Shapes objs={LOGO_SLOTS} />
        </g>
      )}
    </svg>
  );
}

/** The everyday loading mark (beside "..." text, in buttons): a plain ring at text size. The logo is kept for big loads. */
export function Spinner() {
  return <span className="spinner" role="status" aria-label="불러오는 중" />;
}

/** A big loading screen (the whole stage, like a canvas opening): the moving logo, large, above a line of text. */
export function LogoLoading({ text }: { text: string }) {
  return (
    <div className="empty logo-loading">
      <LogoMotion height={112} moving className="logo logo-big" />
      <p className="muted">{text}</p>
    </div>
  );
}
