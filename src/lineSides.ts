// Which sides of two blocks a line leaves and enters by (docs/features/canvas/canvas-controls.md "연결점 규약").
// Pure, so scripts/sides-check.ts runs it in Node.

export type Box = { x: number; y: number; w: number; h: number };
/** The way a layout runs: "down" for a tree or a hierarchy (a parent above, its children under it), "across" for a flow. */
export type Flow = "down" | "across";
export type Side = "top" | "bottom" | "left" | "right";

/** The way lines run in a layout an agent drew with (`maestro draw`): a tree goes down, a flow across; a grid or a row has none. */
export const flowOfLayout = (layout?: string): Flow | undefined => (layout === "tree" ? "down" : layout === "flow" ? "across" : undefined);

/**
 * The sides a line from `a` to `b` leaves and enters by.
 *
 * Without a `flow`: the sides that face each other (up and down when the two are further apart that way, sideways
 * otherwise).
 *
 * With a flow the line keeps to the layout's way however far apart the two are the other way, so every line of a
 * tree leaves its parent by one side and no child far out to the side is joined at the parent's flank:
 * - "down": out of the bottom, in at the top (out of the top, in at the bottom when `b` is the higher one: a line
 *   going back up). Only two on the same row are joined sideways.
 * - "across": out of the right, in at the left (the other way round when `b` is to the left: a flow that turned
 *   back). Only two in the same column are joined up and down (where a flow turns down to its next row).
 *
 * "The same row": they overlap up and down by half the lower one's height or more; "the same column" likewise
 * across. A box without a size (not measured) is a point.
 */
export function sidesFor(a: Box | undefined, b: Box | undefined, flow?: Flow): [Side, Side] {
  if (!a || !b) return ["bottom", "top"];
  const dx = b.x + b.w / 2 - (a.x + a.w / 2), dy = b.y + b.h / 2 - (a.y + a.h / 2);
  const down: [Side, Side] = dy >= 0 ? ["bottom", "top"] : ["top", "bottom"];
  const across: [Side, Side] = dx >= 0 ? ["right", "left"] : ["left", "right"];
  const overlaps = (a0: number, al: number, b0: number, bl: number) => Math.min(al, bl) > 0 && Math.min(a0 + al, b0 + bl) - Math.max(a0, b0) >= Math.min(al, bl) / 2;
  const sameRow = overlaps(a.y, a.h, b.y, b.h), sameColumn = overlaps(a.x, a.w, b.x, b.w);
  if (flow === "down") return sameRow && !sameColumn ? across : down;
  if (flow === "across") return sameColumn && !sameRow ? down : across;
  return Math.abs(dy) >= Math.abs(dx) ? down : across;
}

/** The innermost area that holds both nodes (neither of the two themselves); none when they only share the canvas. */
export function commonParent(nodes: readonly { id: string; parentId?: string }[], a: string, b: string): string | undefined {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const above = new Set<string>();
  for (let p = byId.get(a)?.parentId, k = 0; p && k < 50; p = byId.get(p)?.parentId, k++) above.add(p);
  for (let p = byId.get(b)?.parentId, k = 0; p && k < 50; p = byId.get(p)?.parentId, k++) if (above.has(p)) return p;
  return undefined;
}
