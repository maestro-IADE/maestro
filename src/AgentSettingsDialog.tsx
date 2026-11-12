import { useEffect, useRef, useState } from "react";
import { ipc } from "./ipc";
import { isImeComposing } from "./ime";
import { AgentLogo, BranchSelect, agentLabel, permissionOf, permissionsFor, useModels } from "./agents";
import { copyText } from "./agentActions";
import { Copy, Settings2 } from "./icons";
import { samePath, useStore, useWorktree, type AgentSettingsTarget, type Worktree } from "./store";
import { agentPatch, type AgentFields } from "./agentForm";
import type { Branch } from "./types";

/**
 * The agent settings dialog (docs/features/agents/agent-settings.md): an agent made before gets a new name,
 * description, permission mode, model and (the Maestro) workers' base branch. Opened through the store
 * (`openAgentSettings`: a header chip or ⋯, the sidebar row's menu or F2, a tab's right click), drawn by the
 * window's `AgentSettingsHost` (Workspace.tsx).
 *
 * Saving writes the agent's meta at once (`update_agent`). A running agent keeps the mode and model it started with
 * until it restarts: while it rests (IDLE) "저장하고 다시 시작" saves and restarts it on the same conversation
 * (`restart_agent`); otherwise the dialog says the change waits for the next start.
 */
export function AgentSettingsDialog({ target, onClose }: { target: AgentSettingsTarget; onClose: () => void }) {
  const wt = useWorktree(target.path) ?? useStore.getState().worktrees.find((w) => samePath(w.path, target.path));
  // the agent went away (discarded elsewhere): nothing left to set
  useEffect(() => {
    if (!wt) onClose();
  }, [!wt]);
  if (!wt) return null;
  return <Form wt={wt} focus={target.focus} onClose={onClose} />;
}

type Phase = { kind: "edit" } | { kind: "saved" } | { kind: "restarted" };

