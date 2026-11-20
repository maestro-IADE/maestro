//! The documents Maestro keeps for every project (the sidebar's 문서 tab): the personas, plus
//! `~/.maestro/library/<category>/` folders of shared Markdown (guidelines, PRD templates, …). Dragged onto a
//! canvas they become Markdown blocks; linked to an agent they become its references. A project's own files
//! come from the file tree instead, so nothing here is per project.

use serde::Serialize;
use std::path::Path;

/// Personas: one Markdown file per role, shared by every project (`~/.maestro/personas`). Linking one to a
/// worktree card on the canvas makes that agent take the role.
pub fn personas_dir(home: &Path) -> std::path::PathBuf {
    home.join(".maestro").join("personas")
}

pub fn is_persona(home: &Path, md: &str) -> bool {
    md.replace('\\', "/").to_lowercase().starts_with(&format!("{}/", personas_dir(home).to_string_lossy().replace('\\', "/").to_lowercase()))
}

/// The files Maestro ships (`src-tauri/seed/`): (folder under `~/.maestro`, file name, text).
const BUILTIN: &[(&str, &str, &str)] = &[
    ("personas", "architect.md", include_str!("../seed/personas/architect.md")),
    ("personas", "designer.md", include_str!("../seed/personas/designer.md")),
    ("personas", "docs-writer.md", include_str!("../seed/personas/docs-writer.md")),
    ("personas", "implementer.md", include_str!("../seed/personas/implementer.md")),
    ("personas", "planner.md", include_str!("../seed/personas/planner.md")),
    ("personas", "reviewer.md", include_str!("../seed/personas/reviewer.md")),
    ("personas", "tester.md", include_str!("../seed/personas/tester.md")),
    ("personas", "verifier.md", include_str!("../seed/personas/verifier.md")),
    ("personas", "skeptic.md", include_str!("../seed/personas/skeptic.md")),
    ("personas", "debugger.md", include_str!("../seed/personas/debugger.md")),
    ("library/지침", "agent-rules.md", include_str!("../seed/library/지침/agent-rules.md")),
    ("library/지침", "coding-rules.md", include_str!("../seed/library/지침/coding-rules.md")),
    ("library/지침", "git-rules.md", include_str!("../seed/library/지침/git-rules.md")),
    ("library/PRD", "adr-template.md", include_str!("../seed/library/PRD/adr-template.md")),
    ("library/PRD", "feature-spec.md", include_str!("../seed/library/PRD/feature-spec.md")),
    ("library/PRD", "prd-template.md", include_str!("../seed/library/PRD/prd-template.md")),
    ("library/디자인", "design-principles.md", include_str!("../seed/library/디자인/design-principles.md")),
    ("library/디자인", "motion-guide.md", include_str!("../seed/library/디자인/motion-guide.md")),
    ("library/디자인", "ui-checklist.md", include_str!("../seed/library/디자인/ui-checklist.md")),
    ("library/RAG", "glossary.md", include_str!("../seed/library/RAG/glossary.md")),
    ("library/RAG", "rag-doc-format.md", include_str!("../seed/library/RAG/rag-doc-format.md")),
    ("library/RAG", "troubleshooting.md", include_str!("../seed/library/RAG/troubleshooting.md")),
    ("playbooks", "default.md", include_str!("../seed/playbooks/default.md")),
    ("playbooks", "architecture-first.md", include_str!("../seed/playbooks/architecture-first.md")),
    ("playbooks", "bug-hunt.md", include_str!("../seed/playbooks/bug-hunt.md")),
    ("playbooks", "plan-then-build.md", include_str!("../seed/playbooks/plan-then-build.md")),
    ("playbooks", "race.md", include_str!("../seed/playbooks/race.md")),
    ("playbooks", "review-loop.md", include_str!("../seed/playbooks/review-loop.md")),
    ("playbooks", "split-build.md", include_str!("../seed/playbooks/split-build.md")),
    ("playbooks", "evidence-gate.md", include_str!("../seed/playbooks/evidence-gate.md")),
    ("playbooks", "approve-then-act.md", include_str!("../seed/playbooks/approve-then-act.md")),
    ("playbooks", "multi-lens-review.md", include_str!("../seed/playbooks/multi-lens-review.md")),
    ("playbooks", "make-play.md", include_str!("../seed/playbooks/make-play.md")),
];

