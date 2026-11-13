import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { ipc } from "../ipc";
import { Bell, ChevronRight, GitMerge, GitPullRequest, MessageCircleQuestion, SquareTerminal } from "../icons";
import { approvalSummary, groupByRoot, type AttentionItem } from "../attention";
import { useAttention, useStore } from "../store";
import { showAgentAs } from "../agentActions";
import { keysOf } from "../shortcuts";
import { ToolInputPreview } from "./ToolDiff";
import type { Approval, ApprovalChoice } from "../types";

// 확인할 일 (docs/features/ui/attention.md): the top bar's button and its popover, one row per item (AttentionRow), and
// one tool approval's card (ApprovalItem) that the agent's chat shares. The list itself is attention.ts.

// Monaco: loaded only when a merge request's changes open
const MergeDiffDialog = lazy(() => import("../MergeDiffDialog"));

/**
 * Answers a tool approval: it leaves the store at once (the backend's approval-done would also remove it; no wait),
 * so the same approval can't be pressed twice here and in the chat. A second answer to an id the backend already
 * settled is ignored there (`Approvals::answer`).
 */
export function answerApproval(a: Pick<Approval, "id">, choice: ApprovalChoice) {
  const s = useStore.getState();
  s.dropApproval(a.id);
  ipc.answerApproval(a.id, choice).catch(s.setError);
}

/** The agent in a lasting tab of the group showing its project (its own first view: full screen or canvas). */
export function openAgent(path: string) {
  useStore.getState().select(path, true);
}

/**
 * "터미널에서 답하기" for a terminal question (and the sidebar row's "터미널로 보기"): the agent opens full screen on its
 * terminal, as this agent's exception only (`setViewOverride`); the app's 에이전트 보기 stays as it is.
 */
export function answerInTerminal(path: string) {
  showAgentAs(path, "cli");
}

/**
 * One tool approval as a card in the agent's chat (and its canvas bar): the tool, what it would change (a file
 * change shows its diff), and 허용 / 항상 허용 / 거부, plus 터미널에서 답하기 when a terminal can take it (not a chat
 * runtime prompt). onOpenCli: what else 터미널에서 답하기 does (the agent's terminal view).
 */
export function ApprovalItem({ a, onOpenCli }: { a: Approval; onOpenCli?: () => void }) {
  return (
    <div className="approval">
      <div className="approval-head">
        <span className="approval-badge">승인 요청</span>
        <b>{a.toolName}</b>
      </div>
      <ToolInputPreview name={a.toolName} input={a.toolInput} rows={8} root={a.worktreePath} fallback={<pre className="approval-body nowheel">{approvalSummary(a)}</pre>} />
      {a.toolInput?.description && <div className="muted small">{a.toolInput.description}</div>}
      <div className="approval-actions">
        <ApprovalButtons a={a} />
        <span className="spacer" />
        {a.cliAnswer !== false && (
          <button
            className="link muted small"
            title="터미널의 원래 승인 화면으로 넘겨요"
            onClick={() => {
              answerApproval(a, "cli");
              onOpenCli?.();
            }}
          >
            터미널에서 답하기
          </button>
        )}
      </div>
    </div>
  );
}

/** 허용 (main) · 항상 허용 (when the agent offers a rule) · 거부 */
function ApprovalButtons({ a, small }: { a: Approval; small?: boolean }) {
  const cls = small ? " small" : "";
  return (
    <>
      <button className={`primary${cls}`} onClick={() => answerApproval(a, "allow")}>허용</button>
      {a.canAlways && !small && (
        <button title="이 작업공간에서 같은 요청은 다시 묻지 않아요" onClick={() => answerApproval(a, "always")}>항상 허용</button>
      )}
      <button className={`danger${cls}`} onClick={() => answerApproval(a, "deny")}>거부</button>
    </>
  );
}

