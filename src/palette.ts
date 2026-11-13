// Pure (no React, zustand or DOM): a custom palette (내 팔레트) as three choices, and every colour token the app reads
// worked out from them. Tested by scripts/palette-check.ts. docs/features/ui/themes-and-motion.md "내 팔레트".
//
// The three choices, as in Linear's custom theme: a 바탕 colour (the canvas; its lightness decides dark or light), an
// 강조 colour (the primary action, selection, focus) and 대비 (how far apart the surface layers sit). Surfaces are
// stepped in OKLCH lightness from the base, keeping its hue with a little chroma at most, so the chrome stays near
// neutral; the text roles are worked out from the surfaces so muted text still reads at 4.5:1 on the lightest panel.

export type CustomTheme = {
  /** stable id; the theme setting holds `custom:<id>` */
  id: string;
  name: string;
  /** #rrggbb */
  base: string;
  /** #rrggbb */
  accent: string;
  /** 0–100: how far apart the surface layers sit */
  contrast: number;
};

export type PaletteSeed = Pick<CustomTheme, "base" | "accent" | "contrast">;

/** The tokens a custom palette sets on <html> (inline), and removes again when another theme is picked. */
export const PALETTE_KEYS = [
  "--mae-frame-bg", "--mae-bg", "--mae-canvas", "--mae-panel", "--mae-node", "--mae-panel-2", "--mae-hover",
  "--mae-selected", "--mae-text", "--mae-text-2", "--mae-muted", "--mae-accent", "--mae-on-accent",
  "--mae-status-running", "--mae-status-terminated",
] as const;

export const CUSTOM_PREFIX = "custom:";
export const DEFAULT_CONTRAST = 60;

/** Each built-in theme as a starting point (its canvas, its accent, about how far apart its layers sit). */
export const BUILTIN_SEEDS: Record<string, PaletteSeed> = {
  default: { base: "#111112", accent: "#f5f5f5", contrast: 60 },
  dark: { base: "#0d1017", accent: "#3d6fe0", contrast: 45 },
  graphite: { base: "#19191b", accent: "#f4f4f4", contrast: 75 },
  mocha: { base: "#12110f", accent: "#e0955a", contrast: 40 },
  light: { base: "#ebeef2", accent: "#2459d6", contrast: 45 },
  ivory: { base: "#eee8dd", accent: "#9a4b1b", contrast: 45 },
};

// ---- colour maths: sRGB hex ⇄ OKLab, WCAG contrast

type Rgb = [number, number, number];
const HEX = /^#[0-9a-f]{6}$/i;
export const isHex = (s: unknown): s is string => typeof s === "string" && HEX.test(s);

const toLin = (c: number) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
const fromLin = (c: number) => (c <= 0.0031308 ? 12.92 * c : 1.055 * c ** (1 / 2.4) - 0.055);
const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

function hexToLin(hex: string): Rgb {
  return [1, 3, 5].map((i) => toLin(parseInt(hex.slice(i, i + 2), 16) / 255)) as Rgb;
}
function linToHex([r, g, b]: Rgb): string {
  return "#" + [r, g, b].map((c) => Math.round(clamp(fromLin(clamp(c, 0, 1)), 0, 1) * 255).toString(16).padStart(2, "0")).join("");
}

type Lch = { L: number; C: number; h: number };
function hexToLch(hex: string): Lch {
  const [r, g, b] = hexToLin(hex);
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  const L = 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s;
  const A = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s;
  const B = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s;
  return { L, C: Math.hypot(A, B), h: Math.atan2(B, A) };
}
function lchToHex({ L, C, h }: Lch): string {
  const A = C * Math.cos(h), B = C * Math.sin(h);
  const l = (L + 0.3963377774 * A + 0.2158037573 * B) ** 3;
  const m = (L - 0.1055613458 * A - 0.0638541728 * B) ** 3;
  const s = (L - 0.0894841775 * A - 1.291485548 * B) ** 3;
  return linToHex([
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ]);
}

/** WCAG relative luminance of a #rrggbb colour. */
export function luminance(hex: string): number {
  const [r, g, b] = hexToLin(hex);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}
/** WCAG contrast ratio of two #rrggbb colours (1–21). */
export function contrast(a: string, b: string): number {
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}

// ---- the palette

export type Palette = { light: boolean; vars: Record<(typeof PALETTE_KEYS)[number], string> };

/** Dark or light: decided by the base's lightness alone. */
export const isLightSeed = (p: PaletteSeed) => hexToLch(isHex(p.base) ? p.base : "#111112").L > 0.6;

/**
 * Every token of a custom palette. Surfaces step from the base by `k` in OKLCH lightness (`contrast` 0–100 → about
 * 0.012–0.052); a dark palette's layers grow lighter above the canvas, a light palette's grow darker below it.
 */
