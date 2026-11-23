import { useEffect } from "react";
import { getAllWindows } from "@tauri-apps/api/window";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import type { AttentionItem } from "../attention";
import { BURST_MS, coalesce, live, notices, type Notice, type NotifyAgent, type NotifySnap } from "../osNotify";
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
const snapOf = (s: State, items: AttentionItem[]): NotifySnap => ({
  items,
  agents: s.worktrees.map((w) => ({ path: w.path, name: w.name, root: w.root, status: w.status, isMaestro: w.isMaestro })),
  projects: s.repos,
});

/**
 * OS 알림 (docs/features/ui/os-notifications.md), the main window only (`main`): while no Maestro window has focus,
 * an agent that starts waiting or (설정 "모두") ends its turn is told of in a notification. They gather for
 * `BURST_MS`, then go out as one per agent, or one summary for several, if still true and the app still unfocused.
 */
export function useOsNotify(main: boolean) {
  useEffect(() => {
    if (!main) return;
    let s0 = useStore.getState();
    let last = snapOf(s0, selectAttention(s0));
    let pending: Notice[] = [];
    let timer: ReturnType<typeof setTimeout> | undefined;

    const flush = async () => {
      timer = undefined;
      const s = useStore.getState();
      const out = coalesce(live(pending, snapOf(s, selectAttention(s)), useSettings.getState().osNotify));
      pending = [];
      if (!out.length || (await appFocused()) || !(await permitted())) return;
      for (const n of out) sendNotification(n);
    };

    const un = useStore.subscribe((s) => {
      const items = selectAttention(s);
      // most store changes (streamed chat, layout) touch none of these
      if (items === last.items && s.worktrees === s0.worktrees && s.repos === s0.repos) return;
      s0 = s;
      const next = snapOf(s, items);
      const level = useSettings.getState().osNotify;
      const found = level === "off" || document.hasFocus() ? [] : notices(last, next, level, reason);
      last = next;
      if (!found.length) return;
      pending.push(...found);
      timer ??= setTimeout(() => void flush(), BURST_MS);
    });
    return () => {
      un();
      clearTimeout(timer);
    };
  }, [main]);
}