/// Writes the built-in personas, documents and playbooks that aren't there yet. Each folder's `.seeded` lists what
/// was written once, so a file the user deleted stays deleted, and a newer Maestro still adds the files it newly
/// ships. A file that already exists is not written over, except one still saying exactly what Maestro last wrote in
/// it (`update_untouched`): that one takes the newer text.
pub fn seed(home: &Path) {
    for (dir, name, body) in BUILTIN {
        let dir = home.join(".maestro").join(dir);
        let marker = dir.join(".seeded");
        let done = std::fs::read_to_string(&marker).unwrap_or_default();
        if done.lines().any(|l| l == *name) || std::fs::create_dir_all(&dir).is_err() {
            continue;
        }
        let file = dir.join(name);
        if !file.exists() && std::fs::write(&file, body).is_ok() {
            remember(&dir, name, body);
        }
        let _ = std::fs::write(&marker, format!("{done}{name}\n"));
    }
    rename_runbooks(home);
    update_untouched(home);
}

/// A text's fingerprint (FNV-1a, line endings aside): stable across Maestro versions, unlike std's hasher.
fn print(text: &str) -> u64 {
    text.replace("\r\n", "\n").bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

/// What Maestro last wrote in each built-in file of a folder: its `.shipped`, lines `name<TAB>fingerprint`.
fn written(dir: &Path) -> std::collections::BTreeMap<String, u64> {
    std::fs::read_to_string(dir.join(".shipped"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('\t').and_then(|(n, h)| Some((n.to_string(), u64::from_str_radix(h, 16).ok()?))))
        .collect()
}

fn write_marks(dir: &Path, marks: &std::collections::BTreeMap<String, u64>) {
    let text: String = marks.iter().map(|(n, h)| format!("{n}\t{h:016x}\n")).collect();
    let _ = std::fs::write(dir.join(".shipped"), text);
}

/// Notes that Maestro just wrote `body` into a built-in file.
fn remember(dir: &Path, name: &str, body: &str) {
    let mut marks = written(dir);
    marks.insert(name.to_string(), print(body));
    write_marks(dir, &marks);
}

/// Each built-in file still saying exactly what Maestro last wrote in it was never touched, so it takes the text
/// this Maestro ships. One the user changed stays as it is (기본값으로 is the way back), and so does one written
/// before fingerprints were kept: there is no telling whether it was changed. A file that already says the shipped
/// text is noted, so the next revision reaches it. Deleted ones stay deleted.
fn update_untouched(home: &Path) {
    let mut dirs: Vec<&str> = BUILTIN.iter().map(|b| b.0).collect();
    dirs.dedup();
    for d in dirs {
        let dir = home.join(".maestro").join(d);
        let mut marks = written(&dir);
        let before = marks.clone();
        for (_, name, body) in BUILTIN.iter().filter(|b| b.0 == d) {
            let Ok(text) = std::fs::read_to_string(dir.join(name)) else { continue };
            let (now, ship) = (print(&text), print(body));
            if now != ship && marks.get(*name) == Some(&now) && std::fs::write(dir.join(name), body).is_err() {
                continue;
            }
            if now == ship || marks.get(*name) == Some(&now) {
                marks.insert(name.to_string(), ship);
            }
        }
        if marks != before {
            write_marks(&dir, &marks);
        }
    }
}

/// The runbooks' name until 2026-11-19 (docs/features/ui/terms.md). Kept only to find it in the built-in files an
/// older Maestro wrote; nothing shows it.
const OLD_NAME: &str = "플레이북";

/// Once per folder (its `.renamed` marks it done): the built-in files an older Maestro wrote say the runbooks' old
/// name, and it becomes 런북. Only that word changes, so what the user wrote in them stays; their own files are left
/// alone.
fn rename_runbooks(home: &Path) {
    let mut dirs: Vec<&str> = BUILTIN.iter().map(|b| b.0).collect();
    dirs.dedup();
    for d in dirs {
        let dir = home.join(".maestro").join(d);
        let mark = dir.join(".renamed");
        if mark.exists() || !dir.is_dir() {
            continue;
        }
        for (_, name, _) in BUILTIN.iter().filter(|b| b.0 == d) {
            let file = dir.join(name);
            if let Ok(text) = std::fs::read_to_string(&file) {
                if text.contains(OLD_NAME) {
                    let _ = std::fs::write(&file, text.replace(OLD_NAME, "런북"));
                }
            }
        }
        let _ = std::fs::write(&mark, "runbook\n");
    }
}

/// Line endings aside, the same text.
fn same_text(a: &str, b: &str) -> bool {
    a.replace("\r\n", "\n") == b.replace("\r\n", "\n")
}

/// The built-in file at `path` (folder, name, the text Maestro ships); none for a file the user added.
fn shipped(home: &Path, path: &str) -> Option<&'static (&'static str, &'static str, &'static str)> {
    let want = path.replace('\\', "/");
    BUILTIN.iter().find(|b| home.join(".maestro").join(b.0).join(b.1).to_string_lossy().replace('\\', "/").eq_ignore_ascii_case(&want))
}

/// A file on the 문서·런북 boards against what Maestro ships: `none` (one the user added), `same` or `edited`.
pub fn default_state(home: &Path, path: &str) -> &'static str {
    match shipped(home, path) {
        None => "none",
        Some(b) if std::fs::read_to_string(path).is_ok_and(|t| same_text(&t, b.2)) => "same",
        Some(_) => "edited",
    }
}

