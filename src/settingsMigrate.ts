// Pure (no React, zustand or Tauri): the saved settings' upgrades and which view (chat / terminal) an agent shows.
// Tested by scripts/view-check.ts. docs/reference/settings-keys.md, docs/features/agents/chat-view.md "보기".
import type { AgentView } from "./types";

/** The `maestro.settings` format this build writes. */
export const SETTINGS_VERSION = 7;

/**
 * The theme the v1 and v2 steps move people onto: the default of that time (id "default", called 블랙 now; today's
 * default is another, settings.ts `DEFAULT_THEME`). Kept here so this file needs nothing else.
 */
const THEME_THEN = "default";

/**
 * Settings saved by an older build (`from` = their version), brought up to `SETTINGS_VERSION` one step after
 * another, so a v0 file gets every later step too:
 * - v1: the redesign (DESIGN.md) moves everyone onto the default theme and its one easing curve once
 * - v2: that theme's id went from "chrome" to "default" (named 기본 then, 블랙 now)
 * - v3: `fullView` (the full screen's chat / terminal) became `agentView`, the one 에이전트 보기 of every place
 * - v4: straight lines became the default; a saved "curve" (the old default, rarely picked) moves to straight once
 * - v5: that default line, which bends at right angles, is called "step" (꺾은선) now that "straight" is one straight
 *   line from end to end (a shape to pick): a saved "straight" moves to "step" once, so nobody's lines change
 * - v6: 와이드 ("full") became the default panel style; a saved "cards" (분리, the default until then) moves to it once.
 *   Someone who had picked 분리 on purpose picks it again
 * - v7: 자리 비운 프로젝트 정리 is off by default; a saved 15 minutes (the default until then) moves to off once.
 *   Someone who had picked 15 on purpose picks it again
 */
export function migrateSettings(old: unknown, from: number): Record<string, unknown> {
  let s: Record<string, unknown> = { ...((old ?? {}) as Record<string, unknown>) };
  if (from < 1) s = { ...s, theme: THEME_THEN, easing: "standard" };
  if (from < 2 && s.theme === "chrome") s = { ...s, theme: THEME_THEN };
  if (from < 3) {
    const { fullView, ...rest } = s;
    s = fullView === "cli" || fullView === "chat" ? { ...rest, agentView: fullView } : rest;
  }
  if (from < 4 && s.edgeShape === "curve") s = { ...s, edgeShape: "straight" };
  if (from < 5 && s.edgeShape === "straight") s = { ...s, edgeShape: "step" };
  if (from < 6 && s.panelStyle === "cards") s = { ...s, panelStyle: "full" };
  if (from < 7 && s.awayMinutes === 15) s = { ...s, awayMinutes: 0 };
  return s;
}

/** An agent's key in `viewOverride`: its folder with forward slashes, no trailing slash, lower case. */
export const viewKey = (path: string) => path.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();

/** The view an agent shows: its "이 에이전트만" exception when it has one, else the app's 에이전트 보기. */
export const effectiveView = (path: string, override: Record<string, AgentView>, global: AgentView): AgentView =>
  override[viewKey(path)] ?? global;

/** `override` with agent `path` set to `view`; a view equal to the global one (or null) drops the exception. */
export function withOverride(override: Record<string, AgentView>, path: string, view: AgentView | null, global: AgentView): Record<string, AgentView> {
  const key = viewKey(path);
  const next = { ...override };
  if (view === null || view === global) delete next[key];
  else next[key] = view;
  return next;
}

/**
 * The one view switch of an agent (its header icon, its menus): where a click goes, and what it's called. "돌아가기"
 * when the agent is on an exception (the click goes back to the app's view), "이 에이전트만" when it isn't.
 */
export function viewSwitch(view: AgentView, global: AgentView): { to: AgentView; label: string; menu: string } {
  const to: AgentView = view === "chat" ? "cli" : "chat";
  const back = to === global;
  if (to === "cli") return { to, label: back ? "터미널로 돌아가기" : "이 에이전트만 터미널로 보기", menu: back ? "터미널로 돌아가기" : "터미널로 보기" };
  return { to, label: back ? "채팅으로 돌아가기" : "이 에이전트만 채팅으로 보기", menu: back ? "채팅으로 돌아가기" : "채팅으로 보기" };
}
