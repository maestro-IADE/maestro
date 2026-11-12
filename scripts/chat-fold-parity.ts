// The chat view's fold (src/chatFold.ts) over the fixture the Rust parity test writes
// (src-tauri/tests/fixtures/chat-parity.json, `chat_fold_fixture_is_current`): the common events of each sample
// must fold to the items the pre-R3 fold drew from the same transcript lines, and the `acp-failed` case (turns the
// ACP runtime ends itself) to what the Rust mirror (`events::oracle::items`) draws. Run: `npm test`.
import { readFileSync } from "node:fs";
import { deepStrictEqual } from "node:assert/strict";
import { addDelta, fold } from "../src/chatFold.ts";

const url = new URL("../src-tauri/tests/fixtures/chat-parity.json", import.meta.url);
const { cases } = JSON.parse(readFileSync(url, "utf8")) as { cases: { name: string; events: any[]; items: any[] }[] };
if (!cases.length) throw new Error("no cases in the fixture");
for (const c of cases) {
  // a JSON round trip, as the fixture: keys left undefined drop out
  deepStrictEqual(JSON.parse(JSON.stringify(fold(c.events))), c.items, `chatFold.ts differs from the fixture's items: ${c.name}`);
  console.log(`ok ${c.name}: ${c.events.length} events → ${c.items.length} items`);
}
deepStrictEqual(
  fold([{ type: "notice", turnId: "t", itemId: "t:acp", kind: "failed", text: "x" }]),
  [{ kind: "note", text: "x", error: true }],
  "a failed notice draws as an error note",
);

// The chat runtime's live text (R4, runtime-events.md): deltas on a `live:<n>:<k>` item, retired by an empty
// `offset: 0` delta just before its durable item lands, fold to what the durable events alone fold to. Offsets are
// UTF-16 units, as JS strings count them.
const durable: any[] = [
  { type: "turnStarted", turnId: "t1", initiator: "user" },
  { type: "userMessage", turnId: "t1", itemId: "u1", kind: "prompt", text: "hi", files: [] },
  { type: "agentText", turnId: "t1", itemId: "a1", messageId: "m1", text: "안녕하세요 👋" },
  { type: "turnEnded", turnId: "t1", outcome: { kind: "done", stopReason: "end_turn" } },
];
const delta = (offset: number, text: string, channel = "text") => ({ type: "delta", turnId: "t1", itemId: "live:1:0", channel, offset, text });
const streaming = [durable[0], durable[1], delta(0, "안녕"), delta(2, "하세요 👋")];
deepStrictEqual(fold(streaming).at(-1), { kind: "text", text: "안녕하세요 👋" }, "live text draws while it streams");
const retired = [...streaming, delta(0, ""), durable[2], durable[3]];
deepStrictEqual(fold(retired), fold(durable), "retired live text leaves only the durable items");
deepStrictEqual(fold([durable[0], durable[1], { ...delta(0, "생각"), channel: "thinking" }]), fold(durable.slice(0, 2)), "thinking deltas draw nothing");
console.log("ok live deltas: drawn while streaming, gone once retired");

// What the chat holds (useTranscript): live deltas through `addDelta`, durable events appended. Claude writes
// `end_turn` on the thinking line before the answer's own line, so a turn's end can come mid-answer: the answer
// keeps all its text until its retire, then only the durable items are left.
const hold = (evs: any[]) => evs.reduce((es: any[], e: any) => (e.type === "delta" ? addDelta(es, e) : [...es, e]), []);
const midAnswer = [durable[0], durable[1], delta(0, "안녕"), durable[3], delta(2, "하세요 👋")];
deepStrictEqual(fold(hold(midAnswer)).at(-1), { kind: "text", text: "안녕하세요 👋" }, "a turn's end mid-answer keeps the streamed text");
deepStrictEqual(fold(hold([...midAnswer, delta(0, ""), durable[2]])), fold([durable[0], durable[1], durable[3], durable[2]]), "the retire drops the live text");
deepStrictEqual(hold([...midAnswer, delta(0, "")]).filter((e: any) => e.type === "delta"), [], "a retired item's deltas take no room");
deepStrictEqual(hold([durable[0], { ...delta(0, "생각"), channel: "thinking" }]), [durable[0]], "thinking deltas aren't kept");
console.log("ok live deltas held: a turn's end leaves them, the retire takes them");
