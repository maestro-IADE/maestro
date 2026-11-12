// What an Edit/MultiEdit/Write/NotebookEdit call would change, as diff lines: the approval card (ApprovalCards.tsx)
// and the chat's tool row (ToolRow in AgentNode.tsx) draw it with `ToolPreviewView` (nodes/ToolDiff.tsx).
// Pure (no React, no imports), so `npm test` (scripts/tool-preview-check.ts) loads it with node directly.
//
// The input is what the agent asks for (old_string → new_string, the new content), not the file on disk: a Write
// doesn't say whether the file exists, an Edit has no line numbers. Shapes are Claude Code's (the only agent that
// asks for approval); a call of another shape (Bash, Read, MCP, Antigravity's args) gets null and the caller's own view.

/** "@": a muted marker (folded context, an edit's header, nothing changed), drawn without a prefix */
export type DiffLine = { op: " " | "+" | "-" | "@"; text: string };

/**
 * diff: Edit/MultiEdit (old → new). write: Write/NotebookEdit (the new content, every line "+").
 * adds/dels count the real +/- lines before the cap; total: the content's lines before the cap; cut: lines the cap dropped.
 */
export type ToolPreview =
  | { kind: "diff"; path: string; lines: DiffLine[]; adds: number; dels: number; cut: number; note?: string }
  | { kind: "write"; path: string; lines: DiffLine[]; total: number; cut: number; note?: string };

export const CONTEXT = 3; // unchanged lines kept next to a change
export const MAX_LINES = 400; // lines a preview keeps at most (expanded or not)
export const MAX_LINE_CHARS = 500; // longer lines end in "…"
export const MAX_CELLS = 250_000; // LCS table size; bigger changed blocks show as all − then all +
export const MAX_CHARS = 400_000; // nor LCS over this many chars

export function splitLines(s: string): string[] {
  if (!s) return [];
  const lines = s.replace(/\r\n/g, "\n").split("\n");
  if (lines[lines.length - 1] === "") lines.pop();
  return lines;
}

const same = (k: number): DiffLine => ({ op: "@", text: `⋯ ${k}줄 같음` });

/** Runs of unchanged lines keep CONTEXT lines next to a change; the rest becomes one "⋯ N줄 같음" marker (never for a single line). */
function foldContext(lines: DiffLine[]): DiffLine[] {
  const out: DiffLine[] = [];
  let i = 0;
  while (i < lines.length) {
    if (lines[i].op !== " ") {
      out.push(lines[i++]);
      continue;
    }
    let j = i;
    while (j < lines.length && lines[j].op === " ") j++;
    const run = lines.slice(i, j);
    const head = i === 0, tail = j === lines.length;
    if (head && run.length > CONTEXT + 1) out.push(same(run.length - CONTEXT), ...run.slice(-CONTEXT));
    else if (tail && run.length > CONTEXT + 1) out.push(...run.slice(0, CONTEXT), same(run.length - CONTEXT));
    else if (!head && !tail && run.length > 2 * CONTEXT + 1)
      out.push(...run.slice(0, CONTEXT), same(run.length - 2 * CONTEXT), ...run.slice(-CONTEXT));
    else out.push(...run);
    i = j;
  }
  return out;
}

/**
 * Line diff of a → b (CRLF = LF), unchanged runs folded. Common head and tail are cut first; the changed middle gets an
 * LCS when it is small enough (MAX_CELLS, MAX_CHARS), else all its old lines as − and then its new lines as +.
 */
export function lineDiff(a: string, b: string): { lines: DiffLine[]; adds: number; dels: number } {
  const x = splitLines(a), y = splitLines(b);
  let pre = 0;
  while (pre < x.length && pre < y.length && x[pre] === y[pre]) pre++;
  let suf = 0;
  while (suf < x.length - pre && suf < y.length - pre && x[x.length - 1 - suf] === y[y.length - 1 - suf]) suf++;
  if (pre === x.length && pre === y.length) {
    const text = a === b ? "바뀐 줄이 없어요" : "줄바꿈만 달라요";
    return { lines: [{ op: "@", text }], adds: 0, dels: 0 };
  }
  const xm = x.slice(pre, x.length - suf), ym = y.slice(pre, y.length - suf);
  const n = xm.length, m = ym.length;
  const mid: DiffLine[] = [];
  let chars = 0;
  for (const l of xm) chars += l.length;
  for (const l of ym) chars += l.length;
  if (n && m && n * m <= MAX_CELLS && chars <= MAX_CHARS) {
    // dp[i][j] = LCS of xm[i..] and ym[j..]; walk forward, − before + on a tie
    const w = m + 1;
    const dp = new Uint32Array((n + 1) * w);
    for (let i = n - 1; i >= 0; i--)
      for (let j = m - 1; j >= 0; j--)
        dp[i * w + j] = xm[i] === ym[j] ? dp[(i + 1) * w + j + 1] + 1 : Math.max(dp[(i + 1) * w + j], dp[i * w + j + 1]);
    let i = 0, j = 0;
    while (i < n && j < m) {
      if (xm[i] === ym[j]) {
        mid.push({ op: " ", text: xm[i] });
        i++, j++;
      } else if (dp[(i + 1) * w + j] >= dp[i * w + j + 1]) mid.push({ op: "-", text: xm[i++] });
      else mid.push({ op: "+", text: ym[j++] });
    }
    while (i < n) mid.push({ op: "-", text: xm[i++] });
    while (j < m) mid.push({ op: "+", text: ym[j++] });
  } else {
    for (const t of xm) mid.push({ op: "-", text: t });
    for (const t of ym) mid.push({ op: "+", text: t });
  }
  let adds = 0, dels = 0;
  for (const l of mid) {
    if (l.op === "+") adds++;
    else if (l.op === "-") dels++;
  }
  const all: DiffLine[] = [
    ...x.slice(0, pre).map((text): DiffLine => ({ op: " ", text })),
    ...mid,
    ...x.slice(x.length - suf).map((text): DiffLine => ({ op: " ", text })),
  ];
  return { lines: foldContext(all), adds, dels };
}

