import { useEffect, useState } from "react";
import { ipc } from "./ipc";
import { Spinner } from "./LogoMotion";
import { useStore } from "./store";
import type { MergeMode, Repo } from "./types";

/**
 * One project's merge mode (R6): 로컬 (the app merges with --no-ff in the project folder) or PR (the app pushes the
 * worker's branch and opens a pull request with the user's gh). Choosing PR checks the remote, gh and its login
 * first; when something is missing the reason shows here with "다시 확인", and the mode stays 로컬.
 */
export function MergeModeRow({ repo }: { repo: Repo }) {
  const maestro = useStore((s) => s.worktrees.find((w) => w.isMaestro && w.root === repo.root));
  const [mode, setMode] = useState<MergeMode | null>(maestro?.mergeMode ?? null);
  const [busy, setBusy] = useState(false);
  // the refusal and the mode that was asked for ("다시 확인" only re-asks a PR check)
  const [why, setWhy] = useState<{ text: string; mode: MergeMode } | null>(null);
  const base = maestro?.baseBranch ?? repo.defaultBranch;

  useEffect(() => {
    let live = true;
    ipc.projectMergeMode(repo.root).then((m) => live && setMode(m)).catch(() => {});
    return () => {
      live = false;
    };
  }, [repo.root, maestro?.mergeMode]);

  const choose = async (m: MergeMode, recheck = false) => {
    if (busy || (m === mode && !recheck)) return;
    setBusy(true);
    setWhy(null);
    try {
      // to PR, the backend checks remote, gh and login first and answers with the reason when one is missing
      await ipc.setMergeMode(repo.root, m);
      setMode(m);
      useStore.getState().refreshWorktrees(repo.root).catch(() => {});
    } catch (e) {
      setWhy({ text: String(e), mode: m });
    }
    setBusy(false);
  };

  return (
    <div className="set-row merge-mode-row">
      <div>
        <div>{repo.name} <span className="muted small">· 기준 {base}</span></div>
        <div className="muted small">로컬: 승인하면 프로젝트 폴더에서 --no-ff로 합쳐요. PR: 승인하면 앱이 브랜치를 푸시하고 gh로 PR을 만들어요. 바꾸면 기다리던 병합 요청은 거둬요.</div>
        {why && (
          <div className="err small wrap">
            {why.text}{" "}
            {why.mode === "pr" && <button className="link small" disabled={busy} onClick={() => choose("pr", true)}>다시 확인</button>}
          </div>
        )}
      </div>
      {busy ? (
        <span className="muted small"><Spinner /> 확인 중…</span>
      ) : (
        <div className="seg">
          {(["local", "pr"] as const).map((m) => (
            <button key={m} data-on={m === mode || undefined} onClick={() => choose(m)}>{m === "local" ? "로컬" : "PR"}</button>
          ))}
        </div>
      )}
    </div>
  );
}
