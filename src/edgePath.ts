// The shape of every connection line on the canvases and the project map (설정 → 모양 → 캔버스 → 연결선): a curve
// (the default), a straight line, or a right-angled line with rounded bends. docs/features/canvas/canvas-controls.md "선".
import { ConnectionLineType, getBezierPath, getSmoothStepPath, getStraightPath, type Position } from "@xyflow/react";
import { useSettings, type EdgeShape } from "./settings";

type Ends = { sourceX: number; sourceY: number; targetX: number; targetY: number; sourcePosition: Position; targetPosition: Position };

/** The path of a line in a shape, with the point its label or × sits on. */
export function edgePath(shape: EdgeShape, e: Ends): [path: string, labelX: number, labelY: number] {
  if (shape === "straight") {
    const [path, x, y] = getStraightPath(e);
    return [path, x, y];
  }
  if (shape === "step") {
    const [path, x, y] = getSmoothStepPath({ ...e, borderRadius: 8 });
    return [path, x, y];
  }
  const [path, x, y] = getBezierPath(e);
  return [path, x, y];
}

/** The picked shape (a curve when none was picked). */
export const useEdgeShape = (): EdgeShape => useSettings((s) => s.edgeShape) ?? "curve";

/** React Flow's built-in edge type and the line drawn while connecting, for a shape. */
export const EDGE_TYPE: Record<EdgeShape, string> = { curve: "default", straight: "straight", step: "smoothstep" };
export const CONNECTION_LINE: Record<EdgeShape, ConnectionLineType> = {
  curve: ConnectionLineType.Bezier,
  straight: ConnectionLineType.Straight,
  step: ConnectionLineType.SmoothStep,
};