const isObj = (v: unknown): v is Record<string, any> => !!v && typeof v === "object" && !Array.isArray(v);
const isEdit = (e: unknown) => isObj(e) && typeof e.old_string === "string" && typeof e.new_string === "string";

/** Applies MAX_LINES (cut = lines dropped) and MAX_LINE_CHARS. */
function capped(lines: DiffLine[]): { lines: DiffLine[]; cut: number } {
  const kept = lines.slice(0, MAX_LINES).map((l) =>
    l.text.length > MAX_LINE_CHARS ? { op: l.op, text: l.text.slice(0, MAX_LINE_CHARS) + "…" } : l,
  );
  return { lines: kept, cut: Math.max(0, lines.length - MAX_LINES) };
}

/**
 * The preview of a tool call, or null when it isn't a file change (the caller shows its own summary). Goes by the
 * input's shape, not only the name: on the chat runtime the name can be the call's title when the adapter sends no
 * tool name.
 */
export function toolPreview(name: string, input: any): ToolPreview | null {
  if (!isObj(input)) return null;
  const path = (p: unknown) => (typeof p === "string" ? p : "");
  if (isEdit(input)) {
    const d = lineDiff(input.old_string, input.new_string);
    const note = input.replace_all === true ? "일치하는 곳을 모두 바꿔요" : undefined;
    return { kind: "diff", path: path(input.file_path), ...d, ...capped(d.lines), note };
  }
  if (Array.isArray(input.edits) && input.edits.length && input.edits.every(isEdit)) {
    const n = input.edits.length;
    const lines: DiffLine[] = [];
    let adds = 0, dels = 0;
    input.edits.forEach((e: any, i: number) => {
      const d = lineDiff(e.old_string, e.new_string);
      lines.push({ op: "@", text: `편집 ${i + 1}/${n}${e.replace_all === true ? " · 일치하는 곳 모두" : ""}` }, ...d.lines);
      adds += d.adds;
      dels += d.dels;
    });
    return { kind: "diff", path: path(input.file_path), adds, dels, ...capped(lines) };
  }
  if (typeof input.content === "string" && (typeof input.file_path === "string" || name === "Write")) {
    const lines = splitLines(input.content).map((text): DiffLine => ({ op: "+", text }));
    return { kind: "write", path: path(input.file_path), total: lines.length, ...capped(lines), note: "파일 전체를 이 내용으로 써요" };
  }
  if (typeof input.new_source === "string" || (input.edit_mode === "delete" && typeof input.notebook_path === "string")) {
    const p = path(input.notebook_path);
    if (input.edit_mode === "delete") return { kind: "write", path: p, lines: [], total: 0, cut: 0, note: "셀 삭제" };
    const lines = splitLines(input.new_source).map((text): DiffLine => ({ op: "+", text }));
    return { kind: "write", path: p, total: lines.length, ...capped(lines), note: input.edit_mode === "insert" ? "새 셀" : "셀 내용을 바꿔요" };
  }
  return null;
}

/** p relative to root when it is inside it (slashes either way; drive paths case-insensitively), else p as is. */
export function relPath(p: string, root?: string): string {
  if (!root || !p) return p;
  const norm = (s: string) => s.replace(/\\/g, "/");
  const r = norm(root).replace(/\/+$/, "") + "/";
  const q = norm(p);
  const fold = /^[a-z]:\//i.test(r) || r.startsWith("//");
  const hit = fold ? q.toLowerCase().startsWith(r.toLowerCase()) : q.startsWith(r);
  return hit && q.length > r.length ? q.slice(r.length) : p;
}
