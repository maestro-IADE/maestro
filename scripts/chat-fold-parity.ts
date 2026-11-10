// The chat view's fold (src/chatFold.ts) over the fixture the Rust parity test writes
// (src-tauri/tests/fixtures/chat-parity.json, `chat_fold_fixture_is_current`): the common events of each sample
// must fold to the items the pre-R3 fold drew from the same transcript lines. Run: `npm test`.
import { readFileSync } from "node:fs";
import { deepStrictEqual } from "node:assert/strict";
import { fold } from "../src/chatFold.ts";

const url = new URL("../src-tauri/tests/fixtures/chat-parity.json", import.meta.url);
const { cases } = JSON.parse(readFileSync(url, "utf8")) as { cases: { name: string; events: any[]; items: any[] }[] };
if (!cases.length) throw new Error("no cases in the fixture");
for (const c of cases) {
  // a JSON round trip, as the fixture: keys left undefined drop out
  deepStrictEqual(JSON.parse(JSON.stringify(fold(c.events))), c.items, `chatFold.ts differs from the pre-R3 fold: ${c.name}`);
  console.log(`ok ${c.name}: ${c.events.length} events → ${c.items.length} items`);
}
