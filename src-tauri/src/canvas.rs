//! Reading a canvas for its agent (`maestro canvas`), checking what an agent draws (`maestro draw`'s `lint`) and a
//! worker's done report (the `## 보고` block at the end of its answer). Pure functions over JSON: the canvas file as
//! the frontend saves it (`nodes`, `edges`), the drawing spec, the relations file. docs/features/canvas/canvas-reading.md

use crate::relations::RelFile;
use serde::Serialize;
use serde_json::{json, Map, Value};

// ---------- a worker's done report ----------

/// The report block a worker puts at the start of the answer that ends its turn (shared notes, confirmed by the
/// playbook rules on 2026-11-16): `## 보고`, then `키: 값` lines up to the first blank line or heading.
#[derive(Debug, Serialize, Clone, PartialEq, Default)]
pub struct Report {
    pub status: String,
    /// the worktree's HEAD as a short SHA; None for `없음` or no line
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    pub verified: Vec<Check>,
    pub unverified: Vec<String>,
    pub changed: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining: Option<String>,
}

/// One `검증:` line. `weak`: why it doesn't count as a pass (no result, zero tests, a test run's exit code alone).
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct Check {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weak: Option<&'static str>,
}

pub const STATUSES: &[&str] = &["완료", "부분", "막힘"];
const KEYS: &[&str] = &["상태", "커밋", "검증", "미검증", "변경", "남은 일"];

/// The first `## 보고` block of an answer, or None when there is none or its `상태` isn't exactly 완료 / 부분 / 막힘
/// (never guessed from the prose around it). Unknown keys and lines that aren't `키: 값` are skipped; `검증` and
/// `미검증` gather, the other keys take their first value.
pub fn parse_report(text: &str) -> Option<Report> {
    let mut lines = text.lines().skip_while(|l| l.trim() != "## 보고");
    lines.next()?;
    let mut r = Report::default();
    let (mut status, mut commit, mut changed, mut remaining) = (None, None, None, None);
    for l in lines {
        let l = l.trim();
        if l.is_empty() || l.starts_with('#') {
            break;
        }
        let Some((k, v)) = l.split_once(':') else { continue };
        let (k, v) = (k.trim(), v.trim());
        if !KEYS.contains(&k) {
            continue;
        }
        let none = v == "없음";
        match k {
            "상태" => {
                status.get_or_insert(v.to_string());
            }
            "커밋" => {
                commit.get_or_insert((!none && !v.is_empty()).then(|| v.to_string()));
            }
            "검증" if !none && !v.is_empty() => r.verified.push(Check { text: v.into(), weak: weak_check(v) }),
            "미검증" if !v.is_empty() => r.unverified.push(v.into()),
            "변경" => {
                changed.get_or_insert(if none { vec![] } else { v.split(',').map(str::trim).filter(|f| !f.is_empty()).map(String::from).collect() });
            }
            "남은 일" => {
                remaining.get_or_insert((!none && !v.is_empty()).then(|| v.to_string()));
            }
            _ => {}
        }
    }
    r.status = status.filter(|s| STATUSES.contains(&s.as_str()))?;
    r.commit = commit.flatten();
    r.changed = changed.unwrap_or_default();
    r.remaining = remaining.flatten();
    Some(r)
}

/// `pat` in `hay` as a count of zero: no digit right before it (`0 passed`) or right after it (`통과 0`).
fn zero_count(hay: &str, pat: &str) -> bool {
    hay.match_indices(pat).any(|(i, _)| {
        let before = hay[..i].chars().next_back().is_some_and(|c| c.is_ascii_digit());
        let after = hay[i + pat.len()..].chars().next().is_some_and(|c| c.is_ascii_digit());
        if pat.starts_with('0') { !before } else { !after }
    })
}

/// Why a verification doesn't count as a pass (confirmed report rules): no result after the first `→`, zero tests
/// run, or a test run answered with an exit code alone. A build's `→ 종료 0` is a result.
pub fn weak_check(v: &str) -> Option<&'static str> {
    let Some((cmd, result)) = v.split_once('→') else {
        return Some("결과 없음");
    };
    let (cmd, result) = (cmd.trim().to_lowercase(), result.trim().to_lowercase());
    if result.is_empty() {
        return Some("결과 없음");
    }
    if ["통과 0", "passed 0", "tests 0"].iter().any(|p| zero_count(&result, p)) || ["0 passed", "0 tests", "0 test", "0개 통과", "0개 실행"].iter().any(|p| zero_count(&result, p)) || result.contains("테스트 0개") {
        return Some("테스트 0개");
    }
    let test_run = ["test", "테스트", "jest", "spec"].iter().any(|t| cmd.contains(t));
    let without_exit = ["exit code 0", "exit 0", "종료 코드 0", "종료 0"].iter().fold(result.clone(), |s, p| s.replace(p, ""));
    if test_run && !without_exit.chars().any(|c| c.is_ascii_digit()) && without_exit.trim_matches(|c: char| !c.is_alphanumeric()).is_empty() {
        return Some("종료 코드만");
    }
    None
}

// ---------- reading a canvas ----------

/// Block sizes when the file has none (src/nodes: GRID 24, struct block 10×3, area 18×12 cells).
const GRID: f64 = 24.0;
fn default_size(kind: &str) -> (f64, f64) {
    match kind {
        "zone" => (18.0 * GRID, 12.0 * GRID),
        "struct_block" => (10.0 * GRID, 3.0 * GRID),
        "worktree_card" => (14.0 * GRID, 11.0 * GRID),
        "question_block" => (12.0 * GRID, 6.0 * GRID),
        "view_block" => (16.0 * GRID, 12.0 * GRID),
        _ => (10.0 * GRID, 6.0 * GRID),
    }
}

/// One node of the canvas as an agent sees it: its kind, name and box on the canvas (absolute).
#[derive(Debug, Clone)]
pub struct Node {
    pub id: String,
    pub kind: &'static str,
    pub name: String,
    pub parent: Option<String>,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub raw: Value,
}

/// The nodes of a saved canvas, with absolute boxes. `name_of` names an agent from its worktree path.
pub fn nodes_of(saved: &Value, name_of: &dyn Fn(&str) -> Option<String>) -> Vec<Node> {
    let raw = saved["nodes"].as_array().cloned().unwrap_or_default();
    let by_id = |id: &str| raw.iter().find(|n| n["id"] == id);
    raw.iter()
        .filter_map(|n| {
            let ty = n["type"].as_str()?;
            let kind = match ty {
                "struct_block" => "block",
                "zone" => "area",
                "markdown_block" => "doc",
                "prompt_input_primitive" => "prompt",
                "pty_terminal_node" => "agent",
                "worktree_card" => "worker",
                "question_block" => "question",
                "view_block" => "view",
                _ => return None,
            };
            let d = &n["data"];
            let name = match kind {
                "agent" | "worker" => {
                    let p = d["worktreePath"].as_str().unwrap_or("");
                    name_of(p).unwrap_or_else(|| p.rsplit(['/', '\\']).next().unwrap_or(p).to_string())
                }
                "prompt" => d["name"].as_str().or(d["title"].as_str()).unwrap_or("프롬프트").to_string(),
                // a view block (view-block.md): the title it was shown under, else its file's name
                "view" => {
                    let p = d["path"].as_str().unwrap_or("");
                    d["title"].as_str().map(str::trim).filter(|t| !t.is_empty()).unwrap_or_else(|| p.rsplit(['/', '\\']).next().unwrap_or(p)).to_string()
                }
                _ => d["title"].as_str().unwrap_or("").trim().to_string(),
            };
            // absolute position: the node's plus every enclosing area's
            let (mut x, mut y) = (0.0, 0.0);
            let mut cur = Some(n);
            for _ in 0..50 {
                let Some(c) = cur else { break };
                x += c["position"]["x"].as_f64().unwrap_or(0.0);
                y += c["position"]["y"].as_f64().unwrap_or(0.0);
                cur = c["parentId"].as_str().and_then(by_id);
            }
            let (dw, dh) = default_size(ty);
            let size = |k: &str, d: f64| n[k].as_f64().or(n["measured"][k].as_f64()).or(n["style"][k].as_f64()).unwrap_or(d);
            // a folded area (zones.md) is a small chip on screen: here it has the size it opens back to (`data.full`),
            // so what it holds still lies inside its box
            let full = |k: &str| if ty == "zone" && d["folded"] == true { d["full"][k].as_f64() } else { None };
            Some(Node {
                id: n["id"].as_str()?.to_string(),
                kind,
                name,
                parent: n["parentId"].as_str().map(String::from),
                x,
                y,
                w: full("width").unwrap_or_else(|| size("width", dw)),
                h: full("height").unwrap_or_else(|| size("height", dh)),
                raw: n.clone(),
            })
        })
        .collect()
}

fn round(v: f64) -> i64 {
    v.round() as i64
}

/// The relation name a line shows (relations.md: no relation = the line's default).
fn relation_name(rel: &RelFile, kind: &str, id: Option<&str>) -> String {
    let builtin = |id: &str| crate::relations::BUILTIN.iter().find(|b| b.0 == id).map(|b| b.1.to_string());
    if let Some(id) = id.filter(|i| !i.is_empty()) {
        if let Some(r) = rel.relations.iter().find(|r| r.id == id) {
            return r.name.clone();
        }
        if let Some(n) = builtin(id) {
            return n;
        }
    }
    builtin(if kind == "arrow" { "plain" } else { kind }).unwrap_or_else(|| kind.to_string())
}

