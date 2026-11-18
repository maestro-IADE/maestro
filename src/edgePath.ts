// The shape of every connection line on the canvases and the project map (설정 → 캔버스 → 연결선): lines
// that bend only at right angles (꺾은선, the default), one straight line from end to end, or a curve.
// docs/features/canvas/canvas-controls.md "선".
import { ConnectionLineType, getBezierPath, getSmoothStepPath, getStraightPath, type Position } from "@xyflow/react";
import { useSettings, type EdgeShape } from "./settings";

type Ends = { sourceX: number; sourceY: number; targetX: number; targetY: number; sourcePosition: Position; targetPosition: Position };

/** The path of a line in a shape, with the point its label or × sits on. A straight line goes from end to end; a
 * stepped one runs across and down only, its bends barely rounded. */
export function edgePath(shape: EdgeShape, e: Ends): [path: string, labelX: number, labelY: number] {
  const [path, x, y] = shape === "straight" ? getStraightPath(e) : shape === "step" ? getSmoothStepPath({ ...e, borderRadius: 4 }) : getBezierPath(e);
  return [path, x, y];
}

/**
 * The picked shape: the right-angled line ("step") unless another was picked. Until settings v5 that line was saved
 * as "straight"; settingsMigrate.ts moves such a value to "step" once, so "straight" here is always the straight line
 * someone picked.
 */
export const useEdgeShape = (): EdgeShape => useSettings((s) => shapeOf(s.edgeShape));
export const shapeOf = (saved: unknown): EdgeShape => (saved === "curve" || saved === "straight" ? saved : "step");

/** React Flow's built-in edge type and the line drawn while connecting, for a shape. */
export const EDGE_TYPE: Record<EdgeShape, string> = { curve: "default", step: "smoothstep", straight: "straight" };
export const CONNECTION_LINE: Record<EdgeShape, ConnectionLineType> = {
  curve: ConnectionLineType.Bezier,
  step: ConnectionLineType.SmoothStep,
  straight: ConnectionLineType.Straight,
};
