import { useApprovals, useMerges, usePrs } from "../store";
import { ApprovalItem } from "./Attention";
import { MergeCard } from "./MergeCard";
import { PrCard } from "./PrCard";

/**
 * Permission prompts from this agent (its hook in the terminal, or the chat runtime's own question), answered here
 * instead of in the terminal (one `ApprovalItem` each, the card the 확인할 일 popover shares its answers with). A chat
 * runtime prompt has no terminal to hand it to: no "터미널에서 답하기". A Maestro's merge requests come first
 * (`MergeCard`): not tool approvals, but the same "waiting for you" place.
 * onOpenCli: "터미널에서 답하기" also opens the terminal view, where the terminal now asks.
 */
export function ApprovalCards({ path, onOpenCli }: { path: string; onOpenCli?: () => void }) {
  const approvals = useApprovals(path);
  const merges = useMerges(path);
  const prs = usePrs(path);
  if (!approvals.length && !merges.length && !prs.length) return null;

  return (
    <div className="approvals nodrag nowheel">
      {merges.map((m) => <MergeCard key={m.id} m={m} />)}
      {prs.map((p) => <PrCard key={p.workerPath} p={p} />)}
      {approvals.map((a) => <ApprovalItem key={a.id} a={a} onOpenCli={onOpenCli} />)}
    </div>
  );
}
