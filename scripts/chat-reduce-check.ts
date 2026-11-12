// The chat reducer (src/chatFold.ts `reduceChat`, interface PRD I1) one event at a time: text deltas grow at their
// offset, tools are found by tool call id and updated, a newer epoch starts over, a seq seen before is skipped, what
// the app doesn't know is kept, an event of the latest turn leaves earlier turns as they were, a live item stays one
// item across a turnStarted, and earlier turns are opened by their marks. Run: `npm test`.
import { deepStrictEqual, equal, ok, strictEqual } from "node:assert/strict";
import { chatItems, emptyChat, hasDrawn, lastText, loadChat, reduceChat, type ChatInput, type ChatState } from "../src/chatFold.ts";

/** feeds events in order: durable ones take the next seq, deltas and approvals none */
const LIVE = new Set(["delta", "approvalRequested", "approvalResolved", "exited", "reset"]);
function feed(s: ChatState, evs: any[], epoch = 1, at?: number): ChatState {
  for (const event of evs) s = reduceChat(s, { epoch, seq: LIVE.has(event.type) ? null : s.seq + 1, event, at });
  return s;
}
const start = (turnId: string) => ({ type: "turnStarted", turnId, initiator: "user" });
const user = (turnId: string, text: string) => ({ type: "userMessage", turnId, itemId: `u-${turnId}`, kind: "prompt", text, files: [] });
const delta = (turnId: string, itemId: string, offset: number, text: string, channel = "text") => ({ type: "delta", turnId, itemId, channel, offset, text });
const kinds = (s: ChatState) => chatItems(s).map((it) => it.kind);

// --- deltas grow a live item at their offset; a delta sent again changes nothing
{
  let s = feed(emptyChat(1), [start("t1"), user("t1", "hi")]);
  s = feed(s, [delta("t1", "live:1:1", 0, "한글"), delta("t1", "live:1:1", 2, " 😀"), delta("t1", "live:1:1", 5, "!")]);
  deepStrictEqual(chatItems(s).at(-1), { kind: "text", key: "2", turnId: "t1", id: "live:1:1", text: "한글 😀!", live: true }, "deltas append at their UTF-16 offset");
  const again = reduceChat(s, { epoch: 1, seq: null, event: delta("t1", "live:1:1", 5, "!") });
  strictEqual(again, s, "the same delta twice: same state, nothing to redraw");
  const shorter = reduceChat(s, { epoch: 1, seq: null, event: delta("t1", "live:1:1", 3, "?") });
  equal((chatItems(shorter).at(-1) as any).text, "한글 ?", "a delta at an earlier offset replaces the rest");
  const thinking = feed(s, [delta("t1", "live:1:2", 0, "음", "thinking")]);
  deepStrictEqual(chatItems(thinking).at(-1), { kind: "thinking", key: "3", turnId: "t1", id: "live:1:2", text: "음", live: true }, "a thinking delta is a live thinking item");
  strictEqual(feed(s, [delta("t1", "x", 0, "out", "toolOutput")]), s, "tool output deltas aren't kept yet");
  const retired = feed(s, [delta("t1", "live:1:1", 0, "")]);
  deepStrictEqual(kinds(retired), ["turnStart", "user"], "the empty offset-0 delta retires the live item");
  strictEqual(feed(retired, [delta("t1", "live:1:1", 0, "")]), retired, "retiring again changes nothing");
  // ACP sends a text block empty when it opens, then whole: one item, in its first place
  const acp = feed(s, [
    { type: "agentText", turnId: "t1", itemId: "a1", messageId: null, text: "" },
    { type: "toolStarted", turnId: "t1", toolCallId: "c1", messageId: null, name: "Read", input: {} },
    { type: "agentText", turnId: "t1", itemId: "a1", messageId: null, text: "done" },
  ]);
  deepStrictEqual(chatItems(acp).slice(-2).map((it: any) => [it.kind, it.text ?? it.name]), [["text", "done"], ["tool", "Read"]], "agentText upserts by item id");
  console.log("ok deltas: appended at offsets, idempotent, retired; text upserted by item id");
}