const ICON: Record<AttentionItem["kind"], typeof Bell> = { approval: MessageCircleQuestion, tui: SquareTerminal, merge: GitMerge, pr: GitPullRequest };

/**
 * One 확인할 일 with what can be done about it right there. `full` (the popover): three lines, the who, the what, the
 * buttons. `compact` (a worker card, docs/prd stage 11): one 32px line. onOpen: [열기] (the agent; for a merge request
 * or a PR its Maestro). onDiff: [변경 보기] of a merge request (the changes dialog, where it is merged or rejected).
 * onTerminal: [터미널에서 답하기] of a terminal question (by default `answerInTerminal`; the popover also closes).
 */
export function AttentionRow({
  item,
  size = "full",
  onOpen,
  onDiff,
  onTerminal = answerInTerminal,
}: {
  item: AttentionItem;
  size?: "full" | "compact";
  onOpen: () => void;
  onDiff: (mergeId: number) => void;
  onTerminal?: (path: string) => void;
}) {
  const a = useStore((s) => (item.approvalId == null ? undefined : s.approvals.find((x) => x.id === item.approvalId)));
  const mergeFiles = useStore((s) => (item.mergeId == null ? 0 : (s.merges.find((m) => m.id === item.mergeId)?.files.length ?? 0)));
  const [more, setMore] = useState(false);
  if (item.kind === "approval" && !a) return null;
  const Icon = ICON[item.kind];
  const open = (
    <button className={size === "compact" ? "small" : undefined} title="이 에이전트를 탭으로 열어요" onClick={onOpen}>열기</button>
  );
  const actions =
    item.kind === "approval" ? (
      <>
        <ApprovalButtons a={a!} small={size === "compact"} />
        <span className="spacer" />
        {open}
      </>
    ) : item.kind === "tui" ? (
      <>
        <button className={size === "compact" ? "primary small" : "primary"} onClick={() => onTerminal(item.path)}>터미널에서 답하기</button>
        <span className="spacer" />
        {open}
      </>
    ) : item.kind === "merge" ? (
      <>
        <button
          className={size === "compact" ? "small" : undefined}
          disabled={!mergeFiles}
          title="바뀐 파일과 변경 내용을 봐요 · 로컬 방식은 창 바닥에서 병합해요"
          onClick={() => onDiff(item.mergeId!)}
        >
          변경 보기
        </button>
        <span className="spacer" />
        {open}
      </>
    ) : (
      <>
        <span className="spacer" />
        {open}
      </>
    );

  if (size === "compact") {
    const text =
      item.kind === "approval" ? `${item.tool ?? "도구"} · ${item.detail ?? ""}` : item.kind === "tui" ? "터미널 질문" : item.kind === "merge" ? `병합 요청 · 파일 ${item.files ?? 0}` : item.title;
    return (
      <div className="attn-row nodrag" data-size="compact" data-kind={item.kind}>
        <Icon size={14} className="attn-icon" data-err={item.failed || undefined} />
        <span className="attn-compact-text ellipsis" title={item.detail ?? item.title}>{text}</span>
        {actions}
      </div>
    );
  }

  return (
    <div className="attn-row" data-kind={item.kind}>
      <div className="attn-line1">
        <Icon size={14} className="attn-icon" data-err={item.failed || undefined} />
        <span className="attn-name ellipsis" title={item.path}>{item.name}</span>
      </div>
      {item.kind === "approval" ? (
        <div className="attn-line2">
          <button type="button" className="attn-more" data-open={more || undefined} title={more ? "접기" : "펼쳐서 보기"} aria-label={more ? "접기" : "펼쳐서 보기"} aria-expanded={more} onClick={() => setMore(!more)}>
            <ChevronRight size={12} />
          </button>
          <span className="attn-tool">{item.title}</span>
          <span className="attn-detail ellipsis" title={item.detail}>{item.detail}</span>
        </div>
      ) : (
        <div className="attn-line2 ellipsis" title={item.title}>{item.title}</div>
      )}
      {more && a && (
        <div className="attn-preview nowheel">
          <ToolInputPreview name={a.toolName} input={a.toolInput} rows={8} root={a.worktreePath} fallback={<pre className="approval-body">{approvalSummary(a)}</pre>} />
          {a.toolInput?.description && <div className="muted small">{a.toolInput.description}</div>}
        </div>
      )}
      <div className="attn-actions">{actions}</div>
    </div>
  );
}