/// What `maestro canvas` answers: every area, block, other block (agents, documents, prompts) and line of the saved
/// canvas, with stable ids and boxes; `brief` keeps only a body's first line.
pub fn read(saved: &Value, rel: &RelFile, name_of: &dyn Fn(&str) -> Option<String>, brief: bool) -> Value {
    let nodes = nodes_of(saved, name_of);
    let name = |id: &str| nodes.iter().find(|n| n.id == id).map(|n| n.name.clone()).unwrap_or_else(|| id.to_string());
    let area_of = |n: &Node| n.parent.as_deref().map(name);
    let bx = |n: &Node| json!({ "x": round(n.x), "y": round(n.y), "w": round(n.w), "h": round(n.h) });
    let body = |n: &Node| -> Option<String> {
        let b = n.raw["data"]["body"].as_str().map(str::trim).filter(|b| !b.is_empty())?;
        Some(if brief { b.lines().next().unwrap_or("").to_string() } else { b.to_string() })
    };
    // an area with steps or states (zones.md): its mode and its list, as (id, the name shown)
    let phases_of = |id: &str| -> Option<(String, Vec<(String, String)>)> {
        let d = &nodes.iter().find(|n| n.id == id && n.kind == "area")?.raw["data"];
        let mode = d["mode"].as_str().filter(|m| *m == "step" || *m == "state")?;
        let list: Vec<(String, String)> = d["phases"]
            .as_array()?
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let name = p["name"].as_str().unwrap_or("").trim();
                // a state with no name is called by its place, as on the canvas
                (p["id"].as_str().unwrap_or("").to_string(), if name.is_empty() && mode == "state" { format!("상태 {}", i + 1) } else { name.to_string() })
            })
            .collect();
        (!list.is_empty()).then(|| (mode.to_string(), list))
    };
    // where something limited to some of its area's steps or states exists: ("in_steps", [2, 3]) by number, or
    // ("in_states", ["장애"]) by name; None when it is in all of them (or its area has none)
    let limited = |area: Option<&str>, when: &Value| -> Option<(&'static str, Value)> {
        let (mode, list) = phases_of(area?)?;
        let ids: Vec<&str> = when.as_array()?.iter().filter_map(|v| v.as_str()).collect();
        let mine: Vec<usize> = list.iter().enumerate().filter(|(_, (id, _))| ids.contains(&id.as_str())).map(|(i, _)| i).collect();
        if mine.is_empty() {
            return None;
        }
        Some(if mode == "step" { ("in_steps", json!(mine.iter().map(|i| i + 1).collect::<Vec<_>>())) } else { ("in_states", json!(mine.iter().map(|i| list[*i].1.clone()).collect::<Vec<_>>())) })
    };
    let with_when = |mut v: Value, n: &Node| -> Value {
        if let Some((key, list)) = limited(n.parent.as_deref(), &n.raw["data"]["when"]) {
            v[key] = list;
        }
        v
    };
    let mut areas = vec![];
    let mut blocks = vec![];
    let mut others = vec![];
    let mut questions = vec![];
    for n in &nodes {
        match n.kind {
            // `maestro ask`: the question, its choices and the user's answer (null until answered)
            "question" => {
                let d = &n.raw["data"];
                let mut q = json!({ "id": d["qid"], "title": n.name, "choices": d["choices"].as_array().cloned().unwrap_or_default(), "multi": d["multi"].as_bool().unwrap_or(false), "answer": d["answer"].clone(), "area": area_of(n), "box": bx(n) });
                // a question block the user drew on the canvas (to try it out): not asked by the agent, and its
                // answer was never sent to it
                if d["mine"] == true {
                    q["by"] = json!("user");
                }
                // one of several asked together (`maestro ask --file`): the set's id, which `erase --question` takes too
                if let Some(s) = d["set"].as_str() {
                    q["set"] = json!(s);
                }
                questions.push(with_when(q, n));
            }
            "area" => {
                let holds: Vec<String> = nodes.iter().filter(|c| c.parent.as_deref() == Some(&n.id)).map(|c| c.name.clone()).collect();
                let mut a = json!({ "id": n.id, "title": n.name, "in": area_of(n), "holds": holds, "box": bx(n) });
                // the user folded it to a chip: what it holds is hidden on screen (still listed here, in its open box)
                if n.raw["data"]["folded"] == true {
                    a["folded"] = json!(true);
                }
                if let Some(c) = n.raw["data"]["color"].as_str() {
                    a["color"] = json!(c);
                }
                // the agents' area: the app's own, around the Lead's block and the worker cards (not something drawn)
                if n.raw["data"]["team"] == true {
                    a["team"] = json!(true);
                }
                // a stack (zones.md "묶음"): what it holds sits in one column and reads as one thing, top to bottom
                if n.raw["data"]["stack"] == true {
                    let mut kids: Vec<&Node> = nodes.iter().filter(|c| c.parent.as_deref() == Some(&n.id)).collect();
                    kids.sort_by(|x, y| x.raw["position"]["y"].as_f64().unwrap_or(0.0).total_cmp(&y.raw["position"]["y"].as_f64().unwrap_or(0.0)));
                    a["stack"] = json!(true);
                    a["holds"] = json!(kids.iter().map(|c| c.name.clone()).collect::<Vec<_>>());
                }
                // its steps (by number, `steps` gives their names) or states (by name), and the one showing now: what
                // it holds may be limited to some of them (`in_steps` / `in_states` on that block)
                if let Some((mode, list)) = phases_of(&n.id) {
                    let at = n.raw["data"]["at"].as_str().and_then(|a| list.iter().position(|(id, _)| id == a)).unwrap_or(0);
                    let names: Vec<&str> = list.iter().map(|(_, name)| name.as_str()).collect();
                    a["mode"] = json!(mode);
                    if mode == "step" {
                        a["steps"] = json!(names);
                        a["current"] = json!(at + 1);
                    } else {
                        a["states"] = json!(names);
                        a["current"] = json!(names[at]);
                    }
                }
                areas.push(with_when(a, n));
            }
            "block" => {
                let mut b = json!({ "id": n.id, "title": n.name, "body": body(n), "area": area_of(n), "box": bx(n) });
                // its border colour, when it has one (blocks and areas that belong together wear the same)
                if let Some(c) = n.raw["data"]["color"].as_str() {
                    b["color"] = json!(c);
                }
                blocks.push(with_when(b, n));
            }
            kind => {
                let mut o = json!({ "id": n.id, "kind": kind, "name": n.name, "area": area_of(n), "box": bx(n) });
                if let Some(p) = n.raw["data"]["path"].as_str().or(n.raw["data"]["worktreePath"].as_str()) {
                    o["path"] = json!(p);
                }
                others.push(with_when(o, n));
            }
        }
    }
    let mut arrows = vec![];
    let mut links = vec![];
    for e in saved["edges"].as_array().into_iter().flatten() {
        let kind = e["data"]["kind"].as_str().unwrap_or("arrow");
        let (s, t) = (e["source"].as_str().unwrap_or(""), e["target"].as_str().unwrap_or(""));
        let note = &saved["lines"][e["id"].as_str().unwrap_or("")];
        let pick = |k: &str| e["data"][k].as_str().or(note[k].as_str()).map(str::trim).filter(|v| !v.is_empty()).map(String::from);
        let mut line = json!({ "id": e["id"], "from": name(s), "to": name(t), "relation": relation_name(rel, kind, pick("relation").as_deref()) });
        if let Some(l) = pick("label") {
            line["label"] = json!(l);
        }
        if let Some(b) = pick("body") {
            line["body"] = json!(if brief { b.lines().next().unwrap_or("").to_string() } else { b });
        }
        // a line limited to some steps or states of the area both its ends are in
        if let Some((key, list)) = limited(e["data"]["whenZone"].as_str(), &e["data"]["when"]) {
            line[key] = list;
        }
        if kind == "arrow" {
            arrows.push(line);
        } else {
            line["kind"] = json!(kind);
            links.push(line);
        }
    }
    json!({ "areas": areas, "blocks": blocks, "others": others, "questions": questions, "arrows": arrows, "links": links })
}

// ---------- lint ----------

/// One finding about a drawing or a canvas. `warn`: likely not what was meant; `info`: reads worse than it could.
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct Lint {
    pub level: &'static str,
    pub code: &'static str,
    pub message: String,
}

fn warn(code: &'static str, message: String) -> Lint {
    Lint { level: "warn", code, message }
}
fn info(code: &'static str, message: String) -> Lint {
    Lint { level: "info", code, message }
}

pub const LAYOUTS: &[&str] = &["grid", "flow", "tree"];
pub const LABEL_MAX: usize = 24;

