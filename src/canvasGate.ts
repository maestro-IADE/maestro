// The gate between what an agent wants to show on its canvas and the user letting it in (docs/features/canvas/
// canvas-controls.md "그림 넘기기", 눌러서): the canvas says how many steps wait, and the chat's 허용 lets the next
// one in, the same as pressing the caption on the canvas.
import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";

const keyOf = (path: string) => path.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();

type Gate = {
  /** steps an open canvas holds back, by agent */
  waiting: Record<string, number>;
  /** things that came for an agent whose canvas isn't open (they wait in the backend until it opens) */
  arrived: Record<string, number>;
  /** 허용 pressed while the canvas wasn't open: it lets that many steps in when it opens */
  passes: Record<string, number>;
  /** 허용 pressed while the canvas is open: the canvas watches this count */
  taps: Record<string, number>;
};
export const useGate = create<Gate>(() => ({ waiting: {}, arrived: {}, passes: {}, taps: {} }));

const open = new Set<string>(); // the agents whose canvas is on screen
const bump = (field: keyof Gate, k: string, to: (n: number) => number) => useGate.setState((s) => ({ [field]: { ...s[field], [k]: Math.max(0, to(s[field][k] ?? 0)) } }) as Partial<Gate>);

export const gate = {
  key: keyOf,
  /** How many steps wait for this agent, as far as is known. */
  count: (s: Gate, path: string) => (s.waiting[keyOf(path)] ?? 0) + (s.arrived[keyOf(path)] ?? 0),
  /** A canvas came on screen (it takes what waited in the backend itself) or left. */
  mount(path: string, on: boolean) {
    const k = keyOf(path);
    if (on) open.add(k);
    else open.delete(k);
    bump("arrived", k, () => 0);
    if (!on) bump("waiting", k, () => 0);
  },
  /** The open canvas says how many steps it holds back. */
  setWaiting(path: string, n: number) {
    const k = keyOf(path);
    if ((useGate.getState().waiting[k] ?? 0) !== n) bump("waiting", k, () => n);
  },
  /** 허용: the next step may come in. With the canvas closed it is remembered for when it opens. */
  pass(path: string) {
    const k = keyOf(path);
    if (open.has(k)) return bump("taps", k, (n) => n + 1);
    if ((useGate.getState().arrived[k] ?? 0) > 0) {
      bump("arrived", k, (n) => n - 1);
      bump("passes", k, (n) => n + 1);
    }
  },
  /** The canvas, on opening: how many steps were let in while it was closed (used up by asking). */
  takePasses(path: string) {
    const k = keyOf(path), n = useGate.getState().passes[k] ?? 0;
    if (n) bump("passes", k, () => 0);
    return n;
  },
};

// something was queued for an agent: when its canvas isn't open, nobody takes it yet
void listen<string>("maestro://draw", (e) => {
  const k = keyOf(e.payload);
  if (!open.has(k)) bump("arrived", k, (n) => n + 1);
});
