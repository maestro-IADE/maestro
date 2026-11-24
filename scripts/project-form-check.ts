// The 프로젝트 추가 forms' pure parts (src/projectForm.ts). Run: `npm test`.
// The cases match src-tauri/src/newproject.rs's tests: the dialog fills in the name the backend would pick.
import { equal } from "node:assert/strict";
import { defaultParent, expandHome, folderNameOk, joinPath, repoNameFromUrl, shortPath } from "../src/projectForm.ts";

// repoNameFromUrl: the last path part without .git, "" when no folder could be named so
for (const [url, name] of [
  ["https://github.com/a/repo.git", "repo"],
  ["https://github.com/a/repo", "repo"],
  ["https://github.com/a/repo/", "repo"],
  ["https://github.com/a/repo.git/", "repo"],
  ["https://host/a/repo.git?x=1#top", "repo"],
  ["git@github.com:a/my-app.git", "my-app"],
  ["git@host:repo.git", "repo"],
  ["ssh://git@host:22/team/tool.js.git", "tool.js"],
  ["  https://h/r.git  ", "r"],
  ["https://github.com", "github.com"],
  ["https://h/a/..", ""],
  ["https://h/con.git", ""],
  ["", ""],
]) {
  equal(repoNameFromUrl(url), name, url);
}

// folderNameOk: one folder, one Windows can make
for (const ok of ["repo", " my app ", "한글-폴더", "a.b", "v1.2.3", "COM0", "console"]) equal(folderNameOk(ok), true, ok);
for (const bad of ["", "  ", ".", "..", "a..b", "a/b", "a\\b", "C:", "a:b", "a*", "a?", "a|b", "<a>", '"a"', "-a", "a.", "con", "NUL.txt", "com1", "LPT9", "a\tb"]) {
  equal(folderNameOk(bad), false, JSON.stringify(bad));
}

// joinPath: the name in the folder, with the folder's own slash; a drive or / keeps its one
equal(joinPath(String.raw`C:\Users\me\code`, "app"), String.raw`C:\Users\me\code\app`);
equal(joinPath(String.raw`C:\Users\me\code\ `.trim(), " app "), String.raw`C:\Users\me\code\app`, "a trailing slash, a spaced name");
equal(joinPath("C:/code", "app"), "C:/code/app");
equal(joinPath(String.raw`C:\ `.trim(), "app"), String.raw`C:\app`, "a drive root");
equal(joinPath("/home/me", "app"), "/home/me/app");
equal(joinPath("/", "app"), "/app");
equal(joinPath(String.raw`C:\code`, "  "), String.raw`C:\code`, "no name: the folder alone");

// defaultParent: ~/maestro/projects, written as the home folder is
equal(defaultParent(String.raw`C:\Users\me`), String.raw`C:\Users\me\maestro\projects`);
equal(defaultParent("/home/me/"), "/home/me/maestro/projects");
equal(defaultParent(""), "");

// shortPath: the home folder as ~, only at a folder boundary; on Windows in any case and slash
const home = String.raw`C:\Users\me`;
equal(shortPath(String.raw`C:\Users\me\maestro\projects\app`, home), String.raw`~\maestro\projects\app`);
equal(shortPath("c:/users/ME/code", home), "~/code", "another case and slash");
equal(shortPath(String.raw`C:\Users\me`, home), "~");
equal(shortPath(String.raw`C:\Users\me\ `.trim(), home), "~");
equal(shortPath(String.raw`C:\Users\meg\code`, home), String.raw`C:\Users\meg\code`, "not a folder of home");
equal(shortPath(String.raw`D:\code\app`, home), String.raw`D:\code\app`);
equal(shortPath("/home/me/code", "/home/me"), "~/code");
equal(shortPath("/home/Me/code", "/home/me"), "/home/Me/code", "case counts off Windows");
equal(shortPath(String.raw`C:\code`, ""), String.raw`C:\code`, "no home known");

// expandHome: a place typed as the row writes it goes back to a full path; anything else stays
equal(expandHome("~", home), home);
equal(expandHome(String.raw`~\code`, home), String.raw`C:\Users\me\code`);
equal(expandHome("~/code", "/home/me/"), "/home/me/code");
equal(expandHome("~me/code", home), "~me/code", "another user's ~ is not ours");
equal(expandHome(String.raw`D:\code`, home), String.raw`D:\code`);
equal(expandHome("~/code", ""), "~/code", "no home known yet");
// what the row shows comes back as the same place
equal(expandHome(shortPath(String.raw`C:\Users\me\maestro\projects`, home), home), String.raw`C:\Users\me\maestro\projects`);

console.log("ok project form");
