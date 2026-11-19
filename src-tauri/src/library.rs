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
/// ships. A file that already exists (edited by the user) is never overwritten.
pub fn seed(home: &Path) {
    for (dir, name, body) in BUILTIN {
        let dir = home.join(".maestro").join(dir);
        let marker = dir.join(".seeded");
        let done = std::fs::read_to_string(&marker).unwrap_or_default();
        if done.lines().any(|l| l == *name) || std::fs::create_dir_all(&dir).is_err() {
            continue;
        }
        let file = dir.join(name);
        if !file.exists() {
            let _ = std::fs::write(&file, body);
        }
        let _ = std::fs::write(&marker, format!("{done}{name}\n"));
    }
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

/// Playbooks: how the Maestro coordinates workers (`~/.maestro/playbooks`, the sidebar's 플레이북 tab). The
/// Maestro agent's brief lists them.
pub fn playbooks_dir(home: &Path) -> std::path::PathBuf {
    home.join(".maestro").join("playbooks")
}

/// The playbook folder, as one `LibFolder` (the 플레이북 tab).
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
        Err("Maestro 문서·플레이북 폴더가 아니에요".into())
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
