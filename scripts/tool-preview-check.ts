// The file-change preview of approval cards and tool rows (src/toolPreview.ts): line diff, folded context, caps,
// shape dispatch. Run: `npm test`.
import { deepStrictEqual, equal, ok } from "node:assert/strict";
import { MAX_LINE_CHARS, MAX_LINES, lineDiff, relPath, toolPreview, type DiffLine } from "../src/toolPreview.ts";

const flat = (ls: DiffLine[]) => ls.map((l) => (l.op === "@" ? `@ ${l.text}` : l.op + l.text));
const lines = (n: number, tag: string) => Array.from({ length: n }, (_, i) => `${tag}${i}`).join("\n");

// a: one changed line among unchanged ones
{
  const d = lineDiff("a\nb\nc", "a\nB\nc");
  deepStrictEqual(flat(d.lines), [" a", "-b", "+B", " c"]);
  equal(d.adds, 1);
  equal(d.dels, 1);
}
// b: nothing changed; CRLF is LF
{
  const d = lineDiff("a\nb", "a\nb");
  deepStrictEqual(flat(d.lines), ["@ 바뀐 줄이 없어요"]);
  equal(d.adds + d.dels, 0);
  const e = lineDiff("a\r\nb", "a\nb");
  equal(e.lines.length, 1);
  equal(e.lines[0].op, "@");
  equal(e.adds + e.dels, 0);
  equal(lineDiff("a\nb", "a\nb\n").adds, 0, "a trailing newline is no line");
}
// c: from or to nothing
deepStrictEqual(flat(lineDiff("", "x\ny").lines), ["+x", "+y"]);
deepStrictEqual(flat(lineDiff("x\ny", "").lines), ["-x", "-y"]);
// d: a long unchanged run between two changes folds to 3 + marker + 3; leading/trailing runs keep 3 next to the change
{
  const mid = lines(20, "m");
  const d = lineDiff(`h0\nh1\nh2\nh3\nh4\nA\n${mid}\nB\nt0\nt1\nt2\nt3`, `h0\nh1\nh2\nh3\nh4\nA2\n${mid}\nB2\nt0\nt1\nt2\nt3`);
  deepStrictEqual(flat(d.lines), [
    "@ ⋯ 2줄 같음", " h2", " h3", " h4", "-A", "+A2",
    " m0", " m1", " m2", "@ ⋯ 14줄 같음", " m17", " m18", " m19",
    "-B", "+B2", " t0", " t1", " t2", " t3",
  ]);
  equal(d.adds, 2);
  equal(d.dels, 2);
}
// LCS: an inserted line in the middle is one +, not a rewrite
deepStrictEqual(flat(lineDiff("a\nb\nc\nd", "a\nx\nb\nc\nY\nd").lines), [" a", "+x", " b", " c", "+Y", " d"]);
// e: too big for the LCS table → all − then all +, still counted; the preview caps the lines
{
  const a = lines(600, "a"), b = lines(600, "b");
  const d = lineDiff(a, b);
  equal(d.adds, 600);
  equal(d.dels, 600);
  equal(d.lines.length, 1200);
  ok(d.lines.slice(0, 600).every((l) => l.op === "-") && d.lines.slice(600).every((l) => l.op === "+"));
  const p = toolPreview("Edit", { file_path: "f", old_string: a, new_string: b });
  ok(p && p.kind === "diff");
  equal(p.lines.length, MAX_LINES);
  equal(p.cut, 1200 - MAX_LINES);
  equal(p.adds, 600);
}
// f: MultiEdit: a header per edit, totals summed
{
  const p = toolPreview("MultiEdit", {
    file_path: "f.ts",
    edits: [{ old_string: "a", new_string: "b" }, { old_string: "c\nd", new_string: "c", replace_all: true }],
  });
  ok(p && p.kind === "diff");
  deepStrictEqual(flat(p.lines), ["@ 편집 1/2", "-a", "+b", "@ 편집 2/2 · 일치하는 곳 모두", " c", "-d"]);
  equal(p.adds, 1);
  equal(p.dels, 2);
  equal(p.path, "f.ts");
}
// g: Write: every line +, total before the cap
{
  const p = toolPreview("Write", { file_path: "big.txt", content: lines(1000, "l") + "\n" });
  ok(p && p.kind === "write");
  equal(p.total, 1000);
  equal(p.lines.length, MAX_LINES);
  equal(p.cut, 600);
  ok(p.lines.every((l) => l.op === "+"));
}
// h: a long line is sliced
{
  const p = toolPreview("Write", { file_path: "min.js", content: "x".repeat(2000) });
  ok(p);
  equal(p.lines[0].text, "x".repeat(MAX_LINE_CHARS) + "…");
}
// i: not a file change → null; Claude's shape under another name still is
for (const [name, input] of [["Bash", { command: "ls" }], ["Edit", null], ["Edit", undefined], ["Edit", {}], ["Edit", "str"],
  ["Edit", { file_path: "f", old_string: 1, new_string: "x" }], ["MultiEdit", { file_path: "f", edits: [] }],
  ["MultiEdit", { file_path: "f", edits: [{ old_string: "a" }] }], ["Read", { file_path: "f" }]] as [string, unknown][])
  equal(toolPreview(name, input), null, `${name} ${JSON.stringify(input)}`);
equal(toolPreview("Edit `x`", { file_path: "f", old_string: "a", new_string: "b" })?.kind, "diff");
equal(toolPreview("Edit", { file_path: "f", old_string: "a", new_string: "b", replace_all: true })?.note, "일치하는 곳을 모두 바꿔요");
// NotebookEdit
equal(toolPreview("NotebookEdit", { notebook_path: "n.ipynb", new_source: "print(1)", edit_mode: "insert" })?.note, "새 셀");
deepStrictEqual(toolPreview("NotebookEdit", { notebook_path: "n.ipynb", new_source: "", edit_mode: "delete" }), {
  kind: "write", path: "n.ipynb", lines: [], total: 0, cut: 0, note: "셀 삭제",
});
// j: paths relative to the worktree
equal(relPath("C:\\W\\wt\\src\\a.ts", "c:/w/wt"), "src/a.ts");
equal(relPath("C:\\W\\wt\\src\\a.ts", "C:\\W\\wt\\"), "src/a.ts");
equal(relPath("C:\\W\\other\\a.ts", "c:/w/wt"), "C:\\W\\other\\a.ts");
equal(relPath("C:\\W\\wt2\\a.ts", "c:/w/wt"), "C:\\W\\wt2\\a.ts");
equal(relPath("/home/U/wt/a.ts", "/home/u/wt"), "/home/U/wt/a.ts", "posix paths are case-sensitive");
equal(relPath("/home/u/wt/a.ts", "/home/u/wt"), "a.ts");
equal(relPath("a.ts"), "a.ts");

console.log("ok tool preview");
