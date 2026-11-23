// The 프로젝트 추가 forms' pure parts (ui/ProjectForm.tsx: URL로 복제, 새 프로젝트). No React, no store, no Tauri:
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

/** The folder holding `path` (either slash; a drive root keeps its slash): "" when there's none. */
export const parentOf = (path: string): string => {
  const p = path.replace(/[/\\]+$/, "");
  const i = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
  if (i < 0) return "";
  const up = p.slice(0, i);
  return /^[A-Za-z]:$/.test(up) || up === "" ? p.slice(0, i + 1) : up;
};