export function derivePalette(seed: PaletteSeed): Palette {
  const base = isHex(seed.base) ? seed.base : "#111112";
  const accent = isHex(seed.accent) ? seed.accent : "#f5f5f5";
  const { L: L0, C: C0, h } = hexToLch(base);
  const light = L0 > 0.6;
  const k = 0.012 + (clamp(Number(seed.contrast) || 0, 0, 100) / 100) * 0.04;
  // the surfaces keep the base's hue with a little of its colour at most, so the chrome stays near neutral
  const C = Math.min(C0, 0.03);
  const at = (L: number, c = C) => lchToHex({ L: clamp(L, 0.06, 0.985), C: c, h });

  const s = light
    ? { frame: L0 - 2.5 * k, bg: L0 + 1.5 * k, canvas: L0, panel: L0 - k, node: L0 + 1.5 * k, panel2: L0 - 1.8 * k, hover: L0 - 2.4 * k, selected: L0 - 3.2 * k }
    : { frame: L0 - k, bg: L0 - k, canvas: L0, panel: L0 + k, node: L0 + 2 * k, panel2: L0 + 3 * k, hover: L0 + 3.4 * k, selected: L0 + 4.4 * k };
  const panel2 = at(s.panel2);
  const textL = light ? 0.22 : 0.965;
  const text = at(textL, Math.min(C, 0.006));

  // muted: as far toward the surfaces as it can go while still reading at 4.5:1 on the lightest (dark) or darkest
  // (light) panel the text sits on, the input surface; never past the text itself
  let mutedL = light ? clamp(s.panel2 - 0.2, 0, 1) : clamp(s.panel2 + 0.2, 0, 1);
  const step = light ? -0.005 : 0.005;
  while (contrast(at(mutedL, Math.min(C, 0.01)), panel2) < 4.6 && (light ? mutedL > textL : mutedL < textL)) mutedL += step;
  const muted = at(mutedL, Math.min(C, 0.01));
  const text2 = at((textL + mutedL) / 2, Math.min(C, 0.008));
  const terminated = at((s.panel + mutedL) / 2, Math.min(C, 0.01));

  // the text on the accent: white or a near-black from the base's hue, whichever reads better
  const dark = lchToHex({ L: 0.16, C: Math.min(C, 0.01), h });
  const onAccent = contrast("#ffffff", accent) >= contrast(dark, accent) ? "#ffffff" : dark;

  return {
    light,
    vars: {
      "--mae-frame-bg": at(s.frame),
      "--mae-bg": at(s.bg),
      "--mae-canvas": at(s.canvas),
      "--mae-panel": at(s.panel),
      "--mae-node": at(s.node),
      "--mae-panel-2": panel2,
      "--mae-hover": at(s.hover),
      "--mae-selected": at(s.selected),
      "--mae-text": text,
      "--mae-text-2": text2,
      "--mae-muted": muted,
      "--mae-accent": accent,
      "--mae-on-accent": onAccent,
      "--mae-status-running": accent,
      "--mae-status-terminated": terminated,
    },
  };
}

// ---- sharing: a palette as one line of text

const SHARE_PREFIX = "maestro-palette:";

/** A palette as one line (`maestro-palette:` + base64 of its JSON) to paste on another computer. */
export function encodePalette(t: Pick<CustomTheme, "name" | "base" | "accent" | "contrast">): string {
  const bytes = new TextEncoder().encode(JSON.stringify({ v: 1, n: t.name, b: t.base, a: t.accent, c: t.contrast }));
  let bin = "";
  bytes.forEach((x) => (bin += String.fromCharCode(x)));
  return SHARE_PREFIX + btoa(bin);
}

/** The palette in a shared line, or null when it isn't one (a wrong prefix, broken text, a bad colour). */
export function decodePalette(text: string): Pick<CustomTheme, "name" | "base" | "accent" | "contrast"> | null {
  const s = text.trim();
  if (!s.startsWith(SHARE_PREFIX)) return null;
  try {
    const bin = atob(s.slice(SHARE_PREFIX.length));
    const j = JSON.parse(new TextDecoder().decode(Uint8Array.from(bin, (ch) => ch.charCodeAt(0))));
    if (!isHex(j.b) || !isHex(j.a) || typeof j.c !== "number") return null;
    const name = typeof j.n === "string" && j.n.trim() ? j.n.trim().slice(0, 40) : "가져온 팔레트";
    return { name, base: j.b.toLowerCase(), accent: j.a.toLowerCase(), contrast: Math.round(clamp(j.c, 0, 100)) };
  } catch {
    return null;
  }
}

/** A name not yet taken: "내 팔레트", then "내 팔레트 2", "내 팔레트 3", … */
export function freeName(taken: string[], stem = "내 팔레트"): string {
  if (!taken.includes(stem)) return stem;
  let n = 2;
  while (taken.includes(`${stem} ${n}`)) n++;
  return `${stem} ${n}`;
}
