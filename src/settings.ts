import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { AgentView } from "./types";
import { migrateSettings, SETTINGS_VERSION } from "./settingsMigrate";
import { CUSTOM_PREFIX, derivePalette, PALETTE_KEYS, type CustomTheme } from "./palette";

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
  cards: { label: "분리", hint: "패널마다 둥글게 띄우고 사이에 틈을 둬요" },
  lines: { label: "통합", hint: "패널을 둥근 상자 하나로 붙이고 선으로만 나눠요" },
  full: { label: "와이드", hint: "여백 없이 창 끝까지 채우고 선으로만 나눠요. 가장 넓어요." },
} as const;

// The pattern behind every canvas (React Flow's Background variants; "none" = a plain surface).
export const CANVAS_BGS = {
  dots: { label: "점", hint: "칸마다 작은 점을 찍어요. 가장 조용해요." },
  lines: { label: "격자", hint: "칸마다 가는 선을 그어요. 블록 정렬이 잘 보여요." },
  cross: { label: "십자", hint: "칸이 만나는 곳에 작은 십자를 그려요" },
  none: { label: "없음", hint: "무늬 없이 단색 바탕만 둬요" },
} as const;

// The shape of every connection line (edgePath.ts): lines that bend only at right angles (the default), one straight
// line from end to end, or a curve.
export const EDGE_SHAPES = {
  step: { label: "꺾은선", hint: "가로·세로로만 꺾어 이어요. 구조도처럼 반듯해요." },
  straight: { label: "직선", hint: "끝에서 끝까지 곧게 이어요. 무엇이 무엇과 이어졌는지 바로 보여요." },
  curve: { label: "곡선", hint: "부드러운 곡선으로 이어요. 블록이 흩어져 있을 때 잘 읽혀요." },
} as const;
export type EdgeShape = keyof typeof EDGE_SHAPES;
/**
 * How what the agent puts on the canvas comes in (canvas-controls.md "그림 넘기기"): on its own, a caption's time
 * apart, or one step at a time when the user presses 다음 on the caption (or 허용 in the chat).
 */
export const GUIDE_MODES = {
  auto: { label: "자동", hint: "에이전트가 캔버스에 그리는 것을 묻지 않고 바로 보여 줘요. 설명이 붙어 있으면 안내 시간만큼 머물렀다가 다음 그림으로 넘어가요." },
  manual: { label: "눌러서", hint: "에이전트가 그린 것이 기다렸다가, 캔버스의 안내나 채팅의 '허용'을 누를 때마다 하나씩 나타나요." },
} as const;
export type GuideMode = keyof typeof GUIDE_MODES;
/** how long a caption stays on the canvas, and so how long before the next step comes in on its own (seconds) */
export const GUIDE_SECONDS = [2, 4, 6, 10] as const;
/** when a line's label shows on the canvas (relations.md): all the time, or only on the line picked */
export const LINE_LABELS = {
  always: { label: "항상", hint: "선마다 라벨을 늘 보여 줘요. 60% 미만으로 줄이면 숨어요." },
  selected: { label: "선택할 때", hint: "선을 눌러 선택했을 때만 라벨을 보여 줘요. 마우스를 올리면 요약이 떠요." },
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
  default: { label: "기본", hint: "앱 글꼴(Geist, 한글은 Pretendard)", stack: "var(--mae-font)" },
  system: { label: "시스템", hint: "운영체제 글꼴(Windows는 맑은 고딕)", stack: '"Segoe UI", "Malgun Gothic", "Apple SD Gothic Neo", system-ui, sans-serif' },
  serif: { label: "명조", hint: "긴 글을 읽기 좋은 명조(없으면 바탕)", stack: '"Noto Serif KR", "Nanum Myeongjo", "AppleMyungjo", Batang, Georgia, serif' },
  mono: { label: "고정폭", hint: "코드 글꼴로 모든 글자 폭이 같아요", stack: "var(--mae-mono)" },
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
  /** stop idle agents of a project left this many minutes ago (they resume on return); 0 = never */
  awayMinutes: number;
  /** the code editor saves a second after typing stops */
  autoSave: boolean;
  /** sidebar session rows: two lines (44px: name, then branch · PR or what waits) or one compact line (28px) */
  sidebarDensity: "two" | "one";
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
  persist((set) => ({ theme: DEFAULT_THEME, customThemes: [], easing: "standard", speed: "normal", motion: true, panelStyle: "cards", canvasBg: "lines", edgeShape: "step", lineLabels: "always", canvasFollow: true, guideMode: "auto", guideSeconds: 4, toolsShut: false, canvasGap: 24, canvasAlpha: 100, defaultAgent: "auto", defaultPermission: "acceptEdits", disabledAgents: [], agentModels: {}, assistantAgent: "claude-code", assistantModel: "", agentLayout: "full", maestroLayout: "canvas", agentView: "chat", awayMinutes: 15, autoSave: false, sidebarDensity: "two", chatFont: "default", chatSize: "md", chatLeading: "normal", chatWidth: "normal", chatBubble: "normal", set }), {
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
function applyTheme(theme: string | CustomTheme, list: CustomTheme[]) {
  const root = document.documentElement;
  const c = typeof theme === "string" ? customTheme(theme, list) : theme;
  if (c) {
    const p = derivePalette(c);
    root.dataset.theme = p.light ? "custom-light" : "custom";
    for (const k of PALETTE_KEYS) root.style.setProperty(k, p.vars[k]);
    return;
  }
  for (const k of PALETTE_KEYS) root.style.removeProperty(k);
  root.dataset.theme = THEMES.some((t) => t.id === theme) ? (theme as string) : DEFAULT_THEME;
}

// a look shown without saving it (a theme card under the pointer, the palette being edited); null = the saved one
let preview: string | CustomTheme | null = null;
/** Show a theme on the whole app for a moment without saving it; `null` goes back to the saved theme. */
export function previewTheme(theme: string | CustomTheme | null) {
  preview = theme;
  const s = useSettings.getState();
  applyTheme(theme ?? s.theme, s.customThemes ?? []);
}

/** Push theme + motion onto <html> so CSS tokens pick them up. */
function applySettings(s: Prefs) {
  const root = document.documentElement;
  applyTheme(preview ?? s.theme, s.customThemes ?? []);
  root.dataset.panels = s.panelStyle in PANEL_STYLES ? s.panelStyle : "cards";
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
