import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { ipc } from "../ipc";
import { usePr, useStore } from "../store";
import type { PullRequest } from "../types";
import { PrLogDialog } from "../PrLogDialog";
import { sha7 } from "./MergeCard";

const STATE: Record<string, string> = { OPEN: "열림", CLOSED: "닫힘", MERGED: "합쳐짐" };
const REVIEW: Record<string, string> = { APPROVED: "승인됨", CHANGES_REQUESTED: "변경 요청", REVIEW_REQUIRED: "리뷰 필요" };

/** "열림" / "초안" / "대기열" / "합쳐짐" / "닫힘" */
export const prState = (p: PullRequest) => (p.state === "OPEN" ? (p.queued ? "대기열" : p.isDraft ? "초안" : "열림") : STATE[p.state] ?? p.state);

/** The CI state of the head shown: checks recorded for another commit (before a push) count as pending. */
export const ciState = (p: PullRequest): PullRequest["checks"]["state"] => (p.checks.headSha && p.checks.headSha !== p.headSha ? "pending" : p.checks.state);

/** "통과" / "실패 n" / "진행 중 n" / "확인 중" / "없음" */
export const ciLabel = (p: PullRequest) => {
  const s = ciState(p);
  if (s === "pending") return s === p.checks.state && p.checks.pending > 0 ? `진행 중 ${p.checks.pending}` : "확인 중";
  return s === "failure" ? `실패 ${p.checks.failed}` : s === "success" ? "통과" : "없음";
};

const tooltip = (p: PullRequest) =>
  [`PR #${p.number} · ${prState(p)}`, `${p.branch} → ${p.base}`, p.reviewDecision ? `리뷰: ${REVIEW[p.reviewDecision] ?? p.reviewDecision}` : null, `CI: ${ciLabel(p)}`, p.error ? `오류: ${p.error}` : null, "누르면 브라우저에서 열어요"]
    .filter(Boolean)
    .join("\n");

/** A worker's PR as a small chip (worker card, agent header, folded block); a click opens it in the browser. */
export function PrChip({ path }: { path: string }) {
  const p = usePr(path);
  const setError = useStore((s) => s.setError);
  if (!p) return null;
  return (
    <button
      className="pr-chip nodrag"
      data-ci={ciState(p)}
      data-state={p.state.toLowerCase()}
      title={tooltip(p)}
      onClick={(e) => {
        e.stopPropagation();
        ipc.openPr(p.workerPath).catch(setError);
      }}
      onDoubleClick={(e) => e.stopPropagation()}
    >
      PR #{p.number}
      {ciState(p) === "failure" && <span className="pr-chip-ci"> · CI 실패</span>}
    </button>
  );
}

/**
 * A worker's pull request on the Maestro's blocks (PR mode, R6): state, review, CI and the merge control. "PR 병합"
 * is the user's own click: merge (default) or squash, pinned to the head shown here; a red or missing CI, or a
 * change request, asks once more. After the merge the project folder is fast-forwarded when it's clean and on the
 * base; otherwise "기준 브랜치 받기" does it later.
 */
