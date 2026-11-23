import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { AgentView } from "./types";
import type { OsNotify } from "./osNotify";
import { migrateSettings, SETTINGS_VERSION } from "./settingsMigrate";
import { CUSTOM_PREFIX, derivePalette, inkOn, invertRamp, isHex, luminance, PALETTE_KEYS, RAMP_KEYS, type CustomTheme } from "./palette";

// light: the canvas (React Flow) and xterm use their light palette
// 그래파이트 first: the default (a new install opens in it). 블랙 keeps the id "default": it was the default, named
// 기본, until 2026-11-18, and saved settings name it by that id. It is the website's look (maestro-web/DESIGN.md) and
// the only theme where the logo lights up in colour
export const THEMES = [
  { id: "graphite", label: "그래파이트", light: false },
  { id: "default", label: "블랙", light: false },
  { id: "dark", label: "다크 블루", light: false },
  { id: "mocha", label: "모카", light: false },
  { id: "light", label: "라이트", light: true },
  { id: "ivory", label: "아이보리", light: true },
  // Astryx's Stone (meta/astryx, MIT) in its light mode: violet-tinted grey-whites and a near-black ink accent
  { id: "stone", label: "스톤", light: true },
] as const;

/** The custom palette a theme id names (`custom:<id>`), if it still exists. */
export const customTheme = (id: string, list = useSettings.getState().customThemes): CustomTheme | undefined =>
  id.startsWith(CUSTOM_PREFIX) ? (list ?? []).find((c) => CUSTOM_PREFIX + c.id === id) : undefined;

/** Light themes use the light canvas, terminal and editor palettes; a custom palette is light when its base is. */
export const isLightTheme = (id: string) => {
  const c = customTheme(id);
  return c ? derivePalette(c).light : THEMES.some((t) => t.id === id && t.light);
};
export const DEFAULT_THEME = "graphite";
/** Only 블랙 (id "default") shows the logo's five colours; every other theme keeps the logo one colour. */
export const logoLit = () => (document.documentElement.dataset.theme ?? DEFAULT_THEME) === "default";

// One feel for every UI motion: panel collapse, minimap grow, dialogs, canvas fly-to.
export const EASINGS = {
  standard: { label: "표준", curve: "cubic-bezier(.2, 0, 0, 1)" },
  smooth: { label: "부드럽게", curve: "cubic-bezier(.16, 1, .3, 1)" },
  bouncy: { label: "탄력", curve: "cubic-bezier(.34, 1.56, .64, 1)" },
  linear: { label: "일정하게", curve: "linear" },
} as const;

// How the sidebar, stage and right panel sit inside the window frame (ids kept from the first two-option version).
export const PANEL_STYLES = {
  cards: { label: "분리" },
  lines: { label: "통합" },
  full: { label: "와이드" },
} as const;

// The pattern behind every canvas (React Flow's Background variants; "none" = a plain surface).
export const CANVAS_BGS = {
  dots: { label: "점" },
  lines: { label: "격자" },
  cross: { label: "십자" },
  none: { label: "없음" },
} as const;

// The shape of every connection line (edgePath.ts): lines that bend only at right angles (the default), one straight
// line from end to end, or a curve.
export const EDGE_SHAPES = {
  step: { label: "꺾은선" },
  straight: { label: "직선" },
  curve: { label: "곡선" },
} as const;
export type EdgeShape = keyof typeof EDGE_SHAPES;
/**
 * How what the agent puts on the canvas comes in (canvas-controls.md "그림 넘기기"): on its own, a caption's time
 * apart, or one step at a time when the user presses 다음 on the caption (or 캔버스에 올리기 in the chat).
 */
export const GUIDE_MODES = {
  auto: { label: "바로", hint: "그리는 대로 바로 보여 줘요" },
  manual: { label: "눌러서", hint: "누를 때마다 하나씩 보여 줘요" },
} as const;
export type GuideMode = keyof typeof GUIDE_MODES;
/** how long a caption stays on the canvas, and so how long before the next step comes in on its own (seconds) */
export const GUIDE_SECONDS = [2, 4, 6, 10] as const;
/** when a line's label shows on the canvas (relations.md): all the time, or only on the line picked */
export const LINE_LABELS = {
  always: { label: "항상" },
  selected: { label: "선택할 때" },
} as const;
export type LineLabels = keyof typeof LINE_LABELS;