// --- a live item whose turnStarted lands between its deltas (an ACP turn the agent starts itself: its live text goes
// out under the turn before, `p:<n>`, and the transcript's turnStarted comes after the first delta) stays one item
{
  const answered = [start("p1"), user("p1", "hi"), { type: "agentText", turnId: "p1", itemId: "a1", messageId: null, text: "hey" }, { type: "turnEnded", turnId: "p1", outcome: { kind: "done", stopReason: "end_turn" } }];
  let s = feed(emptyChat(1), answered);
  s = feed(s, [delta("p:0", "live:0:1", 0, "Hello "), { type: "turnStarted", turnId: "a:x", initiator: "agent" }, delta("a:x", "live:0:1", 6, "world")]);
  const lives = chatItems(s).filter((it: any) => it.id === "live:0:1").map((it: any) => it.text);
  deepStrictEqual(lives, ["Hello world"], "the delta after the turnStarted grows the item it started, in the turn before");
  equal(s.tail.length, 1, "the new turn holds only its start");
  const landed = feed(s, [delta("a:x", "live:0:1", 0, ""), { type: "agentText", turnId: "a:x", itemId: "a2", messageId: null, text: "Hello world" }]);
  deepStrictEqual(chatItems(landed).filter((it: any) => it.kind === "text").map((it: any) => it.text), ["hey", "Hello world"], "its retire leaves no fragment behind");
  const thought = feed(feed(emptyChat(1), answered), [delta("p:0", "live:0:2", 0, "음", "thinking"), start("a:y"), delta("a:y", "live:0:2", 1, "…", "thinking")]);
  deepStrictEqual(chatItems(thought).filter((it: any) => it.kind === "thinking").map((it: any) => it.text), ["음…"], "the same for live thinking");
  console.log("ok live items: one item across a turnStarted, retired whole");
}

// --- earlier turns are looked into by their marks: only a turn that holds the item is opened, and a stale mark
// (an item retired from a closed turn) only sends the search on
{
  let s = feed(emptyChat(1), [start("t1"), { type: "approvalRequested", turnId: null, toolCallId: null, approvalId: 3, toolName: "Bash", toolInput: {}, canAlways: false }, start("t2"), user("t2", "x"), start("t3")]);
  deepStrictEqual(s.marks.map((m) => [...m].sort()), [["approval 3", "turnStart t1"], ["turnStart t2", "user u-t2"]], "a closed turn's marks: kind and id of what it holds");
  const r = feed(s, [{ type: "approvalResolved", turnId: null, toolCallId: null, approvalId: 3, resolution: "allow" }]);
  equal((r.turns[0][1] as any).resolution, "allow", "an approval with no turn is found two turns back");
  strictEqual(r.turns[1], s.turns[1], "the turn between is left as it was");
  strictEqual(r.marks, s.marks, "an edit doesn't touch the marks");
  // a live item retired from a closed turn: its mark stays, the next live item with that id is new
  let l = feed(emptyChat(1), [start("t1"), delta("t1", "live:1:1", 0, "a"), start("t2"), delta("t2", "live:1:1", 0, "")]);
  equal(chatItems(l).filter((it) => it.kind === "text").length, 0, "retired from the turn before");
  l = feed(l, [delta("t2", "live:1:1", 0, "b")]);
  deepStrictEqual(chatItems(l).filter((it) => it.kind === "text").map((it: any) => it.text), ["b"], "a stale mark finds nothing and the item is made anew");
  console.log("ok marks: earlier turns opened only when they hold the item");
}