export function PrCard({ p }: { p: PullRequest }) {
  const setError = useStore((s) => s.setError);
  const pause = useStore((s) => s.prPoll[p.host]);
  // shown only while it lasts: the backend says when it ends only after its next poll of that host
  const [, tick] = useState(0);
  useEffect(() => {
    const left = pause ? pause.pausedUntil - Date.now() : 0;
    if (left <= 0) return;
    const t = setTimeout(() => tick((n) => n + 1), left + 50);
    return () => clearTimeout(t);
  }, [pause]);
  const paused = pause && pause.pausedUntil > Date.now() ? pause : null;
  const [open, setOpen] = useState(false);
  const [method, setMethod] = useState<"merge" | "squash">("merge");
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState<"merge" | "sync" | "refresh" | null>(null);
  const [err, setErr] = useState("");
  const [log, setLog] = useState(false);
  const seenNotice = useStore((s) => s.seenNotices[p.workerPath.toLowerCase()]);
  const setSeenNotice = useStore((s) => s.seeNotice);
  const ci = ciState(p);
  // a "그래도 병합" is for the head and CI it was asked about
  useEffect(() => setConfirm(false), [p.headSha, ci, p.reviewDecision]);
  // the merge methods the repo allows (the readiness check is cached in the backend)
  const [allowed, setAllowed] = useState<{ merge: boolean; squash: boolean }>({ merge: true, squash: true });
  useEffect(() => {
    if (!open || p.state !== "OPEN") return;
    let live = true;
    ipc.prReadiness(p.root).then((r) => {
      if (!live || !r.ok) return;
      setAllowed({ merge: r.mergeAllowed, squash: r.squashAllowed });
      if (!r.mergeAllowed && r.squashAllowed) setMethod("squash");
    }).catch(() => {});
    return () => {
      live = false;
    };
  }, [open, p.root, p.state]);

  const risky = ci === "failure" || ci === "none" || p.reviewDecision === "CHANGES_REQUESTED";
  // as the backend refuses them before merging (a required review, a branch protection rule)
  const blocked = p.reviewDecision === "REVIEW_REQUIRED" || p.mergeStateStatus === "BLOCKED";
  // a click already sent (until GitHub shows it queued or merged, or the poller ends it): never a second `gh pr merge`
  const canMerge = p.state === "OPEN" && !p.isDraft && !p.queued && !p.mergeAttempt && p.mergeable !== "CONFLICTING" && ci !== "pending" && !blocked;
  const why = p.state !== "OPEN" ? "열린 PR이 아니에요" : p.isDraft ? "초안 PR이에요. GitHub에서 준비 완료로 바꿔 주세요" : p.queued ? "병합 대기열에 있어요" : p.mergeAttempt ? "병합 요청을 보냈어요. 상태를 확인하는 중이에요" : p.mergeable === "CONFLICTING" ? "기준 브랜치와 충돌해요" : ci === "pending" ? "CI가 진행 중이에요" : blocked ? "리뷰나 보호 규칙 때문에 지금은 병합할 수 없어요. GitHub에서 확인한 뒤 '다시 확인'을 누르세요" : "";

  const run = async (what: "merge" | "sync" | "refresh", f: () => Promise<unknown>) => {
    setBusy(what);
    setErr("");
    try {
      await f();
    } catch (e) {
      setErr(String(e));
    }
    setBusy(null);
  };
  const redCi = ci === "failure" || ci === "none" ? ci : null;
  const changesRequested = p.reviewDecision === "CHANGES_REQUESTED";
  const merge = () => {
    if (risky && !confirm) return setConfirm(true);
    setConfirm(false);
    // exactly what was shown and confirmed: a different state on GitHub by now is refused and shown here
    run("merge", () => ipc.mergePr(p.workerPath, method, p.headSha, risky ? { ci: redCi, changesRequested } : null));
  };

  return (
    <div className="approval merge-card pr-card" data-ci={ci}>
      <div className="approval-head">
        <button className="link approval-badge" onClick={() => setOpen(!open)} title={open ? "접기" : "펼치기"}>PR #{p.number}</button>
        <b className="ellipsis" title={p.workerPath}>{p.workerName}</b>
        <span className="pr-pill" data-kind="state" data-state={p.state.toLowerCase()}>{prState(p)}</span>
        {p.reviewDecision && <span className="pr-pill" data-kind="review" data-review={p.reviewDecision.toLowerCase()}>{REVIEW[p.reviewDecision] ?? p.reviewDecision}</span>}
        <span className="pr-pill" data-kind="ci" data-ci={ci}>CI {ciLabel(p)}</span>
        {!open && p.error && <span className="pr-pill" data-kind="error" title={p.error}>오류</span>}
        <span className="spacer" />
        <span className="muted small merge-sha" title={p.headSha}>{sha7(p.headSha)}</span>
        {p.state === "OPEN" && !open && (
          <button className="primary small" disabled={!canMerge || !!busy} title={why || "펼쳐서 병합 방식을 고르고 병합해요"} onClick={() => setOpen(true)}>PR 병합</button>
        )}
      </div>
      {p.notice && p.notice !== seenNotice && (
        <div className="muted small pr-notice">
          <span className="wrap">{p.notice}</span>
          <button className="link muted small" title="이 안내 숨기기" onClick={() => p.notice && setSeenNotice(p.workerPath, p.notice)}>×</button>
        </div>
      )}
      {open && (
        <>
          <div className="muted small ellipsis">{p.branch} → {p.base} · {p.owner}/{p.repo}</div>
          {p.checks.failedJobs.length > 0 && <div className="merge-warn small">실패한 작업: {p.checks.failedJobs.map((j) => j.name).join(", ")}</div>}
          {p.mergeable === "CONFLICTING" && <div className="merge-warn small">기준 브랜치와 충돌해요: 워커가 기준을 합쳐 충돌을 푼 뒤 다시 요청하게 하세요</div>}
          {p.error && <div className="merge-warn small">{p.error}</div>}
          {blocked && p.state === "OPEN" && <div className="merge-warn small">리뷰나 보호 규칙 때문에 지금은 병합할 수 없어요</div>}
          {paused && <div className="muted small">{paused.message} ({new Date(paused.pausedUntil).toLocaleTimeString()}까지)</div>}
          {p.afterMerge && <div className="muted small">프로젝트 폴더: {p.afterMerge.message}</div>}
          {err && <div className="err small wrap">{err}</div>}
          {confirm && (
            <div className="merge-warn small">
              {[redCi === "failure" && "CI가 실패했어요", redCi === "none" && "CI 결과가 없어요", changesRequested && "변경 요청이 있어요"].filter(Boolean).join(" ")} 그래도 병합할까요?
            </div>
          )}
          <div className="approval-actions">
            <button onClick={() => ipc.openPr(p.workerPath).catch(setError)}>PR 열기</button>
            <button disabled={!!busy} onClick={() => run("refresh", () => ipc.refreshPrs(p.root))}>{busy === "refresh" ? "확인 중…" : "다시 확인"}</button>
            {ci === "failure" && <button onClick={() => setLog(true)}>실패 로그 첨부…</button>}
            <span className="spacer" />
            {p.state === "OPEN" ? (
              <>
                <span className="seg" title="병합 방식 (squash는 PR 모드에서만)">
                  {(["merge", "squash"] as const).filter((m) => allowed[m]).map((m) => (
                    <button key={m} data-on={m === method || undefined} onClick={() => setMethod(m)}>{m === "merge" ? "병합 커밋" : "squash"}</button>
                  ))}
                </span>
                <button className="primary" disabled={!canMerge || !!busy} title={why || `GitHub에서 ${method === "merge" ? "병합 커밋으로" : "squash로"} 합쳐요. 본 머리 커밋(${sha7(p.headSha)})과 다르면 거부돼요`} onClick={merge}>
                  {busy === "merge" ? "병합하는 중…" : confirm ? "그래도 병합" : "PR 병합"}
                </button>
                {confirm && <button onClick={() => setConfirm(false)}>취소</button>}
              </>
            ) : (
              <>
                {p.state === "MERGED" && !p.afterMerge?.baseUpdated && (
                  <button disabled={!!busy} title="프로젝트 폴더가 기준 브랜치에 있고 깨끗할 때만 빨리감기로 받아요" onClick={() => run("sync", () => ipc.prSyncBase(p.workerPath))}>
                    {busy === "sync" ? "받는 중…" : "기준 브랜치 받기"}
                  </button>
                )}
                <button onClick={() => ipc.dismissPr(p.workerPath).catch(setError)}>닫기</button>
              </>
            )}
          </div>
        </>
      )}
      {log && createPortal(<PrLogDialog p={p} onClose={() => setLog(false)} />, document.body)}
    </div>
  );
}