/**
 * The top bar's 확인할 일: a bell and how many things wait (warn), and under it a 400px popover with them, by
 * project. Tool approvals are answered right there; a terminal question opens the agent's terminal; a merge request
 * shows its changes; [열기] goes to the agent. In every window (App, WorkspaceWindow).
 */
export function AttentionButton() {
  const items = useAttention();
  const repos = useStore((s) => s.repos);
  // in the store, so 빠른 이동's "확인할 일" can open it too
  const open = useStore((s) => s.attentionOpen);
  const setOpen = useStore((s) => s.setAttentionOpen);
  const [diff, setDiff] = useState<number | null>(null);
  const box = useRef<HTMLDivElement>(null);

  // a press outside the button and its popover, or Esc, closes it
  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => !box.current?.contains(e.target as Node) && setOpen(false);
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    window.addEventListener("mousedown", away);
    window.addEventListener("keydown", esc);
    return () => {
      window.removeEventListener("mousedown", away);
      window.removeEventListener("keydown", esc);
    };
  }, [open]);

  const n = items.length;
  const hint = `확인할 일 (${keysOf("nextAttention")} 다음)`;
  const nameOf = (root: string) => repos.find((r) => r.root === root)?.name ?? (root || "프로젝트 밖");
  const go = (it: AttentionItem) => {
    setOpen(false);
    openAgent(it.path);
  };

  return (
    <div className="tb-attention" ref={box}>
      <button
        className="tb-text tb-attn-btn"
        data-on={n > 0 || undefined}
        title={hint}
        aria-label={n ? `확인할 일 ${n}개` : "확인할 일"}
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        <Bell />
        {n > 0 && <span className="tb-count">{n}</span>}
      </button>
      {open && (
        <div className="tb-popover attn-pop nowheel" role="dialog" aria-label="확인할 일">
          {n ? (
            groupByRoot(items).map((g) => (
              <section key={g.root} className="attn-group">
                <div className="attn-group-head">
                  <span className="ellipsis">{nameOf(g.root)}</span>
                  <span className="attn-group-n">{g.items.length}</span>
                </div>
                {g.items.map((it) => (
                  <AttentionRow
                    key={it.id}
                    item={it}
                    onOpen={() => go(it)}
                    onDiff={(id) => {
                      setOpen(false);
                      setDiff(id);
                    }}
                    onTerminal={(path) => {
                      setOpen(false);
                      answerInTerminal(path);
                    }}
                  />
                ))}
              </section>
            ))
          ) : (
            <div className="attn-empty">
              <b>확인할 일이 없어요</b>
              <p>에이전트가 승인을 묻거나 마에스트로가 병합을 요청하면 여기에 모여요</p>
            </div>
          )}
        </div>
      )}
      {diff != null && <MergeDiff id={diff} onClose={() => setDiff(null)} />}
    </div>
  );
}

/** A merge request's changes (the dialog the merge card opens), until it closes or the request is over. */
export function MergeDiff({ id, onClose }: { id: number; onClose: () => void }) {
  const m = useStore((s) => s.merges.find((x) => x.id === id));
  useEffect(() => {
    if (!m || !m.files.length) onClose();
  }, [!m || !m.files.length]);
  if (!m || !m.files.length) return null;
  return createPortal(
    <Suspense fallback={null}>
      <MergeDiffDialog m={m} path={m.files[0].path} onClose={onClose} />
    </Suspense>,
    document.body,
  );
}