// spacing of the pattern in px; 24 is the snap grid, the others its halves and multiples so lines still meet blocks
export const CANVAS_GAPS = [
  { px: 12, label: "촘촘" },
  { px: 24, label: "보통" },
  { px: 48, label: "넓게" },
  { px: 96, label: "아주 넓게" },
] as const;

// 채팅 (설정 → 채팅): how a conversation reads, everywhere it shows. Each a few steps, no free values.
export const CHAT_FONTS = {
  default: { label: "기본", stack: "var(--mae-font)" },
  system: { label: "시스템", stack: '"Segoe UI", "Malgun Gothic", "Apple SD Gothic Neo", system-ui, sans-serif' },
  serif: { label: "명조", stack: '"Noto Serif KR", "Nanum Myeongjo", "AppleMyungjo", Batang, Georgia, serif' },
  mono: { label: "고정폭", stack: "var(--mae-mono)" },
} as const;
/** px: `full` on the full screen, `block` in canvas blocks and the bar under a canvas */
export const CHAT_SIZES = {
  sm: { label: "작게", full: 13, block: 12 },
  md: { label: "보통", full: 14, block: 13 },
  lg: { label: "크게", full: 16, block: 14 },
} as const;
export const CHAT_LEADINGS = {
  tight: { label: "좁게", v: 1.6 },
  normal: { label: "보통", v: 1.75 },
  loose: { label: "넓게", v: 1.95 },
} as const;
/** the full screen's reading column */
export const CHAT_WIDTHS = {
  narrow: { label: "좁게", px: 680 },
  normal: { label: "보통", px: 820 },
  wide: { label: "넓게", px: 1040 },
} as const;
/** how wide your own message's bubble may grow, of the column */
export const BUBBLE_WIDTHS = {
  narrow: { label: "좁게", pct: 65 },
  normal: { label: "보통", pct: 85 },
  wide: { label: "넓게", pct: 100 },
} as const;

export const SPEEDS = {
  fast: { label: "빠르게", ms: 150 },
  normal: { label: "보통", ms: 240 },
  slow: { label: "느리게", ms: 360 },
} as const;

type Prefs = {
  /** a built-in theme's id (THEMES), or `custom:<id>` for one of `customThemes` */
  theme: string;
  /** 내 팔레트: the user's own palettes, each three choices the tokens are worked out from (palette.ts) */
  customThemes: CustomTheme[];
  easing: keyof typeof EASINGS;
  speed: keyof typeof SPEEDS;
  motion: boolean;
  /** 색상 반전 (설정 → 모양): the theme's surfaces in the opposite order (palette.ts `invertRamp`) */
  invert: boolean;
  /**
   * The colour of your own message's bubble, by theme (its id, a palette's `custom:<id>`): #rrggbb. A theme that
   * isn't here has its own (자동: the selected surface). Each theme keeps the one it was given.
   */
  bubbleColors: Record<string, string>;
  panelStyle: keyof typeof PANEL_STYLES;
  canvasBg: keyof typeof CANVAS_BGS;
  /** the shape of every connection line, on the canvases and the project map */
  edgeShape: EdgeShape;
  /** when a line's label shows (LINE_LABELS) */
  lineLabels: LineLabels;
  /** the view goes to what the agent put on the canvas (canvas-controls.md "화면이 따라가기"); off: it stays, a notice leads there */
  canvasFollow: boolean;
  /** how the agent's drawings come in (GUIDE_MODES), and how long a caption stays (one of GUIDE_SECONDS) */
  guideMode: GuideMode;
  guideSeconds: number;
  /** the canvases' tool strip folded to its one toggle button (the project map's too) */
  toolsShut: boolean;
  /** pattern spacing (one of CANVAS_GAPS) and how strongly it shows, 10–100 % */
  canvasGap: number;
  canvasAlpha: number;
  defaultAgent: string;
  /** permission mode new worktrees start with */
  defaultPermission: string;
  /** installed but switched off: not offered when creating a worktree */
  disabledAgents: string[];
  /** default --model per agent for new worktrees; missing = the CLI's own default */
  agentModels: Record<string, string>;
  /** the assistant model for light background work (project-canvas summaries): an agent CLI + model, or "off" */
  assistantAgent: string;
  /** "" = that agent's cheap default (Haiku for Claude) */
  assistantModel: string;
  /**
   * What a new tab of an agent opens on: full screen (default) or its canvas. The Maestro has its own
   * (`maestroLayout`). The tab bar's toggle changes one tab only and never writes these.
   */
  agentLayout: "full" | "canvas";
  /** what a new tab of a project's Maestro opens on: its canvas (default), where its workers are, or full screen */
  maestroLayout: "full" | "canvas";
  /**
   * 에이전트 보기: chat or terminal, for every place an agent shows (full screen, canvas blocks, pop-out windows).
   * One agent can differ for a while (the store's `viewOverride`, memory only). A change shows on the views that
   * open next; a conversation already running stays where it is (the terminal offers to take it over).
   */
  agentView: AgentView;
  /** stop idle agents of a project left this many minutes ago (they resume on return); 0 = never, the default */
  awayMinutes: number;
  /** the code editor saves a second after typing stops */
  autoSave: boolean;
  /** sidebar session rows: two lines (44px: name, then branch · PR or what waits) or one compact line (28px) */
  sidebarDensity: "two" | "one";
  /** the sidebar lists worktrees with no Maestro agent (made outside the app: other tools, other sessions); off by default */
  showBareWorktrees: boolean;
  /** OS notifications while no Maestro window has focus (osNotify.ts): waits and finished turns, waits only, or none */
  osNotify: OsNotify;
  /** 채팅: font, text size, line spacing, the full screen's column and your bubble's width (CHAT_* above) */
  chatFont: keyof typeof CHAT_FONTS;
  chatSize: keyof typeof CHAT_SIZES;
  chatLeading: keyof typeof CHAT_LEADINGS;
  chatWidth: keyof typeof CHAT_WIDTHS;
  chatBubble: keyof typeof BUBBLE_WIDTHS;
};

