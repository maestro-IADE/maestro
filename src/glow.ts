import { logoLit } from "./settings";

// The website's hover light (maestro-web/DESIGN.md, "호버: 로고 색 패치"): under the pointer, a soft patch of the logo's
// colour, which of the five depending on where along the element the pointer is, so its whole width runs through all
// five. Default theme only; styles.css draws it from --gx, --gy and --glow. One listener for the whole window.

// the logo's five ball colours, left to right as they sit in the mark
const HUES = ["#ffbe6a", "#ff7a9c", "#86ffb5", "#6fd8ff", "#8492ff"].map((h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16)));
export const hueAt = (f: number) => {
  const p = Math.min(0.9999, Math.max(0, f)) * (HUES.length - 1), i = Math.floor(p), t = p - i;
  return `rgb(${HUES[i].map((v, k) => Math.round(v + (HUES[i + 1][k] - v) * t)).join(",")})`;
};

// what lights up: the primary action wherever it is, and the sidebar's main rows
const LIT = "button.primary, .sb-nav";

window.addEventListener(
  "pointermove",
  (e) => {
    if (!logoLit()) return;
    const el = (e.target as Element | null)?.closest?.(LIT) as HTMLElement | null;
    if (!el) return;
    const r = el.getBoundingClientRect();
    const x = e.clientX - r.left, y = e.clientY - r.top;
    el.style.setProperty("--gx", `${x}px`);
    el.style.setProperty("--gy", `${y}px`);
    el.style.setProperty("--glow", hueAt(x / r.width));
  },
  { passive: true },
);
