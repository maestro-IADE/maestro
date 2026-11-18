// The 캔버스 CLI 시험's command list (src/devSteps.ts; docs/features/ui/dev-tools.md) against the rules the commands
// themselves enforce (control.rs `drawing`, `question`, `erasing`, relations): a step is marked `fails` exactly when
// the backend would refuse it, given what the steps before it drew. A wrong list would make the test blame the CLI
// for the list's own mistake. Run: `npm test`.
import { equal, ok } from "node:assert/strict";
import { CHAT_STEPS, REL, STEPS, typed, type Step } from "../src/devSteps.ts";

type In = { step?: number | number[]; state?: string | string[] };
type Drawing = {
  layout?: string;
  areas?: ({ title?: string; in?: string; layout?: string; color?: string; steps?: number | string[]; states?: string[]; stack?: unknown } & In)[];
  blocks?: ({ title?: string; area?: string; color?: string } & In)[];
  arrows?: ({ from?: string; to?: string; as?: string } & In)[];
};
type Phases = { mode: "step" | "state"; names: string[] };
const many = <T,>(v: T | T[] | undefined): T[] => (v == null ? [] : Array.isArray(v) ? v : [v]);
/** The steps or states a drawing gives an area (canvas.rs `drawn_phases`); a string says why it is refused. */
function drawnPhases(a: NonNullable<Drawing["areas"]>[number]): Phases | string | undefined {
  if (a.steps != null && a.states != null) return "steps and states together";
  if (typeof a.steps === "number") return a.steps >= 1 && a.steps <= 20 ? { mode: "step", names: Array.from({ length: a.steps }, () => "") } : "step count";
  if (a.steps) return a.steps.length >= 1 && a.steps.length <= 20 ? { mode: "step", names: a.steps.map((s) => s.trim()) } : "step count";
  if (a.states) {
    const names = a.states.map((s) => s.trim());
    return !names.length || names.length > 20 ? "state count" : names.some((s, i) => !s || names.indexOf(s) !== i) ? "state names" : { mode: "state", names };
  }
  return undefined;
}
const LAYOUTS = ["grid", "flow", "tree"];
const COLORS = ["blue", "green", "amber", "red", "violet", "teal"]; // a border colour is one of the line colours

// what the steps so far left on the canvas and in the project's relations
let blocks = new Set<string>(), areas = new Set<string>();
let arrows: [string, string][] = [];
const relations = new Set<string>();
const questions = new Set<string>(); // by what they ask (`erase --question` takes the id or the question itself)
const views = new Set<string>(); // the view blocks, by title (else by the file's name)
const viewKind = (path: string) => (/\.(png|jpe?g|gif|webp|svg)$/i.test(path) ? "image" : /\.html?$/i.test(path) ? "html" : null);
const sets = new Map<string, string[]>(); // the question sets, by the title they were asked under: their questions
let atAll = null as { questions: number; sets: number; views: number } | null; // what `erase --all` found still there
/** Why one question is refused (control.rs `question`), or null. */
function askable(title: string, list: string[], multi: boolean): string | null {
  if (!title.trim()) return "no title";
  if (list.length === 1) return "one choice";
  if (list.length > 8) return "too many choices";
  if (new Set(list).size !== list.length) return "a choice twice";
  if (multi && !list.length) return "multi without choices";
  return null;
}
const phased = new Map<string, Phases>(); // the areas that have steps or states, by name

/** `maestro <words> --flag value …`, split as the CLI does. */
function parse(args: string[]) {
  const words: string[] = [], flags = new Map<string, string>();
  for (let i = 0; i < args.length; i++) {
    if (args[i].startsWith("--")) flags.set(args[i].slice(2), args[++i] ?? "");
    else words.push(args[i]);
  }
  return { route: words.join(" "), flag: (k: string) => (flags.get(k) ?? "").trim(), help: args.includes("--help") };
}