function Form({ wt, focus, onClose }: { wt: Worktree; focus: AgentSettingsTarget["focus"]; onClose: () => void }) {
  const repo = useStore((s) => s.repos.find((r) => r.root === wt.root));
  const parent = useStore((s) => s.worktrees.find((w) => w.root === wt.root && w.workers.some((p) => samePath(p, wt.path))));
  const upsert = useStore((s) => s.upsert);
  const maestro = wt.isMaestro;
  // what the agent has now (Worktree.name falls back to the branch: an unset name shows empty, the branch as hint)
  const start = useRef<AgentFields>({
    name: wt.name === wt.branch ? "" : wt.name,
    description: wt.description ?? "",
    permissionMode: wt.permissionMode ?? "",
    model: wt.model ?? "",
    baseBranch: wt.baseBranch ?? repo?.defaultBranch ?? "",
  }).current;
  const [name, setName] = useState(start.name);
  const [description, setDescription] = useState(start.description);
  const [perm, setPerm] = useState(start.permissionMode);
  const [model, setModel] = useState(start.model);
  const [base, setBase] = useState(start.baseBranch);
  const [branches, setBranches] = useState<Branch[] | null>(null);
  useEffect(() => {
    if (maestro) ipc.listBranches(wt.root).then(setBranches).catch(() => setBranches([]));
  }, [maestro, wt.root]);
  const models = useModels(wt.adapterId ?? "");
  const perms = permissionsFor(wt.adapterId);
  const [busy, setBusy] = useState<"" | "save" | "restart">("");
  const [err, setErr] = useState("");
  const [phase, setPhase] = useState<Phase>({ kind: "edit" });

  // first focus: the field the entry point named (F2 → the name, a chip → its own), else the name or description
  const nameRef = useRef<HTMLInputElement>(null);
  const descRef = useRef<HTMLTextAreaElement>(null);
  const permRef = useRef<HTMLSelectElement>(null);
  const modelRef = useRef<HTMLSelectElement>(null);
  useEffect(() => {
    const el = focus === "permission" ? permRef.current : focus === "model" ? modelRef.current : (nameRef.current ?? descRef.current);
    el?.focus();
    if (el instanceof HTMLInputElement) el.select();
  }, []);

  // only what changed is sent: the backend checks only that (an agent made before a rule keeps working)
  const { patch, changed, restart: needsRestart } = agentPatch(start, { name, description, permissionMode: perm, model, baseBranch: base }, maestro);
  const idle = wt.status === "IDLE";
  const working = wt.status === "RUNNING";
  const restartable = idle && !!wt.adapterId;

  // "다시 시작했어요" stays a moment; the dialog then closes, unless another agent's took its place meanwhile
  const closeSoon = () => setTimeout(() => samePath(useStore.getState().agentSettings?.path ?? "", wt.path) && onClose(), 900);
  const save = async (restart: boolean) => {
    if (busy || !changed) return;
    setErr("");
    setBusy(restart ? "restart" : "save");
    let saved = false;
    try {
      const r = await ipc.updateAgent({ projectRoot: wt.root, worktreePath: wt.path, ...patch });
      saved = true;
      upsert(wt.root, r.info);
      // nothing waits for a restart (or it's off: the next start takes it): done
      if (r.applied === "saved") return onClose();
      if (!restart) {
        setBusy("");
        return setPhase({ kind: "saved" });
      }
      await ipc.restartAgent(wt.root, wt.path);
      setPhase({ kind: "restarted" });
      closeSoon();
    } catch (e) {
      setErr(String(e));
      setBusy("");
      // saved, but the restart was refused (it started working meanwhile…): what's left is the restart alone
      if (saved) setPhase({ kind: "saved" });
    }
  };
  // after "저장했어요": the restart alone (what's saved is already on disk)
  const restartNow = async () => {
    if (busy) return;
    setErr("");
    setBusy("restart");
    try {
      await ipc.restartAgent(wt.root, wt.path);
      setPhase({ kind: "restarted" });
      closeSoon();
    } catch (e) {
      setErr(String(e));
      setBusy("");
    }
  };

  const primary = phase.kind === "saved" ? null : restartable && needsRestart ? "restart" : "save";
  const onKey = (e: React.KeyboardEvent) => {
    if (isImeComposing(e)) return;
    if (e.key === "Escape" && !busy) {
      e.stopPropagation();
      return onClose();
    }
    if (e.key !== "Enter") return;
    // Ctrl+Enter: the main button; a plain Enter in a one-line field doesn't send the form
    if (e.ctrlKey || e.metaKey) {
      e.preventDefault();
      if (phase.kind === "saved") return void (restartable && restartNow());
      if (primary) void save(primary === "restart");
    } else if (e.target instanceof HTMLInputElement) e.preventDefault();
  };

  // saving, or saved: the fields show what was saved and no longer change
  const locked = !!busy || phase.kind !== "edit";
  const permNow = perms.find((p) => p.id === perm) ?? (perm ? permissionOf(perm) : null);
  const title = maestro ? "마에스트로 설정" : `${wt.name} 설정`;
  const done = phase.kind === "restarted";

  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && !busy && onClose()}>
      <form className="modal form-dialog agent-settings" onSubmit={(e) => e.preventDefault()} onKeyDown={onKey} aria-label={title}>
        <h3 className="modal-title">
          <Settings2 size={16} />
          <span className="ellipsis">{title}</span>
        </h3>
        {!maestro && (
          <label className="field">
            <span className="field-label">이름 <span className="field-opt">선택</span></span>
            <input ref={nameRef} value={name} maxLength={60} placeholder={`비우면 브랜치 이름(${wt.branch})`} disabled={locked} onChange={(e) => setName(e.target.value)} />
          </label>
        )}
        <label className="field">
          <span className="field-label">설명 <span className="field-opt">선택</span></span>
          <textarea ref={descRef} rows={2} value={description} placeholder="이 에이전트가 맡은 일" disabled={locked} onChange={(e) => setDescription(e.target.value)} />
        </label>
        <div className="field-row">
          <label className="field">
            <span className="field-label">권한</span>
            <select ref={permRef} value={perm} disabled={locked} onChange={(e) => setPerm(e.target.value)}>
              {!perms.some((p) => p.id === perm) && <option value={perm}>{perm ? permissionOf(perm).label : "알 수 없음"}</option>}
              {perms.map((p) => <option key={p.id} value={p.id}>{p.label}</option>)}
            </select>
          </label>
          <label className="field">
            <span className="field-label">모델</span>
            <select ref={modelRef} value={model} disabled={!models || locked} onChange={(e) => setModel(e.target.value)}>
              <option value="">기본 (CLI 기본값)</option>
              {model && !models?.some(([v]) => v === model) && <option value={model}>{model}</option>}
              {models?.map(([v, label]) => <option key={v} value={v}>{label}</option>)}
            </select>
          </label>
        </div>
        {permNow?.hint && <p className="field-hint" data-warn={permNow.warn || undefined}>{permNow.hint}</p>}
        {maestro && (
          <label className="field">
            <span className="field-label">워커 기준 브랜치</span>
            <BranchSelect branches={branches} value={base} onChange={setBase} disabled={locked} />
            <span className="field-hint">새 워커가 여기서 갈라지고, 끝난 작업은 여기에 합쳐요</span>
          </label>
        )}
        <dl className="facts">
          <dt>에이전트 종류</dt>
          <dd><AgentLogo id={wt.adapterId} size={12} />{agentLabel(wt.adapterId)}</dd>
          <dt>브랜치</dt>
          <dd className="mono ellipsis" title={wt.branch}>{wt.branch}</dd>
          <dt>폴더</dt>
          <dd className="facts-path">
            <span className="mono ellipsis" title={wt.path}>{wt.path}</span>
            <button type="button" className="icon" title="폴더 경로 복사" aria-label="폴더 경로 복사" onClick={() => copyText(wt.path)}>
              <Copy size={12} />
            </button>
          </dd>
          {parent && (
            <>
              <dt>상위</dt>
              <dd className="ellipsis">{parent.name}</dd>
            </>
          )}
        </dl>
        <p className="field-hint">
          {maestro && patch.permissionMode !== undefined
            ? "이 뒤에 만드는 워커가 새 권한을 물려받아요. 이미 있는 워커는 그대로예요"
            : "권한과 모델은 에이전트를 다시 시작하면 적용돼요"}
        </p>
        {err && <div className="dialog-error" role="alert"><span className="wrap">{err}</span></div>}
        <div className="dialog-foot">
          {phase.kind === "saved" ? (
            <span className="dialog-status">저장했어요. 다시 시작하면 적용돼요</span>
          ) : done ? (
            <span className="dialog-status">다시 시작했어요</span>
          ) : working && needsRestart ? (
            <span className="dialog-status muted">작업이 끝나면 다시 시작할 수 있어요</span>
          ) : (
            <span className="spacer" />
          )}
          {phase.kind === "saved" ? (
            <>
              <button type="button" autoFocus={!restartable} disabled={!!busy} onClick={onClose}>닫기</button>
              {restartable && (
                <button type="button" className="primary" autoFocus disabled={!!busy} onClick={() => void restartNow()}>
                  {busy === "restart" ? "다시 시작하는 중…" : "지금 다시 시작"}
                </button>
              )}
            </>
          ) : (
            <>
              <button type="button" disabled={!!busy || done} onClick={onClose}>취소</button>
              <button type="button" className={primary === "save" ? "primary" : undefined} disabled={!!busy || done || !changed} onClick={() => void save(false)}>
                {busy === "save" ? "저장하는 중…" : "저장"}
                {primary === "save" && <kbd className="keycap">Ctrl ↵</kbd>}
              </button>
              {primary === "restart" && (
                <button type="button" className="primary" disabled={!!busy || done} onClick={() => void save(true)}>
                  {busy === "restart" ? "다시 시작하는 중…" : "저장하고 다시 시작"}
                  <kbd className="keycap">Ctrl ↵</kbd>
                </button>
              )}
            </>
          )}
        </div>
      </form>
    </div>
  );
}
