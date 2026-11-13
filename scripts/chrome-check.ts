// The window chrome's pure parts: the shortcut table and its guard (src/shortcuts.ts), the toast stack
// (src/ui/toastList.ts), which scrolls close a menu (src/ui/menuScroll.ts). Run: `npm test`.
import { deepStrictEqual, equal, ok } from "node:assert/strict";
import { guardAllows, matchShortcut, SHORTCUTS, tabNumber, tip, type KeyLike, type TargetLike } from "../src/shortcuts.ts";
import { MAX_TOASTS, withToast, type Toast } from "../src/ui/toastList.ts";
import { scrollClosesMenu } from "../src/ui/menuScroll.ts";

const key = (k: Partial<KeyLike>): KeyLike => ({ key: "", code: "", ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...k });
const id = (k: Partial<KeyLike>) => matchShortcut(key(k))?.id;

// a: the keys of the table
equal(id({ key: ",", code: "Comma", ctrlKey: true }), "settings");
equal(id({ key: ",", code: "Comma", metaKey: true }), "settings", "⌘, on macOS");
equal(id({ key: "<", code: "Comma", ctrlKey: true, shiftKey: true }), undefined);
equal(id({ key: "ArrowLeft", altKey: true }), "back");
equal(id({ key: "ArrowRight", altKey: true }), "forward");
equal(id({ key: "ArrowLeft", altKey: true, shiftKey: true }), undefined, "Alt+Shift+← selects words, not ours");
equal(id({ key: "ArrowLeft", ctrlKey: true }), undefined);
equal(id({ key: "ㅓ", code: "KeyJ", ctrlKey: true }), "nextAttention", "by code: the Korean IME's key is a jamo");
equal(id({ key: "ArrowDown", ctrlKey: true, shiftKey: true }), "nextAgent");
equal(id({ key: "ArrowUp", ctrlKey: true, shiftKey: true }), "prevAgent");
equal(id({ key: "3", code: "Digit3", ctrlKey: true }), "tabN");
equal(id({ key: "0", code: "Digit0", ctrlKey: true }), undefined);
equal(id({ key: "!", code: "Digit1", ctrlKey: true, shiftKey: true }), undefined, "Ctrl+Shift+1 is not a tab");
equal(tabNumber(key({ key: "3", code: "Digit3", ctrlKey: true })), 3);
equal(tabNumber(key({ key: "9", code: "Digit9", ctrlKey: true })), 9);
equal(tabNumber(key({ key: "3", code: "Numpad3", ctrlKey: true })), 0, "the number pad is left alone");
equal(id({ key: "F2" }), "rename");
equal(id({ key: "F2", ctrlKey: true }), undefined);
equal(id({ key: "b", code: "KeyB", ctrlKey: true }), undefined, "Ctrl+B stays the terminal's");
equal(id({ key: ",", code: "Comma" }), undefined);
equal(new Set(SHORTCUTS.map((s) => s.id)).size, SHORTCUTS.length, "ids are unique");

// b: tooltips
equal(tip("settings"), "설정 (Ctrl+,)");
equal(tip("back"), "뒤로 (Alt+←)");
equal(tip("forward"), "앞으로 (Alt+→)");

// c: the guard, with stand-in elements that know which selectors they sit inside
const inside = (...sels: string[]): TargetLike => ({ closest: (sel: string) => (sels.some((s) => sel.split(",").map((x) => x.trim()).includes(s)) ? {} : null) });
const body = inside();
const terminal = inside(".xterm");
const editor = inside(".monaco-editor");
const field = inside("textarea");
const sidebar = inside(".sb-body");
const settings = matchShortcut(key({ key: ",", code: "Comma", ctrlKey: true }))!;
const back = matchShortcut(key({ key: "ArrowLeft", altKey: true }))!;
const rename = matchShortcut(key({ key: "F2" }))!;
ok(guardAllows(settings, body, false));
ok(guardAllows(settings, field, false), "Ctrl+, works in a text field");
ok(guardAllows(settings, editor, false), "and in the editor");
ok(guardAllows(settings, body, true), "and over a dialog");
ok(!guardAllows(settings, terminal, false), "a terminal keeps every key");
ok(guardAllows(back, body, false));
ok(guardAllows(back, null, false), "no target (the window itself)");
ok(!guardAllows(back, field, false), "Alt+← moves the caret in a field");
ok(!guardAllows(back, editor, false), "and in Monaco");
ok(!guardAllows(back, terminal, false));
ok(!guardAllows(back, body, true), "nothing moves behind a dialog");
ok(guardAllows(rename, sidebar, false));
ok(!guardAllows(rename, body, false), "F2 only in the sidebar list");
const quick = matchShortcut(key({ key: "ㅏ", code: "KeyK", ctrlKey: true }))!;
equal(quick?.id, "quickSwitch", "Ctrl+K by code");
equal(id({ key: "K", code: "KeyK", ctrlKey: true, shiftKey: true }), undefined);
equal(tip("quickSwitch"), "빠른 이동 (Ctrl+K)");
ok(guardAllows(quick, body, false));
ok(guardAllows(quick, field, false), "Ctrl+K works in the chat composer");
ok(!guardAllows(quick, inside(".monaco-editor", "textarea"), false), "Monaco keeps its Ctrl+K chords");
ok(!guardAllows(quick, terminal, false), "the shell keeps Ctrl+K");
ok(!guardAllows(quick, field, true), "not over a dialog (the palette itself takes Ctrl+K to close)");

// d: the toast stack
{
  let list: Toast[] = [];
  list = withToast(list, "a", 1, 10);
  list = withToast(list, "b", 2, 20);
  list = withToast(list, "a", 3, 30);
  deepStrictEqual(list, [{ id: 1, text: "a", n: 2, at: 30 }, { id: 2, text: "b", n: 1, at: 20 }], "the same text merges, its timer restarts");
  list = withToast(list, "c", 4, 40);
  list = withToast(list, "d", 5, 50);
  equal(list.length, MAX_TOASTS);
  deepStrictEqual(list.map((t) => t.text), ["b", "c", "d"], "the oldest closes first");
}

// e: a menu closes on a scroll that moves its anchor, not on a chat or a terminal scrolling itself
{
  type El = { name: string; kids: El[]; contains: (n: unknown) => boolean };
  const el = (name: string, kids: El[] = []): El => {
    const e: El = { name, kids, contains: (n) => n === e || kids.some((k) => k.contains(n)) };
    return e;
  };
  const button = el("⋯");
  const header = el("header", [button]);
  const chat = el("chat");
  const xterm = el("xterm-viewport");
  const doc = el("document", [el("body", [header, chat, xterm])]);
  equal(scrollClosesMenu(chat, button, false), false, "the chat following its stream leaves the header's menu open");
  equal(scrollClosesMenu(xterm, button, false), false, "a terminal printing too");
  equal(scrollClosesMenu(header, button, false), true, "a box the anchor is in moved it");
  equal(scrollClosesMenu(doc, button, false), true, "the page moved");
  equal(scrollClosesMenu({}, button, false), true, "the window (no contains) moved");
  equal(scrollClosesMenu(null, button, false), true);
  equal(scrollClosesMenu(header, button, true), false, "the menu's own list scrolling");
  equal(scrollClosesMenu(chat, null, false), false, "no anchor: only the wheel closes it");
}

console.log("ok chrome");