/// One built-in file back to the text Maestro ships. A file the user added has none, so it is refused.
pub fn restore_default(home: &Path, path: &str) -> Result<(), String> {
    let (d, name, body) = shipped(home, path).ok_or("Maestro 기본 파일이 아니에요")?;
    let dir = home.join(".maestro").join(d);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(name), body).map_err(|e| e.to_string())?;
    remember(&dir, name, body);
    Ok(())
}

/// Whether a built-in folder shows on the 런북 board (`books`) or the 문서 board (personas and categories).
fn on_board(dir: &str, books: bool) -> bool {
    (dir == "playbooks") == books
}

/// The built-in files of one board that differ from what Maestro ships or are gone, named the way the board names
/// their folder (페르소나, a category, 런북): `지침/agent-rules.md`.
pub fn defaults_off(home: &Path, books: bool) -> Vec<String> {
    BUILTIN
        .iter()
        .filter(|b| on_board(b.0, books) && !std::fs::read_to_string(home.join(".maestro").join(b.0).join(b.1)).is_ok_and(|t| same_text(&t, b.2)))
        .map(|b| {
            let folder = match b.0 {
                "personas" => "페르소나",
                "playbooks" => "런북",
                d => d.strip_prefix("library/").unwrap_or(d),
            };
            format!("{folder}/{}", b.1)
        })
        .collect()
}

/// Every built-in file of one board back to what Maestro ships: edited ones rewritten, deleted ones made again.
/// Files the user added stay as they are. Returns how many were written.
pub fn restore_defaults(home: &Path, books: bool) -> Result<usize, String> {
    let mut n = 0;
    for (d, name, body) in BUILTIN.iter().filter(|b| on_board(b.0, books)) {
        let dir = home.join(".maestro").join(d);
        let file = dir.join(name);
        if std::fs::read_to_string(&file).is_ok_and(|t| same_text(&t, body)) {
            continue;
        }
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(&file, body).map_err(|e| format!("{name}: {e}"))?;
        remember(&dir, name, body);
        n += 1;
    }
    Ok(n)
}

