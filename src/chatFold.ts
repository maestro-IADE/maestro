import type { RuntimeEvent, TurnOutcome } from "./types";

// The chat's screen model (interface PRD I1): common events (events.rs) → chat items, one event at a time. Pure:
// `reduceChat(state, input)` returns a new state and never changes the old one; an input that changes nothing
// returns the same state (no redraw). Only types are imported, so `npm test` runs it under Node.

type Ev<T extends RuntimeEvent["type"]> = Extract<RuntimeEvent, { type: T }>;
export type ToolStatus = NonNullable<Ev<"toolUpdated">["status"]>;
export type Resolution = Ev<"approvalResolved">["resolution"];

/** `key`: unique in a chat, made in arrival order (the same history gives the same keys); `turnId`: as the event said */
type Base = { key: string; turnId: string | null };

/**
 * One thing in the chat. What draws today (until I2's cards) is `isDrawn`: user, text, tool and notice, as before I1.
 * The rest is kept for I2: thinking, approvals, errors, turn edges, and what the app doesn't know (`unknown`, raw kept).
 */
export type ChatItem =
  | (Base & { kind: "user"; id: string; via: Ev<"userMessage">["kind"]; text: string; files: string[] })
  /** agent text (Markdown); `live`: streamed by the chat runtime (`live:<n>:<k>`), dropped once its block lands */
  | (Base & { kind: "text"; id: string; text: string; live?: true })
  | (Base & { kind: "thinking"; id: string; text: string; live?: true })
  | (Base & { kind: "tool"; id: string; name: string; input: any; title?: string; status?: ToolStatus; result?: string; isError?: boolean })
  /** seen live only (approvals aren't in the history) */
  | (Base & { kind: "approval"; id: number; toolCallId: string | null; toolName: string; toolInput: any; canAlways: boolean; resolution?: Resolution })
  | (Base & { kind: "error"; errorKind: string; message: string })
  | (Base & { kind: "notice"; id: string; noticeKind: Ev<"notice">["kind"]; text: string })
  /** `at`: when it arrived (ms), for a turn seen live; none from the history (events carry no time) */
  | (Base & { kind: "turnStart"; id: string; initiator: Ev<"turnStarted">["initiator"]; at?: number })
  /** `ms`: how long the turn took, when both of its edges came live */
  | (Base & { kind: "turnEnd"; id: string; outcome: TurnOutcome; ms?: number })
  /** an `unknown` event, or an event type this build doesn't know (`source` "event:<type>", `raw` the whole event) */
  | (Base & { kind: "unknown"; source: string; raw: unknown });

export type ChatState = {
  /** the stream these items came from (`agent://event` epoch) */
  readonly epoch: number;
  /** the last durable event folded in; one at or below it again is a duplicate */
  readonly seq: number;
  /** earlier turns, one array each, oldest first: never copied by an event of the latest turn */
  readonly turns: readonly (readonly ChatItem[])[];
  /** the latest turn's items, from its turnStart (all of them before any turnStarted) */
  readonly tail: readonly ChatItem[];
  /** the latest turnStarted's id: an event of this turn looks only in `tail` */
  readonly turn: string | null;
  /** keys made so far */
  readonly n: number;
  /** durable events folded in (history and seq'd live ones): moves with the transcript, not with live text */
  readonly durable: number;
};

/** One `agent://event` payload; `at` = when it arrived (ms), for turn durations. */
export type ChatInput = { epoch: number; seq: number | null; event: RuntimeEvent; at?: number };

export function emptyChat(epoch = -1): ChatState {
  return { epoch, seq: -1, turns: [], tail: [], turn: null, n: 0, durable: 0 };
}

/**
 * Folds one event in. An older epoch is ignored; a newer one starts from empty (the stream started over: the caller
 * refetches the history too). A durable event at or below `seq` is a duplicate. Live-only events (`seq` null: text
 * deltas, approvals, exit, reset) never move `seq`.
 */
export function reduceChat(s: ChatState, input: ChatInput): ChatState {
  if (input.epoch < s.epoch) return s;
  const base = input.epoch > s.epoch ? emptyChat(input.epoch) : s;
  const durable = input.seq !== null;
  if (input.seq !== null && input.seq <= base.seq) return base;
  const d = draft(base);
  apply(d, input.event, input.at);
  if (!d.dirty && !durable) return base;
  return done(d, base.epoch, input.seq ?? base.seq, base.durable + (durable ? 1 : 0));
}

/** The history `get_transcript` returned (no per-event seq, no times), folded in one go. */
export function loadChat(t: { epoch: number; seq: number; events: RuntimeEvent[] }): ChatState {
  const d = draft(emptyChat(t.epoch));
  for (const ev of t.events) apply(d, ev);
  return done(d, t.epoch, t.seq, t.events.length);
}

/** Every item in order. The chat itself draws turn by turn (`turns`, then `tail`). */
export function chatItems(s: ChatState): ChatItem[] {
  return [...s.turns.flat(), ...s.tail];
}

