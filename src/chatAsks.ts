// The choices an agent puts in its chat (`maestro ask --in chat`, docs/features/agents/chat-choices.md): which wait
// for the user, by agent. The backend keeps them (memory only); this follows it.
import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { ipc } from "./ipc";

/** A choice in the chat: what it asks, the options (none: free text only), whether several may be picked. */
export type ChatAsk = { id: string; title: string; body?: string; choices: string[]; multi: boolean; other: boolean };

const keyOf = (path: string) => path.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();

export const useChatAsks = create<{ byAgent: Record<string, ChatAsk[]> }>(() => ({ byAgent: {} }));

/** The choices waiting in this agent's chat (a stable empty list when there are none). */
const NONE: ChatAsk[] = [];
export const chatAsksOf = (s: { byAgent: Record<string, ChatAsk[]> }, path: string) => s.byAgent[keyOf(path)] ?? NONE;

/** Reads this agent's waiting choices from the backend (on showing its chat, and whenever they change). */
export const loadChatAsks = (path: string) =>
  ipc
    .chatAsks(path)
    .then((list) => useChatAsks.setState((s) => ({ byAgent: { ...s.byAgent, [keyOf(path)]: list } })))
    .catch(() => {});

// an agent asked, or a choice was answered or closed
void listen<string>("maestro://asks", (e) => void loadChatAsks(e.payload));
