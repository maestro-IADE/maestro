// 빠른 이동's pure parts (src/quickModel.ts): match tiers, scores, ranking, the highlighted row. Run: `npm test`.
import { deepStrictEqual, equal, ok } from "node:assert/strict";
import { matchTier, moveHot, normQuery, quickScore, rankQuick, type QuickItem } from "../src/quickModel.ts";

const item = (id: string, label: string, keywords: string[] = [], kind: QuickItem["kind"] = "agent"): QuickItem => ({ id, kind, label, keywords });
const ids = (list: QuickItem[]) => list.map((x) => x.id);

// a: normalizing
equal(normQuery("  Api   Test "), "api test");
equal(normQuery("\t"), "");

// b: how a term sits in a text
equal(matchTier("설정", "설정"), 4, "the whole text");
equal(matchTier("설정 화면", "설정"), 3, "its start");
equal(matchTier("에이전트 설정", "설정"), 2, "a word's start");
equal(matchTier("agent-3", "3"), 2, "after a dash");
equal(matchTier("feat/login", "login"), 2, "after a slash");
equal(matchTier("환경설정", "설정"), 1, "inside a word");
equal(matchTier("abcab", "ab"), 3);
equal(matchTier("xab ab", "ab"), 2, "a later word start beats an earlier inside match");
equal(matchTier("api", "x"), 0);
equal(matchTier("api", ""), 0);

// c: ranking: start > word start > inside, the whole label first
{
  const items = [item("in", "환경설정"), item("word", "에이전트 설정"), item("start", "설정 화면"), item("none", "로그인"), item("exact", "설정", [], "action")];
  deepStrictEqual(ids(rankQuick(items, "설정")), ["exact", "start", "word", "in"]);
  deepStrictEqual(ids(rankQuick(items, "  설정 ")), ["exact", "start", "word", "in"], "spaces around don't count");
}

// d: case doesn't matter
deepStrictEqual(ids(rankQuick([item("a", "API 테스트"), item("b", "rapid")], "api")), ["a", "b"]);

// e: a keyword match comes after every label match, and only matches when nothing in the label does
{
  const items = [item("branch", "로그인 화면", ["maestro", "main"]), item("label", "domain fix")];
  deepStrictEqual(ids(rankQuick(items, "main")), ["label", "branch"]);
  ok(quickScore(items[0], "main") > 0 && quickScore(items[0], "main") < quickScore(items[1], "main"));
}

// f: several words: each one somewhere (label or keyword), in any order
{
  const items = [item("a", "로그인 화면 고치기", ["maestro", "agent-3"]), item("b", "로그인 api", ["other"]), item("c", "실험", ["maestro"])];
  deepStrictEqual(ids(rankQuick(items, "maestro 로그인")), ["a"]);
  deepStrictEqual(ids(rankQuick(items, "고치기 로그인")), ["a"], "words in any order");
  deepStrictEqual(ids(rankQuick(items, "화면 고")), ["a"], "the whole query in the label");
  ok(quickScore(items[0], "화면 고") > quickScore(items[0], "고치기 로그인"), "the whole query beats its words apart");
  deepStrictEqual(ids(rankQuick(items, "로그인 없음")), [], "a word found nowhere: no match");
}

// g: ties keep the given order; an empty query keeps every item; the limit
{
  const items = [item("p1", "maestro", [], "project"), item("a1", "maestro", [], "agent"), item("x", "기타", [], "action")];
  deepStrictEqual(ids(rankQuick(items, "maestro")), ["p1", "a1"], "equal scores: as given");
  deepStrictEqual(ids(rankQuick(items, "")), ["p1", "a1", "x"]);
  deepStrictEqual(ids(rankQuick(items, " ")), ["p1", "a1", "x"]);
  deepStrictEqual(ids(rankQuick(items, "", 2)), ["p1", "a1"]);
  deepStrictEqual(ids(rankQuick(items, "maestro", 1)), ["p1"]);
  ok(rankQuick(items, "") !== items, "a copy, not the list itself");
}

// h: the highlighted row goes round
equal(moveHot(0, 3, 1), 1);
equal(moveHot(2, 3, 1), 0, "past the last: the first");
equal(moveHot(0, 3, -1), 2, "before the first: the last");
equal(moveHot(-1, 3, 1), 0, "none yet, down: the first");
equal(moveHot(-1, 3, -1), 2, "none yet, up: the last");
equal(moveHot(5, 3, 1), 0, "out of range after the list shrank");
equal(moveHot(0, 0, 1), -1, "nothing to highlight");

console.log("ok quick");
