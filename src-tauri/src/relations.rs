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
pub const NAME_MAX: usize = 40;
pub const ABOUT_MAX: usize = 1000;

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

fn save(home: &std::path::Path, root: &str, file: &RelFile) -> Result<(), String> {
    check(file)?;
    crate::durable::write_json(&path(home, root), file, VERSION)
}

/// Every change to a project's relations goes through here, one at a time: read the file as it is now, change it,
/// check it, write it. Two agents (or an agent and the app) declaring at once both land; neither overwrites the other
/// with an older copy. A file from a newer Maestro is never written over.
static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
pub fn update<T>(home: &std::path::Path, root: &str, change: impl FnOnce(&mut RelFile) -> Result<T, String>) -> Result<(T, RelFile), String> {
    let _held = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut f: RelFile = crate::durable::read_for_write(&path(home, root), VERSION)?;
    let out = change(&mut f)?;
    save(home, root, &f)?;
    Ok((out, f))
}

/// Puts a project relation as the app's editor has it (by id): a new one, or a change to one. A built-in id is refused.
pub fn upsert(f: &mut RelFile, r: Relation) -> Result<(), String> {
    if BUILTIN.iter().any(|b| b.0.eq_ignore_ascii_case(r.id.trim())) {
        return Err("내장 관계는 색만 바꿀 수 있어요".into());
    }
    let mut next = f.clone();
    let r = Relation { name: r.name.trim().to_string(), ..r };
    match next.relations.iter().position(|x| x.id == r.id) {
        Some(i) => next.relations[i] = r,
        None => next.relations.push(r),
    }
    check(&next)?;
    *f = next;
    Ok(())
}

/// A built-in relation's colour (None: back to the line colour).
pub fn set_builtin_color(f: &mut RelFile, id: &str, color: Option<String>) -> Result<(), String> {
    let mut next = f.clone();
    match color.filter(|c| !c.is_empty()) {
        Some(c) => next.builtin.insert(id.to_string(), c),
        None => next.builtin.remove(id),
    };
    check(&next)?;
    *f = next;
    Ok(())
}

fn key(name: &str) -> String {
    name.trim().to_lowercase()
}

