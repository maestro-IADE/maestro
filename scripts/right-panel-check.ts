// The right panel's change rows and the PR card's remembered merge method (src/rightPanelModel.ts). Run: `npm test`.
import { deepStrictEqual, equal } from "node:assert/strict";
import { changeCode, parseRightTab, pickPrMethod, prMethodKey, splitPath } from "../src/rightPanelModel.ts";

// parseRightTab: only "changes" is kept, anything else is the files tab
equal(parseRightTab("changes"), "changes");
equal(parseRightTab("files"), "files");
equal(parseRightTab(null), "files", "nothing stored yet");
equal(parseRightTab("system"), "files", "an unknown value");

// splitPath: the name and the folder as two parts
deepStrictEqual(splitPath("src/nodes/App.tsx"), { name: "App.tsx", dir: "src/nodes" });
deepStrictEqual(splitPath("README.md"), { name: "README.md", dir: "" }, "a file at the top has no folder");
deepStrictEqual(splitPath("docs\\a b\\노트.md"), { name: "노트.md", dir: "docs/a b" }, "backslashes, spaces and Korean");
deepStrictEqual(splitPath("vendor/lib/"), { name: "lib", dir: "vendor" }, "a trailing slash is dropped");

// changeCode: porcelain XY → one letter and its colour group
deepStrictEqual(changeCode("??"), { letter: "U", kind: "add", label: "추적 안 됨" });
equal(changeCode(" M").letter, "M");
equal(changeCode("M ").kind, "mod");
equal(changeCode("MM").letter, "M", "staged and unstaged edits are one M");
equal(changeCode("A ").kind, "add");
equal(changeCode("AM").letter, "A", "added, then edited: still added");
equal(changeCode(" D").kind, "del");
equal(changeCode("R ").label, "이름 바뀜");
equal(changeCode("UU").kind, "conflict");
equal(changeCode("AA").letter, "!", "both added is a conflict too");
equal(changeCode("DD").kind, "conflict");
equal(changeCode("DU").kind, "conflict");

// prMethodKey: one key per project, however its path is spelled
equal(prMethodKey("C:\\Repo\\Maestro"), "maestro.prMethod.c:/repo/maestro");
equal(prMethodKey("c:/repo/maestro/"), prMethodKey("C:\\Repo\\Maestro"), "case and slashes don't matter");

// pickPrMethod: the remembered method while the repo allows it, else the first allowed
const both = { merge: true, squash: true };
equal(pickPrMethod(null, both), "merge", "nothing remembered: merge first");
equal(pickPrMethod("squash", both), "squash", "remembered");
equal(pickPrMethod("squash", { merge: true, squash: false }), "merge", "the repo stopped allowing squash");
equal(pickPrMethod("merge", { merge: false, squash: true }), "squash", "merge commits not allowed");
equal(pickPrMethod("rebase", both), "merge", "an unknown stored value is ignored");
equal(pickPrMethod("squash", { merge: false, squash: false }), "squash", "nothing allowed: keep what was remembered, GitHub says why");

console.log("right-panel-check: ok");