/// A new persona file `<name>.md` with a `# name` heading; returns its path.
pub fn create_persona(home: &Path, name: &str) -> Result<String, String> {
    let name = name.trim();
    let file: String = name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect();
    if file.trim_matches('-').is_empty() {
        return Err("페르소나 이름을 적어 주세요".into());
    }
    let path = personas_dir(home).join(format!("{file}.md"));
    if path.exists() {
        return Err("같은 이름의 페르소나가 있어요".into());
    }
    std::fs::create_dir_all(personas_dir(home)).map_err(|e| e.to_string())?;
    std::fs::write(&path, format!("# {name}\n\n이 역할이 무엇을 하고, 무엇을 하지 않는지 적어 주세요.\n")).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().replace('\\', "/"))
}

/// Playbooks: how the Maestro coordinates workers (`~/.maestro/playbooks`, the sidebar's 런북 tab). The
/// Maestro agent's brief lists them.
pub fn playbooks_dir(home: &Path) -> std::path::PathBuf {
    home.join(".maestro").join("playbooks")
}

/// The playbook folder, as one `LibFolder` (the 런북 tab).
pub fn playbooks(home: &Path) -> Vec<LibFolder> {
    let dir = playbooks_dir(home);
    vec![LibFolder { files: scan(&dir), folder: dir.to_string_lossy().replace('\\', "/"), persona: false }]
}

/// Shared documents, one folder per category (`~/.maestro/library/<category>`).
pub fn docs_dir(home: &Path) -> std::path::PathBuf {
    home.join(".maestro").join("library")
}

/// The persona folder first, then each category folder (by name).
pub fn all(home: &Path) -> Vec<LibFolder> {
    let personas = personas_dir(home);
    let mut cats: Vec<std::path::PathBuf> = std::fs::read_dir(docs_dir(home))
        .map(|r| r.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect())
        .unwrap_or_default();
    cats.sort();
    std::iter::once((personas, true))
        .chain(cats.into_iter().map(|c| (c, false)))
        .map(|(f, persona)| LibFolder { files: scan(&f), folder: f.to_string_lossy().replace('\\', "/"), persona })
        .collect()
}

/// `folder` must be the persona folder or a category folder: the only places the 문서 tab writes.
fn managed(home: &Path, folder: &str) -> Result<std::path::PathBuf, String> {
    let f = Path::new(folder);
    let same = |a: &Path, b: &Path| a.to_string_lossy().replace('\\', "/").eq_ignore_ascii_case(&b.to_string_lossy().replace('\\', "/"));
    if same(f, &personas_dir(home)) || same(f, &playbooks_dir(home)) || f.parent().is_some_and(|p| same(p, &docs_dir(home))) {
        Ok(f.to_path_buf())
    } else {
        Err("Maestro 문서·런북 폴더가 아니에요".into())
    }
}

/// A file name from what the user typed: letters, digits, `-`, `_` kept, the rest made `-`.
fn file_stem(name: &str) -> Result<String, String> {
    let stem: String = name.trim().chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect();
    if stem.trim_matches('-').is_empty() {
        return Err("이름을 적어 주세요".into());
    }
    Ok(stem)
}

/// A new `<name>.md` with a `# name` heading in a managed folder; returns its path.
pub fn create_doc(home: &Path, folder: &str, name: &str) -> Result<String, String> {
    let dir = managed(home, folder)?;
    let path = dir.join(format!("{}.md", file_stem(name)?));
    if path.exists() {
        return Err("같은 이름의 문서가 있어요".into());
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(&path, format!("# {}\n\n", name.trim())).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().replace('\\', "/"))
}

