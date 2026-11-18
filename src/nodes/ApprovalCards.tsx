import { useEffect } from "react";
import { useApprovals, useMerges, usePrs } from "../store";
import { chatAsksOf, loadChatAsks, useChatAsks } from "../chatAsks";
import { ChoiceCard } from "./ChoiceCard";
import { useSettings } from "../settings";
import { chatView, gate, useGate } from "../canvasGate";
import { VisualCards } from "./VisualCard";
import { ApprovalItem } from "./Attention";
import { MergeCard } from "./MergeCard";
import { PrCard } from "./PrCard";

/**
 * Permission prompts from this agent (its hook in the terminal, or the chat runtime's own question), answered here
 * instead of in the terminal (one `ApprovalItem` each, the card the 확인할 일 popover shares its answers with). A chat
 * runtime prompt has no terminal to hand it to: no "터미널에서 답하기". A Maestro's merge requests come first
 * (`MergeCard`): not tool approvals, but the same "waiting for you" place.
 * onOpenCli: "터미널에서 답하기" also shows this agent on its terminal (its own view for now, not the app's), where the
 * terminal now asks.
 * root, onCanvas: for the canvas card (`VisualCards`) of an agent the app doesn't list (the developer tests' stand-in).
 */
export function ApprovalCards({ path, onOpenCli, root, onCanvas }: { path: string; onOpenCli?: () => void; root?: string; onCanvas?: () => void }) {
  const approvals = useApprovals(path);
  const merges = useMerges(path);
  const prs = usePrs(path);
  // what this agent puts up for its canvas (chat-visuals.md): the next thing as a small picture, to be put on the
  // canvas or thrown away; with the canvas closed, what goes up when it opens
  const byHand = useSettings((s) => (s.guideMode ?? "auto") === "manual");
  const shows = useGate((s) => {
    const c = chatView(s, path, byHand);
    return !!c.v || c.posted > 0;
  });
  // (what waited before this chat was shown, while no canvas of the agent's is open)
  useEffect(() => void gate.peek(path), [path]);
  // the choices the agent put in the chat (`maestro ask --in chat`): numbered options and 직접 입력
  const asks = useChatAsks((s) => chatAsksOf(s, path));
  useEffect(() => void loadChatAsks(path), [path]);
  if (!approvals.length && !merges.length && !prs.length && !shows && !asks.length) return null;

  return (
    <div className="approvals nodrag nowheel">
      <VisualCards path={path} root={root} onCanvas={onCanvas} />
      {asks.map((a) => <ChoiceCard key={a.id} path={path} ask={a} />)}
      {merges.map((m) => <MergeCard key={m.id} m={m} />)}
      {prs.map((p) => <PrCard key={p.workerPath} p={p} />)}
      {approvals.map((a) => <ApprovalItem key={a.id} a={a} onOpenCli={onOpenCli} />)}
    </div>
  );
}
