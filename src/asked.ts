// The questions agents put to the user that this window heard being asked (`maestro ask`: a choice in the chat, a
// question or question set on the canvas), for the OS notice (docs/features/ui/os-notifications.md). Read in by
// chatAsks.ts (the chat's choices) and by canvasGate.ts and Canvas.tsx (canvas questions: waiting in the backend for
// a closed canvas, or taken onto an open one). The main window's `useOsNotify` reads the list and lets canvas
// questions go when their agent begins another turn (`askedAfter`).
import { create } from "zustand";
import { newAsks, type Asked } from "./osNotify";

export const useAsked = create<{ list: Asked[] }>(() => ({ list: [] }));

// every question met in this window, heard or read: one met before is never news again (`newAsks`)
const known = new Set<string>();
const MAX = 100;

/**
 * Questions read for agent `path`. `heard`: read because the agent just asked (its `maestro://asks` or
 * `maestro://draw` event), so the ones not met before are news; otherwise (read on showing its chat or opening its
 * canvas) they had been waiting already and are only remembered.
 */
export function noteAsks(path: string, found: ReadonlyArray<Omit<Asked, "path">>, heard: boolean) {
  const news = newAsks(found, known, heard);
  if (!news.length) return;
  useAsked.setState((s) => ({ list: [...s.list, ...news.map((a) => ({ ...a, path }))].slice(-MAX) }));
}
