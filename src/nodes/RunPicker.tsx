import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { changeAgent, restartDue, restartIfDue, useRestartDue } from "../agentActions";
import { agentDef, effortLabel, modelsOnce, permissionOf, permissionsFor } from "../agents";
import { Check } from "../icons";
import type { Worktree } from "../store";
import type { ModelChoice } from "../types";
import { openMenu, place, type MenuItem } from "../ui/Menu";

// An agent's run settings picked where the next message is written (docs/features/agents/chat-view.md "입력창"): the
// model and the reasoning level side by side in one small panel, the permission mode in a menu. For the message box's
// chips and, in the terminal view, the header's.

/** the chip whose press just closed its own panel: the click that follows must not open it again (a toggle) */
let shutBy: { el: Element; t: number } | null = null;

type Pick = { model: string; effort: string };

/**
 * The panel of the model chip: the models on the left (the wider column), the reasoning levels on the right, the
 * current one of each ticked. A press on a model moves the tick and the panel stays, so a level can be picked after
 * it; a press on a level closes the panel. What was picked is handed over once, when it closes (a level picked, a
 * press outside, Esc, the window losing the focus), so the agent restarts once for both.
 * `models`: null while the agent's CLI hasn't named them yet.
 */
function RunPanel({ at, now, models, levels, corner, onClose }: { at: Element; now: Pick; models: Array<[string, string]> | null; levels: Array<[string, string]>; corner: boolean; onClose: (picked: Pick) => void }) {
  const box = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ x: number; y: number } | null>(null);
  const [picked, setPicked] = useState(now);
  const pickedRef = useRef(picked);
  pickedRef.current = picked;

  // as a menu (ui/Menu.tsx `place`): under the chip, or from its bottom right corner (`corner`: over it, right
  // edges in line); placed again when the models arrive and the panel grows
  useLayoutEffect(() => {
    const el = box.current;
    if (el) setPos(place(at, el.offsetWidth, el.offsetHeight, corner));
  }, [models?.length ?? -1]);

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

  const column = (name: string, list: Array<[string, string]>, cur: string, set: (v: string) => void, wait = false) => (
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
      aria-label="모델과 추론 수준"
      tabIndex={-1}
      style={{ left: pos?.x ?? 0, top: pos?.y ?? 0, visibility: pos ? undefined : "hidden" }}
      onContextMenu={(e) => e.preventDefault()}
    >
      {column("모델", [["", "기본"], ...(models ?? [])], picked.model, (model) => setPicked((p) => ({ ...p, model })), !models)}
      {column("추론 수준", [["", "기본"], ...levels], picked.effort, (effort) => onClose({ ...pickedRef.current, effort }))}
    </div>
  );
}

/**
 * What the chips of an agent's run settings show and open: one chip for the model and the reasoning level (`model`
 * opens their panel; an agent with no reasoning levels gets a plain menu of models), one for the permission mode
 * (`perm` opens its menu). What is picked is saved at once (agentActions.ts `changeAgent`): a running agent restarts
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
  const [at, setAt] = useState<Element | null>(null);

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
    const shut = shutBy;
    shutBy = null;
    if (shut && shut.el === el && performance.now() - shut.t < 800) return;
    setAt(el);
  };
  const perm = (el: Element) => {
    if (!wt) return;
    const now = wt.permissionMode ?? "";
    const list = permissionsFor(adapterId).map((p): [string, string] => [p.id, p.label]);
    openMenu(el, items(now, list.some(([v]) => v === now) || !now ? list : [[now, permissionOf(now).label], ...list], (p) => void changeAgent(wt, { permissionMode: p })), { minWidth: 160, corner });
  };
  const closed = (picked: Pick) => {
    setAt(null);
    if (!wt) return;
    const patch = { ...(picked.model !== savedModel && { model: picked.model }), ...(picked.effort !== (wt.effort ?? "") && { effort: picked.effort }) };
    if (Object.keys(patch).length) void changeAgent(wt, patch);
  };
  const panel =
    wt && at
      ? createPortal(
          <RunPanel at={at} now={{ model: savedModel, effort: wt.effort ?? "" }} models={names ? models : null} levels={efforts.map((e): [string, string] => [e, effortLabel(e)])} corner={corner} onClose={closed} />,
          document.body,
        )
      : null;
  const modelName = wt?.model ? (names?.find((m) => m.id === wt.model)?.label ?? wt.model) : "기본 모델";
  // (the level beside the model's name; none shown while it is the CLI's own)
  const effortName = wt?.effort && efforts.length ? effortLabel(wt.effort) : "";
  return { due, model, perm, modelName, effortName, levels: efforts.length > 0, panel };
}
