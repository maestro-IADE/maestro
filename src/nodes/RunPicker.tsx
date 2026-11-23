import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { changeAgent, restartDue, restartIfDue, useRestartDue } from "../agentActions";
import { agentDef, effortLabel, modelsOnce, permissionOf, permissionsFor, SUGGEST_LEVELS, SUGGEST_NAMES, suggestOf } from "../agents";
import { Check } from "../icons";
import type { Worktree } from "../store";
import type { ModelChoice } from "../types";
import { openMenu, place, type MenuItem } from "../ui/Menu";

// An agent's run settings picked where the next message is written (docs/features/agents/chat-view.md "입력창"): the
// model and the reasoning level side by side in one small panel, the permission mode and the 추천 수준 side by side in
// another. For the message box's chips and, in the terminal view, the header's.

/** the chip whose press just closed its own panel: the click that follows must not open it again (a toggle) */
let shutBy: { el: Element; t: number } | null = null;

/** One column of a run panel: its heading, its choices ([value, label]), the current one, and (`wait`) still loading. */
type Col = { name: string; list: Array<[string, string]>; now: string; wait?: boolean };
/** what a run panel's two columns have picked when it closes: [left, right] */
type Pick = [string, string];

/**
 * A panel of two columns side by side, the current one of each ticked: the model chip's (the models, the wider
 * column, and the reasoning levels) and the 권한 / 추천 chip's (the permission modes and the 추천 수준). A press on
 * the left moves the tick and the panel stays, so the right can be picked after it; a press on the right closes the
 * panel. What was picked is handed over once, when it closes (a press on the right, a press outside, Esc, the window
 * losing the focus), so the agent restarts once for both.
 */
function RunPanel({ at, label, left, right, corner, onClose }: { at: Element; label: string; left: Col; right: Col; corner: boolean; onClose: (picked: Pick) => void }) {
  const box = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ x: number; y: number } | null>(null);
  const [picked, setPicked] = useState<Pick>([left.now, right.now]);
  const pickedRef = useRef(picked);
  pickedRef.current = picked;

  // as a menu (ui/Menu.tsx `place`): under the chip, or from its bottom right corner (`corner`: over it, right
  // edges in line); placed again when the models arrive and the panel grows
  useLayoutEffect(() => {
    const el = box.current;
    if (el) setPos(place(at, el.offsetWidth, el.offsetHeight, corner));
  }, [left.wait ? -1 : left.list.length]);

  useEffect(() => {
    const before = document.activeElement as HTMLElement | null;
    box.current?.focus({ preventScroll: true });
    at.setAttribute("data-menu-open", "");
    const inside = (t: EventTarget | null) => t instanceof Node && !!box.current?.contains(t);
    const close = () => onClose(pickedRef.current);
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      close();
    };
    const onDown = (e: PointerEvent) => {
      if (inside(e.target)) return;
      if (e.target instanceof Node && at.contains(e.target)) shutBy = { el: at, t: performance.now() };
      close();
    };
    const onWheel = (e: Event) => !inside(e.target) && close();
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("pointerdown", onDown, true);
    window.addEventListener("wheel", onWheel, true);
    window.addEventListener("blur", close);
    window.addEventListener("resize", close);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("wheel", onWheel, true);
      window.removeEventListener("blur", close);
      window.removeEventListener("resize", close);
      at.removeAttribute("data-menu-open");
      if (before?.isConnected && document.activeElement === box.current) before.focus({ preventScroll: true });
    };
  }, []);

  const column = ({ name, list, wait }: Col, cur: string, set: (v: string) => void) => (
    <div className="run-col" role="listbox" aria-label={name}>
      <div className="menu-heading">{name}</div>
      {list.map(([v, label]) => (
        <button key={v} type="button" role="option" aria-selected={v === cur} className="menu-item" tabIndex={-1} onClick={() => set(v)}>
          <span className="menu-label ellipsis">{label}</span>
          <span className="menu-icon" aria-hidden>{v === cur && <Check size={14} />}</span>
        </button>
      ))}
      {wait && <div className="menu-heading">불러오는 중…</div>}
    </div>
  );
  return (
    <div
      ref={box}
      className="menu run-panel nodrag nopan nowheel"
      role="dialog"
      aria-label={label}
      tabIndex={-1}
      style={{ left: pos?.x ?? 0, top: pos?.y ?? 0, visibility: pos ? undefined : "hidden" }}
      onContextMenu={(e) => e.preventDefault()}
    >
      {column(left, picked[0], (v) => setPicked((p) => [v, p[1]]))}
      {column(right, picked[1], (v) => onClose([pickedRef.current[0], v]))}
    </div>
  );
}

/**
 * What the chips of an agent's run settings show and open: one chip for the model and the reasoning level (`model`
 * opens their panel; an agent with no reasoning levels gets a plain menu of models), one for the permission mode and
 * the 추천 수준 (`perm` opens their panel). What is picked is saved at once (agentActions.ts `changeAgent`): a running agent restarts
 * on the same conversation, now when it rests, else when its turn ends (`due` until then). `panel`: the open panel,
 * to be put anywhere in what the caller draws (it draws itself over the window). `corner`: the chips sit at the
 * foot of a box, on its right (the message box), so what they open goes over them with the right edges in line.
 */
