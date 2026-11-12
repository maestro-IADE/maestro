import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { AgentView } from "./types";
import { migrateSettings, SETTINGS_VERSION } from "./settingsMigrate";

// light: the canvas (React Flow) and xterm use their light palette
// "기본" first: the default, the website's look (maestro-web/DESIGN.md), the only one where the logo lights up in colour
export const THEMES = [
  { id: "default", label: "기본", light: false },
  { id: "dark", label: "블루 다크", light: false },
  { id: "graphite", label: "그래파이트", light: false },
  { id: "mocha", label: "모카", light: false },
  { id: "light", label: "라이트", light: true },
  { id: "ivory", label: "아이보리", light: true },
] as const;

export const isLightTheme = (id: string) => THEMES.some((t) => t.id === id && t.light);
export const DEFAULT_THEME = "default";
/** Only the default theme shows the logo's five colours; every other theme keeps the logo one colour. */
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

// spacing of the pattern in px; 24 is the snap grid, the others its halves and multiples so lines still meet blocks
export const CANVAS_GAPS = [
  { px: 12, label: "촘촘" },
  { px: 24, label: "보통" },
  { px: 48, label: "넓게" },
  { px: 96, label: "아주 넓게" },
] as const;

export const SPEEDS = {
  fast: { label: "빠르게", ms: 150 },
  normal: { label: "보통", ms: 240 },
  slow: { label: "느리게", ms: 360 },
} as const;

type Prefs = {
  theme: (typeof THEMES)[number]["id"];
  easing: keyof typeof EASINGS;
  speed: keyof typeof SPEEDS;
  motion: boolean;
  panelStyle: keyof typeof PANEL_STYLES;
  canvasBg: keyof typeof CANVAS_BGS;
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
};

export const AWAY_CHOICES = [
  { minutes: 5, label: "5분" },
  { minutes: 15, label: "15분" },
  { minutes: 60, label: "1시간" },
  { minutes: 0, label: "끄지 않음" },
] as const;

export const useSettings = create<Prefs & { set: (p: Partial<Prefs>) => void }>()(
  persist((set) => ({ theme: DEFAULT_THEME, easing: "standard", speed: "normal", motion: true, panelStyle: "cards", canvasBg: "dots", canvasGap: 24, canvasAlpha: 100, defaultAgent: "auto", defaultPermission: "acceptEdits", disabledAgents: [], agentModels: {}, assistantAgent: "claude-code", assistantModel: "", agentLayout: "full", maestroLayout: "canvas", agentView: "chat", awayMinutes: 15, autoSave: false, sidebarDensity: "two", set }), {
    name: "maestro.settings",
    // v1 the default theme for everyone once, v2 its id "chrome" → "default", v3 the full screen's view → agentView
    // (settingsMigrate.ts, each step in turn)
    version: SETTINGS_VERSION,
    migrate: (old, from) => migrateSettings(old, from) as unknown as Prefs,
  }),
);

/** Motion duration in ms; 0 when the user or the OS turned motion off. */
export const motionMs = (s: Prefs) =>
  s.motion && !matchMedia("(prefers-reduced-motion: reduce)").matches ? SPEEDS[s.speed].ms : 0;

/** Push theme + motion onto <html> so CSS tokens pick them up. */
function applySettings(s: Prefs) {
  const root = document.documentElement;
  // a theme removed since it was saved (e.g. forest) falls back to the default
  root.dataset.theme = THEMES.some((t) => t.id === s.theme) ? s.theme : DEFAULT_THEME;
  root.dataset.panels = s.panelStyle in PANEL_STYLES ? s.panelStyle : "cards";
  root.style.setProperty("--mae-ease", EASINGS[s.easing].curve);
  root.style.setProperty("--mae-dur", `${motionMs(s)}ms`);
}
applySettings(useSettings.getState()); // before first paint: no flash of the default theme
useSettings.subscribe(applySettings);
// another window (main ↔ pop-out) changed a setting: pick it up here too
window.addEventListener("storage", (e) => e.key === "maestro.settings" && void useSettings.persist.rehydrate());
