import { createContext, useContext } from "react";

/**
 * A place in a tab group's tab bar that the full-screen agent on screen fills with its own controls (stop, chat ↔
 * terminal, its menu): the agent panel portals them there instead of drawing a header row under the tabs
 * (docs/features/ui/stage-tabs.md). `slot` is null while the group shows something else (a canvas, a file).
 */
export const TabSlot = createContext<{ slot: HTMLElement | null; setSlot: (el: HTMLElement | null) => void } | null>(null);
export const useTabSlot = () => useContext(TabSlot);
