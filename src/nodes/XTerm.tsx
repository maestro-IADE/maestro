import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { b64ToBytes, ipc } from "../ipc";
import { useStore, useWorktree } from "../store";
import type { AgentRuntimePayload, PtyDataPayload, PtySnapshot } from "../types";

const css = (name: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();

/**
 * A backend PTY in xterm: the agent's real terminal (CLI view), or a settings setup terminal.
 * `start` makes sure the PTY exists (default: start/resume the worktree's agent); then scrollback, then live output.
 * `takeOver` (an agent with a chat runtime): while the chat runtime holds the conversation there is no terminal, and
 * a muted bar offers to move it here; while the backend waits for the chat's turn to end before moving it
 * ("agent://switch", the store's `switchPending`), the bar says so. A slow terminal start alone shows nothing.
 */
export function XTerm({ root = "", path, start, takeOver }: { root?: string; path: string; start?: () => Promise<unknown>; takeOver?: () => void }) {
  const host = useRef<HTMLDivElement>(null);
  const setError = useStore((s) => s.setError);
  // the chat runtime owns the conversation (`pty_snapshot.terminal` false, or "agent://runtime" said so)
  const [chatOwns, setChatOwns] = useState(false);
  // a switch to this view waits for the chat's turn to end (only the backend knows: it says so in "agent://switch")
  const waiting = useWorktree(path)?.switchPending === "cli";

  useEffect(() => {
    const term = new Terminal({
      fontFamily: "Cascadia Mono, Consolas, monospace",
      fontSize: 13,
      theme: { background: css("--mae-bg"), foreground: css("--mae-text"), cursor: css("--mae-text") },
      scrollback: 5000,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host.current!);
    fit.fit();

    // Snapshot first, then live batches; seq drops batches the snapshot already holds. A newer epoch is a new
    // process under this key (a restart): the terminal starts over from its snapshot.
    let seq = -1, epoch = -1, syncing = true;
    const queued: PtyDataPayload[] = [];
    let disposed = false;
    const write = (p: PtyDataPayload) => {
      if (syncing) return void queued.push(p);
      if (p.epoch < epoch) return;
      if (p.epoch > epoch) {
        queued.push(p);
        return void resync(p.epoch).catch(setError);
      }
      if (p.seq <= seq) return;
      term.write(b64ToBytes(p.dataB64));
      seq = p.seq;
    };
    // the new process may not be registered yet when its first batch arrives: ask again shortly
    const snapshot = (min: number, tries = 0): Promise<PtySnapshot> =>
      ipc.ptySnapshot(path).then((s) => (s.epoch >= min || tries >= 20 ? s : new Promise((r) => setTimeout(r, 100)).then(() => snapshot(min, tries + 1))));
    const resync = (min: number) => {
      syncing = true;
      term.reset();
      return snapshot(min).then((s) => {
        if (disposed) return;
        setChatOwns(s.terminal === false);
        if (s.epoch >= min) term.write(b64ToBytes(s.dataB64));
        epoch = Math.max(s.epoch, min);
        seq = s.epoch >= min ? s.seq : -1;
        syncing = false;
        queued.splice(0).forEach(write);
        // our size may differ from the PTY's; resizing also makes the TUI redraw cleanly
        ipc.ptyResize(path, term.cols, term.rows).catch(() => {});
      });
    };
    const same = (p: string) => p.toLowerCase() === path.toLowerCase();
    const unlisten = listen<PtyDataPayload>("pty://data", (e) => {
      if (!same(e.payload.worktreePath)) return;
      write(e.payload);
    });
    // the conversation moved: to the chat runtime (no terminal), or to a new terminal (its output resyncs by epoch)
    const unRuntime = takeOver
      ? listen<AgentRuntimePayload>("agent://runtime", (e) => same(e.payload.worktreePath) && setChatOwns(e.payload.runtime === "acp"))
      : null;
    unlisten
      .then(() => (start ? start() : ipc.ensureAgent(root, path)))
      .then(() => resync(-1))
      .then(() => !disposed && term.focus())
      .catch(setError);

    // xterm auto-answers cursor-position queries; the backend already answers them (agent.rs), and a second
    // reply would land in the TUI's input as text
    const input = term.onData((d) => {
      if (/^\x1b\[\d+;\d+R$/.test(d)) return;
      ipc.ptyWrite(path, d).catch(() => {});
    });

    // Only real size changes reach here: canvas zoom is a CSS transform and doesn't resize the box.
    let timer: number | undefined;
    const ro = new ResizeObserver(() => {
      clearTimeout(timer);
      timer = window.setTimeout(() => {
        fit.fit();
        ipc.ptyResize(path, term.cols, term.rows).catch(() => {});
      }, 150);
    });
    ro.observe(host.current!);

    return () => {
      disposed = true;
      clearTimeout(timer);
      ro.disconnect();
      input.dispose();
      unlisten.then((f) => f());
      unRuntime?.then((f) => f());
      term.dispose();
    };
  }, [root, path]);

  const bar = takeOver && (waiting || chatOwns);
  return (
    <>
      {bar && (
        <div className="xterm-bar small muted">
          {waiting ? (
            <>
              채팅의 턴이 끝나면 CLI로 전환돼요 ·{" "}
              <button className="link small" onClick={() => ipc.stop(path).catch(setError)}>지금 중단하고 전환</button>
            </>
          ) : (
            <>
              지금은 채팅이 대화를 쓰고 있어요 ·{" "}
              <button className="link small" onClick={takeOver}>CLI로 전환</button>
            </>
          )}
        </div>
      )}
      <div ref={host} className="xterm-host nodrag nowheel nopan" />
    </>
  );
}
