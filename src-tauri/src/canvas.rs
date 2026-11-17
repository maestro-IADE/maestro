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
                _ => return None,
            };
            let d = &n["data"];
            let name = match kind {
                "agent" | "worker" => {
                    let p = d["worktreePath"].as_str().unwrap_or("");
                    name_of(p).unwrap_or_else(|| p.rsplit(['/', '\\']).next().unwrap_or(p).to_string())
                }
                "prompt" => d["name"].as_str().or(d["title"].as_str()).unwrap_or("프롬프트").to_string(),
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
            Some(Node {
                id: n["id"].as_str()?.to_string(),
                kind,
                name,
                parent: n["parentId"].as_str().map(String::from),
                x,
                y,
                w: size("width", dw),
                h: size("height", dh),
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
    let mut areas = vec![];
    let mut blocks = vec![];
    let mut others = vec![];
    let mut questions = vec![];
    for n in &nodes {
        match n.kind {
            // `maestro ask`: the question, its choices and the user's answer (null until answered)
            "question" => {
                let d = &n.raw["data"];
                questions.push(json!({ "id": d["qid"], "title": n.name, "choices": d["choices"].as_array().cloned().unwrap_or_default(), "multi": d["multi"].as_bool().unwrap_or(false), "answer": d["answer"].clone(), "box": bx(n) }));
            }
            "area" => {
                let holds: Vec<String> = nodes.iter().filter(|c| c.parent.as_deref() == Some(&n.id)).map(|c| c.name.clone()).collect();
                areas.push(json!({ "id": n.id, "title": n.name, "in": area_of(n), "holds": holds, "box": bx(n) }));
            }
            "block" => blocks.push(json!({ "id": n.id, "title": n.name, "body": body(n), "area": area_of(n), "box": bx(n) })),
            kind => {
                let mut o = json!({ "id": n.id, "kind": kind, "name": n.name, "area": area_of(n), "box": bx(n) });
                if let Some(p) = n.raw["data"]["path"].as_str().or(n.raw["data"]["worktreePath"].as_str()) {
                    o["path"] = json!(p);
                }
                others.push(o);
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
    ("", &["areas", "blocks", "arrows", "relations", "layout"]),
    ("areas", &["title", "in", "layout"]),
    ("blocks", &["title", "body", "area"]),
    ("arrows", &["from", "to", "label", "as", "body"]),
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
