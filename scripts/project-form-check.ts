// The 프로젝트 추가 forms' pure parts (src/projectForm.ts). Run: `npm test`.
// The cases match src-tauri/src/newproject.rs's tests: the dialog fills in the name the backend would pick.
import { equal } from "node:assert/strict";
import { folderNameOk, parentOf, repoNameFromUrl } from "../src/projectForm.ts";

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

// parentOf: the folder a project sits in, the default 위치 when none was used yet
equal(parentOf("C:/Users/me/code/app"), "C:/Users/me/code");
equal(parentOf("C:\\Users\\me\\code\\app\\"), "C:\\Users\\me\\code");
equal(parentOf("C:/app"), "C:/", "a drive root keeps its slash");
equal(parentOf("/home/me/app"), "/home/me");
equal(parentOf("/app"), "/");
equal(parentOf("app"), "");

console.log("ok project form");
