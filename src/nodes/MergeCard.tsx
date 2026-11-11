import { lazy, Suspense, useState } from "react";
import { createPortal } from "react-dom";
import { ipc } from "../ipc";
import { useStore } from "../store";
import type { MergeRequest } from "../types";

// Monaco: loaded only when the diff opens
const MergeDiffDialog = lazy(() => import("../MergeDiffDialog"));

export const sha7 = (s: string) => s.slice(0, 7);

const VERIFIED_LINES = 8;
const COMMITS_SHOWN = 10;

/**
 * A conductor's `maestro merge-request`, on the asking Maestro's blocks next to its tool approvals: the branch, what
 * the conductor checked, the commits and files, and the diff as recorded. 병합: the app merges with `--no-ff`
 * (merge.rs); a folder that isn't ready keeps the card with the reason. 거부: the conductor gets the reason.
 */
export function MergeCard({ m }: { m: MergeRequest }) {
  const setError = useStore((s) => s.setError);
  const [busy, setBusy] = useState<"merge" | "reject" | null>(null);
  const [err, setErr] = useState("");
  const [rejecting, setRejecting] = useState(false);
  const [reason, setReason] = useState("");
  const [allVerified, setAllVerified] = useState(false);
  const [allCommits, setAllCommits] = useState(false);
  const [diffAt, setDiffAt] = useState<string | null>(null);

  // no optimistic drop: "maestro://merge-done" removes the card once it's really over
  const answer = async (approve: boolean) => {
    setBusy(approve ? "merge" : "reject");
    setErr("");
    try {
      const r = await ipc.answerMerge(m.id, approve, approve ? undefined : reason.trim() || undefined);
      if (r.outcome === "stale" || r.outcome === "conflict" || r.outcome === "failed") setError(r.message);
    } catch (e) {
      setErr(String(e));
    }
    setBusy(null);
  };

  const lines = (m.verified ?? "").split("\n");
  const longVerified = lines.length > VERIFIED_LINES;
  const verified = longVerified && !allVerified ? lines.slice(0, VERIFIED_LINES).join("\n") + "\n…" : m.verified;
  const commits = allCommits ? m.commits : m.commits.slice(0, COMMITS_SHOWN);

  return (
    <div className="approval merge-card">
      <div className="approval-head">
        <span className="approval-badge">병합 요청</span>
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
          <div className="muted small">지휘자가 검증 내용을 적지 않았어요</div>
        )}
      </div>

      {(m.projectDirty.length > 0 || m.workerUntracked > 0 || !m.conflictsChecked) && (
        <div className="merge-warn small">
          {m.projectDirty.length > 0 && <div>요청 시점에 프로젝트 폴더에 커밋하지 않은 변경 {m.projectDirty.length}개: 병합 전에 정리해야 해요</div>}
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

      {err && <div className="err small wrap">{err}</div>}
      {rejecting ? (
        <div className="approval-actions">
          <input
            className="merge-reason nokey"
            autoFocus
            value={reason}
            disabled={!!busy}
            placeholder="거부 사유 (선택, 지휘자에게 전해요)"
            onChange={(e) => setReason(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.nativeEvent.isComposing) answer(false);
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
          <button className="primary" disabled={!!busy} title="기준 브랜치에 --no-ff로 합쳐요. 프로젝트 폴더에 커밋하지 않은 변경이 없어야 해요" onClick={() => answer(true)}>
            {busy === "merge" ? "합치는 중…" : "병합"}
          </button>
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
