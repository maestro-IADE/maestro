import { MessageCircleQuestion } from "./icons";
import { useWaiting, type Worktree } from "./store";
import type { AgentStatus } from "./types";

/**
 * What an agent is doing, as one glyph whose shape (not just its colour) tells the states apart. In priority order:
 * 승인 대기 (something waits for the user: a tool approval, a terminal question, a merge request) > 작업 중 (a turn
 * runs) > 새 응답 (finished while not looked at) > 쉬는 중 > 꺼짐 (or no agent at all). Only 작업 중 moves.
 */
export type GlyphState = "waiting" | "running" | "unread" | "idle" | "off" | "none";

export function glyphState({ status, waiting, unread, noAgent }: { status: AgentStatus; waiting?: boolean; unread?: boolean; noAgent?: boolean }): GlyphState {
  if (noAgent) return "none";
  if (waiting) return "waiting";
  if (status === "RUNNING") return "running";
  if (unread) return "unread";
  if (status === "IDLE") return "idle";
  return "off";
}

const TEXT: Record<GlyphState, string> = {
  waiting: "승인 대기",
  running: "작업 중",
  unread: "새 응답",
  idle: "쉬는 중",
  off: "꺼짐",
  none: "에이전트 없음",
};

/** The state in words: the glyph's tooltip and label, and the status line on a card. */
export const statusText = (s: GlyphState) => TEXT[s];

/**
 * The glyph in a square lane (`size`, 16 by default): a question bubble (12), a turning ring (8), a filled dot (8 new,
 * 6 resting) or an empty ring (6). With `onToggleRead` the lane is a button (the sidebar row's status lane).
 */
export function StatusGlyph({ status, waiting, unread, noAgent, size = 16, onToggleRead }: {
  status: AgentStatus;
  waiting?: boolean;
  unread?: boolean;
  noAgent?: boolean;
  size?: number;
  onToggleRead?: () => void;
}) {
  const s = glyphState({ status, waiting, unread, noAgent });
  // the lane of a sidebar row: a click marks it read / unread (while it waits or works, it opens it instead)
  const hint = !onToggleRead ? "" : s === "unread" ? " · 누르면 읽음" : s === "idle" || s === "off" ? " · 누르면 안 읽음" : "";
  const label = statusText(s) + hint;
  const mark = s === "waiting" ? <MessageCircleQuestion size={12} /> : <i />;
  const style = { "--glyph": `${size}px` } as React.CSSProperties;
  if (onToggleRead)
    return (
      <button
        type="button"
        className="glyph"
        data-s={s}
        style={style}
        title={label}
        aria-label={label}
        onClick={(e) => {
          e.stopPropagation();
          onToggleRead();
        }}
        onDoubleClick={(e) => e.stopPropagation()}
      >
        {mark}
      </button>
    );
  return (
    <span className="glyph" data-s={s} style={style} title={label} role="img" aria-label={label}>
      {mark}
    </span>
  );
}

/** An agent's glyph, with 승인 대기 read from the store (`useWaiting`). */
export function AgentGlyph({ w, size, onToggleRead }: { w: Worktree; size?: number; onToggleRead?: () => void }) {
  const waiting = useWaiting(w.path);
  return <StatusGlyph status={w.status} waiting={waiting} unread={w.unread} noAgent={!w.adapterId} size={size} onToggleRead={onToggleRead} />;
}