/** Why the backend refuses the step, or null when it takes it (what it changes is then applied here). */
function refused(s: Step): string | null {
  const { route, flag, help } = parse(s.args);
  if (help) return null;
  if (route === "canvas" || route === "selection") return null;
  // the line said with a canvas command (canvas.rs `check_say`): one line, 120 characters at most
  const line = (t: string) => (t.includes("\n") ? "a say of several lines" : [...t].length > 120 ? "a say too long" : null);
  if (line(flag("say"))) return line(flag("say"));
  if (route === "say") return flag("text") ? line(flag("text")) : "no text";
  // (a name that isn't on the canvas is a note in the answer, not a refusal: it may have been drawn a moment ago)
  if (route === "focus") return flag("title") ? null : "no title";
  if (route === "tidy") return null;
  if (route === "relation define") {
    if (!flag("name")) return "no name";
    if (flag("pattern") && !["solid", "dashed", "dotted"].includes(flag("pattern"))) return "pattern";
    if (flag("head") && !["one", "both", "none"].includes(flag("head"))) return "head";
    if (flag("color") && !["blue", "green", "amber", "red", "violet", "teal"].includes(flag("color"))) return "color";
    relations.add(flag("name"));
    return null;
  }
  if (route === "relation remove") {
    if (!relations.delete(flag("name"))) return "no such relation";
    return null;
  }
  if (route === "show") {
    // control.rs `showing`: a file (the step's own, or --file), an image or a page, that is there
    const path = s.text != null ? (s.fileName ?? "") : flag("file");
    if (!path) return "no file";
    if (!viewKind(path)) return "not an image or a page";
    if (s.text == null) return "no such file (the test's caller has no folder of its own)";
    views.add(flag("title") || path);
    return null;
  }
  if (route === "ask") {
    const split = (c: string) => c.split("|").map((x) => x.trim()).filter(Boolean);
    if (s.file != null) {
      // several at once (control.rs `question_set`): 2 to 10, each checked like a single one, none twice
      type Q = { title?: string; choices?: string[] | string; multi?: boolean };
      const spec = s.file as { title?: string; questions?: Q[] } | Q[];
      const list = Array.isArray(spec) ? spec : spec.questions;
      if (!Array.isArray(list)) return "no questions";
      if (list.length < 2) return "one question is not a set";
      if (list.length > 10) return "too many questions";
      const titles: string[] = [];
      for (const q of list) {
        const why = askable(q.title ?? "", Array.isArray(q.choices) ? q.choices.map((c) => c.trim()).filter(Boolean) : split(q.choices ?? ""), !!q.multi);
        if (why) return why;
        if (titles.includes(q.title!.trim())) return "a question twice";
        titles.push(q.title!.trim());
      }
      sets.set(((Array.isArray(spec) ? "" : spec.title) ?? "").trim() || `질문 ${list.length}개`, titles);
      for (const t of titles) questions.add(t);
      return null;
    }
    const why = askable(flag("title"), split(flag("choices")), flag("multi") === "true");
    if (why) return why;
    questions.add(flag("title"));
    return null;
  }
  if (route === "draw") {
    const d = s.file as Drawing | undefined;
    if (!d) return "no file";
    const as = d.areas ?? [], bs = d.blocks ?? [];
    if (!as.length && !bs.length) return "nothing to draw";
    const names = [...as, ...bs].map((x) => (x.title ?? "").trim());
    if (names.some((n) => !n)) return "no title";
    for (const a of d.arrows ?? []) for (const end of [a.from, a.to]) if (!names.includes(end ?? "")) return `arrow end '${end}'`;
    const areaNames = as.map((a) => (a.title ?? "").trim());
    for (const x of [...as.map((a) => a.in), ...bs.map((b) => b.area)]) if (x && !areaNames.includes(x)) return `area '${x}'`;
    for (const l of [d.layout, ...as.map((a) => a.layout)]) if (l && !LAYOUTS.includes(l)) return `layout '${l}'`;
    for (const c of [...as, ...bs].map((x) => x.color)) if (c && !COLORS.includes(c)) return `color '${c}'`;
    // stacks (canvas.rs `check_stacks`): true or false, and never with steps or states
    for (const a of as) {
      if (a.stack != null && typeof a.stack !== "boolean") return `${a.title}: stack`;
      if (a.stack && (a.steps != null || a.states != null)) return `${a.title}: a stack with steps or states`;
      // (canvas.rs `check_stacks_hold`: a stack holds something of the drawing, or is an area already there)
      if (a.stack && !bs.some((b) => b.area?.trim() === a.title?.trim()) && !as.some((x) => x.in?.trim() === a.title?.trim()) && !areas.has(a.title!.trim())) return `${a.title}: an empty stack`;
    }
    // steps and states (canvas.rs `check_phases`): what the drawing gives an area comes first, then what the canvas has
    const given = new Map<string, Phases>();
    for (const a of as) {
      const p = drawnPhases(a);
      if (typeof p === "string") return `${a.title}: ${p}`;
      if (p) given.set(a.title!.trim(), p);
    }
    for (const [x, parent] of [...as.map((a) => [a, a.in] as const), ...bs.map((b) => [b, b.area] as const)]) {
      const steps = many(x.step), states = many(x.state);
      if (!steps.length && !states.length) continue;
      if (steps.length && states.length) return `${x.title}: step and state together`;
      const p = parent ? (given.get(parent.trim()) ?? phased.get(parent.trim())) : undefined;
      if (!p) return `${x.title}: its area has no steps or states`;
      if (p.mode !== (steps.length ? "step" : "state")) return `${x.title}: the area is of the other kind`;
      if (steps.some((n) => !Number.isInteger(n) || n < 1 || n > p.names.length)) return `${x.title}: no such step`;
      if (states.some((s) => !p.names.includes(s.trim()))) return `${x.title}: no such state`;
    }
    for (const a of d.arrows ?? []) {
      if (many(a.step).some((n) => !Number.isInteger(n) || n < 1)) return "an arrow's step";
      if (many(a.state).some((s) => typeof s !== "string")) return "an arrow's state";
    }
    // an area keeps the steps or states it has; a plain or new one takes the drawing's
    for (const [name, p] of given) if (!phased.has(name)) phased.set(name, p);
    for (const a of as) areas.add(a.title!.trim());
    for (const b of bs) blocks.add(b.title!.trim());
    for (const a of d.arrows ?? []) {
      if (!arrows.some(([f, t]) => f === a.from && t === a.to)) arrows.push([a.from!, a.to!]);
      if (a.as) relations.add(a.as); // an `as` not declared yet is declared by the drawing
    }
    return null;
  }
  if (route === "step" || route === "state") {
    // canvas.rs `phase_move`: the area must be on the canvas with steps (for `step`) or states (for `state`)
    const area = flag("area"), to = flag("to");
    if (!area) return "no area";
    if (!areas.has(area)) return `no area '${area}'`;
    const p = phased.get(area);
    if (!p) return "a plain area";
    if (p.mode !== route) return "the other kind of area";
    if (route === "step") return !to || to === "next" || to === "prev" || (/^\d+$/.test(to) && +to >= 1 && +to <= p.names.length) ? null : `no step '${to}'`;
    return p.names.includes(to) ? null : `no state '${to}'`;
  }
  if (route === "erase") {
    const all = flag("all") === "true", title = flag("title"), from = flag("from"), to = flag("to"), question = flag("question"), view = flag("view");
    if (all && (title || from || to || question || view)) return "--all with something else";
    if (!all && !title && !from && !to && !question && !view) return "nothing to erase";
    if (view && !views.delete(view)) return `no view '${view}'`;
    if (!!from !== !!to) return "half an arrow";
    if (!blocks.size && !areas.size && (title || from)) return "nothing drawn";
    if (title && !blocks.has(title) && !areas.has(title)) return `no '${title}'`;
    if (from && !arrows.some(([f, t]) => f === from && t === to)) return `no arrow '${from}' → '${to}'`;
    if (question) {
      // a question by what it asks, else a set by the title it was asked under (the set goes whole)
      const set = sets.get(question);
      if (questions.delete(question)) {
        for (const [name, qs] of [...sets]) {
          if (!qs.includes(question)) continue;
          if (qs.length > 1) sets.set(name, qs.filter((q) => q !== question));
          else sets.delete(name);
        }
      } else if (set) {
        for (const q of set) questions.delete(q);
        sets.delete(question);
      } else return `no question '${question}'`;
    }
    if (all) {
      // everything put on the canvas goes: the drawing, the questions (their sets too) and the view blocks
      atAll = { questions: questions.size, sets: sets.size, views: views.size };
      questions.clear();
      sets.clear();
      views.clear();
      blocks = new Set();
      areas = new Set();
      arrows = [];
      phased.clear();
    }
    if (title) {
      // the block (a name used by both is the block's) or the area goes, with every line touching it; what an area
      // held stays
      if (!blocks.has(title)) phased.delete(title);
      (blocks.has(title) ? blocks : areas).delete(title);
      arrows = arrows.filter(([f, t]) => f !== title && t !== title);
    }
    if (from) arrows = arrows.filter(([f, t]) => !(f === from && t === to));
    return null;
  }
  return `unknown command '${route}'`;
}