/// Fields each part of a drawing takes, and the fix for a few that agents reach for.
const FIELDS: &[(&str, &[&str])] = &[
    ("", &["areas", "blocks", "arrows", "relations", "layout", "say"]),
    ("areas", &["title", "in", "layout", "color", "steps", "states", "step", "state"]),
    ("blocks", &["title", "body", "area", "color", "step", "state"]),
    ("arrows", &["from", "to", "label", "as", "body", "step", "state"]),
    ("relations", &["name", "description", "pattern", "head", "color", "extends"]),
];
fn suggest(field: &str) -> Option<&'static str> {
    Some(match field {
        "meaning" | "about" | "desc" | "explanation" => "description",
        "name" => "title",
        "parent" | "zone" | "group" => "area 또는 in",
        "relation" | "kind" | "type" => "as",
        "text" | "content" | "note" | "notes" => "body",
        "source" => "from",
        "target" => "to",
        _ => return None,
    })
}

fn unknown_fields(part: &str, v: &Value, at: &str, out: &mut Vec<Lint>) {
    let Some(obj) = v.as_object() else { return };
    let known = FIELDS.iter().find(|f| f.0 == part).map_or(&[][..], |f| f.1);
    for k in obj.keys() {
        if known.contains(&k.as_str()) {
            continue;
        }
        // `description` is a relation's field, a block's notes are its `body`
        let hint = match (part, k.as_str()) {
            ("blocks" | "areas", "description") => Some("body"),
            _ => suggest(k),
        };
        let msg = match hint {
            Some(h) => format!("{at}의 '{k}'은(는) 모르는 칸이라 무시됐어요 ('{h}'을(를) 쓰려던 건가요?)"),
            None => format!("{at}의 '{k}'은(는) 모르는 칸이라 무시됐어요 (쓸 수 있는 칸: {})", known.join(", ")),
        };
        out.push(warn("unknown-field", msg));
    }
}

/// What `maestro draw` says back about a drawing (`spec` as written; `canvas` as last saved; `rel` the relations
/// before this drawing declared any): fields it ignored, names that clash, arrows it skips, blocks left alone,
/// lines that don't read as a sentence, relations it declared without a meaning.
pub fn lint_drawing(spec: &Value, canvas: &[Node], canvas_arrows: &[(String, String)], rel: &RelFile) -> Vec<Lint> {
    let mut out = vec![];
    unknown_fields("", spec, "그림", &mut out);
    let list = |k: &str| spec[k].as_array().cloned().unwrap_or_default();
    let (areas, blocks, arrows, relations) = (list("areas"), list("blocks"), list("arrows"), list("relations"));
    let title = |v: &Value| v["title"].as_str().map(str::trim).unwrap_or("").to_string();
    for (k, items) in [("areas", &areas), ("blocks", &blocks), ("arrows", &arrows), ("relations", &relations)] {
        for (i, v) in items.iter().enumerate() {
            let at = match k {
                "areas" | "blocks" if !title(v).is_empty() => format!("'{}'", title(v)),
                "relations" => format!("관계 '{}'", v["name"].as_str().unwrap_or("")),
                _ => format!("{k}[{i}]"),
            };
            unknown_fields(k, v, &at, &mut out);
        }
    }

    // a name used twice: arrows can only go to one of them (the block)
    let names: Vec<(String, &str)> = areas.iter().map(|a| (title(a), "영역")).chain(blocks.iter().map(|b| (title(b), "블록"))).collect();
    let mut seen: Vec<&str> = vec![];
    for (n, _) in &names {
        if seen.contains(&n.as_str()) {
            continue;
        }
        seen.push(n);
        let kinds: Vec<&str> = names.iter().filter(|(m, _)| m == n).map(|(_, k)| *k).collect();
        if kinds.len() > 1 {
            out.push(warn("same-name", format!("'{n}'이(가) {}에 함께 쓰였어요. 화살표는 그중 하나에만 이어져요: 이름을 다르게 지어 주세요", kinds.join("·"))));
        }
    }
    // the same name on the canvas as another kind: `draw` updates the canvas one
    for (n, k) in &names {
        let other = if *k == "영역" { "block" } else { "area" };
        if canvas.iter().any(|c| c.kind == other && &c.name == n) {
            out.push(warn("same-name", format!("'{n}'은(는) 캔버스에 이미 {}(으)로 있어요. 새로 생기지 않고 그쪽이 쓰여요", if other == "block" { "블록" } else { "영역" })));
        }
    }

    // arrows the canvas skips or merges
    let mut pairs: Vec<(String, String)> = vec![];
    for a in &arrows {
        let (f, t) = (a["from"].as_str().unwrap_or("").trim().to_string(), a["to"].as_str().unwrap_or("").trim().to_string());
        if f == t {
            out.push(warn("self-arrow", format!("'{f}' → '{f}' 화살표는 자기 자신으로 가서 그려지지 않아요")));
            continue;
        }
        if pairs.contains(&(f.clone(), t.clone())) {
            out.push(warn("duplicate-arrow", format!("'{f}' → '{t}' 화살표가 두 번 있어요. 같은 방향의 선은 하나라 뒤의 것이 앞의 것을 고쳐 써요")));
        }
        pairs.push((f.clone(), t.clone()));
        let label = a["label"].as_str().map(str::trim).unwrap_or("");
        let named = a["as"].as_str().is_some_and(|s| !s.trim().is_empty());
        if label.is_empty() && !named {
            out.push(info("no-label", format!("'{f}' → '{t}'에 라벨도 관계(as)도 없어요. 선이 문장으로 읽히게 짧은 동사구를 붙여 보세요")));
        }
        if label.chars().count() > LABEL_MAX {
            out.push(info("long-label", format!("'{f}' → '{t}'의 라벨이 {LABEL_MAX}자를 넘어요. 짧은 동사구로 줄이고 긴 설명은 body에 써요")));
        }
    }

    // blocks no line touches (in this drawing or on the canvas)
    let touched = |n: &str| pairs.iter().chain(canvas_arrows).any(|(f, t)| f == n || t == n);
    let lone: Vec<String> = blocks.iter().map(title).filter(|n| !n.is_empty() && !touched(n)).collect();
    if !lone.is_empty() && !pairs.is_empty() {
        out.push(info("orphan", format!("어느 선에도 이어지지 않은 블록: {}", lone.join(", "))));
    }
    // areas that hold nothing
    for a in &areas {
        let n = title(a);
        let holds = blocks.iter().any(|b| b["area"].as_str().map(str::trim) == Some(&n)) || areas.iter().any(|x| x["in"].as_str().map(str::trim) == Some(&n));
        let on_canvas = canvas.iter().find(|c| c.kind == "area" && c.name == n).is_some_and(|z| canvas.iter().any(|c| c.parent.as_deref() == Some(&z.id)));
        if !holds && !on_canvas && !n.is_empty() {
            out.push(warn("empty-area", format!("영역 '{n}'에 든 것이 없어요 (블록의 area나 영역의 in으로 넣어요)")));
        }
    }

    // relations: new names get the default look and no meaning
    let declared = |n: &str| crate::relations::find(rel, n).is_some() || relations.iter().any(|r| r["name"].as_str().map(str::trim) == Some(n));
    let mut told: Vec<String> = vec![];
    for a in &arrows {
        if let Some(n) = a["as"].as_str().map(str::trim).filter(|n| !n.is_empty()) {
            if !declared(n) && !told.iter().any(|t| t == n) {
                told.push(n.to_string());
            }
        }
    }
    if !told.is_empty() {
        out.push(warn("undeclared-relation", format!("선언하지 않은 관계를 기본 모양·뜻 없음으로 선언했어요: {}. relations에 description(과 모양)을 함께 적어 주세요", told.join(", "))));
    }
    for r in &relations {
        let n = r["name"].as_str().unwrap_or("").trim();
        let has_about = r["description"].as_str().is_some_and(|d| !d.trim().is_empty());
        let existing = crate::relations::find(rel, n).and_then(|id| rel.relations.iter().find(|x| x.id == id)).is_some_and(|x| x.description.is_some());
        if !n.is_empty() && !has_about && !existing {
            out.push(info("relation-no-meaning", format!("관계 '{n}'에 description(뜻)이 없어요. 사람이 선에 마우스를 올리면 이 뜻이 보여요")));
        }
    }
    out
}

