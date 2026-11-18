import { useEffect } from "react";
import { useApprovals, useMerges, usePrs } from "../store";
import { chatAsksOf, loadChatAsks, useChatAsks } from "../chatAsks";
import { ChoiceCard } from "./ChoiceCard";
import { useSettings } from "../settings";
import { gate, useGate } from "../canvasGate";
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
 */
export function ApprovalCards({ path, onOpenCli }: { path: string; onOpenCli?: () => void }) {
  const approvals = useApprovals(path);
  const merges = useMerges(path);
  const prs = usePrs(path);
  // what this agent wants to show on its canvas, waiting to be let in (설정 → 그림 넘기기 → 눌러서)
  const byHand = useSettings((s) => (s.guideMode ?? "auto") === "manual");
  const waiting = useGate((s) => gate.count(s, path));
  const shows = byHand && waiting > 0;
  // the choices the agent put in the chat (`maestro ask --in chat`): numbered options and 직접 입력
  const asks = useChatAsks((s) => chatAsksOf(s, path));
  useEffect(() => void loadChatAsks(path), [path]);
  if (!approvals.length && !merges.length && !prs.length && !shows && !asks.length) return null;

  return (
    <div className="approvals nodrag nowheel">
      {shows && (
        <div className="approval gate-card">
          <div className="approval-head">
            <span className="approval-badge">캔버스</span>
            <b>보여 줄 것이 {waiting}개 기다려요</b>
          </div>
          <div className="gate-card-row">
            <span className="muted small">허용하면 캔버스에 하나씩 나타나요. 캔버스의 안내를 누르는 것과 같아요.</span>
            <button className="primary" onClick={() => gate.pass(path)}>허용</button>
          </div>
        </div>
      )}
      {asks.map((a) => <ChoiceCard key={a.id} path={path} ask={a} />)}
      {merges.map((m) => <MergeCard key={m.id} m={m} />)}
      {prs.map((p) => <PrCard key={p.workerPath} p={p} />)}
      {approvals.map((a) => <ApprovalItem key={a.id} a={a} onOpenCli={onOpenCli} />)}
    </div>
  );
}