for (const [i, s] of STEPS.entries()) {
  const why = refused(s);
  equal(why !== null, !!s.fails, `step ${i + 1} (${s.title}): ${why ? `the backend refuses it (${why})` : "the backend takes it"}, but it is marked ${s.fails ? "fails" : "as passing"}`);
  ok(s.see.trim(), `step ${i + 1} says what to look for`);
  equal(s.file != null || s.text != null, !!s.fileName, `step ${i + 1}: a file and its shown name go together`);
  ok(s.file == null || s.text == null, `step ${i + 1}: one file at most`);
}
// the test tidies up after itself: nothing it put on the canvas is left, no relation of its own
equal(blocks.size + areas.size + arrows.length, 0, "the last steps erase what was drawn");
equal(questions.size + sets.size + views.size, 0, "`erase --all` takes the questions, their sets and the view blocks too");
equal(relations.size, 0, "the relations it declared are removed again");
// and `erase --all` has all three kinds to take: the steps before it erase only some by name
equal(atAll?.questions, 5, "one question and one set are cleared by `erase --question`; two questions and a set of three are left for `erase --all`");
equal(atAll?.sets, 1, "one set is left for `erase --all`");
equal(atAll?.views, 1, "one view block is erased with `erase --view`, one is left for `erase --all`");
ok(STEPS.some((s) => parse(s.args).route === "ask" && s.file != null && !s.fails), "a question set is asked");
ok(STEPS.some((s) => !s.fails && ((s.file as Drawing | undefined)?.areas ?? []).some((a) => a.stack === true)), "a stack is drawn");
// what it declares is named so dev.rs can take it away when the test is cut short
ok(REL.startsWith("시험-"));
for (const s of STEPS) {
  const { route, flag } = parse(s.args);
  if (route === "relation define") ok(flag("name").startsWith("시험-"), `a declared relation is named 시험-…: ${flag("name")}`);
  for (const a of (s.file as Drawing | undefined)?.arrows ?? []) if (a.as) ok(a.as.startsWith("시험-"), `an arrow's relation is named 시험-…: ${a.as}`);
}
// every kind of canvas command is in the run, the refusals too
const routes = STEPS.map((s) => parse(s.args).route);
for (const r of ["draw", "ask", "show", "erase", "canvas", "selection", "relation define", "relation remove", "step", "state", "say", "focus", "tidy"]) ok(routes.includes(r), `${r} is run`);
ok(STEPS.some((s) => parse(s.args).route === "draw" && parse(s.args).flag("say")), "a drawing comes with a line");
ok(STEPS.filter((s) => s.fails).length >= 5);
// shown as it would be typed
equal(typed({ title: "", see: "", args: ["ask", "--title", "어느 DB로 갈까요?", "--choices", "A|B"] }), 'maestro ask --title "어느 DB로 갈까요?" --choices "A|B"');
equal(typed({ title: "", see: "", args: ["draw"], fileName: "1.json", file: {} }), "maestro draw --file 1.json");