/// What `maestro canvas` says about the canvas as saved: blocks on top of each other, names used twice, blocks no
/// line touches, areas that hold nothing, lines that don't read as a sentence.
pub fn lint_canvas(nodes: &[Node], saved: &Value) -> Vec<Lint> {
    let mut out = vec![];
    let ancestor = |a: &Node, b: &Node| {
        let mut p = b.parent.clone();
        for _ in 0..50 {
            match p {
                Some(ref id) if *id == a.id => return true,
                Some(ref id) => p = nodes.iter().find(|n| &n.id == id).and_then(|n| n.parent.clone()),
                None => return false,
            }
        }
        false
    };
    // overlap: two non-area blocks, or two areas neither of which holds the other, sharing more than a sliver
    let solid: Vec<&Node> = nodes.iter().collect();
    let mut overlaps = vec![];
    for (i, a) in solid.iter().enumerate() {
        for b in &solid[i + 1..] {
            let areas = (a.kind == "area") as u8 + (b.kind == "area") as u8;
            if areas == 1 || ancestor(a, b) || ancestor(b, a) {
                continue; // a block in an area's box is how areas look; a nested one is inside on purpose
            }
            let ow = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
            let oh = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
            if ow > GRID / 2.0 && oh > GRID / 2.0 {
                overlaps.push(format!("'{}' ↔ '{}'", a.name, b.name));
            }
        }
    }
    if !overlaps.is_empty() {
        out.push(warn("overlap", format!("겹쳐 있어요: {}", overlaps.join(", "))));
    }
    // a block whose middle is in an area it doesn't belong to (dropped on it, not in it)
    for b in nodes.iter().filter(|n| n.kind != "area") {
        let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
        if let Some(z) = nodes.iter().find(|z| z.kind == "area" && !ancestor(z, b) && cx > z.x && cx < z.x + z.w && cy > z.y && cy < z.y + z.h && !ancestor(b, z)) {
            out.push(info("loose-in-area", format!("'{}'이(가) 영역 '{}' 위에 있지만 그 영역에 속하지 않아요", b.name, z.name)));
        }
    }
    // names used twice by drawn blocks and areas (`draw`/`erase` take the first)
    let drawn: Vec<&Node> = nodes.iter().filter(|n| n.kind == "block" || n.kind == "area").collect();
    let mut seen: Vec<&str> = vec![];
    for n in &drawn {
        if !seen.contains(&n.name.as_str()) && drawn.iter().filter(|m| m.name == n.name).count() > 1 {
            out.push(warn("same-name", format!("'{}' 이름이 캔버스에 둘 이상이에요. 이름으로 가리키면 첫 것만 쓰여요", n.name)));
        }
        seen.push(&n.name);
    }
    let edges = saved["edges"].as_array().cloned().unwrap_or_default();
    let touched = |id: &str| edges.iter().any(|e| e["source"] == id || e["target"] == id);
    let lone: Vec<String> = nodes.iter().filter(|n| n.kind == "block" && !touched(&n.id)).map(|n| n.name.clone()).collect();
    if !lone.is_empty() && edges.iter().any(|e| e["data"]["kind"] == "arrow") {
        out.push(info("orphan", format!("어느 선에도 이어지지 않은 블록: {}", lone.join(", "))));
    }
    for z in nodes.iter().filter(|n| n.kind == "area") {
        if !nodes.iter().any(|n| n.parent.as_deref() == Some(&z.id)) {
            out.push(warn("empty-area", format!("영역 '{}'에 든 것이 없어요", z.name)));
        }
    }
    let name = |id: &str| nodes.iter().find(|n| n.id == id).map(|n| n.name.clone()).unwrap_or_default();
    let bare: Vec<String> = edges
        .iter()
        .filter(|e| e["data"]["kind"] == "arrow" && e["data"]["label"].as_str().map_or(true, |l| l.trim().is_empty()) && e["data"]["relation"].as_str().map_or(true, |r| r.is_empty() || r == "plain"))
        .map(|e| format!("'{}' → '{}'", name(e["source"].as_str().unwrap_or("")), name(e["target"].as_str().unwrap_or(""))))
        .collect();
    if !bare.is_empty() {
        out.push(info("no-label", format!("라벨도 관계도 없는 선: {}", bare.join(", "))));
    }
    out
}

/// The drawn arrows of a saved canvas as (from title, to title), for `lint_drawing`'s "no line touches it".
pub fn arrows_of(nodes: &[Node], saved: &Value) -> Vec<(String, String)> {
    let name = |id: &str| nodes.iter().find(|n| n.id == id).map(|n| n.name.clone()).unwrap_or_default();
    saved["edges"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|e| (name(e["source"].as_str().unwrap_or("")), name(e["target"].as_str().unwrap_or(""))))
        .collect()
}

/// The longest line an agent says with what it puts on the canvas: it is shown as one short caption.
pub const SAY_MAX: usize = 120;

/// The line an agent says with what it puts on the canvas (`--say`, or `"say"` in a drawing; canvas-controls.md
/// "안내"): what this is, in one short line, shown on the canvas as it comes in. None when empty; more than one line or
/// too long comes back as an error that says how long it may be.
pub fn check_say(say: &str) -> Result<Option<String>, String> {
    let s = say.trim();
    if s.is_empty() {
        return Ok(None);
    }
    if s.contains('\n') {
        return Err("say는 한 줄이에요 (줄바꿈 없이 짧게: 이 그림이 무엇인지)".into());
    }
    let n = s.chars().count();
    if n > SAY_MAX {
        return Err(format!("say가 {n}자예요. {SAY_MAX}자 안으로 줄이세요 (캔버스에 한 줄로 잠깐 보이는 설명이에요)"));
    }
    Ok(Some(s.to_string()))
}

/// Whether the canvas has something of one of these kinds by that name (`maestro focus`, `maestro tidy --area`).
pub fn has_named(canvas: &[Node], name: &str, kinds: &[&str]) -> bool {
    canvas.iter().any(|n| kinds.contains(&n.kind) && n.name == name.trim())
}

/// A layout name from a drawing (`layout` at the top or on an area): one of `LAYOUTS`, checked before drawing.
pub fn check_layouts(spec: &Value) -> Result<(), String> {
    let bad = |v: &Value, at: &str| -> Result<(), String> {
        match v["layout"].as_str() {
            None => match &v["layout"] {
                Value::Null => Ok(()),
                _ => Err(format!("{at}의 layout은 글자여야 해요 ({})", LAYOUTS.join(" | "))),
            },
            Some(l) if LAYOUTS.contains(&l.trim()) => Ok(()),
            Some(l) => Err(format!("{at}의 layout '{l}'을(를) 몰라요 ({} 중 하나)", LAYOUTS.join(" | "))),
        }
    };
    bad(spec, "그림")?;
    for a in spec["areas"].as_array().into_iter().flatten() {
        bad(a, &format!("영역 '{}'", a["title"].as_str().unwrap_or("")))?;
    }
    Ok(())
}

/// The most steps or states an area takes.
pub const PHASES_MAX: usize = 20;

/// An area's steps or states as a drawing gives them (zones.md): ("step", names) from `steps` (a count, or a list
/// of names, "" for a step with none), ("state", names) from `states`. Never both: an area in an area gives both.
pub fn drawn_phases(area: &Value) -> Result<Option<(&'static str, Vec<String>)>, String> {
    let at = format!("영역 '{}'", area["title"].as_str().unwrap_or(""));
    let (steps, states) = (&area["steps"], &area["states"]);
    if !steps.is_null() && !states.is_null() {
        return Err(format!("{at}에 steps와 states를 함께 줄 수 없어요 (둘 다 필요하면 영역 안에 영역을 넣어요)"));
    }
    let names = |v: &Value, key: &str| -> Result<Vec<String>, String> {
        let list = v.as_array().ok_or(format!("{at}의 {key}는 이름 목록이어야 해요"))?;
        let out: Vec<String> = list.iter().map(|x| x.as_str().map(|s| s.trim().to_string()).ok_or(format!("{at}의 {key}에는 글자만 넣어요"))).collect::<Result<_, _>>()?;
        if out.is_empty() || out.len() > PHASES_MAX {
            return Err(format!("{at}의 {key}는 1개에서 {PHASES_MAX}개까지예요 ({}개)", out.len()));
        }
        Ok(out)
    };
    if let Some(n) = steps.as_u64() {
        if n == 0 || n as usize > PHASES_MAX {
            return Err(format!("{at}의 steps는 1에서 {PHASES_MAX}까지예요 ({n})"));
        }
        return Ok(Some(("step", vec![String::new(); n as usize])));
    }
    if !steps.is_null() {
        return Ok(Some(("step", names(steps, "steps")?)));
    }
    if !states.is_null() {
        let list = names(states, "states")?;
        if let Some(dup) = list.iter().enumerate().find_map(|(i, s)| (s.is_empty() || list[..i].contains(s)).then_some(s)) {
            return Err(if dup.is_empty() { format!("{at}의 states에 빈 이름이 있어요") } else { format!("{at}의 states에 '{dup}'이(가) 두 번 있어요") });
        }
        return Ok(Some(("state", list)));
    }
    Ok(None)
}

/// An area's steps or states on the canvas: (mode, the names shown), a state with no name called by its place.
fn canvas_phases(nodes: &[Node], title: &str) -> Option<(String, Vec<String>)> {
    let d = &nodes.iter().find(|n| n.kind == "area" && n.name == title && n.raw["data"]["mode"].is_string())?.raw["data"];
    let mode = d["mode"].as_str()?.to_string();
    let list: Vec<String> = d["phases"]
        .as_array()?
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let name = p["name"].as_str().unwrap_or("").trim();
            if name.is_empty() && mode == "state" { format!("상태 {}", i + 1) } else { name.to_string() }
        })
        .collect();
    (!list.is_empty()).then_some((mode, list))
}

/// One value or a list of them, as a list.
fn one_or_many(v: &Value) -> Vec<Value> {
    match v {
        Value::Null => vec![],
        Value::Array(a) => a.clone(),
        other => vec![other.clone()],
    }
}

