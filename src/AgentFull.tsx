import { AgentPanel } from "./nodes/AgentNode";
import { useSettings } from "./settings";

/**
 * Full screen ↔ canvas for one worktree tab: one icon button showing where a click takes you. Each tab keeps its
 * own; the last one picked (settings `agentLayout`) is what new tabs open with.
 */
export function LayoutToggle({ value, onChange }: { value: "full" | "canvas"; onChange: (v: "full" | "canvas") => void }) {
  const set = useSettings((s) => s.set);
  const full = value === "full";
  const label = full ? "캔버스로 보기" : "전체 화면으로 보기";
  return (
    <button
      className="icon layout-toggle"
      aria-pressed={full}
      title={label}
      aria-label={label}
      onClick={() => {
        const next = full ? "canvas" : "full";
        onChange(next);
        set({ agentLayout: next });
      }}
    >
      {/* Lucide "workflow" / "maximize-2" (ISC) */}
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
        {full ? (
          <>
            <rect x="3" y="3" width="8" height="8" rx="2" />
            <path d="M7 11v4a2 2 0 0 0 2 2h4" />
            <rect x="13" y="13" width="8" height="8" rx="2" />
          </>
        ) : (
          <path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7" />
        )}
      </svg>
    </button>
  );
}

/** The default worktree view: its agent filling the stage, like the Claude Code app. */
export function AgentFull({ path, toggle }: { path: string; toggle: React.ReactNode }) {
  const { fullView, set } = useSettings();
  return <AgentPanel className="agent-full" path={path} view={fullView} onView={(v) => set({ fullView: v })} extra={toggle} />;
}
