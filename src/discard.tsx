import { useState } from "react";
import { createPortal } from "react-dom";
import { ipc } from "./ipc";
import { Spinner } from "./LogoMotion";
import { ask } from "./ui/Confirm";
import { samePath, useStore, type Worktree } from "./store";
import type { DiscardCheck, DiscardChoice } from "./types";

/** How a PR was merged, in words: the user's click (병합 커밋, squash) or GitHub on its own (merge queue, auto-merge, the site). */
const mergeMethod = (m?: string) => ({ merge: "병합 커밋", squash: "squash", github: "GitHub에서 합침" } as Record<string, string>)[m ?? ""] ?? m ?? "";

/**
 * Discarding an agent (docs/features/worktrees/discard-worktree.md), one flow for every place that offers it: the
 * agent header's button and the sidebar row menu. `discard()` asks first, then removes it:
 * - the Maestro: "마에스트로 없애기", only its agent and worker links go (the project folder and branch stay);
 * - a worktree made outside Maestro: only the agent is taken off (the folder and branch stay);
 * - any other: what it would lose is checked first; nothing to lose asks once, work to lose opens the dialog with
 *   보관 / 버리기 / 취소 (`dialog`, to render where the hook is used).
 * `busy` says which step runs ("check": looking for work to lose, "remove": removing; both run several git commands).
 */
export function useDiscard(wt: Worktree | undefined) {
  // work that discarding would lose: the user picks 보관 / 버리기 / 취소
  const [losing, setLosing] = useState<DiscardCheck | null>(null);
  const [busy, setBusy] = useState<"check" | "remove" | null>(null);

  const finish = async (choice: DiscardChoice) => {
    if (!wt) return;
    setBusy("remove");
    try {
      await ipc.discardWorktree(wt.root, wt.path, wt.branch, choice);
      useStore.getState().remove(wt.path);
    } finally {
      setBusy(null);
    }
  };

  const discard = async () => {
    if (!wt || busy) return;
    setBusy("check");
    try {
      // the Maestro and outside worktrees only lose the agent: their folder and branch stay
      if (wt.isMaestro || wt.external) {
        // its workers' PRs stay (each on its own worker's block from then on); their notes go to nobody
        const prs = wt.isMaestro && useStore.getState().prs.some((p) => !p.hidden && samePath(p.maestroPath, wt.path));
        const question = wt.isMaestro
          ? {
              title: `마에스트로 없애기 · ${wt.name}`,
              body: `프로젝트 폴더와 브랜치는 그대로 두고, 에이전트와 워커 연결만 지워요.${prs ? "\n워커들의 PR 카드는 각 워커 블록으로 옮겨 가요. 그 PR의 CI·병합 소식은 더 이상 어느 마에스트로에게도 전하지 않아요." : ""}`,
              confirm: "마에스트로 없애기",
            }
          : {
              title: `에이전트 폐기 · ${wt.name}`,
              body: "폴더는 그대로 두고 에이전트만 떼어 내요. 앱 밖에서 만든 워크트리라 브랜치도 남아요.",
              confirm: "폐기",
            };
        setBusy(null);
        if (await ask({ ...question, danger: true })) await finish("clean");
        return;
      }
      const check = await ipc.checkDiscard(wt.root, wt.path, wt.branch);
      setBusy(null);
      if (check.changes.length || check.commits.length) return setLosing(check);
      const pr = check.mergedByPr;
      const body = check.keepBranch
        ? `워크트리 폴더를 지워요. 브랜치 '${wt.branch}'는 남겨요.\n커밋하지 않은 작업은 없어요. 대화 기록은 사라져요.`
        : pr && !pr.open
          ? `워크트리와 브랜치 '${wt.branch}'를 지워요.\n잃는 작업은 없어요: PR #${pr.number}으로 합쳐졌어요 (${mergeMethod(pr.method)}).${check.backup ? " 로컬 브랜치는 백업 ref를 남기고 지워요." : ""} 대화 기록은 사라져요.`
          : `워크트리와 브랜치 '${wt.branch}'를 지워요.\n잃는 작업은 없어요: 커밋은 모두 '${check.base}'에 들어가 있어요. 대화 기록은 사라져요.`;
      const remote = pr?.open ? `\n열린 PR #${pr.number}과 원격 브랜치는 남아요.` : "";
      if (await ask({ title: `에이전트 폐기 · ${wt.name}`, body: body + remote, confirm: "폐기", danger: true })) await finish("clean");
    } catch (e) {
      useStore.getState().setError(e);
    } finally {
      setBusy(null);
    }
  };

  const dialog = losing && wt ? createPortal(<DiscardDialog wt={wt} check={losing} onChoose={finish} onClose={() => setLosing(null)} />, document.body) : null;
  return { discard, busy, dialog };
}

