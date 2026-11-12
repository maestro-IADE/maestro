import { useEffect, useRef, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { ArrowDownToLine } from "./icons";

// Auto update (docs/features/ui/auto-update.md): a while after start and then every few hours, ask
// releases/latest/download/latest.json whether a newer signed build exists; if so, a button appears in the top bar
// with a small popover under it. Nothing downloads until the user presses "지금 업데이트". Release builds only: dev
// builds have no installer to replace. Main window only: the other windows are the same app.
const FIRST_CHECK_MS = 10_000;
const EVERY_MS = 6 * 60 * 60 * 1000;
// Settings → 정보 "업데이트 확인" hands a found update to the button through this window event
const OFFER = "maestro:update-offer";
// the last version whose popover opened by itself: each version opens it once, then only the button shows
const SEEN_KEY = "maestro.updateSeen";

/** Offer `u` now and open its popover (the user asked, in settings). */
export function offerUpdate(u: Update) {
  window.dispatchEvent(new CustomEvent<Update>(OFFER, { detail: u }));
}

type State =
  | { kind: "idle" }
  | { kind: "available"; update: Update }
  | { kind: "downloading"; update: Update; got: number; total?: number }
  | { kind: "installing"; update: Update }
  | { kind: "failed"; update: Update; message: string };

const seen = () => {
  try {
    return localStorage.getItem(SEEN_KEY);
  } catch {
    return null;
  }
};
const markSeen = (version: string) => {
  try {
    localStorage.setItem(SEEN_KEY, version);
  } catch {}
};

// the release notes, when they are words: the release workflow writes the release page's address there, which
// this popover cannot open (no opener plugin), so a bare address is left out
const notesOf = (u: Update) => {
  const body = u.body?.trim();
  return body && !/^https?:\/\/\S+$/.test(body) ? body : null;
};

/** The top bar's update button (only while a newer version exists) and its 288px popover. */
export function UpdateNotice() {
  const [state, setState] = useState<State>({ kind: "idle" });
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (import.meta.env.DEV) return;
    let stop = false;
    const found = (u: Update, show: boolean) => {
      // don't replace an update the user is installing
      setState((s) => (s.kind === "idle" || s.kind === "available" || s.kind === "failed" ? { kind: "available", update: u } : s));
      if (show || seen() !== u.version) {
        markSeen(u.version);
        setOpen(true);
      }
    };
    const run = () =>
      check()
        .then((u) => !stop && u && found(u, false))
        // offline, GitHub down, no release yet: try again next round, say nothing
        .catch(() => {});
    const offered = (e: Event) => found((e as CustomEvent<Update>).detail, true);
    window.addEventListener(OFFER, offered);
    const first = setTimeout(run, FIRST_CHECK_MS);
    const every = setInterval(run, EVERY_MS);
    return () => {
      stop = true;
      window.removeEventListener(OFFER, offered);
      clearTimeout(first);
      clearInterval(every);
    };
  }, []);

  // a press outside the button and its popover, or Esc, closes the popover
  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => !box.current?.contains(e.target as Node) && setOpen(false);
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    window.addEventListener("mousedown", away);
    window.addEventListener("keydown", esc);
    return () => {
      window.removeEventListener("mousedown", away);
      window.removeEventListener("keydown", esc);
    };
  }, [open]);

  if (state.kind === "idle") return null;
  const { update } = state;
  const notes = notesOf(update);

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
      setOpen(true);
    }
  };

  const pct = state.kind === "downloading" && state.total ? Math.min(100, Math.round((state.got / state.total) * 100)) : null;
  const label = state.kind === "downloading" ? `업데이트${pct === null ? "" : ` ${pct}%`}` : "업데이트";

  return (
    <div className="tb-update" ref={box}>
      <button className="tb-text" title={`새 버전 v${update.version}`} aria-expanded={open} onClick={() => setOpen(!open)}>
        <ArrowDownToLine size={14} />
        {label}
      </button>
      {open && (
        <div className="tb-popover update-pop" role="dialog" aria-label="업데이트">
          <div className="update-title">새 버전 v{update.version}이 나왔어요</div>
          {notes && <p className="update-notes">{notes}</p>}
          {state.kind === "available" && (
            <>
              <p className="update-text">업데이트하면 앱이 다시 시작되고 실행 중인 에이전트는 멈춰요</p>
              <div className="update-actions">
                <button onClick={() => setOpen(false)}>나중에</button>
                <button className="primary" onClick={install}>지금 업데이트</button>
              </div>
            </>
          )}
          {state.kind === "downloading" && (
            <>
              <p className="update-text">내려받는 중{pct === null ? "…" : ` ${pct}%`}</p>
              <div className="update-bar"><span style={{ transform: `scaleX(${(pct ?? 0) / 100})` }} /></div>
            </>
          )}
          {state.kind === "installing" && <p className="update-text">설치하고 곧 다시 시작해요</p>}
          {state.kind === "failed" && (
            <>
              <p className="update-text">업데이트하지 못했어요 · {state.message}</p>
              <div className="update-actions">
                <button onClick={() => setOpen(false)}>닫기</button>
                <button className="primary" onClick={install}>다시 시도</button>
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}
