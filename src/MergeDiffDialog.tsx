import { useEffect, useState } from "react";
import { DiffEditor } from "@monaco-editor/react";
import { languageOf, OPTIONS } from "./monaco";
import { ipc } from "./ipc";
import { X } from "./icons";
import { isLightTheme, useSettings } from "./settings";
import type { MergeFileDiff, MergeRequest } from "./types";

const sha7 = (s: string) => s.slice(0, 7);

/**
 * A merge request's changes, file by file, read-only: the recorded merge base against the recorded tip (what the
 * approval merges), not the worker's files as they are now.
 */
export default function MergeDiffDialog({ m, path, onClose }: { m: MergeRequest; path: string; onClose: () => void }) {
  const theme = useSettings((s) => (isLightTheme(s.theme) ? "maestro-light" : "maestro-dark"));
  const [at, setAt] = useState(path);
  const [diff, setDiff] = useState<MergeFileDiff | null>(null);
  const [err, setErr] = useState("");
  useEffect(() => {
    let live = true;
    setDiff(null);
    setErr("");
    ipc.mergeFile(m.id, at)
      .then((d) => live && setDiff(d))
      .catch((e) => live && setErr(String(e)));
    return () => {
      live = false;
    };
  }, [m.id, at]);
  const f = m.files.find((x) => x.path === at);

  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal merge-diff-dialog nokey" tabIndex={-1} onKeyDown={(e) => e.key === "Escape" && onClose()}>
        <div className="merge-diff-head">
          <h3 className="ellipsis">변경 보기 · {m.workerName}</h3>
          <span className="muted small ellipsis" title={`${m.mergeBase}..${m.tipSha}`}>
            {m.branch} → {m.base} · {sha7(m.mergeBase)}..{sha7(m.tipSha)} (요청 시점)
          </span>
          <span className="spacer" />
          <button className="icon" title="닫기" aria-label="닫기" onClick={onClose}><X /></button>
        </div>
        <ul className="merge-diff-files nowheel">
          {m.files.map((x) => (
            <li key={x.path}>
              <button className="merge-file" data-on={x.path === at || undefined} title={x.oldPath ? `${x.oldPath} → ${x.path}` : x.path} onClick={() => setAt(x.path)}>
                <span className="code">{x.code}</span>
                <span className="ellipsis">{x.path}</span>
              </button>
            </li>
          ))}
        </ul>
        <div className="merge-diff-body">
          {f?.oldPath && <div className="muted small merge-diff-note">이름 바뀜: {f.oldPath} → {f.path}</div>}
          {err ? (
            <div className="err small wrap">{err}</div>
          ) : !diff ? (
            <div className="muted small merge-diff-note">불러오는 중…</div>
          ) : diff.binary ? (
            <div className="muted small merge-diff-note">바이너리 파일이라 비교를 보여주지 않아요</div>
          ) : diff.tooLarge ? (
            <div className="muted small merge-diff-note">파일이 커서(2MB 초과) 비교를 보여주지 않아요</div>
          ) : (
            <DiffEditor
              original={diff.original ?? ""}
              modified={diff.modified ?? ""}
              language={languageOf(at)}
              originalModelPath={`inmemory://merge/${m.id}/base/${at}`}
              modifiedModelPath={`inmemory://merge/${m.id}/tip/${at}`}
              theme={theme}
              wrapperProps={{ className: "merge-diff-editor" }}
              options={{ ...OPTIONS, readOnly: true, originalEditable: false }}
            />
          )}
        </div>
      </div>
    </div>
  );
}
