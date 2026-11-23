import { useEffect } from "react";
import { getAllWindows } from "@tauri-apps/api/window";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import type { AttentionItem } from "../attention";
import { useAsked } from "../asked";
import { useChatAsks } from "../chatAsks";
// (its `maestro://draw` listener reads canvas questions in for a closed canvas)
import "../canvasGate";
import { askedAfter, BURST_MS, coalesce, heldAfter, live, loaded, notices, waitingAsks, type Notice, type NotifyAgent, type NotifySnap } from "../osNotify";
import { useSettings } from "../settings";
import { reasonText, rowMeta } from "../sidebarModel";
import { selectAttention, useStore } from "../store";

// asked once per run, the first time something would go out; a refusal keeps it quiet (no toast, no asking again)
let allowed: Promise<boolean> | null = null;
const permitted = () =>
  (allowed ??= isPermissionGranted()
    .then(async (ok) => ok || (await requestPermission()) === "granted")
    .catch(() => false));

/** The user is in the app: this window or a pop-out one has focus. */
async function appFocused(): Promise<boolean> {
  if (document.hasFocus()) return true;
  try {
    const wins = await getAllWindows();
    return (await Promise.all(wins.map((w) => w.isFocused().catch(() => false)))).some(Boolean);
  } catch {
    return false;
  }
}

// the words its sidebar row says (sidebarModel.ts); an agent that asks has one, so never "에이전트 연결 필요"
const reason = (a: Pick<NotifyAgent, "path" | "isMaestro">, items: AttentionItem[]) =>
  reasonText(rowMeta({ ...a, adapterId: "asking" }, items));

type State = ReturnType<typeof useStore.getState>;
/** What a snapshot is made of: when none of these changed, nothing the notifier looks at did. */
type Src = { items: AttentionItem[]; worktrees: State["worktrees"]; repos: State["repos"]; asked: ReturnType<typeof useAsked.getState>["list"]; chat: ReturnType<typeof useChatAsks.getState>["byAgent"] };
const srcOf = (s: State): Src => ({ items: selectAttention(s), worktrees: s.worktrees, repos: s.repos, asked: useAsked.getState().list, chat: useChatAsks.getState().byAgent });
const sameSrc = (a: Src, b: Src) => a.items === b.items && a.worktrees === b.worktrees && a.repos === b.repos && a.asked === b.asked && a.chat === b.chat;
const snapOf = (src: Src, held: string[]): NotifySnap => ({
  items: src.items,
  asks: waitingAsks(src.asked, src.chat),
  agents: src.worktrees.map((w) => ({ path: w.path, name: w.name, root: w.root, status: w.status, isMaestro: w.isMaestro, workers: w.workers })),
  projects: src.repos,
  held,
});

/**
 * OS 알림 (docs/features/ui/os-notifications.md), the main window only (`main`): while no Maestro window has focus,
 * an agent that starts waiting (확인할 일, or a question it asks: asked.ts) or (설정 "모두") is really done is told of
 * in a notification. They gather for `BURST_MS`, then go out as one per agent, or one summary for several, if still
 * true and the app still unfocused. Nothing until the launch has loaded (`loaded`): what it reads in had been waiting
 * already.
 */
export function useOsNotify(main: boolean) {
  useEffect(() => {
    if (!main) return;
    let src = srcOf(useStore.getState());
    // the waits that came in since their agent last began a turn (`heldAfter`): they keep its end from being done
    let held: string[] = [];
    let last = snapOf(src, held);
    // once true it stays: a project added later loads its agents with the app in hand
    let started = loaded(useStore.getState());
    let pending: Notice[] = [];
    let timer: ReturnType<typeof setTimeout> | undefined;

    const flush = async () => {
      timer = undefined;
      const out = coalesce(live(pending, snapOf(srcOf(useStore.getState()), held), useSettings.getState().osNotify));
      pending = [];
      if (!out.length || (await appFocused()) || !(await permitted())) return;
      for (const n of out) sendNotification(n);
    };

    // any of the three stores changed (the app's, the chat's choices, the questions heard)
    const check = () => {
      const s = useStore.getState();
      const now = srcOf(s);
      // most store changes (streamed chat, layout) touch none of these
      if (sameSrc(now, src)) return;
      src = now;
      const before = last;
      // (counted before the notices, so a worker's wait that comes with its Lead's turn end holds that end too)
      const seen = snapOf(now, held);
      held = heldAfter(held, before, seen);
      const next = { ...seen, held };
      last = next;
      const level = useSettings.getState().osNotify;
      started ||= loaded(s);
      const found = !started || level === "off" || document.hasFocus() ? [] : notices(before, next, level, reason);
      if (found.length) {
        pending.push(...found);
        timer ??= setTimeout(() => void flush(), BURST_MS);
      }
      // an agent that began another turn: its canvas questions no longer wait (this runs `check` again, at the end)
      const kept = askedAfter(now.asked, before, next);
      if (kept !== now.asked) useAsked.setState({ list: kept });
    };
    const offs = [useStore.subscribe(check), useAsked.subscribe(check), useChatAsks.subscribe(check)];
    return () => {
      for (const off of offs) off();
      clearTimeout(timer);
    };
  }, [main]);
}
