// The 프로젝트 추가 forms' pure parts (ui/ProjectForm.tsx: 저장소 클론, 새 프로젝트). No React, no store, no Tauri:
// scripts/project-form-check.ts tests them. Docs: docs/features/ui/add-project.md.

/** A folder name the backend takes (src-tauri/src/newproject.rs `check_name`): one folder Windows can make. */
export const folderNameOk = (name: string): boolean => {
  const n = name.trim();
  if (!n || n === "." || n.includes("..") || n.startsWith("-") || n.endsWith(".")) return false;
  if (/[/\\:*?"<>|\u0000-\u001f\u007f-\u009f]/.test(n)) return false;
  const stem = n.split(".")[0].toUpperCase();
  return !/^(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])$/.test(stem);
};

/**
 * The folder a clone gets until the user types one: the URL's last path part without `.git`
 * (`https://github.com/a/repo.git` → `repo`, `git@host:repo` → `repo`); "" when that's no usable name. The same as
 * newproject.rs `repo_name_from_url`, which the backend uses when the name comes empty.
 */
export const repoNameFromUrl = (url: string): string => {
  let u = url.trim().split(/[?#]/)[0].replace(/[/\\]+$/, "");
  if (u.endsWith(".git")) u = u.slice(0, -4).replace(/[/\\]+$/, "");
  const last = u.split(/[/\\:]/).pop() ?? "";
  return folderNameOk(last) ? last : "";
};

/**
 * `name` in folder `parent`, joined with the slash `parent` uses (a backslash if it has one); `parent` as it is when
 * there's no name. A drive (`C:\`) or `/` keeps its one slash.
 */
export const joinPath = (parent: string, name: string): string => {
  const raw = parent.trim();
  const n = name.trim();
  if (!n) return raw;
  const sep = raw.includes("\\") ? "\\" : "/";
  const p = raw.replace(/[/\\]+$/, "");
  if (!p) return raw ? sep + n : n;
  return p + sep + n;
};

/** Where new projects go until another 위치 is used: `maestro/projects` in the home folder (like Orca's `~/orca/projects`). */
export const defaultParent = (home: string): string => (home.trim() ? joinPath(joinPath(home, "maestro"), "projects") : "");

/**
 * A place typed the way the 위치 row writes it (`~`, `~/code`, `~\code`) with the home folder put back, for the backend,
 * which takes only full paths. Anything else, or no home known yet, as it is.
 */
export const expandHome = (path: string, home: string): string => {
  const h = home.trim().replace(/[/\\]+$/, "");
  if (!h || !/^~([/\\]|$)/.test(path)) return path;
  return h + path.slice(1);
};

/**
 * `path` as the 위치 row shows it: the home folder written `~` (on Windows in any case and with either slash), the rest
 * as it is. Any other path as it is.
 */
export const shortPath = (path: string, home: string): string => {
  const h = home.trim().replace(/[/\\]+$/, "");
  if (!h) return path;
  const win = /^[A-Za-z]:/.test(h);
  const key = (s: string) => (win ? s.replace(/\\/g, "/").toLowerCase() : s);
  if (key(path.replace(/[/\\]+$/, "")) === key(h)) return "~";
  const next = path.charAt(h.length);
  const under = key(path.slice(0, h.length)) === key(h) && (next === "/" || (win && next === "\\"));
  return under ? "~" + path.slice(h.length) : path;
};