export function useRunMenus(wt: Worktree | undefined, corner = false) {
  const adapterId = wt?.adapterId ?? "";
  // every model the agent's CLI names (offered are the `listed` ones; the chip names whatever is saved)
  // (null: not answered yet. The first time in a run of the app the CLI is started to ask it, a few seconds)
  const [names, setNames] = useState<ModelChoice[] | null>(null);
  useEffect(() => {
    setNames(null);
    let live = true;
    if (adapterId) void modelsOnce(adapterId).then((m) => live && setNames(m)).catch(() => live && setNames([]));
    return () => {
      live = false;
    };
  }, [adapterId]);
  const efforts = agentDef(adapterId)?.efforts ?? [];
  const due = useRestartDue((s) => restartDue(s, wt?.path ?? ""));
  useEffect(() => {
    if (wt && due) void restartIfDue(wt);
  }, [due, wt?.status]);
  // the open panel: the chip it hangs from, and which chip
  const [at, setAt] = useState<{ el: Element; kind: "model" | "perm" } | null>(null);
  const open = (el: Element, kind: "model" | "perm") => {
    const shut = shutBy;
    shutBy = null;
    if (shut && shut.el === el && performance.now() - shut.t < 800) return;
    setAt({ el, kind });
  };

  const items = (now: string, list: Array<[string, string]>, pick: (v: string) => void): MenuItem[] =>
    list.map(([v, label]) => ({ label, icon: v === now ? <Check size={14} /> : undefined, onSelect: () => v !== now && pick(v) }));
  const savedModel = wt?.model ?? "";
  // (a model saved from before that the list doesn't offer is still listed, as what is picked)
  const offered = (names ?? []).filter((m) => m.listed || m.id === savedModel).map((m): [string, string] => [m.id, m.label]);
  const models: Array<[string, string]> = savedModel && !offered.some(([v]) => v === savedModel) ? [[savedModel, savedModel], ...offered] : offered;
  const model = (el: Element) => {
    if (!wt) return;
    // one kind of choice: a plain menu
    if (!efforts.length) return openMenu(el, [...items(savedModel, [["", "기본"], ...models], (m) => void changeAgent(wt, { model: m })), ...(names ? [] : [{ label: "불러오는 중…", disabled: true, onSelect: () => {} }])], { minWidth: 180, corner });
    open(el, "model");
  };
  const perm = (el: Element) => {
    if (wt) open(el, "perm");
  };
  // the permission mode (a launch flag) and the 추천 수준 (in the brief): a running agent takes either on a restart
  const permNow = wt?.permissionMode ?? "";
  const perms = permissionsFor(adapterId).map((p): [string, string] => [p.id, p.label]);
  const suggestNow = suggestOf(wt?.suggest);
  const closed = (kind: "model" | "perm", [a, b]: Pick) => {
    setAt(null);
    if (!wt) return;
    const patch =
      kind === "model"
        ? { ...(a !== savedModel && { model: a }), ...(b !== (wt.effort ?? "") && { effort: b }) }
        : { ...(a !== permNow && { permissionMode: a }), ...(b !== suggestNow && { suggest: b }) };
    if (Object.keys(patch).length) void changeAgent(wt, patch);
  };
  const cols: { left: Col; right: Col; label: string } | null = !at
    ? null
    : at.kind === "model"
      ? {
          label: "모델과 추론 수준",
          left: { name: "모델", list: [["", "기본"], ...(names ? models : [])], now: savedModel, wait: !names },
          right: { name: "추론 수준", list: [["", "기본"], ...efforts.map((e): [string, string] => [e, effortLabel(e)])], now: wt?.effort ?? "" },
        }
      : {
          label: "권한과 캔버스 추천",
          // (a mode saved from before that the list doesn't offer is still listed, as what is picked)
          left: { name: "권한", list: perms.some(([v]) => v === permNow) || !permNow ? perms : [[permNow, permissionOf(permNow).label], ...perms], now: permNow },
          right: { name: "캔버스 추천", list: SUGGEST_LEVELS.map((id): [string, string] => [id, SUGGEST_NAMES[id].label]), now: suggestNow },
        };
  const panel =
    wt && at && cols
      ? createPortal(<RunPanel key={at.kind} at={at.el} label={cols.label} left={cols.left} right={cols.right} corner={corner} onClose={(picked) => closed(at.kind, picked)} />, document.body)
      : null;
  const modelName = wt?.model ? (names?.find((m) => m.id === wt.model)?.label ?? wt.model) : "기본 모델";
  // (the level beside the model's name; none shown while it is the CLI's own)
  const effortName = wt?.effort && efforts.length ? effortLabel(wt.effort) : "";
  return { due, model, perm, suggestNow, modelName, effortName, levels: efforts.length > 0, panel };
}