/// Copies Markdown files from elsewhere into a managed folder (a taken name gets `-2`, `-3`, …).
pub fn import_docs(home: &Path, folder: &str, files: &[String]) -> Result<(), String> {
    let dir = managed(home, folder)?;
    for f in files {
        let src = Path::new(f);
        let stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "doc".into());
        let mut to = dir.join(format!("{stem}.md"));
        for n in 2.. {
            if !to.exists() {
                break;
            }
            to = dir.join(format!("{stem}-{n}.md"));
        }
        std::fs::copy(src, &to).map_err(|e| format!("{f}: {e}"))?;
    }
    Ok(())
}

/// Deletes one file of a managed folder (the 문서 tab asks first).
pub fn delete_doc(home: &Path, path: &str) -> Result<(), String> {
    let p = Path::new(path);
    managed(home, &p.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default())?;
    std::fs::remove_file(p).map_err(|e| e.to_string())
}

/// A new category folder.
pub fn create_category(home: &Path, name: &str) -> Result<(), String> {
    let dir = docs_dir(home).join(name.trim().replace(['/', '\\'], "-"));
    if name.trim().is_empty() {
        return Err("분류 이름을 적어 주세요".into());
    }
    if dir.exists() {
        return Err("같은 이름의 분류가 있어요".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())
}

const MAX_DEPTH: usize = 4;
const MAX_FILES: usize = 500;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MdFile {
    pub path: String,
    pub title: String,
    /// the first line of body text (for the system tab's cards)
    pub summary: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibFolder {
    pub folder: String,
    pub files: Vec<MdFile>,
    /// the shared persona folder (always listed first, can't be unregistered)
    pub persona: bool,
}

/// First `# ` heading (frontmatter skipped), else the file name without `.md`.
pub fn title_of(path: &Path) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    heading(&text).unwrap_or_else(|| path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default())
}

fn heading(text: &str) -> Option<String> {
    let mut lines = text.lines();
    let mut in_front = false;
    let mut first = true;
    for l in lines.by_ref() {
        let t = l.trim();
        if first && t == "---" {
            in_front = true;
            first = false;
            continue;
        }
        first = false;
        if in_front {
            in_front = t != "---";
            continue;
        }
        if let Some(h) = t.strip_prefix("# ") {
            return Some(h.trim().to_string()).filter(|h| !h.is_empty());
        }
    }
    None
}

/// The first body line: frontmatter, headings, fences and list/quote marks skipped, at most 140 chars.
fn summary(text: &str) -> String {
    let body = text.strip_prefix("---").and_then(|r| r.split_once("\n---").map(|(_, b)| b)).unwrap_or(text);
    let line = body.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("```") && *l != "---").unwrap_or("");
    let line = line.trim_start_matches(['-', '*', '>', ' ']);
    line.chars().take(140).collect()
}

