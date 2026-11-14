//! Relations (docs/features/canvas/relations.md): the kinds of line a project's canvases draw, declared once and used
//! by every line of that kind (a relation is a class, a line its instance). Four are built in and do something (who
//! leads whom, prompts, references) or are the default for a drawn line; the project's own only mean something.
//! Kept per project in `relations.json`: the project's relations, and the colour picked for a built-in one.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

const VERSION: u64 = 1;

/// (id, name, pattern) of the built-in relations. Their names are reserved; only their colour can change.
pub const BUILTIN: &[(&str, &str, &str, &str)] = &[
    ("plain", "기본 관계", "solid", "구조 블록·영역 사이에 그냥 그은 선"),
    ("orchestrates", "지휘", "solid", "Lead → 워커: 그 워커를 이끌어요"),
    ("command_pipe", "지시", "solid", "프롬프트 블록 → 에이전트: 그 프롬프트를 보내요"),
    ("reference", "참조", "dashed", "문서 → 에이전트: 그 문서를 읽혀요"),
];
pub const PATTERNS: &[&str] = &["solid", "dashed", "dotted"];
pub const HEADS: &[&str] = &["one", "both", "none"];
pub const COLORS: &[&str] = &["blue", "green", "amber", "red", "violet", "teal"];

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Relation {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// the relation this one builds on: its look fills the fields left empty here, and the root's built-in behaviour
    /// (if any) is this one's too
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
}

/// What `relations.json` holds.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct RelFile {
    #[serde(default)]
    pub relations: Vec<Relation>,
    /// a built-in relation's colour, by its id
    #[serde(default)]
    pub builtin: BTreeMap<String, String>,
}

pub fn path(home: &std::path::Path, root: &str) -> PathBuf {
    crate::worktree::project_dir(home, root).join("relations.json")
}

pub fn load(home: &std::path::Path, root: &str) -> RelFile {
    crate::durable::read_or_default(&path(home, root), VERSION)
}

pub fn save(home: &std::path::Path, root: &str, file: &RelFile) -> Result<(), String> {
    check(file)?;
    crate::durable::write_json(&path(home, root), file, VERSION)
}

fn key(name: &str) -> String {
    name.trim().to_lowercase()
}

fn builtin_of(name: &str) -> Option<&'static str> {
    BUILTIN.iter().find(|b| key(b.1) == key(name) || b.0 == name.trim()).map(|b| b.0)
}

fn one_of(what: &str, v: &Option<String>, allowed: &[&str]) -> Result<(), String> {
    match v.as_deref() {
        Some(x) if !allowed.contains(&x) => Err(format!("{what} '{x}'은(는) 쓸 수 없어요 ({} 중 하나)", allowed.join(", "))),
        _ => Ok(()),
    }
}

/// Names filled, unique and not a built-in one's; pattern, head and colour from the fixed sets; `extends` naming a
/// relation that exists, without a loop.
pub fn check(f: &RelFile) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for r in &f.relations {
        if r.name.trim().is_empty() || r.id.trim().is_empty() {
            return Err("관계에 이름이 없어요".into());
        }
        if builtin_of(&r.name).is_some() {
            return Err(format!("'{}'은(는) 내장 관계 이름이라 쓸 수 없어요", r.name.trim()));
        }
        if !seen.insert(key(&r.name)) {
            return Err(format!("'{}' 관계가 둘이에요", r.name.trim()));
        }
        one_of("패턴", &r.pattern, PATTERNS)?;
        one_of("머리", &r.head, HEADS)?;
        one_of("색", &r.color, COLORS)?;
    }
    for (id, c) in &f.builtin {
        if !BUILTIN.iter().any(|b| b.0 == id) {
            return Err(format!("'{id}'은(는) 내장 관계가 아니에요"));
        }
        one_of("색", &Some(c.clone()), COLORS)?;
    }
    let exists = |id: &str| BUILTIN.iter().any(|b| b.0 == id) || f.relations.iter().any(|r| r.id == id);
    for r in &f.relations {
        let mut at = r.extends.clone();
        let mut steps = 0;
        while let Some(p) = at {
            if !exists(&p) {
                return Err(format!("'{}'이(가) 잇는 관계 '{p}'이(가) 없어요", r.name.trim()));
            }
            steps += 1;
            if p == r.id || steps > 32 {
                return Err(format!("'{}'의 상속이 자기 자신으로 돌아와요", r.name.trim()));
            }
            at = f.relations.iter().find(|x| x.id == p).and_then(|x| x.extends.clone());
        }
    }
    Ok(())
}

/// A relation id by name or id: a built-in one, or one of the project's.
pub fn find(f: &RelFile, name: &str) -> Option<String> {
    builtin_of(name).map(String::from).or_else(|| f.relations.iter().find(|r| key(&r.name) == key(name) || r.id == name.trim()).map(|r| r.id.clone()))
}

fn new_id(f: &RelFile) -> String {
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    (0..).map(|n| format!("r_{ms}_{n}")).find(|id| !f.relations.iter().any(|r| &r.id == id)).unwrap()
}

/// What a `define` sets (empty fields leave the old value as it is).
#[derive(Default, Clone, Debug)]
pub struct Def {
    pub description: Option<String>,
    pub pattern: Option<String>,
    pub head: Option<String>,
    pub color: Option<String>,
    pub extends: Option<String>,
}