/** What the chat draws until I2: a user bubble, non-blank agent text, a tool row, a notice line. */
export function isDrawn(it: ChatItem): boolean {
  return it.kind === "user" || it.kind === "tool" || it.kind === "notice" || (it.kind === "text" && !!it.text.trim());
}

/** Something to draw (else the chat shows its empty line). Stops at the first one. */
export function hasDrawn(s: ChatState): boolean {
  return s.turns.some((t) => t.some(isDrawn)) || s.tail.some(isDrawn);
}

/** The last few lines said, for a block too small to show the chat. */
export function lastText(s: ChatState): string {
  const pick = (items: readonly ChatItem[]) => {
    for (let i = items.length - 1; i >= 0; i--) {
      const it = items[i];
      if ((it.kind === "text" && it.text.trim()) || it.kind === "user") return it.text.split("\n").slice(-3).join("\n");
    }
    return null;
  };
  const t = pick(s.tail);
  if (t !== null) return t;
  for (let k = s.turns.length - 1; k >= 0; k--) {
    const r = pick(s.turns[k]);
    if (r !== null) return r;
  }
  return "";
}

// ---------- the fold ----------

/** A state being changed: arrays are copied the first time they're written (`own` holds the copies). */
type Draft = {
  turns: (readonly ChatItem[])[];
  tail: ChatItem[];
  turn: string | null;
  n: number;
  own: Set<readonly unknown[]>;
  dirty: boolean;
};
/** where an item is: `k` -1 = tail, else `turns[k]` */
type Loc = { k: number; i: number };

const draft = (s: ChatState): Draft => ({
  turns: s.turns as (readonly ChatItem[])[],
  tail: s.tail as ChatItem[],
  turn: s.turn,
  n: s.n,
  own: new Set(),
  dirty: false,
});

const done = (d: Draft, epoch: number, seq: number, durable: number): ChatState => ({
  epoch,
  seq,
  turns: d.turns,
  tail: d.tail,
  turn: d.turn,
  n: d.n,
  durable,
});

function mine<T>(d: Draft, a: readonly T[]): T[] {
  if (d.own.has(a)) return a as T[];
  const c = a.slice();
  d.own.add(c);
  return c;
}

const newKey = (d: Draft) => String(d.n++);

/** Only the latest turn's items can hold what an event of the latest turn names; anything else may be older. */
const deep = (d: Draft, turnId: string | null) => turnId === null || turnId !== d.turn;

/** Newest first: the tail, then (deep) the earlier turns. The latest turn is searched alone, so a lookup costs that
 *  turn's size, not the chat's. */
function find(d: Draft, deep: boolean, hit: (it: ChatItem) => boolean): Loc | null {
  for (let i = d.tail.length - 1; i >= 0; i--) if (hit(d.tail[i])) return { k: -1, i };
  if (deep)
    for (let k = d.turns.length - 1; k >= 0; k--) {
      const c = d.turns[k];
      for (let i = c.length - 1; i >= 0; i--) if (hit(c[i])) return { k, i };
    }
  return null;
}

const itemAt = (d: Draft, l: Loc) => (l.k < 0 ? d.tail[l.i] : d.turns[l.k][l.i]);

function edit(d: Draft, l: Loc, change: (items: ChatItem[]) => void) {
  if (l.k < 0) {
    d.tail = mine(d, d.tail);
    change(d.tail);
  } else {
    d.turns = mine(d, d.turns);
    const c = mine(d, d.turns[l.k]);
    change(c);
    d.turns[l.k] = c;
  }
  d.dirty = true;
}
const put = (d: Draft, l: Loc, it: ChatItem) => edit(d, l, (items) => void (items[l.i] = it));
const drop = (d: Draft, l: Loc) => edit(d, l, (items) => void items.splice(l.i, 1));

function push(d: Draft, it: ChatItem) {
  d.tail = mine(d, d.tail);
  d.tail.push(it);
  d.dirty = true;
}

/** Text and thinking blocks are upserted by item id: ACP sends a block empty, then whole. */
function upsertText(d: Draft, kind: "text" | "thinking", turnId: string, id: string, next: (old: string) => string, live: boolean) {
  const l = find(d, deep(d, turnId), (it) => it.kind === kind && it.id === id);
  if (!l) return push(d, { kind, key: newKey(d), turnId, id, text: next(""), ...(live ? { live: true as const } : {}) });
  const old = itemAt(d, l) as Extract<ChatItem, { kind: "text" | "thinking" }>;
  const text = next(old.text);
  if (text !== old.text) put(d, l, { ...old, text });
}

