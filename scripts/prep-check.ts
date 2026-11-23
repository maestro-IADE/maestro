// 워크트리 준비's "복사할 파일" field (src/settings/prepText.ts). Run: `npm test`.
import { deepStrictEqual, equal } from "node:assert/strict";
import { copyFilesChanged, copyFilesText, parseCopyFiles } from "../src/settings/prepText.ts";

// parseCopyFiles: commas, trimmed, no empties, each once, in the typed order
deepStrictEqual(parseCopyFiles(".env*"), [".env*"]);
deepStrictEqual(parseCopyFiles(" .env* , config/local.json,, "), [".env*", "config/local.json"]);
deepStrictEqual(parseCopyFiles(".env, .env ,x"), [".env", "x"], "each once");
deepStrictEqual(parseCopyFiles(""), [], "emptied: nothing is copied");
deepStrictEqual(parseCopyFiles(" , "), []);

// copyFilesText: back as the field shows it
equal(copyFilesText([".env*", "config/local.json"]), ".env*, config/local.json");
equal(copyFilesText([]), "");
deepStrictEqual(parseCopyFiles(copyFilesText([".env*", "a/*.json"])), [".env*", "a/*.json"], "round trip");

// copyFilesChanged: only a real edit saves
equal(copyFilesChanged(".env*", [".env*"]), false);
equal(copyFilesChanged(" .env* ,", [".env*"]), false, "spacing and a trailing comma are no edit");
equal(copyFilesChanged(".env*, x", [".env*"]), true);
equal(copyFilesChanged("", [".env*"]), true, "emptying is an edit");
equal(copyFilesChanged("x, .env*", [".env*", "x"]), true, "the order is kept as typed");

console.log("ok prep");
