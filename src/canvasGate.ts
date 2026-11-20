// The gate between what an agent puts up for its canvas and the canvas itself (docs/features/agents/chat-visuals.md,
// docs/features/canvas/canvas-controls.md "그림 넘기기"). The chat is the window onto it: it shows the next thing as a
// small picture, 캔버스에 올리기 lets it onto the canvas (the same as pressing the line on the canvas) and 버리기 throws
// it away. An open canvas holds the waiting steps itself and says so here; for a closed one they wait in the backend
// and are read from there, left where they are.
import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { ipc } from "./ipc";
import { stepsOf } from "./canvasGuide";
import { keyOf, visualOf, type Gate, type Queued, type Visual } from "./visualSteps";
import type { Mini } from "./canvasMini";

export { chatView, type Queued, type Visual } from "./visualSteps";

/**
 * How the chat draws a step's picture when no open canvas made one (chatVisuals.ts puts its functions here as it
 * loads). Not imported by the card itself: chatVisuals.ts needs the canvas's blocks, which need the agent block, which
 * shows the card, and that circle would run the blocks' sizes before the grid they are counted in exists.
 */
export const painter: {
  items?: (items: readonly Queued[], owner: string) => Mini | null;
  pointed?: (root: string, key: string, owner: string, before: readonly Queued[], names: readonly string[]) => Promise<Mini | null>;
} = {};

export const useGate = create<Gate>(() => ({ open: {}, waiting: {}, queued: {}, passes: {}, closed: {}, loose: {} }));
const put = <F extends keyof Gate>(field: F, k: string, v: Gate[F][string]) => useGate.setState((s) => ({ [field]: { ...s[field], [k]: v } }) as Partial<Gate>);

/** What the chat says to an open canvas: let the next step in, or throw one away. */
export type Signal = { type: "pass" } | { type: "drop"; id: string };
const ears = new Map<string, Set<(s: Signal) => void>>();

const NONE: Visual[] = [];
const lastSeq = (v: Visual) => Math.max(0, ...v.seqs);

export const gate = {
  key: keyOf,
  /** A canvas came on screen (it takes what waited in the backend itself) or left. */
  mount(path: string, on: boolean) {
    const k = keyOf(path);
    put("open", k, on);
    if (on) put("queued", k, NONE);
    else {
      put("waiting", k, NONE);
      void gate.peek(path);
    }
  },
  /** The open canvas says which steps it holds back. */
  setWaiting(path: string, list: Visual[]) {
    const k = keyOf(path), now = useGate.getState().waiting[k] ?? NONE;
    if (now.length !== list.length || now.some((v, i) => v.id !== list[i].id)) put("waiting", k, list);
  },
  /** Reads what waits in the backend for a canvas that isn't open. */
  peek(path: string) {
    const k = keyOf(path);
    return ipc
      .peekCanvasDrawings(path)
      .then((items) => {
        const s = useGate.getState();
        if (s.open[k]) return;
        const steps = stepsOf(items as Queued[]).map((st, i) => visualOf(`q${st.items[0]?.seq ?? i}`, st));
        put("queued", k, steps);
        if ((s.passes[k] ?? 0) > steps.length) put("passes", k, steps.length);
      })
      .catch(() => {});
  },
  /** 캔버스에 올리기: the next step may come in. With the canvas closed it is remembered for when it opens. */
  pass(path: string) {
    const k = keyOf(path), s = useGate.getState();
    if (s.open[k]) return void ears.get(k)?.forEach((f) => f({ type: "pass" }));
    const n = s.passes[k] ?? 0;
    if (n < (s.queued[k] ?? NONE).length) put("passes", k, n + 1);
  },
  /**
   * 버리기: the step doesn't go onto the canvas, and the agent is told. An open canvas takes away what it was keeping
   * out of sight; a closed one's step leaves the backend's queue.
   */
  drop(path: string, v: Visual) {
    const k = keyOf(path);
    if (useGate.getState().open[k]) return void ears.get(k)?.forEach((f) => f({ type: "drop", id: v.id }));
    return ipc.discardCanvasDrawing(path, v.seqs, v.what).then(() => gate.peek(path));
  },
  /** The chat's card closed without a decision: what waits now isn't shown again (it still goes to the canvas). */
  close(path: string) {
    const k = keyOf(path);
    put("closed", k, Math.max(0, ...(useGate.getState().queued[k] ?? NONE).map(lastSeq)));
  },
  /**
   * The user named a playbook for the turn that starts now (`on`), or that turn ended: while on, what the agent shows
   * comes in without a press. Naming one is saying yes to what it does; an agent left to pick for itself still waits.
   */
  loosen(path: string, on: boolean) {
    const k = keyOf(path);
    if (!!useGate.getState().loose[k] !== on) put("loose", k, on);
  },
  isLoose: (path: string) => !!useGate.getState().loose[keyOf(path)],
  /** The canvas, on opening: how many steps were let in while it was closed (used up by asking). */
  takePasses(path: string) {
    const k = keyOf(path), n = useGate.getState().passes[k] ?? 0;
    if (n) put("passes", k, 0);
    return n;
  },
  /** The open canvas listens for what the chat says; returns how to stop. */
  listen(path: string, f: (s: Signal) => void) {
    const k = keyOf(path), set = ears.get(k) ?? new Set();
    ears.set(k, set);
    set.add(f);
    return () => void set.delete(f);
  },
};

// something was queued for an agent, or thrown away: when its canvas isn't open, the chat reads what waits now
void listen<string>("maestro://draw", (e) => {
  if (!useGate.getState().open[keyOf(e.payload)]) void gate.peek(e.payload);
});
