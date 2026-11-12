import { useEffect, useRef, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

// Auto update (docs/features/ui/auto-update.md): a while after start and then every few hours, ask
// releases/latest/download/latest.json whether a newer signed build exists; if so, offer it in a small card.
// Nothing downloads until the user presses "업데이트". Release builds only: dev builds have no installer to replace.
const FIRST_CHECK_MS = 10_000;
const EVERY_MS = 6 * 60 * 60 * 1000;

type State =
  | { kind: "idle" }
  | { kind: "available"; update: Update }
  | { kind: "downloading"; update: Update; got: number; total?: number }
  | { kind: "installing"; update: Update }
  | { kind: "failed"; update: Update; message: string };

export function UpdateNotice() {
  const [state, setState] = useState<State>({ kind: "idle" });
  // "나중에" hides that version until the next launch; a newer one found later shows again
  const dismissed = useRef<string | null>(null);

  useEffect(() => {
    if (import.meta.env.DEV) return;
    let stop = false;
    const run = () =>
      check()
        .then((u) => {
          if (stop || !u || u.version === dismissed.current) return;
          // don't replace a card the user is acting on
          setState((s) => (s.kind === "idle" || s.kind === "available" ? { kind: "available", update: u } : s));
        })
        // offline, GitHub down, no release yet: try again next round, say nothing
        .catch(() => {});
    const first = setTimeout(run, FIRST_CHECK_MS);
    const every = setInterval(run, EVERY_MS);
    return () => {
      stop = true;
      clearTimeout(first);
      clearInterval(every);
    };
  }, []);

  if (state.kind === "idle") return null;
  const { update } = state;

  const install = async () => {
    let got = 0;
    let total: number | undefined;
    setState({ kind: "downloading", update, got });
    try {
      await update.downloadAndInstall((e) => {
        if (e.event === "Started") total = e.data.contentLength;
        else if (e.event === "Progress") {
          got += e.data.chunkLength;
          setState({ kind: "downloading", update, got, total });
        } else if (e.event === "Finished") setState({ kind: "installing", update });
      });
      // Windows: the installer has already closed the app. macOS and Linux: start the new version.
      await relaunch();
    } catch (e) {
      setState({ kind: "failed", update, message: String(e) });
    }
  };

  const later = () => {
    dismissed.current = update.version;
    setState({ kind: "idle" });
  };

  const pct = state.kind === "downloading" && state.total ? Math.min(100, Math.round((state.got / state.total) * 100)) : null;

  return (
    <div className="update-card" role="status">
      <div className="update-title">새 버전 v{update.version}이 나왔어요</div>
      {state.kind === "available" && (
        <>
          <p className="update-text">업데이트하면 앱이 다시 시작되고, 실행 중인 에이전트는 멈춰요.</p>
          <div className="update-actions">
            <button className="link small muted" onClick={later}>나중에</button>
            <button className="primary" onClick={install}>업데이트</button>
          </div>
        </>
      )}
      {state.kind === "downloading" && (
        <>
          <p className="update-text">내려받는 중{pct === null ? "…" : ` ${pct}%`}</p>
          <div className="update-bar"><span style={{ width: `${pct ?? 0}%` }} /></div>
        </>
      )}
      {state.kind === "installing" && <p className="update-text">설치하는 중… 곧 다시 시작해요.</p>}
      {state.kind === "failed" && (
        <>
          <p className="update-text">업데이트하지 못했어요: {state.message}</p>
          <div className="update-actions">
            <button className="link small muted" onClick={later}>닫기</button>
            <button className="primary" onClick={install}>다시 시도</button>
          </div>
        </>
      )}
    </div>
  );
}
