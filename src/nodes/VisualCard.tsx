import { useEffect, useMemo, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { chatView, gate, painter, useGate, type Visual } from "../canvasGate";
import type { Mini } from "../canvasMini";
import { samePath, useStore } from "../store";
import { useSettings } from "../settings";
import { X } from "../icons";
import { CanvasMini } from "./CanvasMini";

const folderName = (path: string) => path.split(/[\\/]/).pop() ?? path;
const fail = (e: unknown) => useStore.getState().setError(String(e));

/**
 * The picture of a step: the one its open canvas made; else what its queued things bring, drawn on an empty sheet;
 * else, for a step that only points at something (`maestro focus`), that thing as the canvas's saved file has it.
 */
function useMini(path: string, root: string | undefined, v: Visual | null): Mini | null {
  const drawn = useMemo(() => (v && v.mini === undefined && v.brings ? (painter.items?.(v.items, path) ?? null) : null), [v, path]);
  const wt = useStore((s) => s.worktrees.find((w) => samePath(w.path, path)));
  const at = root ?? wt?.root;
  const key = wt?.isMaestro ? "overview" : folderName(path);
  const points = !!v && v.mini === undefined && !v.brings && v.points.length > 0;
  const [pointed, setPointed] = useState<{ id: string; mini: Mini | null } | null>(null);
  useEffect(() => {
    if (!points || !v || !at) return;
    let live = true;
    // (what waits before this step may have drawn what it points at)
    const queued = useGate.getState().queued[gate.key(path)] ?? [];
    const before = queued.slice(0, Math.max(0, queued.findIndex((q) => q.id === v.id))).flatMap((q) => q.items);
    void (painter.pointed?.(at, key, path, before, v.points) ?? Promise.resolve(null)).then((mini) => live && setPointed({ id: v.id, mini }));
    return () => {
      live = false;
    };
  }, [points, v, at, key, path]);
  if (!v) return null;
  if (v.mini !== undefined) return v.mini;
  return v.brings ? drawn : pointed?.id === v.id ? pointed.mini : null;
}

/**
 * The chat's window onto what the agent puts up for its canvas (docs/features/agents/chat-visuals.md). A thing shown
 * here isn't on the canvas yet: 캔버스에 올리기 puts it there (the same as pressing the line on the canvas), 버리기
 * throws it away and tells the agent. When steps come in on their own (설정 → 추천 그림 → 바로) nothing waits for a
 * yes; with the canvas closed the newest thing is shown as what goes up when it opens. It is Maestro's 추천, so it
 * wears that look (`.suggest-card`), not the approval cards' of 권한.
 * `root`: the project, for an agent the app doesn't list (the developer tests' stand-in). `onCanvas`: how to bring
 * this agent's canvas on screen (otherwise its tab is switched to the canvas).
 */
export function VisualCards({ path, root, onCanvas }: { path: string; root?: string; onCanvas?: () => void }) {
  const byHand = useSettings((s) => (s.guideMode ?? "auto") === "manual");
  const { v, mode, more, posted, open } = useGate(useShallow((s) => chatView(s, path, byHand)));
  const mini = useMini(path, root, v);
  const [busy, setBusy] = useState(false);
  if (!v && !posted) return null;

  const toCanvas = () => {
    if (onCanvas) return onCanvas();
    const s = useStore.getState();
    const groups = Object.values(s.groups).filter((g) => g.path && samePath(g.path, path));
    const g = groups.find((x) => x.id === s.focus) ?? groups[0];
    if (g) s.setTabLayout(g.id, path, "canvas");
  };
  const drop = (x: Visual) => {
    setBusy(true);
    void Promise.resolve(gate.drop(path, x)).catch(fail).finally(() => setBusy(false));
  };
  // a step that brings nothing and points nowhere (a line on its own, a tidy, an area going to its next step)
  const plain = !!v && !v.brings && !v.points.length;
  return (
    <>
      {v && (
        <div className="approval visual-card suggest-card" data-mode={mode}>
          <div className="approval-head">
            <span className="approval-badge">추천</span>
            <b>{v.what || "에이전트가 캔버스에 보여 줄 것"}</b>
            <span className="spacer" />
            {mode === "record" && (
              <button className="icon" title="닫기 · 채팅에서만 치워요 (캔버스를 열면 그대로 올라가요)" aria-label="그림 닫기" onClick={() => gate.close(path)}>
                <X size={14} />
              </button>
            )}
          </div>
          {mini && (
            <div className="visual-pic">
              <CanvasMini mini={mini} />
            </div>
          )}
          <div className="approval-actions">
            {mode === "record" ? (
              <button className="primary" title="이 에이전트의 캔버스를 열어요. 열면 그림이 올라가요" onClick={toCanvas}>
                캔버스에서 보기
              </button>
            ) : v.brings ? (
              <button className="primary" title="캔버스에 올려요. 캔버스의 안내를 누르는 것과 같아요" onClick={() => gate.pass(path)}>
                캔버스에 올리기
              </button>
            ) : (
              <button
                className="primary"
                title={plain ? "다음으로 넘어가요" : "캔버스에서 그곳을 봐요"}
                onClick={() => {
                  gate.pass(path);
                  if (!plain && !open) toCanvas();
                }}
              >
                {plain ? "넘기기" : "캔버스에서 보기"}
              </button>
            )}
            {v.brings && (
              <button disabled={busy} title="캔버스에 올리지 않고 버려요. 에이전트에게 버렸다고 알려요" onClick={() => drop(v)}>
                버리기
              </button>
            )}
            <span className="muted small">
              {[mode === "record" ? "캔버스를 열면 올라가요" : "", more > 0 ? `${mode === "record" ? "앞에" : "뒤에"} ${more}개 더` : ""].filter(Boolean).join(" · ")}
            </span>
          </div>
        </div>
      )}
      {posted > 0 && (
        <div className="approval visual-card visual-posted">
          <div className="approval-head">
            <span className="approval-badge">캔버스</span>
            <span className="muted small">올린 것 {posted}개</span>
            <span className="spacer" />
            <button onClick={toCanvas}>캔버스에서 보기</button>
          </div>
        </div>
      )}
    </>
  );
}