/// Declares a relation, or changes the one of that name. A built-in one only takes a colour. Returns its id.
pub fn define(f: &mut RelFile, name: &str, d: Def) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("--name 이 필요해요".into());
    }
    if let Some(b) = builtin_of(name) {
        if d.description.is_some() || d.pattern.is_some() || d.head.is_some() || d.extends.is_some() {
            return Err(format!("'{name}'은(는) 내장 관계라 색만 바꿀 수 있어요"));
        }
        if let Some(c) = d.color {
            let mut next = f.clone();
            next.builtin.insert(b.into(), c);
            check(&next)?;
            *f = next;
        }
        return Ok(b.into());
    }
    let extends = match d.extends.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(p) => Some(find(f, p).ok_or(format!("잇는 관계 '{p}'이(가) 없어요"))?),
        None => None,
    };
    let mut next = f.clone();
    let at = next.relations.iter().position(|r| key(&r.name) == key(name));
    let id = at.map(|i| next.relations[i].id.clone()).unwrap_or_else(|| new_id(f));
    let mut r = at.map(|i| next.relations[i].clone()).unwrap_or(Relation { id: id.clone(), name: name.into(), ..Default::default() });
    let set = |old: &mut Option<String>, new: Option<String>| {
        if let Some(v) = new.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()) {
            *old = Some(v);
        }
    };
    set(&mut r.description, d.description);
    set(&mut r.pattern, d.pattern);
    set(&mut r.head, d.head);
    set(&mut r.color, d.color);
    if extends.is_some() {
        r.extends = extends;
    }
    match at {
        Some(i) => next.relations[i] = r,
        None => next.relations.push(r),
    }
    check(&next)?;
    *f = next;
    Ok(id)
}

/// The id for a relation named in a drawing: one that exists, or a new one in the default look (declaring is never
/// required first).
pub fn resolve_or_declare(f: &mut RelFile, name: &str) -> Result<String, String> {
    match find(f, name) {
        Some(id) => Ok(id),
        None => define(f, name, Def::default()),
    }
}

/// The built-in relations (with a picked colour) and the project's, for `maestro relation list`.
pub fn listing(f: &RelFile) -> serde_json::Value {
    let builtin: Vec<_> = BUILTIN
        .iter()
        .map(|(id, name, pattern, about)| serde_json::json!({ "id": id, "name": name, "pattern": pattern, "head": "one", "color": f.builtin.get(*id), "description": about, "builtin": true }))
        .collect();
    serde_json::json!({ "builtin": builtin, "relations": f.relations })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn define_declares_updates_and_guards_builtin_names() {
        let mut f = RelFile::default();
        let a = define(&mut f, "영향", Def { pattern: Some("dashed".into()), ..Default::default() }).unwrap();
        assert_eq!(f.relations.len(), 1);
        // same name (any case, spaces) changes it in place, empty fields keep what was there
        let b = define(&mut f, " 영향 ", Def { color: Some("teal".into()), ..Default::default() }).unwrap();
        assert_eq!(a, b);
        assert_eq!(f.relations[0].pattern.as_deref(), Some("dashed"));
        assert_eq!(f.relations[0].color.as_deref(), Some("teal"));
        // a built-in name: only its colour
        assert_eq!(define(&mut f, "참조", Def { color: Some("violet".into()), ..Default::default() }).unwrap(), "reference");
        assert_eq!(f.builtin.get("reference").map(String::as_str), Some("violet"));
        assert!(define(&mut f, "지휘", Def { pattern: Some("dotted".into()), ..Default::default() }).is_err());
        // values outside the fixed sets
        assert!(define(&mut f, "x", Def { pattern: Some("wavy".into()), ..Default::default() }).is_err());
        assert!(define(&mut f, "x", Def { color: Some("#ff0000".into()), ..Default::default() }).is_err());
        assert_eq!(f.relations.len(), 1, "a refused define changes nothing");
    }

    #[test]
    fn extends_resolves_by_name_and_refuses_loops() {
        let mut f = RelFile::default();
        let d = define(&mut f, "설계 참조", Def { extends: Some("참조".into()), color: Some("amber".into()), ..Default::default() }).unwrap();
        assert_eq!(f.relations[0].extends.as_deref(), Some("reference"));
        let e = define(&mut f, "세부 설계 참조", Def { extends: Some("설계 참조".into()), ..Default::default() }).unwrap();
        assert!(define(&mut f, "없는 부모", Def { extends: Some("없음".into()), ..Default::default() }).is_err());
        // a loop written into the file by hand is refused on save
        let mut bad = f.clone();
        bad.relations.iter_mut().find(|r| r.id == d).unwrap().extends = Some(e);
        assert!(check(&bad).is_err());
    }

    #[test]
    fn drawing_names_resolve_or_declare() {
        let mut f = RelFile::default();
        assert_eq!(resolve_or_declare(&mut f, "기본 관계").unwrap(), "plain");
        let id = resolve_or_declare(&mut f, "포함").unwrap();
        assert_eq!(resolve_or_declare(&mut f, "포함").unwrap(), id);
        assert_eq!(f.relations.len(), 1);
    }

    #[test]
    fn duplicate_or_reserved_names_fail_check() {
        let r = |id: &str, name: &str| Relation { id: id.into(), name: name.into(), ..Default::default() };
        assert!(check(&RelFile { relations: vec![r("a", "x"), r("b", "X")], ..Default::default() }).is_err());
        assert!(check(&RelFile { relations: vec![r("a", "지시")], ..Default::default() }).is_err());
        assert!(check(&RelFile { relations: vec![r("a", "x")], ..Default::default() }).is_ok());
    }
}