// --- tools: found by tool call id, updated, finished; a finish with no start is dropped
{
  let s = feed(emptyChat(1), [start("t1"), { type: "toolStarted", turnId: "t1", toolCallId: "c1", messageId: null, name: "Bash", input: { command: "ls" } }]);
  s = feed(s, [{ type: "toolUpdated", turnId: "t1", toolCallId: "c1", status: "running", title: "ls -la", input: { command: "ls -la" }, output: null }]);
  deepStrictEqual(chatItems(s).at(-1), { kind: "tool", key: "1", turnId: "t1", id: "c1", name: "Bash", input: { command: "ls -la" }, title: "ls -la", status: "running" }, "an update changes only what it carries");
  const failed = feed(s, [{ type: "toolUpdated", turnId: "t1", toolCallId: "c1", status: "failed", title: null, input: null, output: "boom" }]);
  deepStrictEqual([(chatItems(failed).at(-1) as any).isError, (chatItems(failed).at(-1) as any).result], [true, "boom"], "a failed update marks the error");
  const finished = feed(s, [{ type: "toolFinished", turnId: "t1", toolCallId: "c1", output: "a\nb", isError: false }]);
  deepStrictEqual([(chatItems(finished).at(-1) as any).result, (chatItems(finished).at(-1) as any).isError], ["a\nb", false], "the finish sets the result");
  const orphan = feed(s, [{ type: "toolFinished", turnId: "t1", toolCallId: "nope", output: "", isError: false }]);
  deepStrictEqual(chatItems(orphan), chatItems(s), "a finish with no start draws nothing");
  equal(orphan.seq, s.seq + 1, "but its seq is taken");
  strictEqual(feed(s, [{ type: "toolUpdated", turnId: "t1", toolCallId: "nope", status: "running", title: null, input: null, output: null }]).tail, s.tail, "an update with no tool leaves the items");
  // a straggler: the earlier turn's tool finishing after the next turn began is still found
  const late = feed(s, [start("t2"), { type: "toolFinished", turnId: "t1", toolCallId: "c1", output: "late", isError: true }]);
  deepStrictEqual([(late.turns[0][1] as any).result, late.tail.length], ["late", 1], "an earlier turn's tool is found by its id");
  console.log("ok tools: started, updated, finished by tool call id");
}

// --- epochs and seqs
{
  const s = feed(emptyChat(1), [start("t1"), user("t1", "one")]);
  strictEqual(reduceChat(s, { epoch: 0, seq: 99, event: user("t1", "old") }), s, "an older epoch is ignored");
  strictEqual(reduceChat(s, { epoch: 1, seq: s.seq, event: user("t1", "dup") }), s, "the same seq again is ignored");
  strictEqual(reduceChat(s, { epoch: 1, seq: 1, event: user("t1", "dup") }), s, "an older seq is ignored");
  const live = reduceChat(s, { epoch: 1, seq: null, event: delta("t1", "live:1:1", 0, "x") });
  deepStrictEqual([live.seq, live.durable], [s.seq, s.durable], "a live event moves neither seq nor the durable count");
  const fresh = reduceChat(s, { epoch: 2, seq: 1, event: start("t9") });
  deepStrictEqual([fresh.epoch, fresh.seq, fresh.durable, kinds(fresh), fresh.turns.length], [2, 1, 1, ["turnStart"], 0], "a newer epoch starts from empty");
  const quiet = reduceChat(s, { epoch: 3, seq: null, event: { type: "reset", reason: "restart" } });
  deepStrictEqual([quiet.epoch, chatItems(quiet).length], [3, 0], "a newer epoch empties even on an event that draws nothing");
  strictEqual(reduceChat(s, { epoch: 1, seq: null, event: { type: "exited", turnId: "t1" } }), s, "exit draws nothing");
  const loaded = loadChat({ epoch: 4, seq: 7, events: [start("t1"), user("t1", "a")] });
  deepStrictEqual([loaded.epoch, loaded.seq, loaded.durable], [4, 7, 2], "the history sets the epoch and seq it was read at");
  strictEqual(reduceChat(loaded, { epoch: 4, seq: 7, event: user("t1", "again") }), loaded, "a live event the history already had is skipped");
  console.log("ok epochs and seqs: older epoch and seen seq skipped, newer epoch starts over");
}

// --- what the app doesn't know is kept, raw
{
  const raw = { type: "system", subtype: "compact_boundary" };
  const s = feed(emptyChat(1), [start("t1"), { type: "unknown", turnId: "t1", source: "claude", raw }, { type: "futureThing", turnId: "t1", extra: [1, 2] }]);
  deepStrictEqual(chatItems(s).slice(1), [
    { kind: "unknown", key: "1", turnId: "t1", source: "claude", raw },
    { kind: "unknown", key: "2", turnId: "t1", source: "event:futureThing", raw: { type: "futureThing", turnId: "t1", extra: [1, 2] } },
  ], "unknown events and unknown event types are kept whole");
  const err = feed(s, [{ type: "error", turnId: "t1", kind: "api_retry", message: "다시 시도 중" }]);
  deepStrictEqual(chatItems(err).at(-1), { kind: "error", key: "3", turnId: "t1", errorKind: "api_retry", message: "다시 시도 중" }, "an error is an item");
  ok(!hasDrawn(err), "none of these draw before I2");
  console.log("ok unknown kept: the unknown event's raw, a new event type whole");
}

