// The saved settings' upgrades and an agent's chat / terminal view (src/settingsMigrate.ts). Run: `npm test`.
import { deepStrictEqual, equal } from "node:assert/strict";
import { effectiveView, migrateSettings, SETTINGS_VERSION, viewKey, viewSwitch, withOverride } from "../src/settingsMigrate.ts";

equal(SETTINGS_VERSION, 7);

// v0 (before the redesign): the default theme and easing once, and fullView renamed, every step in turn
deepStrictEqual(
  migrateSettings({ theme: "dark", easing: "bouncy", fullView: "cli", motion: false }, 0),
  { theme: "default", easing: "standard", agentView: "cli", motion: false },
);
deepStrictEqual(migrateSettings({ theme: "mocha" }, 0), { theme: "default", easing: "standard" }, "no fullView: agentView comes from the defaults");
// v1: the theme stays, the view is carried over and the old key goes
deepStrictEqual(migrateSettings({ theme: "mocha", fullView: "cli" }, 1), { theme: "mocha", agentView: "cli" });
equal("fullView" in migrateSettings({ fullView: "chat" }, 1), false, "the old key is gone");
equal(migrateSettings({ fullView: "chat" }, 1).agentView, "chat");
// v1 with the old id of the default theme
deepStrictEqual(migrateSettings({ theme: "chrome", fullView: "chat" }, 1), { theme: "default", agentView: "chat" });
// v2: chrome → default no more (it was v2's own step), only the rename
deepStrictEqual(migrateSettings({ theme: "chrome", fullView: "cli" }, 2), { theme: "chrome", agentView: "cli" });
deepStrictEqual(migrateSettings({ theme: "graphite", fullView: "chat", sidebarDensity: "one" }, 2), { theme: "graphite", agentView: "chat", sidebarDensity: "one" });
// a value that isn't a view is dropped, not carried over
deepStrictEqual(migrateSettings({ theme: "dark", fullView: "terminal" }, 2), { theme: "dark" });
// v3: the old default curve becomes the right-angled line once (saved as "straight" by v4, called "step" since v5)
deepStrictEqual(migrateSettings({ theme: "dark", agentView: "cli" }, 3), { theme: "dark", agentView: "cli" });
deepStrictEqual(migrateSettings({ edgeShape: "curve" }, 3), { edgeShape: "step" });
deepStrictEqual(migrateSettings({ edgeShape: "straight" }, 3), { edgeShape: "step" });
// v4: a curve picked after the switch stays; "straight" was the right-angled line then, and becomes "step"
deepStrictEqual(migrateSettings({ edgeShape: "curve" }, 4), { edgeShape: "curve" });
deepStrictEqual(migrateSettings({ edgeShape: "straight" }, 4), { edgeShape: "step" });
// v5 already: "straight" is the straight line someone picked, and stays
deepStrictEqual(migrateSettings({ edgeShape: "straight" }, 5), { edgeShape: "straight" });
deepStrictEqual(migrateSettings({ edgeShape: "step" }, 5), { edgeShape: "step" });
// v6: 와이드 is the default panel style: a saved 분리 (the old default) moves to it once; the other choices stay,
// and so does a 분리 picked after that
deepStrictEqual(migrateSettings({ panelStyle: "cards", theme: "dark" }, 5), { panelStyle: "full", theme: "dark" });
deepStrictEqual(migrateSettings({ panelStyle: "lines" }, 5), { panelStyle: "lines" });
deepStrictEqual(migrateSettings({ panelStyle: "cards" }, 6), { panelStyle: "cards" });
equal(migrateSettings({ panelStyle: "cards", edgeShape: "curve" }, 0).panelStyle, "full", "an old save gets this step too");
// v7: the away stop is off by default: a saved 15 (the old default) moves to off once; other choices stay, and so
// does a 15 picked after that
deepStrictEqual(migrateSettings({ awayMinutes: 15, theme: "dark" }, 6), { awayMinutes: 0, theme: "dark" });
deepStrictEqual(migrateSettings({ awayMinutes: 60 }, 6), { awayMinutes: 60 });
deepStrictEqual(migrateSettings({ awayMinutes: 5 }, 6), { awayMinutes: 5 });
deepStrictEqual(migrateSettings({ awayMinutes: 15 }, 7), { awayMinutes: 15 });
// nothing saved at all
deepStrictEqual(migrateSettings(undefined, 0), { theme: "default", easing: "standard" });
// the input is not changed in place
const saved = { theme: "chrome", fullView: "cli" };
migrateSettings(saved, 1);
deepStrictEqual(saved, { theme: "chrome", fullView: "cli" });

// viewKey: one key per folder, however it's written
equal(viewKey("C:\\Repo\\wt-1\\"), "c:/repo/wt-1");
equal(viewKey("c:/repo/wt-1"), viewKey("C:\\REPO\\WT-1"));

// effectiveView: the agent's exception, else the app's view
equal(effectiveView("C:\\repo\\a", {}, "chat"), "chat", "no exception");
equal(effectiveView("C:\\repo\\a", {}, "cli"), "cli");
equal(effectiveView("C:\\repo\\a", { "c:/repo/a": "cli" }, "chat"), "cli", "its exception");
equal(effectiveView("c:/REPO/a/", { "c:/repo/a": "cli" }, "chat"), "cli", "case and slashes don't matter");
equal(effectiveView("C:\\repo\\b", { "c:/repo/a": "cli" }, "chat"), "chat", "another agent's exception doesn't leak");

// withOverride: set, back to the app's view (dropped), cleared
const one = withOverride({}, "C:\\repo\\a", "cli", "chat");
deepStrictEqual(one, { "c:/repo/a": "cli" });
deepStrictEqual(withOverride(one, "c:/repo/a", "chat", "chat"), {}, "the app's view ends the exception");
deepStrictEqual(withOverride(one, "C:\\Repo\\A", null, "chat"), {}, "null ends it");
deepStrictEqual(withOverride({}, "c:/repo/a", "cli", "cli"), {}, "the same as the app's: nothing to keep");
deepStrictEqual(withOverride(one, "c:/repo/b", "chat", "cli"), { "c:/repo/a": "cli", "c:/repo/b": "chat" }, "others stay");
deepStrictEqual(one, { "c:/repo/a": "cli" }, "not changed in place");

// viewSwitch: the header icon and menus
deepStrictEqual(viewSwitch("chat", "chat"), { to: "cli", label: "이 에이전트만 터미널로 보기", menu: "터미널로 보기" });
deepStrictEqual(viewSwitch("cli", "chat"), { to: "chat", label: "채팅으로 돌아가기", menu: "채팅으로 돌아가기" });
deepStrictEqual(viewSwitch("cli", "cli"), { to: "chat", label: "이 에이전트만 채팅으로 보기", menu: "채팅으로 보기" });
deepStrictEqual(viewSwitch("chat", "cli"), { to: "cli", label: "터미널로 돌아가기", menu: "터미널로 돌아가기" });

console.log("ok view");
