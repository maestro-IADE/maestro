// The shape of every connection line on the canvases and the project map (설정 → 모양 → 캔버스 → 연결선): a curve
// (the default) or straight lines that bend only at right angles. docs/features/canvas/canvas-controls.md "선".
import { ConnectionLineType, getBezierPath, getSmoothStepPath, type Position } from "@xyflow/react";
import { useSettings, type EdgeShape } from "./settings";

type Ends = { sourceX: number; sourceY: number; targetX: number; targetY: number; sourcePosition: Position; targetPosition: Position };

/** The path of a line in a shape, with the point its label or × sits on. Straight lines run across and down only,
 * their bends barely rounded. */
export function edgePath(shape: EdgeShape, e: Ends): [path: string, labelX: number, labelY: number] {
  const [path, x, y] = shape === "straight" ? getSmoothStepPath({ ...e, borderRadius: 4 }) : getBezierPath(e);
  return [path, x, y];
}

/** The picked shape: a curve unless straight was picked (a "step" saved by an earlier build counts as straight). */
export const useEdgeShape = (): EdgeShape =>
  useSettings((s) => ((s.edgeShape as string) === "curve" || !s.edgeShape ? "curve" : "straight"));

/** React Flow's built-in edge type and the line drawn while connecting, for a shape. */
export const EDGE_TYPE: Record<EdgeShape, string> = { curve: "default", straight: "smoothstep" };
export const CONNECTION_LINE: Record<EdgeShape, ConnectionLineType> = {
  curve: ConnectionLineType.Bezier,
  straight: ConnectionLineType.SmoothStep,
};