/** Discarding would lose work: lists it and lets the user keep it on the branch, throw it away (a backup ref stays) or cancel. */
function DiscardDialog({ wt, check, onChoose, onClose }: {
  wt: Worktree;
  check: DiscardCheck;
  onChoose: (choice: DiscardChoice) => Promise<void>;
  onClose: () => void;
}) {
  const setError = useStore((s) => s.setError);
  const [busy, setBusy] = useState(false);
  const choose = async (choice: DiscardChoice) => {
    setBusy(true);
    try {
      await onChoose(choice);
    } catch (e) {
      setError(e);
    }
    setBusy(false);
    onClose();
  };
  const list = (items: string[]) => (
    <ul className="discard-list">
      {items.slice(0, 8).map((x) => <li key={x} className="ellipsis" title={x}>{x}</li>)}
      {items.length > 8 && <li className="muted">… 외 {items.length - 8}개</li>}
    </ul>
  );
  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && !busy && onClose()}>
      <div className="modal discard" onKeyDown={(e) => e.key === "Escape" && !busy && onClose()}>
        <h3>에이전트 폐기 · {wt.name}</h3>
        <p className="muted small">지우면 잃는 작업이 있어요</p>
        {check.changes.length > 0 && (
          <div>
            <div className="small">커밋하지 않은 파일 {check.changes.length}개</div>
            {list(check.changes)}
          </div>
        )}
        {check.commits.length > 0 && (
          <div>
            <div className="small">'{check.base}'에 없는 커밋 {check.commits.length}개</div>
            {list(check.commits)}
          </div>
        )}
        {check.mergedByPr && (
          <p className="muted small">
            {check.mergedByPr.open
              ? `열린 PR #${check.mergedByPr.number}과 원격 브랜치는 남아요 (앱은 원격 브랜치를 지우지 않아요).`
              : `PR #${check.mergedByPr.number}으로 합쳐졌어요 (${mergeMethod(check.mergedByPr.method)}). 그 PR의 머리까지 커밋은 합쳐진 것으로 봐요.`}
          </p>
        )}
        <p className="muted small">
          <b>보관</b>: 폴더와 에이전트만 지우고 브랜치 '{wt.branch}'는 남겨요. 커밋하지 않은 파일은 그 브랜치에 커밋으로 넣어요.
          <br />
          <b>버리기</b>: {check.keepBranch ? "폴더와 에이전트를 지워요(브랜치는 남아요)." : "브랜치까지 지워요."} 되돌릴 수 있게 <code>refs/maestro/trash/</code>에 백업을 남겨요.
          <br />
          .gitignore로 무시한 파일은 어느 쪽이든 지워져요.
        </p>
        <div className="modal-actions">
          <button disabled={busy} onClick={onClose}>취소</button>
          <button className="danger" disabled={busy} onClick={() => choose("discard")}>버리기</button>
          <button className="primary" disabled={busy} autoFocus onClick={() => choose("keep")}>{busy ? <Spinner /> : "보관"}</button>
        </div>
      </div>
    </div>
  );
}