/// Steps and states in a drawing, checked before drawing: each area's `steps` / `states` (`drawn_phases`), and each
/// block's or inner area's `step` (numbers from 1) or `state` (names), which must be ones its area has: in this
/// drawing, or on the canvas already. An arrow's are only checked for their kind (the canvas reads them against the
/// area both its ends are in).
pub fn check_phases(spec: &Value, canvas: &[Node]) -> Result<(), String> {
    let areas: Vec<&Value> = spec["areas"].as_array().into_iter().flatten().collect();
    for a in &areas {
        drawn_phases(a)?;
    }
    let of = |area: &str| -> Option<(String, Vec<String>)> {
        // what this drawing gives the area comes first (it is what the area will have), then what the canvas has
        areas.iter().find(|a| a["title"].as_str().map(str::trim) == Some(area)).and_then(|a| drawn_phases(a).ok().flatten()).map(|(m, l)| (m.to_string(), l)).or_else(|| canvas_phases(canvas, area))
    };
    for (key, parent, what) in [("areas", "in", "영역"), ("blocks", "area", "블록")] {
        for x in spec[key].as_array().into_iter().flatten() {
            let (step, state) = (one_or_many(&x["step"]), one_or_many(&x["state"]));
            if step.is_empty() && state.is_empty() {
                continue;
            }
            let at = format!("{what} '{}'", x["title"].as_str().unwrap_or(""));
            if !step.is_empty() && !state.is_empty() {
                return Err(format!("{at}에 step과 state를 함께 줄 수 없어요"));
            }
            let (word, flag) = if step.is_empty() { ("state", "states") } else { ("step", "steps") };
            let area = x[parent].as_str().map(str::trim).filter(|a| !a.is_empty()).ok_or(format!("{at}의 {word}은(는) 영역 안에 있을 때만 써요 ({parent}로 영역을 주세요)"))?;
            let (mode, list) = of(area).ok_or(format!("{at}의 {word}: 영역 '{area}'에 {flag}가 없어요 (그 영역에 {flag}를 주세요)"))?;
            if mode != word {
                return Err(format!("{at}의 {word}: 영역 '{area}'은(는) {} 영역이에요", if mode == "step" { "단계(steps)" } else { "상태(states)" }));
            }
            for v in step {
                match v.as_u64() {
                    Some(n) if n >= 1 && n as usize <= list.len() => {}
                    _ => return Err(format!("{at}의 step {v}: 영역 '{area}'의 단계는 1에서 {}까지예요", list.len())),
                }
            }
            for v in state {
                match v.as_str().map(str::trim) {
                    Some(s) if list.iter().any(|l| l == s) => {}
                    _ => return Err(format!("{at}의 state {v}: 영역 '{area}'의 상태는 {} 중 하나예요", list.join(" | "))),
                }
            }
        }
    }
    for a in spec["arrows"].as_array().into_iter().flatten() {
        let at = format!("화살표 '{}' → '{}'", a["from"].as_str().unwrap_or(""), a["to"].as_str().unwrap_or(""));
        if one_or_many(&a["step"]).iter().any(|v| v.as_u64().map_or(true, |n| n == 0)) {
            return Err(format!("{at}의 step은 1부터 세는 숫자예요"));
        }
        if one_or_many(&a["state"]).iter().any(|v| !v.is_string()) {
            return Err(format!("{at}의 state는 상태 이름이에요"));
        }
    }
    Ok(())
}

/// `maestro step` / `maestro state`: moving an area on the caller's canvas to another step or state, checked against
/// the canvas as last saved. Answers (what the canvas is told, what the caller is told): the canvas goes by the move
/// itself (`next` and `prev` count from where it is then), the caller gets where that lands from what was saved.
pub fn phase_move(route: &str, area: &str, to: &str, canvas: &[Node]) -> Result<(Value, Value), String> {
    let (area, to) = (area.trim(), to.trim());
    if area.is_empty() {
        return Err(format!("--area 로 영역 이름을 주세요 (`maestro {route} --area 이름 --to …`)"));
    }
    if !canvas.iter().any(|n| n.kind == "area" && n.name == area) {
        return Err(format!("'{area}' 영역이 내 캔버스에 없어요 (방금 그렸다면 잠시 뒤 다시 해 보세요)"));
    }
    let (mode, list) = canvas_phases(canvas, area).ok_or(format!("'{area}' 영역에는 단계도 상태도 없어요 (draw에서 steps나 states를 주세요)"))?;
    if mode != route {
        return Err(format!("'{area}'은(는) {} 영역이에요: `maestro {mode} --area …`를 쓰세요", if mode == "step" { "단계" } else { "상태" }));
    }
    let z = canvas.iter().find(|n| n.kind == "area" && n.name == area && n.raw["data"]["mode"].is_string()).unwrap();
    let now = z.raw["data"]["at"].as_str().and_then(|a| z.raw["data"]["phases"].as_array()?.iter().position(|p| p["id"] == a)).unwrap_or(0);
    let last = list.len() - 1;
    if route == "step" {
        let (told, lands) = match to {
            "" | "next" => (json!("next"), (now + 1).min(last)),
            "prev" => (json!("prev"), now.saturating_sub(1)),
            n => match n.parse::<usize>() {
                Ok(n) if n >= 1 && n <= list.len() => (json!(n), n - 1),
                _ => return Err(format!("--to 는 1에서 {} 사이의 숫자이거나 next, prev예요 ({to})", list.len())),
            },
        };
        return Ok((json!({ "area": area, "to": told }), json!({ "area": area, "mode": "step", "current": lands + 1, "of": list.len(), "name": list[lands] })));
    }
    if !list.iter().any(|s| s == to) {
        return Err(format!("--to 는 이 영역의 상태 이름이에요: {} ({})", list.join(" | "), if to.is_empty() { "없음" } else { to }));
    }
    Ok((json!({ "area": area, "to": to }), json!({ "area": area, "mode": "state", "current": to, "states": list })))
}

/// A border colour from a drawing (`color` on an area or a block): one of the line colours (`relations::COLORS`),
/// checked before drawing, like a layout.
pub fn check_colors(spec: &Value) -> Result<(), String> {
    let colors = crate::relations::COLORS;
    for (key, what) in [("areas", "영역"), ("blocks", "블록")] {
        for x in spec[key].as_array().into_iter().flatten() {
            let at = format!("{what} '{}'", x["title"].as_str().unwrap_or(""));
            match &x["color"] {
                Value::Null => {}
                Value::String(c) if colors.contains(&c.trim()) => {}
                Value::String(c) => return Err(format!("{at}의 color '{c}'을(를) 몰라요 ({} 중 하나)", colors.join(" | "))),
                _ => return Err(format!("{at}의 color는 글자여야 해요 ({})", colors.join(" | "))),
            }
        }
    }
    Ok(())
}

/// What a view block can show (docs/features/canvas/view-block.md), by the file's extension: an image the webview
/// draws itself, or a page shown in a frame that can't reach the app. None: not something to show.
pub fn view_kind(path: &str) -> Option<&'static str> {
    let ext = std::path::Path::new(path).extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" => Some("image"),
        "html" | "htm" => Some("html"),
        _ => None,
    }
}

/// The media type of a file a view block shows as an image.
pub fn view_mime(path: &str) -> &'static str {
    match std::path::Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => "text/html",
    }
}

/// The biggest file a view block shows.
pub const VIEW_MAX: u64 = 10 << 20;
pub const VIEW_KINDS: &str = "png, jpg, jpeg, gif, webp, svg, html";

/// A drawing's stacks (zones.md "묶음"): `stack` on an area is true or false, and a stack has no steps or states (it
/// is one thing: all of it shows, or none).
pub fn check_stacks(spec: &Value) -> Result<(), String> {
    for a in spec["areas"].as_array().into_iter().flatten() {
        let at = format!("영역 '{}'", a["title"].as_str().unwrap_or(""));
        match &a["stack"] {
            Value::Null | Value::Bool(false) => {}
            Value::Bool(true) if a["steps"].is_null() && a["states"].is_null() => {}
            Value::Bool(true) => return Err(format!("{at}: 묶음(stack)에는 steps나 states를 줄 수 없어요 (단계가 필요하면 묶음을 단계가 있는 영역 안에 넣어요)")),
            _ => return Err(format!("{at}의 stack은 true나 false예요")),
        }
    }
    Ok(())
}

/// A stack drawn with nothing in it would be gone the moment it is drawn (a stack is only ever what it holds): each
/// stack of a drawing has to hold a block or area of the same drawing, or be an area the canvas already has with
/// something in it.
pub fn check_stacks_hold(spec: &Value, canvas: &[Node]) -> Result<(), String> {
    for a in spec["areas"].as_array().into_iter().flatten().filter(|a| a["stack"] == true) {
        let t = a["title"].as_str().unwrap_or("").trim();
        let named = |list: &str, key: &str| spec[list].as_array().into_iter().flatten().any(|x| x[key].as_str().map(str::trim) == Some(t));
        let there = canvas.iter().find(|n| n.kind == "area" && n.name == t).is_some_and(|z| canvas.iter().any(|c| c.parent.as_deref() == Some(z.id.as_str())));
        if !named("blocks", "area") && !named("areas", "in") && !there {
            return Err(format!("영역 '{t}': 묶음(stack)에 든 것이 없어요. 빈 묶음은 캔버스에서 바로 사라져요. 같은 파일의 블록에 \"area\": \"{t}\"를 줘서 묶음에 넣으세요"));
        }
    }
    Ok(())
}

