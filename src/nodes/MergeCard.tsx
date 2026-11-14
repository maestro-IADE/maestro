import { lazy, Suspense, useState } from "react";
import { isImeComposing } from "../ime";
import { createPortal } from "react-dom";
import { ipc } from "../ipc";
import { usePr, useStore } from "../store";
import type { MergeRequest, PrOptions, PrStep } from "../types";

// Monaco: loaded only when the diff opens
const MergeDiffDialog = lazy(() => import("../MergeDiffDialog"));

export const sha7 = (s: string) => s.slice(0, 7);

const VERIFIED_LINES = 8;
const COMMITS_SHOWN = 10;

const STEP_BUSY: Partial<Record<PrStep, string>> = { committing: "커밋하는 중…", pushing: "푸시하는 중…", creating: "PR 만드는 중…" };

/**
 * Answers a merge request: approve (a local merge, or in PR mode the push and PR with `pr`) or reject with an
 * optional reason for the conductor. The card and the change dialog's footer (MergeDiffDialog) both call this.
 * No optimistic drop: "maestro://merge-done" removes the request once it's really over. An outcome that keeps the
 * request (stale, conflict, failed) or a reject that found something to say goes to a toast; a failed call throws.
 */
export async function answerMergeRequest(id: number, approve: boolean, reason?: string, pr?: PrOptions): Promise<void> {
  const r = await ipc.answerMerge(id, approve, approve ? undefined : reason?.trim() || undefined, pr);
  // a PR outcome finishes the request ("maestro://merge-done" drops this card): its message stays on the PR's card
  // a reject that found the PR an unclear create made, or couldn't look, says so
  if (r.outcome === "stale" || r.outcome === "conflict" || r.outcome === "failed" || (r.outcome === "rejected" && r.message !== "병합 요청을 거부했어요")) useStore.getState().setError(r.message);
}

/**
 * A conductor's `maestro merge-request`, on the asking Maestro's blocks next to its tool approvals: the branch, what
 * the conductor checked, the commits and files, and the diff as recorded. 병합: the app merges with `--no-ff`
 * (merge.rs); a folder that isn't ready keeps the card with the reason. 거부: the conductor gets the reason.
 * In PR mode (R6) the approval pushes the recorded commit and opens (or updates) the pull request instead: the card
 * then has the title and body, the commit tick for uncommitted files, and the unclear-create buttons.
 */