fn builtin_of(name: &str) -> Option<&'static str> {
    BUILTIN.iter().find(|b| key(b.1) == key(name) || b.0.eq_ignore_ascii_case(name.trim())).map(|b| b.0)
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
    let mut ids = std::collections::HashSet::new();
    for r in &f.relations {
        if r.name.trim().is_empty() || r.id.trim().is_empty() {
            return Err("관계에 이름이 없어요".into());
        }
        // an id is the line's link to its relation: one per relation, never a built-in one's (it would take its place)
        if BUILTIN.iter().any(|b| b.0.eq_ignore_ascii_case(r.id.trim())) || !ids.insert(r.id.trim().to_lowercase()) {
            return Err(format!("'{}' 관계의 id({})가 겹쳐요", r.name.trim(), r.id));
        }
        if builtin_of(&r.name).is_some() {
            return Err(format!("'{}'은(는) 내장 관계 이름이라 쓸 수 없어요", r.name.trim()));
        }
        // a name sits in a toolbar, a legend row and on a line: short; the meaning is the description's job
        if r.name.trim().chars().count() > NAME_MAX || r.description.as_deref().map_or(0, |d| d.chars().count()) > ABOUT_MAX {
            return Err(format!("'{}': 이름은 {NAME_MAX}자, 뜻은 {ABOUT_MAX}자까지예요", r.name.trim().chars().take(20).collect::<String>()));
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

/// The built-in behaviour a relation has through its parents (`orchestrates`, `command_pipe`, `reference`), if any.
pub fn behavior_of(f: &RelFile, id: &str) -> Option<&'static str> {
    let mut at = Some(id.to_string());
    for _ in 0..33 {
        let cur = at?;
        if let Some(b) = BUILTIN.iter().find(|b| b.0 == cur && b.0 != "plain") {
            return Some(b.0);
        }
        at = f.relations.iter().find(|r| r.id == cur).and_then(|r| r.extends.clone());
    }
    None
}

/// Takes a project relation away. Lines drawn as it fall back to their default (the canvas does that when it can't
/// find the id); relations that built on it now build on its parent (so a child of 참조 stays one). A built-in one
/// can't go. Returns its id.
pub fn remove(f: &mut RelFile, name: &str) -> Result<String, String> {
    if builtin_of(name).is_some() {
        return Err(format!("'{}'은(는) 내장 관계라 지울 수 없어요", name.trim()));
    }
    let id = f.relations.iter().find(|r| key(&r.name) == key(name) || r.id == name.trim()).map(|r| r.id.clone()).ok_or(format!("'{}' 관계가 없어요 (`maestro relation list`)", name.trim()))?;
    let parent = f.relations.iter().find(|r| r.id == id).and_then(|r| r.extends.clone());
    f.relations.retain(|r| r.id != id);
    for r in f.relations.iter_mut().filter(|r| r.extends.as_deref() == Some(id.as_str())) {
        r.extends = parent.clone();
    }
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
    fn remove_frees_children_and_keeps_builtins() {
        let mut f = RelFile::default();
        let a = define(&mut f, "원인", Def::default()).unwrap();
        define(&mut f, "가정", Def { extends: Some("원인".into()), ..Default::default() }).unwrap();
        assert!(remove(&mut f, "참조").is_err());
        assert!(remove(&mut f, "없음").is_err());
        assert_eq!(remove(&mut f, " 원인 ").unwrap(), a);
        assert_eq!(f.relations.len(), 1);
        assert_eq!(f.relations[0].extends, None, "a child of a root relation has no parent left");
        assert!(check(&f).is_ok());
        // a child of a child of 참조 keeps the behaviour: it now builds on its grandparent
        let mut g = RelFile::default();
        define(&mut g, "설계 참조", Def { extends: Some("참조".into()), ..Default::default() }).unwrap();
        let c = define(&mut g, "세부", Def { extends: Some("설계 참조".into()), ..Default::default() }).unwrap();
        remove(&mut g, "설계 참조").unwrap();
        assert_eq!(g.relations[0].extends.as_deref(), Some("reference"));
        assert_eq!(behavior_of(&g, &c), Some("reference"));
    }

    #[test]
    fn ids_are_unique_and_never_builtin() {
        let r = |id: &str, name: &str| Relation { id: id.into(), name: name.into(), ..Default::default() };
        assert!(check(&RelFile { relations: vec![r("reference", "x")], ..Default::default() }).is_err(), "a built-in id");
        assert!(check(&RelFile { relations: vec![r("Plain", "x")], ..Default::default() }).is_err(), "any case");
        assert!(check(&RelFile { relations: vec![r("a", "x"), r("a", "y")], ..Default::default() }).is_err(), "two of one id");
        assert!(define(&mut RelFile::default(), "Reference", Def { color: Some("red".into()), ..Default::default() }).unwrap() == "reference", "the built-in's id names it too");
        let mut f = RelFile::default();
        assert!(upsert(&mut f, r("orchestrates", "지휘 2")).is_err());
        upsert(&mut f, r("r1", " 원인 ")).unwrap();
        assert_eq!(f.relations[0].name, "원인");
        upsert(&mut f, Relation { color: Some("teal".into()), ..r("r1", "원인") }).unwrap();
        assert_eq!(f.relations.len(), 1, "same id: changed in place");
        assert!(upsert(&mut f, r("r2", "원인")).is_err(), "a second 원인");
        set_builtin_color(&mut f, "reference", Some("violet".into())).unwrap();
        set_builtin_color(&mut f, "reference", None).unwrap();
        assert!(f.builtin.is_empty());
    }

    #[test]
    fn behaviour_comes_from_the_builtin_root() {
        let mut f = RelFile::default();
        let a = define(&mut f, "설계 참조", Def { extends: Some("참조".into()), ..Default::default() }).unwrap();
        let b = define(&mut f, "세부", Def { extends: Some("설계 참조".into()), ..Default::default() }).unwrap();
        let c = define(&mut f, "원인", Def::default()).unwrap();
        assert_eq!(behavior_of(&f, &a), Some("reference"));
        assert_eq!(behavior_of(&f, &b), Some("reference"));
        assert_eq!(behavior_of(&f, &c), None);
        assert_eq!(behavior_of(&f, "plain"), None);
        assert_eq!(behavior_of(&f, "orchestrates"), Some("orchestrates"));
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
        assert!(check(&RelFile { relations: vec![r("a", &"가".repeat(41))], ..Default::default() }).is_err(), "too long a name");
    }
}