export const AWAY_CHOICES = [
  { minutes: 5, label: "5분" },
  { minutes: 15, label: "15분" },
  { minutes: 60, label: "1시간" },
  { minutes: 0, label: "끄지 않음" },
] as const;

export const useSettings = create<Prefs & { set: (p: Partial<Prefs>) => void }>()(
  persist((set) => ({ theme: DEFAULT_THEME, customThemes: [], easing: "standard", speed: "normal", motion: true, invert: false, bubbleColors: {}, panelStyle: "full", canvasBg: "lines", edgeShape: "step", lineLabels: "always", canvasFollow: true, guideMode: "auto", guideSeconds: 4, toolsShut: false, canvasGap: 24, canvasAlpha: 100, defaultAgent: "auto", defaultPermission: "acceptEdits", disabledAgents: [], agentModels: {}, assistantAgent: "claude-code", assistantModel: "", agentLayout: "full", maestroLayout: "canvas", agentView: "chat", awayMinutes: 0, autoSave: false, sidebarDensity: "two", showBareWorktrees: false, osNotify: "all", chatFont: "default", chatSize: "md", chatLeading: "normal", chatWidth: "normal", chatBubble: "normal", set }), {
    name: "maestro.settings",
    // v1 the default theme for everyone once, v2 its id "chrome" → "default", v3 the full screen's view → agentView,
    // v4 and v5 the line shapes (settingsMigrate.ts, each step in turn)
    version: SETTINGS_VERSION,
    migrate: (old, from) => migrateSettings(old, from) as unknown as Prefs,
  }),
);

/** Motion duration in ms; 0 when the user or the OS turned motion off. */
export const motionMs = (s: Prefs) =>
  s.motion && !matchMedia("(prefers-reduced-motion: reduce)").matches ? SPEEDS[s.speed].ms : 0;

/**
 * Put a theme on <html>: a built-in one by its data-theme, a custom palette as `custom` / `custom-light` with its
 * tokens inline (removed again for any other theme). A theme removed since it was saved (e.g. forest, or a deleted
 * palette) falls back to the default. `theme` may be a palette not saved yet (the editor's live look).
 */
/** A theme's key in `bubbleColors`: a built-in's id, a palette's `custom:<id>` (also while it is being edited). */
export const themeKey = (theme: string | CustomTheme) => (typeof theme === "string" ? theme : CUSTOM_PREFIX + theme.id);

/**
 * Your own bubble's colour for the theme just applied: the one given to it (`--mae-bubble`), or none, and the
 * stylesheet's own applies. The text in it takes the ink that reads on that colour (`--mae-bubble-text`: dark on a
 * light colour, light on a dark one). A bubble darker than the block it sits on gets a hairline, or its edge is lost.
 */
