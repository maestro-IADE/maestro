import { useEffect, useState } from "react";
import { ipc } from "./ipc";
import { Spinner } from "./LogoMotion";
import { useStore } from "./store";
import type { PullRequest } from "./types";

/**
 * A failed CI job's log for an agent (PR mode, R6): the backend fetches and trims it (the job's own
 * `gh api …/actions/jobs/{id}/logs`, else `gh run view --log-failed`),
 * the user reads and edits it here, and only "보내기" sends it as a message to the worker (or the Maestro): it is
 * submitted at once, and an agent that is off is started first. Nothing is stored or sent by itself.
 */
export function PrLogDialog({ p, onClose }: { p: PullRequest; onClose: () => void }) {
  const setError = useStore((s) => s.setError);
  const [text, setText] = useState<string | null>(null);
  const [err, setErr] = useState("");
  const [to, setTo] = useState<"worker" | "maestro">("worker");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let live = true;
    ipc.prFailedLog(p.workerPath)
      .then((t) => live && setText(t))
      .catch((e) => live && setErr(String(e)));
    return () => {
      live = false;
    };
  }, [p.workerPath]);

  const send = async () => {
    if (!text?.trim()) return;
    setBusy(true);
    try {
      await ipc.send(p.root, to === "worker" ? p.workerPath : p.maestroPath, text, []);
      onClose();
    } catch (e) {
      setError(e);
    }
    setBusy(false);
  };

  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && !busy && onClose()}>
      <div className="modal pr-log" onKeyDown={(e) => e.key === "Escape" && !busy && onClose()}>
        <h3>CI 실패 로그 · PR #{p.number}</h3>
        <p className="muted small">실패한 작업의 로그를 줄여서 가져왔어요. 고친 뒤 보내기를 누르면 그 에이전트에게 메시지로 바로 보내요(꺼져 있으면 켜서 보내요).</p>
        {err ? (
          <div className="err small wrap">{err}</div>
        ) : text == null ? (
          <div className="muted small"><Spinner /> 로그를 받는 중…</div>
        ) : (
          <textarea className="pr-log-text nokey" value={text} disabled={busy} spellCheck={false} onChange={(e) => setText(e.target.value)} />
        )}
        <div className="modal-actions">
          <span className="seg" title="보낼 곳">
            <button data-on={to === "worker" || undefined} onClick={() => setTo("worker")}>워커 ({p.workerName})</button>
            <button data-on={to === "maestro" || undefined} disabled={!p.maestroPath} title={p.maestroPath ? undefined : "이 PR을 요청한 Lead가 없어요"} onClick={() => setTo("maestro")}>Lead</button>
          </span>
          <span className="spacer" />
          <button disabled={busy} onClick={onClose}>취소</button>
          <button className="primary" disabled={busy || !text?.trim()} onClick={send}>{busy ? <Spinner /> : "보내기"}</button>
        </div>
      </div>
    </div>
  );
}
