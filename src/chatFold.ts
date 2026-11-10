import type { RuntimeEvent } from "./types";

/** What the chat view draws: a user bubble, agent text (Markdown), a tool row, a muted note. */
export type Item =
  | { kind: "user"; text: string; files: string[] }
  | { kind: "text"; text: string }
  | { kind: "tool"; id: string; name: string; input: any; result?: string; isError?: boolean }
  | { kind: "note"; text: string; error?: boolean };

type TextItem = Extract<Item, { kind: "text" }>;
type ToolItem = Extract<Item, { kind: "tool" }>;

/**
 * Common events (events.rs) → chat items. Text is upserted by `itemId` (the ACP runtime sends a block empty, then
 * whole) and grown by deltas at their offset; tools are found by `toolCallId`. Other events draw nothing here.
 */
export function fold(events: RuntimeEvent[]): Item[] {
  const items: Item[] = [];
  const texts = new Map<string, TextItem>();
  const tools = new Map<string, ToolItem>();
  const text = (id: string) => {
    let t = texts.get(id);
    if (!t) {
      t = { kind: "text", text: "" };
      texts.set(id, t);
      items.push(t);
    }
    return t;
  };
  for (const ev of events) {
    switch (ev.type) {
      case "userMessage":
        items.push({ kind: "user", text: ev.text, files: ev.files ?? [] });
        break;
      case "agentText":
        text(ev.itemId).text = ev.text;
        break;
      case "delta":
        if (ev.channel === "text") {
          const t = text(ev.itemId);
          t.text = t.text.slice(0, ev.offset) + ev.text;
        }
        break;
      case "toolStarted": {
        const t: ToolItem = { kind: "tool", id: ev.toolCallId, name: ev.name, input: ev.input };
        tools.set(ev.toolCallId, t);
        items.push(t);
        break;
      }
      case "toolUpdated": {
        const t = tools.get(ev.toolCallId);
        if (t && ev.input != null) t.input = ev.input;
        if (t && ev.output != null) t.result = ev.output;
        if (t && ev.status === "failed") t.isError = true;
        break;
      }
      case "toolFinished": {
        const t = tools.get(ev.toolCallId);
        if (t) {
          t.result = ev.output;
          t.isError = ev.isError;
        }
        break;
      }
      case "notice":
        items.push({ kind: "note", text: ev.text });
        break;
    }
  }
  // blank text draws nothing
  return items.filter((it) => it.kind !== "text" || it.text.trim());
}

/**
 * The chat's event list with one more live delta (useTranscript). Only text deltas draw, so only they are kept. A
 * live item's deltas go once it is retired (its empty `offset: 0` delta, sent just before its durable item lands),
 * not at a turn's end: Claude writes `end_turn` on the thinking line before the answer's own line, so a durable
 * `turnEnded` can come while the answer is still live, and later deltas only carry the rest from their offset.
 */
export function addDelta(events: RuntimeEvent[], ev: Extract<RuntimeEvent, { type: "delta" }>): RuntimeEvent[] {
  if (ev.channel !== "text") return events;
  if (ev.offset === 0 && ev.text === "") return events.filter((e) => e.type !== "delta" || e.itemId !== ev.itemId);
  return [...events, ev];
}

/** The last few lines said, for a block too small to show the chat. */
export function lastText(items: Item[]) {
  for (let i = items.length - 1; i >= 0; i--) {
    const it = items[i];
    if (it.kind === "text" || it.kind === "user") return it.text.split("\n").slice(-3).join("\n");
  }
  return "";
}