function applyBubble(color: string | undefined) {
  const root = document.documentElement;
  if (!isHex(color)) {
    for (const k of ["--mae-bubble", "--mae-bubble-text", "--mae-bubble-line"]) root.style.removeProperty(k);
    return;
  }
  root.style.setProperty("--mae-bubble", color);
  root.style.setProperty("--mae-bubble-text", inkOn(color));
  const node = getComputedStyle(root).getPropertyValue("--mae-node").trim();
  if (isHex(node) && luminance(color) < luminance(node)) root.style.setProperty("--mae-bubble-line", "var(--mae-border)");
  else root.style.removeProperty("--mae-bubble-line");
}

function applyTheme(theme: string | CustomTheme, list: CustomTheme[], invert: boolean) {
  const root = document.documentElement;
  const c = typeof theme === "string" ? customTheme(theme, list) : theme;
  if (c) {
    const p = derivePalette(c);
    const vars = invert ? invertRamp(p.vars) : p.vars;
    root.dataset.theme = p.light ? "custom-light" : "custom";
    for (const k of PALETTE_KEYS) root.style.setProperty(k, vars[k]);
    return;
  }
  for (const k of PALETTE_KEYS) root.style.removeProperty(k);
  root.dataset.theme = THEMES.some((t) => t.id === theme) ? (theme as string) : DEFAULT_THEME;
  if (!invert) return;
  // a built-in theme's surfaces are in the stylesheet: read as they come out for this theme, then put back in the
  // opposite order (inline, like a palette's; cleared above whenever the theme is applied again)
  const now = getComputedStyle(root);
  const vars = Object.fromEntries(RAMP_KEYS.map((k) => [k, now.getPropertyValue(k).trim()])) as Record<(typeof RAMP_KEYS)[number], string>;
  if (RAMP_KEYS.some((k) => !vars[k])) return;
  const turned = invertRamp(vars);
  for (const k of RAMP_KEYS) root.style.setProperty(k, turned[k]);
}

// a look shown without saving it (a theme card under the pointer, the palette being edited); null = the saved one
let preview: string | CustomTheme | null = null;
/** Show a theme on the whole app for a moment without saving it; `null` goes back to the saved theme. */
export function previewTheme(theme: string | CustomTheme | null) {
  preview = theme;
  const s = useSettings.getState();
  applyTheme(theme ?? s.theme, s.customThemes ?? [], !!s.invert);
  applyBubble(s.bubbleColors?.[themeKey(theme ?? s.theme)]);
}

/** Push theme + motion onto <html> so CSS tokens pick them up. */
function applySettings(s: Prefs) {
  const root = document.documentElement;
  applyTheme(preview ?? s.theme, s.customThemes ?? [], !!s.invert);
  applyBubble(s.bubbleColors?.[themeKey(preview ?? s.theme)]);
  root.dataset.panels = s.panelStyle in PANEL_STYLES ? s.panelStyle : "full";
  root.style.setProperty("--mae-ease", EASINGS[s.easing].curve);
  root.style.setProperty("--mae-dur", `${motionMs(s)}ms`);
  // 채팅: an unknown value (a setting from a newer build) falls back to the default step
  const size = CHAT_SIZES[s.chatSize] ?? CHAT_SIZES.md;
  root.style.setProperty("--chat-font", (CHAT_FONTS[s.chatFont] ?? CHAT_FONTS.default).stack);
  root.style.setProperty("--chat-fs", `${size.full}px`);
  root.style.setProperty("--chat-fs-block", `${size.block}px`);
  root.style.setProperty("--chat-leading", String((CHAT_LEADINGS[s.chatLeading] ?? CHAT_LEADINGS.normal).v));
  root.style.setProperty("--chat-width", `${(CHAT_WIDTHS[s.chatWidth] ?? CHAT_WIDTHS.normal).px}px`);
  root.style.setProperty("--chat-bubble", `${(BUBBLE_WIDTHS[s.chatBubble] ?? BUBBLE_WIDTHS.normal).pct}%`);
}
applySettings(useSettings.getState()); // before first paint: no flash of the default theme
useSettings.subscribe(applySettings);
// another window (main ↔ pop-out) changed a setting: pick it up here too
window.addEventListener("storage", (e) => e.key === "maestro.settings" && void useSettings.persist.rehydrate());