function apply(d: Draft, ev: RuntimeEvent, at?: number) {
  switch (ev.type) {
    case "turnStarted":
      // the latest turn's items become an earlier turn's, as they are (no copy)
      if (d.tail.length) {
        d.turns = mine(d, d.turns);
        d.turns.push(d.tail);
      }
      d.tail = [];
      d.own.add(d.tail);
      d.turn = ev.turnId;
      return push(d, { kind: "turnStart", key: newKey(d), turnId: ev.turnId, id: ev.turnId, initiator: ev.initiator, ...(at !== undefined ? { at } : {}) });
    case "userMessage":
      return push(d, { kind: "user", key: newKey(d), turnId: ev.turnId, id: ev.itemId, via: ev.kind, text: ev.text, files: ev.files ?? [] });
    case "agentText":
      return upsertText(d, "text", ev.turnId, ev.itemId, () => ev.text, false);
    case "thinking":
      return upsertText(d, "thinking", ev.turnId, ev.itemId, () => ev.text, false);
    case "delta": {
      if (ev.channel === "toolOutput") return; // not streamed yet
      const kind = ev.channel === "text" ? "text" : "thinking";
      // the empty `offset: 0` delta retires a live item just before its durable block lands. Not a turn's end:
      // Claude writes `end_turn` on the thinking line before the answer's own line, so a turn can end mid-answer.
      if (ev.offset === 0 && ev.text === "") {
        const l = find(d, true, (it) => it.kind === kind && it.id === ev.itemId && !!it.live);
        if (l) drop(d, l);
        return;
      }
      // grown at its offset (UTF-16 units, as JS strings count): a delta sent twice changes nothing
      return upsertText(d, kind, ev.turnId, ev.itemId, (t) => t.slice(0, ev.offset) + ev.text, true);
    }
    case "toolStarted":
      return push(d, { kind: "tool", key: newKey(d), turnId: ev.turnId, id: ev.toolCallId, name: ev.name, input: ev.input });
    case "toolUpdated": {
      const l = find(d, deep(d, ev.turnId), (it) => it.kind === "tool" && it.id === ev.toolCallId);
      if (!l) return;
      const t = { ...(itemAt(d, l) as Extract<ChatItem, { kind: "tool" }>) };
      if (ev.input != null) t.input = ev.input;
      if (ev.output != null) t.result = ev.output;
      if (ev.title != null) t.title = ev.title;
      if (ev.status) t.status = ev.status;
      if (ev.status === "failed") t.isError = true;
      return put(d, l, t);
    }
    case "toolFinished": {
      const l = find(d, deep(d, ev.turnId), (it) => it.kind === "tool" && it.id === ev.toolCallId);
      if (!l) return; // no start to pair with: dropped
      return put(d, l, { ...(itemAt(d, l) as Extract<ChatItem, { kind: "tool" }>), result: ev.output, isError: ev.isError });
    }
    case "notice":
      return push(d, { kind: "notice", key: newKey(d), turnId: ev.turnId, id: ev.itemId, noticeKind: ev.kind, text: ev.text });
    case "error":
      return push(d, { kind: "error", key: newKey(d), turnId: ev.turnId, errorKind: ev.kind, message: ev.message });
    case "turnEnded": {
      const l = find(d, deep(d, ev.turnId), (it) => it.kind === "turnStart" && it.id === ev.turnId);
      const from = l ? (itemAt(d, l) as Extract<ChatItem, { kind: "turnStart" }>).at : undefined;
      const ms = from !== undefined && at !== undefined ? Math.max(0, at - from) : undefined;
      return push(d, { kind: "turnEnd", key: newKey(d), turnId: ev.turnId, id: ev.turnId, outcome: ev.outcome, ...(ms !== undefined ? { ms } : {}) });
    }
    case "unknown":
      return push(d, { kind: "unknown", key: newKey(d), turnId: ev.turnId, source: ev.source, raw: ev.raw });
    case "approvalRequested": {
      const it: ChatItem = {
        kind: "approval",
        key: "",
        turnId: ev.turnId,
        id: ev.approvalId,
        toolCallId: ev.toolCallId,
        toolName: ev.toolName,
        toolInput: ev.toolInput,
        canAlways: ev.canAlways,
      };
      const l = find(d, deep(d, ev.turnId), (x) => x.kind === "approval" && x.id === ev.approvalId);
      return l ? put(d, l, { ...it, key: itemAt(d, l).key }) : push(d, { ...it, key: newKey(d) });
    }
    case "approvalResolved": {
      const l = find(d, deep(d, ev.turnId), (x) => x.kind === "approval" && x.id === ev.approvalId);
      if (!l) return;
      const old = itemAt(d, l) as Extract<ChatItem, { kind: "approval" }>;
      if (old.resolution !== ev.resolution) put(d, l, { ...old, resolution: ev.resolution });
      return;
    }
    case "exited": // the process ended; the next start comes as a new epoch
    case "reset": // the epoch changed: the caller refetches
      return;
    default: {
      // a type this build doesn't know: kept whole
      const raw = ev as { type?: unknown; turnId?: unknown };
      const turnId = typeof raw.turnId === "string" ? raw.turnId : null;
      return push(d, { kind: "unknown", key: newKey(d), turnId, source: `event:${String(raw.type)}`, raw: ev });
    }
  }
}
