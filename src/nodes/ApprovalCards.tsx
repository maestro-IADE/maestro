import { ipc } from "../ipc";
import { useApprovals, useStore } from "../store";
import type { Approval, ApprovalChoice } from "../types";

// What the tool wants to do, in one line where possible.
function summary(a: Approval): string {
  const i = a.toolInput ?? {};
  return i.command ?? i.file_path ?? i.url ?? i.pattern ?? i.path ?? JSON.stringify(i);
}

/**
 * Permission prompts from this agent (its hook in the terminal, or the chat runtime's own question), answered here
 * instead of in the terminal. A chat-runtime prompt has no terminal to hand it to: no "CLI에서 답하기".
 */
export function ApprovalCards({ path }: { path: string }) {
  const approvals = useApprovals(path);
  const { dropApproval, setError } = useStore();
  if (!approvals.length) return null;

  const answer = (a: Approval, choice: ApprovalChoice) => {
    dropApproval(a.id); // the backend's approval-done event would also remove it; don't wait for it
    ipc.answerApproval(a.id, choice).catch(setError);
  };

  return (
    <div className="approvals nodrag">
      {approvals.map((a) => (
        <div key={a.id} className="approval">
          <div className="approval-head">
            <span className="approval-badge">승인 요청</span>
            <b>{a.toolName}</b>
          </div>
          <pre className="approval-body nowheel">{summary(a)}</pre>
          {a.toolInput?.description && <div className="muted small">{a.toolInput.description}</div>}
          <div className="approval-actions">
            <button className="primary" onClick={() => answer(a, "allow")}>허용</button>
            {a.canAlways && <button title="이 작업공간에서 같은 요청은 다시 묻지 않아요" onClick={() => answer(a, "always")}>항상 허용</button>}
            <button className="danger" onClick={() => answer(a, "deny")}>거부</button>
            <span className="spacer" />
            {a.cliAnswer !== false && (
              <button className="link muted small" title="터미널의 원래 승인 화면으로 넘겨요" onClick={() => answer(a, "cli")}>CLI에서 답하기</button>
            )}
          </div>
        </div>
      ))}
    </div>
  );
}