// --- turns: edges with a duration when seen live, earlier turns untouched by the latest one's events
{
  let s = feed(emptyChat(1), [start("t1"), user("t1", "one")], 1, 1_000);
  s = feed(s, [{ type: "turnEnded", turnId: "t1", outcome: { kind: "done", stopReason: "end_turn" } }], 1, 4_500);
  deepStrictEqual(chatItems(s).at(-1), { kind: "turnEnd", key: "2", turnId: "t1", id: "t1", outcome: { kind: "done", stopReason: "end_turn" }, ms: 3_500 }, "a live turn's end has its duration");
  const hist = loadChat({ epoch: 1, seq: 3, events: [start("t1"), user("t1", "one"), { type: "turnEnded", turnId: "t1", outcome: { kind: "cancelled" } }] });
  ok(!("ms" in chatItems(hist)[2]) && !("at" in chatItems(hist)[0]), "the history has no times, so no duration");
  s = feed(s, [start("t2"), user("t2", "two")], 1, 5_000);
  const first = s.turns[0];
  deepStrictEqual([s.turns.length, kinds(s)], [1, ["turnStart", "user", "turnEnd", "turnStart", "user"]], "a turn start closes the turn before it");
  const after = feed(s, [{ type: "toolStarted", turnId: "t2", toolCallId: "c", messageId: null, name: "Read", input: {} }, delta("t2", "live:2:1", 0, "읽는 중"), delta("t2", "live:2:1", 4, "…")]);
  strictEqual(after.turns, s.turns, "the latest turn's events don't copy the list of earlier turns");
  strictEqual(after.turns[0], first, "nor any earlier turn");
  equal(lastText(after), "읽는 중…", "lastText reads from the end");
  equal(lastText(feed(after, [delta("t2", "live:2:1", 0, "")])), "two", "a retired text isn't the last thing said");
  console.log("ok turns: edges, duration when live, earlier turns shared");
}

// --- approvals: seen live, resolved by id
{
  const s = feed(emptyChat(1), [start("t1"), { type: "approvalRequested", turnId: "t1", toolCallId: "c1", approvalId: 7, toolName: "Bash", toolInput: { command: "rm" }, canAlways: true }]);
  deepStrictEqual(chatItems(s).at(-1), { kind: "approval", key: "1", turnId: "t1", id: 7, toolCallId: "c1", toolName: "Bash", toolInput: { command: "rm" }, canAlways: true }, "an approval request is an item");
  const r = feed(s, [{ type: "approvalResolved", turnId: "t1", toolCallId: "c1", approvalId: 7, resolution: "deny" }]);
  equal((chatItems(r).at(-1) as any).resolution, "deny", "its answer is kept on it");
  strictEqual(feed(s, [{ type: "approvalResolved", turnId: "t1", toolCallId: null, approvalId: 8, resolution: "allow" }]), s, "an answer to an approval not seen changes nothing");
  console.log("ok approvals: requested and resolved by id");
}

// --- pure: the state given in is never changed
{
  const s = feed(emptyChat(1), [start("t1"), user("t1", "one"), start("t2"), { type: "toolStarted", turnId: "t2", toolCallId: "c", messageId: null, name: "Read", input: {} }]);
  const before = JSON.stringify(s);
  const inputs: ChatInput[] = [
    { epoch: 1, seq: 9, event: { type: "toolFinished", turnId: "t2", toolCallId: "c", output: "x", isError: false } },
    { epoch: 1, seq: null, event: { type: "delta", turnId: "t2", itemId: "live:1", channel: "text", offset: 0, text: "y" } },
    { epoch: 1, seq: 10, event: { type: "toolFinished", turnId: "t1", toolCallId: "nope", output: "", isError: false } },
    { epoch: 1, seq: 11, event: { type: "turnStarted", turnId: "t3", initiator: "agent" } },
    { epoch: 2, seq: 1, event: { type: "turnStarted", turnId: "t1", initiator: "user" } },
  ];
  for (const i of inputs) reduceChat(s, i);
  equal(JSON.stringify(s), before, "reduceChat leaves its input state as it was");
  console.log("ok pure: the old state is unchanged");
}