export function MergeCard({ m }: { m: MergeRequest }) {
  const isPr = m.mode === "pr";
  const open = usePr(m.workerPath);
  const draft = m.prDraft ?? { title: "", body: "", draft: true };
  const [title, setTitle] = useState(m.pr?.title || draft.title);
  const [body, setBody] = useState(m.pr?.body || draft.body);
  const [asDraft, setAsDraft] = useState(m.pr ? m.pr.draft : draft.draft);
  const [showBody, setShowBody] = useState(false);
  const [commitIt, setCommitIt] = useState(false);
  const [commitMsg, setCommitMsg] = useState(m.pr?.commitMessage || `${m.workerName}: 작업 커밋`);
  const [updateBody, setUpdateBody] = useState(false);
  const dirty = m.workerDirty ?? [];
  const step = m.pr?.step ?? "none";
  const [busy, setBusy] = useState<"merge" | "reject" | "check" | "recreate" | null>(null);
  const [err, setErr] = useState("");
  const [rejecting, setRejecting] = useState(false);
  const [reason, setReason] = useState("");
  const [allVerified, setAllVerified] = useState(false);
  const [allCommits, setAllCommits] = useState(false);
  const [diffAt, setDiffAt] = useState<string | null>(null);

  const answer = async (approve: boolean, how: "merge" | "check" | "recreate" = "merge") => {
    setBusy(approve ? how : "reject");
    setErr("");
    try {
      const pr = isPr && approve
        ? { commitMessage: commitIt && dirty.length ? commitMsg.trim() : null, title, body, draft: asDraft, updateBody, recreate: how === "recreate", checkOnly: how === "check" }
        : undefined;
      await answerMergeRequest(m.id, approve, reason, pr);
    } catch (e) {
      setErr(String(e));
    }
    setBusy(null);
  };
  const needsCommit = isPr && dirty.length > 0 && !m.pr?.appCommit && !(commitIt && commitMsg.trim());
  // the app's commit isn't what the card showed: only a reject (and the worker's new request) moves on
  const mismatch = isPr ? m.pr?.commitMismatch ?? null : null;
  // a create ran (here or in the request this one replaced) and its PR wasn't found yet: look before making another
  const unconfirmed = isPr && (step === "unconfirmed" || (m.pr?.createAttemptAt != null && !STEP_BUSY[step]));
  // "다시 만들기": only after a lookup found nothing at least 30 s after the attempt (the backend checks again)
  const canRecreate = unconfirmed && m.pr?.lastEmptyLookupAt != null && m.pr?.createAttemptAt != null && m.pr.lastEmptyLookupAt - m.pr.createAttemptAt >= 30_000;
  // "계속" only for a step under way; after the app's commit (step "committed") the next click still pushes to the PR
  const resumable = step !== "none" && step !== "unconfirmed" && !(step === "committed" && m.existingPr);
  const primary = isPr ? (resumable ? "계속" : m.existingPr ? `푸시해서 PR #${m.existingPr} 갱신` : "푸시하고 PR 만들기") : "병합";

  const lines = (m.verified ?? "").split("\n");
  const longVerified = lines.length > VERIFIED_LINES;
  const verified = longVerified && !allVerified ? lines.slice(0, VERIFIED_LINES).join("\n") + "\n…" : m.verified;
  const commits = allCommits ? m.commits : m.commits.slice(0, COMMITS_SHOWN);

  return (
    <div className="approval merge-card">
      <div className="approval-head">
        <span className="approval-badge">{isPr ? "병합 요청 · PR" : "병합 요청"}</span>
        <b className="ellipsis" title={m.workerPath}>{m.workerName}</b>
        <span className="muted small ellipsis">{m.branch} → {m.base}</span>
        <span className="spacer" />
        <span className="muted small merge-sha" title={`${m.base} ${m.baseSha}\n${m.branch} ${m.tipSha}\n이 커밋들을 합쳐요 (요청 시점 기준)`}>
          {sha7(m.baseSha)}..{sha7(m.tipSha)}
        </span>
        <span className="small merge-stat">
          <span className="merge-add">+{m.adds}</span> <span className="merge-del">−{m.dels}</span>
        </span>
      </div>

      <div className="merge-section">
        <div className="small">검증</div>
        {m.verified ? (
          <>
            <pre className="approval-body nowheel merge-verified">{verified}</pre>
            {longVerified && <button className="link muted small" onClick={() => setAllVerified(!allVerified)}>{allVerified ? "접기" : "더 보기"}</button>}
          </>
        ) : (
          <div className="muted small">Lead가 검증 내용을 적지 않았어요</div>
        )}
      </div>

      {isPr && (
        <div className="muted small ellipsis" title={m.cmpRef ? `커밋과 충돌은 ${m.cmpRef} 기준으로 봤어요. 최종 판단은 GitHub가 해요` : undefined}>
          원격 {m.remote ?? "?"} · {m.base} ← {m.branch}{open?.state === "OPEN" ? ` · 열린 PR #${open.number}` : ""}
        </div>
      )}

      {((!isPr && m.projectDirty.length > 0) || m.workerUntracked > 0 || !m.conflictsChecked) && (
        <div className="merge-warn small">
          {!isPr && m.projectDirty.length > 0 && <div>요청 시점에 프로젝트 폴더에 커밋하지 않은 변경 {m.projectDirty.length}개: 병합 전에 정리해야 해요</div>}
          {m.workerUntracked > 0 && <div>워커에 커밋하지 않은 새 파일 {m.workerUntracked}개: 병합에 들어가지 않아요</div>}
          {!m.conflictsChecked && <div>충돌 사전 점검 못 함 (git 2.38 미만)</div>}
        </div>
      )}

      <div className="merge-section">
        <div className="small">커밋 {m.commitCount}개</div>
        <ul className="discard-list nowheel">
          {commits.map((c) => <li key={c} className="ellipsis" title={c}>{c}</li>)}
          {m.commits.length > COMMITS_SHOWN && (
            <li>
              <button className="link muted small" onClick={() => setAllCommits(!allCommits)}>{allCommits ? "접기" : `더 보기 (${m.commits.length - COMMITS_SHOWN}개)`}</button>
            </li>
          )}
          {m.commitCount > m.commits.length && <li className="muted">… 외 {m.commitCount - m.commits.length}개</li>}
        </ul>
      </div>

      <div className="merge-section">
        <div className="small">변경 파일 {m.fileCount}개</div>
        <ul className="discard-list nowheel merge-files">
          {m.files.map((f) => (
            <li key={f.path}>
              <button className="merge-file" title={f.oldPath ? `${f.oldPath} → ${f.path}` : f.path} onClick={() => setDiffAt(f.path)}>
                <span className="code">{f.code}</span>
                <span className="ellipsis">{f.path}</span>
                <span className="muted">{f.add == null ? "바이너리" : `+${f.add} −${f.del}`}</span>
              </button>
            </li>
          ))}
          {m.fileCount > m.files.length && <li className="muted">… 외 {m.fileCount - m.files.length}개</li>}
        </ul>
      </div>

      {isPr && dirty.length > 0 && !m.pr?.appCommit && (
        <div className="merge-section">
          <div className="small merge-warn">워커에 커밋하지 않은 변경 {dirty.length}개</div>
          <ul className="discard-list nowheel">
            {dirty.slice(0, 8).map((f) => <li key={f} className="ellipsis" title={f}>{f}</li>)}
            {dirty.length > 8 && <li className="muted">… 외 {dirty.length - 8}개</li>}
          </ul>
          <label className="small pr-check">
            <input type="checkbox" checked={commitIt} disabled={!!busy} onChange={(e) => setCommitIt(e.target.checked)} /> 커밋하고 진행
            <span className="muted"> (추적 안 된 파일은 포함 안 돼요)</span>
          </label>
          {commitIt && <input className="merge-reason nokey" value={commitMsg} disabled={!!busy} placeholder="커밋 메시지" onChange={(e) => setCommitMsg(e.target.value)} />}
        </div>
      )}

      {isPr && (
        <div className="merge-section pr-fields">
          <input className="merge-reason nokey" value={title} disabled={!!busy} placeholder="PR 제목" onChange={(e) => setTitle(e.target.value)} />
          <button className="link muted small" onClick={() => setShowBody(!showBody)}>{showBody ? "본문 접기" : "본문 보기·고치기"}</button>
          {showBody && <textarea className="pr-body nokey nowheel" value={body} disabled={!!busy} spellCheck={false} onChange={(e) => setBody(e.target.value)} />}
          <div className="pr-opts small">
            <label className="pr-check" title={m.verified ? undefined : "검증 없음 → 초안"}>
              <input type="checkbox" checked={asDraft} disabled={!!busy} onChange={(e) => setAsDraft(e.target.checked)} /> 초안으로
            </label>
            {!!m.existingPr && open?.appCreated && (
              <label className="pr-check" title="앱이 만든 PR만 제목과 본문을 다시 써요">
                <input type="checkbox" checked={updateBody} disabled={!!busy} onChange={(e) => setUpdateBody(e.target.checked)} /> 본문 갱신
              </label>
            )}
            {!!m.existingPr && <span className="muted">초안 여부는 GitHub에서 바꿔요</span>}
          </div>
        </div>
      )}

      {unconfirmed && (
        <div className="merge-warn small">
          PR이 만들어졌는지 확인하지 못했어요. '다시 확인'은 찾기만 하고, '다시 만들기'는 30초 뒤 다시 찾아 없을 때만 만들어요.
        </div>
      )}
      {(err || m.pr?.error) && <div className="err small wrap">{err || m.pr?.error}</div>}
      {!err && !m.pr?.error && m.pr?.notice && <div className="muted small wrap">{m.pr.notice}</div>}
      {rejecting ? (
        <div className="approval-actions">
          <input
            className="merge-reason nokey"
            autoFocus
            value={reason}
            disabled={!!busy}
            placeholder="거부 사유 (선택, Lead에게 전해요)"
            onChange={(e) => setReason(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !isImeComposing(e)) answer(false);
              if (e.key === "Escape") setRejecting(false);
            }}
          />
          <button className="danger" disabled={!!busy} onClick={() => answer(false)}>{busy === "reject" ? "거부하는 중…" : "확인"}</button>
          <button disabled={!!busy} onClick={() => setRejecting(false)}>취소</button>
        </div>
      ) : (
        <div className="approval-actions">
          <button disabled={!m.files.length} onClick={() => setDiffAt(m.files[0].path)}>변경 보기</button>
          <span className="spacer" />
          {unconfirmed ? (
            <>
              <button disabled={!!busy} title="PR을 찾기만 해요 (커밋·푸시·만들기 없음)" onClick={() => answer(true, "check")}>{busy === "check" ? "확인하는 중…" : "다시 확인"}</button>
              <button disabled={!!busy || !canRecreate} title={canRecreate ? "한 번 더 찾아보고 없을 때만 만들어요" : "만든 지 30초가 지난 뒤 '다시 확인'으로 PR이 없다는 걸 확인해야 해요"} onClick={() => answer(true, "recreate")}>
                {busy === "recreate" ? "만드는 중…" : "다시 만들기"}
              </button>
            </>
          ) : (
            <button
              className="primary"
              disabled={!!busy || needsCommit || !!mismatch || (isPr && !title.trim())}
              title={
                !isPr
                  ? "기준 브랜치에 --no-ff로 합쳐요. 프로젝트 폴더에 커밋하지 않은 변경이 없어야 해요"
                  : mismatch
                    ? mismatch
                  : needsCommit
                    ? "워커에 커밋하지 않은 변경이 있어요: '커밋하고 진행'을 고르고 메시지를 적거나, 워커가 커밋한 뒤 다시 요청하게 하세요"
                    : "승인한 커밋을 원격 브랜치에 푸시해요 (강제 푸시는 하지 않아요). 만들기 직전에 한 번 더 확인하고, 이미 열린 PR이 있으면 그 PR을 써요"
              }
              onClick={() => answer(true)}
            >
              {busy === "merge" ? (isPr ? STEP_BUSY[step] ?? "확인하는 중…" : "합치는 중…") : primary}
            </button>
          )}
          <button className="danger" disabled={!!busy} onClick={() => setRejecting(true)}>거부</button>
        </div>
      )}
      {diffAt != null &&
        createPortal(
          <Suspense fallback={null}>
            <MergeDiffDialog m={m} path={diffAt} onClose={() => setDiffAt(null)} />
          </Suspense>,
          document.body,
        )}
    </div>
  );
}