// ---- the 채팅 창구 시험's list (chat-visuals.md), on a canvas of its own: every command is one the backend takes
blocks = new Set();
areas = new Set();
arrows = [];
for (const s of [questions, sets, views, relations, phased]) s.clear();
for (const [i, s] of CHAT_STEPS.entries()) {
  const why = refused(s);
  equal(why, null, `chat step ${i + 1} (${s.title}): the backend refuses it (${why})`);
  ok(!s.fails && s.see.trim(), `chat step ${i + 1} passes and says what to look for`);
  equal(s.file != null || s.text != null, !!s.fileName, `chat step ${i + 1}: a file and its shown name go together`);
}
const chat = CHAT_STEPS.map((s) => parse(s.args));
// a drawing with a line comes first (the card with a picture), and something to throw away after it
equal(chat[0].route, "draw");
ok(chat[0].flag("say") && chat[1].route === "draw" && chat[1].flag("say"), "the first two steps are drawings with a line");
// a choice in the chat beside the picture, a pointer at something drawn before, a view block, a line on its own
ok(chat.some((c) => c.route === "ask" && c.flag("in") === "chat"), "a chat choice is asked");
const pointer = chat.findIndex((c) => c.route === "focus");
ok(pointer > 0 && ((CHAT_STEPS[0].file as Drawing).areas ?? []).some((a) => a.title === chat[pointer].flag("title")), "the pointer names an area the first step drew");
ok(chat.some((c) => c.route === "show") && chat.some((c) => c.route === "say"));
// nothing in it erases: ending the test takes everything away (dev.rs `clear`)
ok(!chat.some((c) => c.route === "erase"));

console.log("ok dev steps");
