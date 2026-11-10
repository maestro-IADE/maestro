// The commands themselves come per agent from the backend's catalog (`ipc.slashCommands`, `ipc.agentModels`).
export type SlashCmd = { name: string; description: string; source: string; note?: string };

/** Prefix matches first, then substring matches in name or description. */
export function matchCommands(all: SlashCmd[], query: string): SlashCmd[] {
  const q = query.toLowerCase();
  const pre = all.filter((c) => c.name.toLowerCase().startsWith(q));
  const sub = all.filter((c) => !pre.includes(c) && (c.name.toLowerCase().includes(q) || c.description.toLowerCase().includes(q)));
  return [...pre, ...sub];
}
