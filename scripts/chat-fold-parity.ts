// The chat's screen model (src/chatFold.ts) over the fixture the Rust parity test writes
// (src-tauri/tests/fixtures/chat-parity.json, `chat_fold_fixture_is_current`): what the chat draws from each sample's
// common events must be the items the pre-R3 fold drew from the same transcript lines, and for the `acp-failed` case
// (turns the ACP runtime ends itself) what the Rust mirror (`events::oracle::items`) draws. Both ways in: the history
// at once (`loadChat`) and event by event as the stream sends them (`reduceChat`). Run: `npm test`.
import { readFileSync } from "node:fs";
import { deepStrictEqual } from "node:assert/strict";
import { chatItems, emptyChat, isDrawn, loadChat, reduceChat, type ChatItem, type ChatState } from "../src/chatFold.ts";

/** What the chat draws, in the item shape before I1 (`Item`: user, text, tool, note), as the fixture holds it. */
function drawn(s: ChatState) {
  const out = chatItems(s)
    .filter(isDrawn)
    .map((it: ChatItem) => {
      switch (it.kind) {
        case "user":
          return { kind: "user", text: it.text, files: it.files };
        case "text":
          return { kind: "text", text: it.text };
        case "tool":
          return { kind: "tool", id: it.id, name: it.name, input: it.input, result: it.result, isError: it.isError };
        case "notice":
          return it.noticeKind === "failed" ? { kind: "note", text: it.text, error: true } : { kind: "note", text: it.text };
        default:
          throw new Error(`isDrawn let ${it.kind} through`);
      }
    });
  // a JSON round trip, as the fixture: keys left undefined drop out
  return JSON.parse(JSON.stringify(out));
}

const url = new URL("../src-tauri/tests/fixtures/chat-parity.json", import.meta.url);
const { cases } = JSON.parse(readFileSync(url, "utf8")) as { cases: { name: string; events: any[]; items: any[] }[] };
if (!cases.length) throw new Error("no cases in the fixture");
for (const c of cases) {
  const loaded = loadChat({ epoch: 1, seq: c.events.length, events: c.events });
  deepStrictEqual(drawn(loaded), c.items, `the history drawn differs from the fixture's items: ${c.name}`);
  const streamed = c.events.reduce((s: ChatState, event, i) => reduceChat(s, { epoch: 1, seq: i + 1, event }), emptyChat(1));
  deepStrictEqual(chatItems(streamed), chatItems(loaded), `event by event differs from the history at once: ${c.name}`);
  deepStrictEqual([streamed.seq, streamed.durable], [c.events.length, c.events.length], `seq and durable count: ${c.name}`);
  console.log(`ok ${c.name}: ${c.events.length} events → ${c.items.length} drawn items (${chatItems(loaded).length} in all)`);
}
const failed = loadChat({ epoch: 1, seq: 1, events: [{ type: "notice", turnId: "t", itemId: "t:acp", kind: "failed", text: "x" }] });
deepStrictEqual(drawn(failed), [{ kind: "note", text: "x", error: true }], "a failed notice draws as an error note");

// The chat runtime's live text (R4, runtime-events.md): deltas on a `live:<n>:<k>` item, retired by an empty
// `offset: 0` delta just before its durable item lands, leave what the durable events alone draw. Offsets are UTF-16
// units, as JS strings count them.
const durable: any[] = [
  { type: "turnStarted", turnId: "t1", initiator: "user" },
  { type: "userMessage", turnId: "t1", itemId: "u1", kind: "prompt", text: "hi", files: [] },
  { type: "agentText", turnId: "t1", itemId: "a1", messageId: "m1", text: "안녕하세요 👋" },
  { type: "turnEnded", turnId: "t1", outcome: { kind: "done", stopReason: "end_turn" } },
];
const delta = (offset: number, text: string, channel = "text") => ({ type: "delta", turnId: "t1", itemId: "live:1:0", channel, offset, text });
/** durable events take the next seq, live ones none, as `agent://event` sends them */
const stream = (evs: any[]) => {
  let seq = 0;
  return evs.reduce((s: ChatState, event) => reduceChat(s, { epoch: 1, seq: event.type === "delta" ? null : ++seq, event }), emptyChat(1));
};
const streaming = [durable[0], durable[1], delta(0, "안녕"), delta(2, "하세요 👋")];
deepStrictEqual(drawn(stream(streaming)).at(-1), { kind: "text", text: "안녕하세요 👋" }, "live text draws while it streams");
const retired = [...streaming, delta(0, ""), durable[2], durable[3]];
deepStrictEqual(drawn(stream(retired)), drawn(stream(durable)), "retired live text leaves only the durable items");
deepStrictEqual(drawn(stream([durable[0], durable[1], delta(0, "생각", "thinking")])), drawn(stream(durable.slice(0, 2))), "thinking deltas draw nothing");
console.log("ok live deltas: drawn while streaming, gone once retired");

// Claude writes `end_turn` on the thinking line before the answer's own line, so a turn's end can come mid-answer:
// the answer keeps all its text until its retire, then only the durable items are left.
const midAnswer = [durable[0], durable[1], delta(0, "안녕"), durable[3], delta(2, "하세요 👋")];
deepStrictEqual(drawn(stream(midAnswer)).at(-1), { kind: "text", text: "안녕하세요 👋" }, "a turn's end mid-answer keeps the streamed text");
deepStrictEqual(drawn(stream([...midAnswer, delta(0, ""), durable[2]])), drawn(stream([durable[0], durable[1], durable[3], durable[2]])), "the retire drops the live text");
deepStrictEqual(chatItems(stream([...midAnswer, delta(0, "")])).filter((it) => it.kind === "text"), [], "a retired item takes no room");
console.log("ok live deltas held: a turn's end leaves them, the retire takes them");
