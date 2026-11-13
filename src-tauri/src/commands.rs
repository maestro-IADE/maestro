//! The chat composer's `/` menu for one agent: its built-ins from the catalog, then the custom commands and
//! skills from where that agent reads them. Claude Code: `.claude/commands/**/*.md` and
//! `.claude/skills/*/SKILL.md`, user and project.
//! shortcut: plugin-provided commands aren't listed; they still work when typed.

use crate::catalog::{AgentDef, Slash};
use serde::Serialize;
use std::path::Path;

#[derive(Serialize, Debug, PartialEq)]
pub struct SlashCommand {
    pub name: String,
    pub description: String,
    pub source: &'static str,
    /// shown next to it, e.g. that it only opens in the CLI view
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<&'static str>,
}

/// `description:` from YAML frontmatter, else the first non-empty body line.
fn describe(md: &str) -> String {
    let mut lines = md.lines();
    if md.starts_with("---") {
        lines.next();
        for l in lines.by_ref() {
            if l.trim() == "---" {
                break;
            }
            if let Some(d) = l.strip_prefix("description:") {
                return d.trim().trim_matches('"').to_string();
            }
        }
    }
    lines.map(str::trim).find(|l| !l.is_empty()).unwrap_or("").trim_start_matches('#').trim().to_string()
}

/// commands/a.md → "a", commands/git/pr.md → "git:pr" (Claude's namespacing)
fn walk_commands(dir: &Path, prefix: &str, source: &'static str, out: &mut Vec<SlashCommand>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        let stem = p.file_stem().unwrap_or_default().to_string_lossy().to_string();
        if p.is_dir() {
            walk_commands(&p, &format!("{prefix}{stem}:"), source, out);
        } else if p.extension().is_some_and(|x| x == "md") {
            let description = describe(&std::fs::read_to_string(&p).unwrap_or_default());
            out.push(SlashCommand { name: format!("{prefix}{stem}"), description, source, note: None });
        }
    }
}

fn skills(dir: &Path, source: &'static str, out: &mut Vec<SlashCommand>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let Ok(md) = std::fs::read_to_string(e.path().join("SKILL.md")) else { continue };
        out.push(SlashCommand { name: e.file_name().to_string_lossy().into(), description: describe(&md), source, note: None });
    }
}

fn claude_custom(home: &Path, worktree: &Path) -> Vec<SlashCommand> {
    let mut out = vec![];
    for (base, source) in [(worktree.join(".claude"), "프로젝트"), (home.join(".claude"), "사용자")] {
        walk_commands(&base.join("commands"), "", source, &mut out);
        skills(&base.join("skills"), source, &mut out);
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|b, a| a.name == b.name); // project entries come first and win
    out
}

/// Built-ins first (catalog order), then the agent's custom commands by name.
pub fn list(def: &AgentDef, home: &Path, worktree: &Path) -> Vec<SlashCommand> {
    let mut out: Vec<SlashCommand> = def.slash_builtins.iter().map(|&(name, d, note)| SlashCommand { name: name.into(), description: d.into(), source: "기본", note }).collect();
    match def.slash {
        Slash::Claude => out.extend(claude_custom(home, worktree)),
        Slash::None => {}
    }
    out
}

/// Under a runtime that reports its own commands (ACP): what it doesn't take is marked for the CLI view (`/model`
/// stays: Maestro sets the model itself), and what only it reports is added at the end.
pub fn with_runtime(mut list: Vec<SlashCommand>, runtime: Option<Vec<(String, String)>>) -> Vec<SlashCommand> {
    let Some(reported) = runtime else { return list };
    for c in list.iter_mut() {
        if c.name != "model" && c.note.is_none() && !reported.iter().any(|(n, _)| *n == c.name) {
            c.note = Some(CHAT_CANT);
        }
    }
    for (name, description) in reported {
        if !list.iter().any(|c| c.name == name) {
            list.push(SlashCommand { name, description, source: "에이전트", note: None });
        }
    }
    list
}

/// The chat (ACP runtime) can't run it.
const CHAT_CANT: &str = "채팅에서는 안 돼요 · 터미널에서";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_runtime_list_marks_and_adds_commands() {
        let c = |n: &str, note: Option<&'static str>| SlashCommand { name: n.into(), description: String::new(), source: "기본", note };
        let base = vec![c("compact", None), c("model", None), c("config", Some("터미널에서 열려요")), c("cost", None)];
        assert_eq!(with_runtime(base.iter().map(|x| c(&x.name, x.note)).collect(), None), base);
        let got = with_runtime(base, Some(vec![("compact".into(), "Compact".into()), ("mcp-tool".into(), "From MCP".into())]));
        let notes: Vec<(&str, Option<&str>)> = got.iter().map(|x| (x.name.as_str(), x.note)).collect();
        assert_eq!(notes, [("compact", None), ("model", None), ("config", Some("터미널에서 열려요")), ("cost", Some(CHAT_CANT)), ("mcp-tool", None)]);
        assert_eq!((got[4].source, got[4].description.as_str()), ("에이전트", "From MCP"));
    }

    #[test]
    fn describes_from_frontmatter_or_first_line() {
        assert_eq!(describe("---\nname: x\ndescription: \"Ship it\"\n---\nbody"), "Ship it");
        assert_eq!(describe("---\nname: x\n---\n\n# Title here\nmore"), "Title here");
        assert_eq!(describe("plain line\nnext"), "plain line");
    }

    #[test]
    fn lists_namespaced_commands_and_skills() {
        let root = std::env::temp_dir().join(format!("maestro-cmds-{}", std::process::id()));
        let wt = root.join("wt");
        std::fs::create_dir_all(wt.join(".claude/commands/git")).unwrap();
        std::fs::create_dir_all(wt.join(".claude/skills/deploy")).unwrap();
        std::fs::write(wt.join(".claude/commands/git/pr.md"), "Open a PR").unwrap();
        std::fs::write(wt.join(".claude/skills/deploy/SKILL.md"), "---\ndescription: Deploy\n---").unwrap();
        let got = claude_custom(&root.join("nohome"), &wt);
        assert_eq!(got.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["deploy", "git:pr"]);
        assert_eq!(got[1].description, "Open a PR");
        // per agent: Claude gets its built-ins and these, Antigravity neither
        let claude = list(crate::catalog::get("claude-code").unwrap(), &root.join("nohome"), &wt);
        assert_eq!(claude[0].name, "add-dir");
        assert!(claude.iter().any(|c| c.name == "git:pr" && c.source == "프로젝트"));
        assert!(list(crate::catalog::get("antigravity").unwrap(), &root.join("nohome"), &wt).is_empty());
        let _ = std::fs::remove_dir_all(root);
    }
}
