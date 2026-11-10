import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { b64ToBytes, ipc } from "../ipc";
import { useStore } from "../store";
import type { PtyDataPayload } from "../types";

const css = (name: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();

/**
 * A backend PTY in xterm: the agent's real terminal (CLI view), or a settings setup terminal.
 * `start` makes sure the PTY exists (default: start/resume the worktree's agent); then scrollback, then live output.
 */
export function XTerm({ root = "", path, start }: { root?: string; path: string; start?: () => Promise<unknown> }) {
  const host = useRef<HTMLDivElement>(null);
  const setError = useStore((s) => s.setError);

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
    const snapshot = (min: number, tries = 0): Promise<{ dataB64: string; seq: number; epoch: number }> =>
      ipc.ptySnapshot(path).then((s) => (s.epoch >= min || tries >= 20 ? s : new Promise((r) => setTimeout(r, 100)).then(() => snapshot(min, tries + 1))));
    const resync = (min: number) => {
      syncing = true;
      term.reset();
      return snapshot(min).then((s) => {
        if (disposed) return;
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
      term.dispose();
    };
  }, [root, path]);

  return <div ref={host} className="xterm-host nodrag nowheel nopan" />;
}