/// `.md` files under a folder (skipping .git / node_modules / target), titled.
pub fn scan(folder: &Path) -> Vec<MdFile> {
    let mut out = vec![];
    walk(folder, 0, &mut out);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<MdFile>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        if out.len() >= MAX_FILES {
            return;
        }
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if p.is_dir() {
            if depth < MAX_DEPTH && !name.starts_with('.') && name != "node_modules" && name != "target" {
                walk(&p, depth + 1, out);
            }
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("md")) {
            let text = std::fs::read_to_string(&p).unwrap_or_default();
            let title = heading(&text).unwrap_or_else(|| p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
            out.push(MdFile { path: p.to_string_lossy().replace('\\', "/"), title, summary: summary(&text) });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_skips_frontmatter_and_needs_h1() {
        assert_eq!(heading("---\ntitle: x\n---\n\n# 리뷰어\n본문").as_deref(), Some("리뷰어"));
        assert_eq!(heading("intro\n## not h1\n# Real").as_deref(), Some("Real"));
        assert_eq!(heading("no heading"), None);
    }

    #[test]
    fn personas_seed_create_and_detect() {
        let home = std::env::temp_dir().join(format!("maestro-persona-{}", std::process::id()));
        seed(&home);
        assert_eq!(scan(&personas_dir(&home)).len(), 10);
        // a deleted built-in stays deleted
        std::fs::remove_file(personas_dir(&home).join("tester.md")).unwrap();
        seed(&home);
        assert_eq!(scan(&personas_dir(&home)).len(), 9);
        let p = create_persona(&home, "보안 담당").unwrap();
        assert!(p.ends_with("/보안-담당.md"));
        assert_eq!(title_of(Path::new(&p)), "보안 담당");
        assert!(create_persona(&home, "보안 담당").is_err());
        assert!(create_persona(&home, "  ").is_err());
        assert!(is_persona(&home, &p));
        assert!(!is_persona(&home, "C:/elsewhere/x.md"));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn built_in_files_take_the_runbook_name_once() {
        let home = std::env::temp_dir().join(format!("maestro-rename-{}", std::process::id()));
        let books = home.join(".maestro/playbooks");
        std::fs::create_dir_all(&books).unwrap();
        // an older Maestro wrote default.md (and the user edited it); a file of the user's own also says the old name
        std::fs::write(books.join(".seeded"), "default.md\n").unwrap();
        std::fs::write(books.join("default.md"), format!("# 기본 지휘\n\n다른 {OLD_NAME}이 맞지 않을 때. 내가 고친 줄\n")).unwrap();
        std::fs::write(books.join("mine.md"), format!("# 내 것\n\n{OLD_NAME}\n")).unwrap();
        seed(&home);
        let d = std::fs::read_to_string(books.join("default.md")).unwrap();
        assert!(d.contains("다른 런북이 맞지 않을 때. 내가 고친 줄") && !d.contains(OLD_NAME));
        assert!(std::fs::read_to_string(books.join("mine.md")).unwrap().contains(OLD_NAME));
        // once: the old word written again later stays
        std::fs::write(books.join("default.md"), OLD_NAME).unwrap();
        seed(&home);
        assert_eq!(std::fs::read_to_string(books.join("default.md")).unwrap(), OLD_NAME);
        // the shipped files no longer say it
        assert!(BUILTIN.iter().all(|b| !b.2.contains(OLD_NAME)));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn built_in_files_go_back_to_what_ships() {
        let home = std::env::temp_dir().join(format!("maestro-restore-{}", std::process::id()));
        seed(&home);
        let books = playbooks_dir(&home);
        let def = books.join("default.md").to_string_lossy().replace('\\', "/");
        assert_eq!(default_state(&home, &def), "same");
        assert!(defaults_off(&home, true).is_empty() && defaults_off(&home, false).is_empty());
        // a built-in runbook edited, another deleted, one of the user's own added
        std::fs::write(&def, "내가 고침").unwrap();
        std::fs::remove_file(books.join("race.md")).unwrap();
        let mine = create_doc(&home, &books.to_string_lossy(), "내 런북").unwrap();
        assert_eq!(default_state(&home, &def), "edited");
        assert_eq!(default_state(&home, &mine), "none");
        assert!(restore_default(&home, &mine).is_err());
        assert_eq!(defaults_off(&home, true), ["런북/default.md", "런북/race.md"]);
        assert!(defaults_off(&home, false).is_empty());
        // one file
        restore_default(&home, &def).unwrap();
        assert_eq!(default_state(&home, &def), "same");
        // the board: the deleted one comes back, the user's own stays
        assert_eq!(restore_defaults(&home, true).unwrap(), 1);
        assert!(books.join("race.md").exists() && Path::new(&mine).exists());
        assert!(defaults_off(&home, true).is_empty());
        // the 문서 board: a persona and a category file
        std::fs::write(personas_dir(&home).join("tester.md"), "x").unwrap();
        std::fs::remove_file(docs_dir(&home).join("지침").join("git-rules.md")).unwrap();
        assert_eq!(defaults_off(&home, false), ["페르소나/tester.md", "지침/git-rules.md"]);
        assert_eq!(restore_defaults(&home, false).unwrap(), 2);
        assert!(defaults_off(&home, false).is_empty());
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn untouched_built_ins_take_the_new_text() {
        let home = std::env::temp_dir().join(format!("maestro-shipped-{}", std::process::id()));
        seed(&home);
        let books = playbooks_dir(&home);
        let at = |n: &str| books.join(n).to_string_lossy().replace('\\', "/");
        let read = |n: &str| std::fs::read_to_string(books.join(n)).unwrap();
        assert_eq!(written(&books).len(), BUILTIN.iter().filter(|b| b.0 == "playbooks").count());
        // an older Maestro wrote older texts: default.md untouched since, race.md then changed by the user
        let old = "# 기본 지휘\n\n옛 판\n";
        std::fs::write(books.join("default.md"), old).unwrap();
        std::fs::write(books.join("race.md"), "# 경쟁\n\n내가 고친 옛 판\n").unwrap();
        let mut marks = written(&books);
        marks.insert("default.md".into(), print(old));
        marks.insert("race.md".into(), print("# 경쟁\n\n옛 판\n"));
        // bug-hunt.md: written before fingerprints were kept
        marks.remove("bug-hunt.md");
        write_marks(&books, &marks);
        std::fs::write(books.join("bug-hunt.md"), "옛 판, 기록 없음").unwrap();
        // a deleted one
        std::fs::remove_file(books.join("split-build.md")).unwrap();
        seed(&home);
        assert_eq!(default_state(&home, &at("default.md")), "same");
        assert_eq!(read("race.md"), "# 경쟁\n\n내가 고친 옛 판\n");
        assert_eq!(read("bug-hunt.md"), "옛 판, 기록 없음");
        assert!(!books.join("split-build.md").exists());
        // 기본값으로 notes what it wrote, so the next revision reaches that file too
        restore_default(&home, &at("bug-hunt.md")).unwrap();
        assert_eq!(written(&books).get("bug-hunt.md"), Some(&print(&read("bug-hunt.md"))));
        // line endings don't make a file look changed
        assert_eq!(print("a\r\nb\r\n"), print("a\nb\n"));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn scans_markdown_with_titles() {
        let root = std::env::temp_dir().join(format!("maestro-lib-{}", std::process::id()));
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join("a.md"), "# 기획자").unwrap();
        std::fs::write(root.join("sub/b.MD"), "plain").unwrap();
        std::fs::write(root.join(".git/c.md"), "# hidden").unwrap();
        std::fs::write(root.join("x.txt"), "# not md").unwrap();
        let got = scan(&root);
        assert_eq!(got.iter().map(|f| f.title.as_str()).collect::<Vec<_>>(), ["기획자", "b"]);
        assert_eq!(got[1].summary, "plain");
        assert_eq!(summary("---\nname: x\n---\n\n# 리뷰어\n\n- 변경을 읽고 판단한다\n"), "변경을 읽고 판단한다");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn docs_live_only_in_managed_folders() {
        let home = std::env::temp_dir().join(format!("maestro-docs-{}", std::process::id()));
        seed(&home);
        let cats = all(&home);
        assert!(cats[0].persona);
        let guide = cats.iter().find(|c| c.folder.ends_with("/지침")).expect("seeded category").folder.clone();
        let made = create_doc(&home, &guide, "커밋 규칙").unwrap();
        assert!(made.ends_with("/커밋-규칙.md"));
        assert!(create_doc(&home, &guide, "커밋 규칙").is_err());
        // outside the library: refused
        assert!(create_doc(&home, &home.to_string_lossy(), "x").is_err());
        assert!(delete_doc(&home, &home.join("other.md").to_string_lossy()).is_err());
        create_category(&home, "체크리스트").unwrap();
        assert!(all(&home).iter().any(|c| c.folder.ends_with("/체크리스트")));
        delete_doc(&home, &made).unwrap();
        let _ = std::fs::remove_dir_all(&home);
    }
}