/// Lints as JSON for an answer (empty list when there's nothing to say).
pub fn lint_json(l: &[Lint]) -> Value {
    Value::Array(l.iter().map(|x| serde_json::to_value(x).unwrap_or(Value::Object(Map::new()))).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_parses_the_confirmed_block() {
        let text = "## 보고\n상태: 완료\n커밋: 3f2a9c1\n검증: cargo test → 통과 42, 실패 0, 건너뜀 0 (종료 0)\n검증: npm run build → 종료 0\n\
                    미검증: 좁은 창 레이아웃 — 화면을 볼 수 없음\n변경: src/a.ts, src/b.ts\n남은 일: 없음\n\n긴 설명이 이어져요.\n상태: 막힘\n";
        let r = parse_report(text).unwrap();
        assert_eq!(r.status, "완료", "the block ends at the first blank line");
        assert_eq!(r.commit.as_deref(), Some("3f2a9c1"));
        assert_eq!(r.verified.len(), 2);
        assert_eq!(r.verified[0].weak, None, "counts with a zero for failures pass");
        assert_eq!(r.verified[1].weak, None, "a build's exit code is its result");
        assert_eq!(r.unverified, vec!["좁은 창 레이아웃 — 화면을 볼 수 없음"]);
        assert_eq!(r.changed, vec!["src/a.ts", "src/b.ts"]);
        assert_eq!(r.remaining, None, "없음 means nothing left");
    }

    #[test]
    fn report_takes_the_first_block_and_skips_what_it_doesnt_know() {
        let text = "앞말\n  ## 보고  \n상태: 부분\n커밋: 없음\n담당: 나\n그냥 한 줄\n검증: 없음\n변경: 없음\n남은 일: 문서\n남은 일: 두 번째\n# 다음\n\n## 보고\n상태: 막힘\n";
        let r = parse_report(text).unwrap();
        assert_eq!(r.status, "부분");
        assert_eq!(r.commit, None);
        assert!(r.verified.is_empty(), "검증: 없음 is no verification");
        assert!(r.changed.is_empty());
        assert_eq!(r.remaining.as_deref(), Some("문서"), "first value wins");
    }

    #[test]
    fn report_missing_or_broken_is_none() {
        assert_eq!(parse_report("다 됐어요! 테스트도 통과했어요."), None, "never guessed from prose");
        assert_eq!(parse_report("## 보고\n검증: x → y\n"), None, "no 상태");
        assert_eq!(parse_report("## 보고\n상태: 완료했어요\n"), None, "상태 is exactly one of the three");
        assert_eq!(parse_report("### 보고\n상태: 완료\n"), None, "only ## 보고 opens a report");
        assert_eq!(parse_report("## 보고\n\n상태: 완료\n"), None, "a blank line ends it");
    }

    #[test]
    fn weak_checks_do_not_count_as_a_pass() {
        assert_eq!(weak_check("cargo test"), Some("결과 없음"));
        assert_eq!(weak_check("cargo test → "), Some("결과 없음"));
        assert_eq!(weak_check("pytest → 0 passed in 0.1s"), Some("테스트 0개"));
        assert_eq!(weak_check("cargo test → 통과 0, 건너뜀 12"), Some("테스트 0개"));
        assert_eq!(weak_check("cargo test → 종료 0"), Some("종료 코드만"));
        assert_eq!(weak_check("cargo test → 10 passed"), None, "10 is not 0");
        assert_eq!(weak_check("cargo test → 통과 10"), None);
        assert_eq!(weak_check("npm run build → 종료 0"), None);
    }

    fn canvas() -> Value {
        json!({
            "nodes": [
                { "id": "z1", "type": "zone", "position": { "x": 100, "y": 100 }, "width": 600, "height": 300, "data": { "title": "실행" } },
                { "id": "b1", "type": "struct_block", "parentId": "z1", "position": { "x": 24, "y": 48 }, "width": 240, "height": 72, "data": { "title": "빌드", "body": "첫 줄\n둘째 줄" } },
                { "id": "b2", "type": "struct_block", "parentId": "z1", "position": { "x": 300, "y": 48 }, "width": 240, "height": 72, "data": { "title": "배포" } },
                { "id": "b3", "type": "struct_block", "position": { "x": 2000, "y": 0 }, "width": 240, "height": 72, "data": { "title": "외톨이" } },
                { "id": "w1", "type": "worktree_card", "position": { "x": -500, "y": 0 }, "data": { "worktreePath": "C:/p/.maestro/api" } },
                { "id": "m1", "type": "markdown_block", "position": { "x": -500, "y": 400 }, "data": { "title": "설계", "path": "docs/a.md" } }
            ],
            "edges": [
                { "id": "e1", "source": "b1", "target": "b2", "type": "arrow", "data": { "kind": "arrow", "relation": "r1", "label": "끝나면" } },
                { "id": "e2", "source": "b2", "target": "b1", "type": "arrow", "data": { "kind": "arrow" } },
                { "id": "orch_x>y", "source": "lead", "target": "w1", "type": "link", "data": { "kind": "orchestrates" } }
            ],
            "lines": { "orch_x>y": { "label": "API 담당" } }
        })
    }

    fn rel() -> RelFile {
        let mut f = RelFile::default();
        crate::relations::define(&mut f, "의존", crate::relations::Def { description: Some("앞이 끝나야 뒤".into()), ..Default::default() }).unwrap();
        f.relations[0].id = "r1".into();
        f
    }

    #[test]
    fn read_gives_areas_blocks_lines_with_ids_and_boxes() {
        let name_of = |p: &str| (p == "C:/p/.maestro/api").then(|| "api".to_string());
        let v = read(&canvas(), &rel(), &name_of, false);
        assert_eq!(v["areas"][0]["title"], "실행");
        assert_eq!(v["areas"][0]["holds"], json!(["빌드", "배포"]));
        // a border colour is read back when a block or area has one, and left out when it has none
        let mut painted = canvas();
        painted["nodes"][0]["data"]["color"] = json!("teal");
        painted["nodes"][1]["data"]["color"] = json!("blue");
        let p = read(&painted, &rel(), &name_of, false);
        assert_eq!((p["areas"][0]["color"].clone(), p["blocks"][0]["color"].clone()), (json!("teal"), json!("blue")));
        assert_eq!((v["areas"][0].get("color"), v["blocks"][0].get("color"), p["blocks"][1].get("color")), (None, None, None));
        let b = &v["blocks"][0];
        assert_eq!(b["id"], "b1");
        assert_eq!(b["area"], "실행");
        assert_eq!(b["box"], json!({ "x": 124, "y": 148, "w": 240, "h": 72 }), "absolute: the area's position added");
        assert_eq!(b["body"], "첫 줄\n둘째 줄");
        assert_eq!(v["arrows"][0], json!({ "id": "e1", "from": "빌드", "to": "배포", "relation": "의존", "label": "끝나면" }));
        assert_eq!(v["arrows"][1]["relation"], "기본 관계");
        assert_eq!(v["links"][0]["relation"], "지휘");
        assert_eq!(v["links"][0]["label"], "API 담당", "a hierarchy line's label is kept apart (lines)");
        let names: Vec<&str> = v["others"].as_array().unwrap().iter().map(|o| o["name"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["api", "설계"]);
        let brief = read(&canvas(), &rel(), &name_of, true);
        assert_eq!(brief["blocks"][0]["body"], "첫 줄");
        assert_eq!(v["questions"], json!([]));
    }

    #[test]
    fn read_gives_questions_with_their_answers() {
        let mut c = canvas();
        let answer = json!({ "picked": ["SQLite"], "at": 1 });
        c["nodes"].as_array_mut().unwrap().push(json!({ "id": "question_q1", "type": "question_block", "position": { "x": 0, "y": 0 }, "data": { "qid": "q1", "title": "어느 DB?", "choices": ["Postgres", "SQLite"], "answer": answer } }));
        c["nodes"].as_array_mut().unwrap().push(json!({ "id": "question_q2", "type": "question_block", "position": { "x": 0, "y": 0 }, "data": { "qid": "q2", "title": "이름은?" } }));
        let v = read(&c, &rel(), &|_| None, false);
        let q = v["questions"].as_array().unwrap();
        assert_eq!((q[0]["id"].clone(), q[0]["title"].clone(), q[0]["choices"].clone(), q[0]["multi"].clone(), q[0]["answer"].clone()), (json!("q1"), json!("어느 DB?"), json!(["Postgres", "SQLite"]), json!(false), answer));
        assert_eq!((q[1]["choices"].clone(), q[1]["answer"].clone()), (json!([]), Value::Null), "free text, not answered yet");
        assert!(v["others"].as_array().unwrap().iter().all(|o| o["kind"] != "question"));
        assert_eq!(q[0]["area"], Value::Null, "at the top level");
    }

    #[test]
    fn read_gives_steps_and_states_and_what_exists_when() {
        let mut c = canvas();
        // 실행 has three steps and shows the second; 빌드 exists in steps 1 and 2, 배포 in every step
        c["nodes"][0]["data"] = json!({ "title": "실행", "mode": "step", "at": "p2", "phases": [{ "id": "p1", "name": "" }, { "id": "p2", "name": "검토" }, { "id": "p3", "name": "" }] });
        c["nodes"][1]["data"]["when"] = json!(["p2", "p1", "gone"]);
        // a line between the two, only in step 3
        c["edges"][0]["data"]["when"] = json!(["p3"]);
        c["edges"][0]["data"]["whenZone"] = json!("z1");
        // an area with states inside it, showing the unnamed second one; a question that exists only in 정상
        c["nodes"].as_array_mut().unwrap().push(json!({ "id": "z2", "type": "zone", "parentId": "z1", "position": { "x": 24, "y": 150 }, "width": 400, "height": 120, "data": { "title": "서버", "mode": "state", "at": "s2", "phases": [{ "id": "s1", "name": "정상" }, { "id": "s2", "name": " " }], "when": ["p3"] } }));
        c["nodes"].as_array_mut().unwrap().push(json!({ "id": "question_q1", "type": "question_block", "parentId": "z2", "position": { "x": 24, "y": 48 }, "data": { "qid": "q1", "title": "재시작할까요?", "when": ["s1"] } }));
        let v = read(&c, &rel(), &|_| None, false);
        let a = &v["areas"][0];
        assert_eq!((a["mode"].clone(), a["steps"].clone(), a["current"].clone()), (json!("step"), json!(["", "검토", ""]), json!(2)));
        assert_eq!(v["blocks"][0]["in_steps"], json!([1, 2]), "by number, in the area's order; an id it no longer has is dropped");
        assert_eq!(v["blocks"][1].get("in_steps"), None, "in every step");
        assert_eq!(v["arrows"][0]["in_steps"], json!([3]));
        let s = &v["areas"][1];
        assert_eq!((s["mode"].clone(), s["states"].clone(), s["current"].clone(), s["in_steps"].clone()), (json!("state"), json!(["정상", "상태 2"]), json!("상태 2"), json!([3])));
        assert_eq!(v["questions"][0]["in_states"], json!(["정상"]));
        // a plain area says nothing of this, whatever its blocks carry
        let plain = read(&canvas(), &rel(), &|_| None, false);
        assert_eq!((plain["areas"][0].get("mode"), plain["areas"][0].get("current")), (None, None));
    }

    #[test]
    fn drawn_steps_and_states_are_checked() {
        let none: Vec<Node> = vec![];
        let ok = |v: Value| check_phases(&v, &none);
        // steps as a count or as names, states as names; blocks say where they are
        assert!(ok(json!({ "areas": [{ "title": "배포", "steps": 3 }, { "title": "서버", "states": ["정상", "장애"] }], "blocks": [{ "title": "빌드", "area": "배포", "step": 1 }, { "title": "올리기", "area": "배포", "step": [2, 3] }, { "title": "알림", "area": "서버", "state": "장애" }] })).is_ok());
        assert_eq!(drawn_phases(&json!({ "title": "a", "steps": ["준비", ""] })).unwrap(), Some(("step", vec!["준비".to_string(), String::new()])));
        assert_eq!(drawn_phases(&json!({ "title": "a" })).unwrap(), None);
        assert!(ok(json!({ "areas": [{ "title": "a", "steps": 2, "states": ["x"] }] })).unwrap_err().contains("함께"));
        assert!(ok(json!({ "areas": [{ "title": "a", "steps": 0 }] })).is_err());
        assert!(ok(json!({ "areas": [{ "title": "a", "steps": 21 }] })).is_err());
        assert!(ok(json!({ "areas": [{ "title": "a", "states": ["x", "x"] }] })).unwrap_err().contains("두 번"));
        assert!(ok(json!({ "areas": [{ "title": "a", "states": ["x", " "] }] })).unwrap_err().contains("빈 이름"));
        // a block's step or state must be one its area has, and the block must be in an area
        assert!(ok(json!({ "areas": [{ "title": "배포", "steps": 2 }], "blocks": [{ "title": "b", "area": "배포", "step": 3 }] })).unwrap_err().contains("1에서 2까지"));
        assert!(ok(json!({ "areas": [{ "title": "배포", "steps": 2 }], "blocks": [{ "title": "b", "area": "배포", "state": "x" }] })).unwrap_err().contains("단계(steps)"));
        assert!(ok(json!({ "areas": [{ "title": "서버", "states": ["정상"] }], "blocks": [{ "title": "b", "area": "서버", "state": "장애" }] })).unwrap_err().contains("정상"));
        assert!(ok(json!({ "blocks": [{ "title": "b", "step": 1 }] })).unwrap_err().contains("영역 안"));
        assert!(ok(json!({ "areas": [{ "title": "그냥" }], "blocks": [{ "title": "b", "area": "그냥", "step": 1 }] })).unwrap_err().contains("steps가 없어요"));
        assert!(ok(json!({ "areas": [{ "title": "배포", "steps": 2 }], "blocks": [{ "title": "b", "area": "배포", "step": 1, "state": "x" }] })).unwrap_err().contains("함께"));
        // an inner area can be in a step too; an arrow's are checked for their kind only
        assert!(ok(json!({ "areas": [{ "title": "배포", "steps": 2 }, { "title": "안", "in": "배포", "step": 2 }], "blocks": [{ "title": "a" }, { "title": "b" }], "arrows": [{ "from": "a", "to": "b", "step": [1, 2] }] })).is_ok());
        assert!(ok(json!({ "blocks": [{ "title": "a" }, { "title": "b" }], "arrows": [{ "from": "a", "to": "b", "step": "둘" }] })).is_err());
        // an area already on the canvas with steps: a drawing can put a block in one of them without repeating them
        let mut c = canvas();
        c["nodes"][0]["data"] = json!({ "title": "실행", "mode": "step", "at": "p2", "phases": [{ "id": "p1", "name": "" }, { "id": "p2", "name": "검토" }] });
        let nodes = nodes_of(&c, &|_| None);
        assert!(check_phases(&json!({ "areas": [{ "title": "실행" }], "blocks": [{ "title": "새", "area": "실행", "step": 2 }] }), &nodes).is_ok());
        assert!(check_phases(&json!({ "areas": [{ "title": "실행" }], "blocks": [{ "title": "새", "area": "실행", "step": 3 }] }), &nodes).is_err());

        // moving it: next and prev stop at the ends, a number goes straight there; the answer says where it lands
        let (told, answer) = phase_move("step", "실행", "", &nodes).unwrap();
        assert_eq!((told, answer["current"].clone(), answer["of"].clone()), (json!({ "area": "실행", "to": "next" }), json!(2), json!(2)), "already at the last step");
        assert_eq!(phase_move("step", "실행", "prev", &nodes).unwrap().1["current"], 1);
        assert_eq!(phase_move("step", " 실행 ", "1", &nodes).unwrap().0, json!({ "area": "실행", "to": 1 }));
        assert!(phase_move("step", "실행", "3", &nodes).unwrap_err().contains("1에서 2"));
        assert!(phase_move("state", "실행", "검토", &nodes).unwrap_err().contains("단계"));
        assert!(phase_move("step", "없음", "next", &nodes).unwrap_err().contains("없어요"));
        assert!(phase_move("step", "", "next", &nodes).unwrap_err().contains("--area"));
        c["nodes"][0]["data"] = json!({ "title": "실행", "mode": "state", "at": "s1", "phases": [{ "id": "s1", "name": "정상" }, { "id": "s2", "name": "" }] });
        let nodes = nodes_of(&c, &|_| None);
        assert_eq!(phase_move("state", "실행", "상태 2", &nodes).unwrap().1["current"], "상태 2");
        assert!(phase_move("state", "실행", "장애", &nodes).unwrap_err().contains("정상 | 상태 2"));
        let plain = nodes_of(&canvas(), &|_| None);
        assert!(phase_move("step", "실행", "next", &plain).unwrap_err().contains("단계도 상태도"));
    }

    #[test]
    fn view_blocks_are_read_with_their_file() {
        assert_eq!((view_kind("out/chart.PNG"), view_kind("a/b/report.html"), view_kind("x.svg"), view_kind("notes.md"), view_kind("noext")), (Some("image"), Some("html"), Some("image"), None, None));
        assert_eq!((view_mime("a.jpeg"), view_mime("a.svg"), view_mime("a.htm")), ("image/jpeg", "image/svg+xml", "text/html"));
        let mut c = canvas();
        c["nodes"].as_array_mut().unwrap().push(json!({ "id": "view_1", "type": "view_block", "parentId": "z1", "position": { "x": 24, "y": 400 }, "width": 384, "height": 288, "data": { "path": "C:/w/out/chart.png", "kind": "image", "title": " 응답 시간 " } }));
        c["nodes"].as_array_mut().unwrap().push(json!({ "id": "view_2", "type": "view_block", "position": { "x": 5000, "y": 0 }, "data": { "path": "C:/w/out/report.html", "kind": "html" } }));
        let v = read(&c, &rel(), &|_| None, false);
        let views: Vec<&Value> = v["others"].as_array().unwrap().iter().filter(|o| o["kind"] == "view").collect();
        assert_eq!(views.len(), 2);
        assert_eq!((views[0]["name"].clone(), views[0]["path"].clone(), views[0]["area"].clone()), (json!("응답 시간"), json!("C:/w/out/chart.png"), json!("실행")));
        assert_eq!((views[1]["name"].clone(), views[1]["box"]["w"].clone()), (json!("report.html"), json!(384)), "named by its file, at the default size until it is measured");
    }

    #[test]
    fn a_say_is_one_short_line() {
        assert_eq!(check_say("  "), Ok(None));
        assert_eq!(check_say(" 배포는 세 단계예요 "), Ok(Some("배포는 세 단계예요".into())));
        assert!(check_say("첫 줄\n둘째 줄").unwrap_err().contains("한 줄"));
        assert!(check_say(&"가".repeat(SAY_MAX)).is_ok());
        assert!(check_say(&"가".repeat(SAY_MAX + 1)).unwrap_err().contains("121자"));
        // a drawing may carry it without a lint about an unknown field
        assert!(FIELDS[0].1.contains(&"say"));
        // names for focus and tidy: by kind
        let nodes = nodes_of(&canvas(), &|_| None);
        assert!(has_named(&nodes, " 빌드 ", &["block", "area"]) && has_named(&nodes, "실행", &["area"]));
        assert!(!has_named(&nodes, "빌드", &["area"]) && !has_named(&nodes, "없음", &["block", "area"]));
    }

    #[test]
    fn stacks_are_read_in_order_and_checked() {
        let mut c = canvas();
        // 실행 as a stack: what it holds is listed from the top down, whatever order the file has it in
        c["nodes"][0]["data"] = json!({ "title": "실행", "stack": true });
        let kids: Vec<(usize, String)> = c["nodes"].as_array().unwrap().iter().enumerate().filter(|(_, n)| n["parentId"] == "z1").map(|(i, n)| (i, n["data"]["title"].as_str().unwrap_or("").to_string())).collect();
        assert!(kids.len() >= 2, "the fixture has two blocks in 실행");
        c["nodes"][kids[0].0]["position"]["y"] = json!(500);
        c["nodes"][kids[1].0]["position"]["y"] = json!(48);
        let v = read(&c, &rel(), &|_| None, false);
        assert_eq!(v["areas"][0]["stack"], true);
        assert_eq!(v["areas"][0]["holds"][0], kids[1].1.as_str(), "the one on top first");
        assert_eq!(read(&canvas(), &rel(), &|_| None, false)["areas"][0].get("stack"), None, "only said of a stack");

        assert!(check_stacks(&json!({ "areas": [{ "title": "a", "stack": true }, { "title": "b", "stack": false }, { "title": "c" }] })).is_ok());
        assert!(check_stacks(&json!({ "areas": [{ "title": "a", "stack": "yes" }] })).unwrap_err().contains("true나 false"));
        assert!(check_stacks(&json!({ "areas": [{ "title": "a", "stack": true, "steps": 2 }] })).unwrap_err().contains("steps나 states"));
        assert!(check_stacks(&json!({ "areas": [{ "title": "a", "stack": true, "states": ["x"] }] })).is_err());
        // a stack has to hold something: a block of the same drawing, or what the canvas already has in that area
        let nodes = nodes_of(&canvas(), &|_| None);
        assert!(check_stacks_hold(&json!({ "areas": [{ "title": "새 묶음", "stack": true }], "blocks": [{ "title": "하나", "area": "새 묶음" }] }), &nodes).is_ok());
        assert!(check_stacks_hold(&json!({ "areas": [{ "title": "바깥", "stack": true }, { "title": "안", "in": "바깥" }] }), &nodes).is_ok());
        assert!(check_stacks_hold(&json!({ "areas": [{ "title": "실행", "stack": true }] }), &nodes).is_ok(), "an area of the canvas that holds blocks");
        assert!(check_stacks_hold(&json!({ "areas": [{ "title": "빈 묶음", "stack": true }] }), &nodes).unwrap_err().contains("든 것이 없어요"));
        assert!(check_stacks_hold(&json!({ "areas": [{ "title": "보통 영역" }] }), &nodes).is_ok(), "only stacks");
    }

    #[test]
    fn read_gives_a_folded_area_at_its_open_size() {
        let mut c = canvas();
        // 실행 folded to its chip: on screen 240×48, opening back to 600×300; a question dragged into it
        c["nodes"][0]["width"] = json!(240);
        c["nodes"][0]["height"] = json!(48);
        c["nodes"][0]["data"] = json!({ "title": "실행", "folded": true, "full": { "width": 600, "height": 300 } });
        c["nodes"].as_array_mut().unwrap().push(json!({ "id": "question_q1", "type": "question_block", "parentId": "z1", "position": { "x": 24, "y": 150 }, "data": { "qid": "q1", "title": "어느 DB?" } }));
        c["nodes"].as_array_mut().unwrap().push(json!({ "id": "z2", "type": "zone", "position": { "x": 3000, "y": 0 }, "width": 400, "height": 300, "data": { "title": "열린 곳" } }));
        let v = read(&c, &rel(), &|_| None, false);
        assert_eq!(v["areas"][0]["folded"], true);
        assert_eq!(v["areas"][0]["box"], json!({ "x": 100, "y": 100, "w": 600, "h": 300 }), "the size it opens back to, not the chip's");
        assert_eq!(v["areas"][0]["holds"], json!(["빌드", "배포", "어느 DB?"]), "what a folded area holds is still listed");
        assert_eq!(v["areas"][1].get("folded"), None, "only said of a folded area");
        assert_eq!(v["questions"][0]["area"], "실행");
        assert_eq!(v["questions"][0]["box"]["x"], 124, "absolute, like a block's");
        // nothing sticks out of the folded area's box, so the lint has nothing new to say about it
        let nodes = nodes_of(&c, &|_| None);
        assert!(lint_canvas(&nodes, &c).iter().all(|l| l.code != "loose-in-area" && l.code != "overlap"));
    }

    #[test]
    fn canvas_lint_finds_overlap_orphans_and_bare_lines() {
        let mut c = canvas();
        // a second block right on top of 빌드, and an empty area
        c["nodes"].as_array_mut().unwrap().push(json!({ "id": "b4", "type": "struct_block", "parentId": "z1", "position": { "x": 40, "y": 60 }, "width": 240, "height": 72, "data": { "title": "빌드" } }));
        c["nodes"].as_array_mut().unwrap().push(json!({ "id": "z2", "type": "zone", "position": { "x": 3000, "y": 0 }, "width": 400, "height": 300, "data": { "title": "빈 곳" } }));
        let nodes = nodes_of(&c, &|_| None);
        let l = lint_canvas(&nodes, &c);
        let codes: Vec<&str> = l.iter().map(|x| x.code).collect();
        assert!(codes.contains(&"overlap"), "{l:?}");
        assert!(codes.contains(&"same-name"));
        assert!(codes.contains(&"empty-area"));
        assert!(l.iter().any(|x| x.code == "orphan" && x.message.contains("외톨이")));
        assert!(l.iter().any(|x| x.code == "no-label" && x.message.contains("'배포' → '빌드'")));
        // blocks inside their own area don't count as overlapping it
        assert!(!l.iter().any(|x| x.code == "overlap" && x.message.contains("실행")));
    }

    #[test]
    fn drawing_lint_catches_ignored_fields_clashes_and_skipped_arrows() {
        let spec = json!({
            "relations": [{ "name": "게이트", "meaning": "확인이 통과해야 넘어간다" }],
            "areas": [{ "title": "검증" }, { "title": "빈 영역" }],
            "blocks": [
                { "title": "검증", "area": "검증" },
                { "title": "구현", "area": "검증", "text": "본문" },
                { "title": "혼자", "area": "검증" }
            ],
            "arrows": [
                { "from": "구현", "to": "검증", "as": "게이트" },
                { "from": "구현", "to": "검증", "label": "다시" },
                { "from": "구현", "to": "구현" },
                { "from": "검증", "to": "구현", "as": "되돌아감" },
                { "from": "검증", "to": "구현", "label": "실패하면 앞 단계로 돌아가서 처음부터 다시 한다" }
            ]
        });
        let l = lint_drawing(&spec, &[], &[], &RelFile::default());
        let has = |code: &str, part: &str| l.iter().any(|x| x.code == code && x.message.contains(part));
        assert!(has("unknown-field", "'meaning'") && has("unknown-field", "'description'"), "{l:?}");
        assert!(has("unknown-field", "'text'"));
        assert!(has("same-name", "'검증'"));
        assert!(has("duplicate-arrow", "'구현' → '검증'"));
        assert!(has("self-arrow", "'구현'"));
        assert!(has("orphan", "혼자"));
        assert!(has("empty-area", "빈 영역"));
        assert!(has("undeclared-relation", "되돌아감"));
        assert!(!has("undeclared-relation", "게이트"), "declared in the same drawing");
        assert!(has("relation-no-meaning", "게이트"), "its meaning went to an unknown field");
        assert!(has("long-label", "'검증' → '구현'"));
        assert!(!l.iter().any(|x| x.code == "no-label"), "every arrow has a label or a relation");
    }

    #[test]
    fn drawing_lint_is_quiet_for_a_clean_drawing() {
        let spec = json!({
            "layout": "flow",
            "relations": [{ "name": "다음", "description": "그다음 단계", "pattern": "solid" }],
            "areas": [{ "title": "설계", "layout": "tree" }],
            "blocks": [{ "title": "A", "area": "설계" }, { "title": "B", "area": "설계" }],
            "arrows": [{ "from": "A", "to": "B", "as": "다음", "label": "넘긴다" }]
        });
        assert_eq!(lint_drawing(&spec, &[], &[], &RelFile::default()), vec![]);
        assert!(check_layouts(&spec).is_ok());
        assert!(check_layouts(&json!({ "layout": "circle", "areas": [] })).is_err());
        assert!(check_layouts(&json!({ "areas": [{ "title": "x", "layout": 3 }] })).is_err());
    }
}
