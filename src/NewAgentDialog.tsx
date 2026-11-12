import { useEffect, useState } from "react";
import { ipc } from "./ipc";
import { useSettings } from "./settings";
import { useStore, type NewAgentTarget } from "./store";
import { MaestroIcon } from "./icons";
import { AGENTS, AgentLogo, BranchSelect, PERMISSIONS, STATUS_LABEL, agentLabel, agentUsable } from "./agents";
import type { Branch, Repo } from "./types";

/**
 * New agent: pick an agent; Maestro makes its worktree and starts it. Opened through the store (`openNewAgent`), drawn
 * by the window's `NewAgentHost` (Workspace.tsx); the entry point decides the mode.
 */
export function NewAgentDialog({ repo, mode, parentPath, onClose }: {
  repo: Repo;
  /** "maestro": the project's Maestro (coordinator on the project folder) instead of a worktree */
  mode: NewAgentTarget["mode"];
  /** worker mode: make the new worktree a worker of this one */
  parentPath?: string;
  onClose: () => void;
}) {
  const maestro = mode === "maestro";
  const count = useStore((s) => s.worktrees.filter((w) => w.root === repo.root).length);
  const { upsert, cli } = useStore();
  const { defaultAgent, defaultPermission, disabledAgents, agentModels } = useSettings();
  const usable = (id: string) => agentUsable(id, cli, disabledAgents);
  // the default ("auto" = first usable), unless it's been switched off or uninstalled since
  const [agent, setAgent] = useState(() => (usable(defaultAgent) ? defaultAgent : (AGENTS.find((a) => usable(a.id))?.id ?? AGENTS[0]?.id ?? "")));
  const agentMissing = !!cli && !usable(agent); // cli null = still checking
  const [perm, setPerm] = useState(defaultPermission);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [branch, setBranch] = useState(`agent-${count + 1}`);
  // workers branch off the Maestro's base branch unless picked otherwise
  const maestroBase = useStore((s) => s.worktrees.find((w) => w.root === repo.root && w.isMaestro)?.baseBranch);
  const [base, setBase] = useState(maestroBase ?? repo.defaultBranch);
  const [existing, setExisting] = useState(""); // "" = make a new branch
  const [branches, setBranches] = useState<Branch[] | null>(null);
  useEffect(() => void ipc.listBranches(repo.root).then(setBranches).catch(() => setBranches([])), [repo.root]);
  const [more, setMore] = useState(false);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setErr("");
    try {
      const model = agentModels[agent] ?? "";
      if (maestro) {
        upsert(repo.root, await ipc.createMaestro({ projectRoot: repo.root, adapterId: agent, permissionMode: perm, model, baseBranch: base }));
        return onClose();
      }
      const info = await ipc.createAgent({
        projectRoot: repo.root,
        adapterId: agent,
        permissionMode: perm,
        branchName: existing || branch.trim(),
        baseBranch: base.trim() || "HEAD",
        existingBranch: !!existing,
        name,
        description,
        model,
      });
      upsert(repo.root, info); // the project canvas adds its card and pans to it; double-click to go in
      if (parentPath) {
        // made by dragging a line out of a card: it's that card's worker
        await ipc.linkWorktrees(repo.root, parentPath, info.path);
        await useStore.getState().refreshWorktrees(repo.root);
      }
      onClose();
    } catch (e) {
      setErr(String(e));
      setBusy(false);
    }
  };

  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && !busy && onClose()}>
      <form className="modal" onSubmit={submit} onKeyDown={(e) => e.key === "Escape" && !busy && onClose()}>
        <h3 className="modal-title">{maestro ? <><MaestroIcon />마에스트로 · {repo.name}</> : parentPath ? `새 워커 · ${repo.name}` : `새 워크트리 · ${repo.name}`}</h3>
        {maestro ? (
          <p className="muted small">
            프로젝트 폴더(지금 체크아웃된 브랜치)에서 돌아가는 조율 에이전트예요. 일을 나눠 워커 워크트리에 맡기고, 결과를 기준 브랜치에 모아 합쳐요.
            원본 폴더에서 직접 실행되니 권한을 신중히 골라 주세요.
          </p>
        ) : (
          <>
            <label>
              이름
              <input autoFocus value={name} placeholder={`비우면 브랜치 이름(${existing || branch})`} onChange={(e) => setName(e.target.value)} />
            </label>
            <label>
              설명
              <textarea rows={2} value={description} placeholder="이 워크트리에서 할 일 (선택)" onChange={(e) => setDescription(e.target.value)} />
            </label>
          </>
        )}
        <label>
          에이전트
          <span className="agent-select">
            <AgentLogo id={agent} size={18} />
            <select value={agent} onChange={(e) => setAgent(e.target.value)}>
              {AGENTS.filter((a) => !disabledAgents.includes(a.id)).map((a) => {
                const st = cli?.agents.find((c) => c.id === a.id)?.status;
                return (
                  <option key={a.id} value={a.id} disabled={!usable(a.id)}>
                    {a.name}{!a.runtimes.length ? " (준비 중)" : st && st !== "ready" ? ` · ${STATUS_LABEL[st]}` : ""}
                  </option>
                );
              })}
            </select>
          </span>
        </label>
        <label>
          권한
          <select value={perm} onChange={(e) => setPerm(e.target.value)}>
            {PERMISSIONS.map((p) => (
              <option key={p.id} value={p.id}>{p.label}</option>
            ))}
          </select>
          <span className="muted small">{PERMISSIONS.find((p) => p.id === perm)?.hint}</span>
        </label>
        {maestro ? (
          <label>
            워커 기준 브랜치
            <BranchSelect branches={branches} value={base} onChange={setBase} />
            <span className="muted small">새 워커가 여기서 갈라지고, 끝난 작업은 여기에 합쳐요. 워커마다 만들 때 다른 기준을 고를 수도 있어요.</span>
          </label>
        ) : more ? (
          <>
            <div className="seg branch-mode">
              <button type="button" data-on={!existing || undefined} onClick={() => setExisting("")}>새 브랜치</button>
              <button
                type="button"
                data-on={!!existing || undefined}
                disabled={!branches?.some((b) => !b.inUse)}
                onClick={() => setExisting(branches?.find((b) => !b.inUse)?.name ?? "")}
              >
                기존 브랜치
              </button>
            </div>
            {existing ? (
              <label>
                체크아웃할 브랜치
                <BranchSelect branches={branches} value={existing} onChange={setExisting} pickable />
                <span className="muted small">이 브랜치에서 이어서 작업해요. 워크트리를 폐기해도 브랜치는 남아요.</span>
              </label>
            ) : (
              <>
                <label>
                  새 브랜치 이름
                  <input value={branch} onChange={(e) => setBranch(e.target.value)} />
                </label>
                <label>
                  기준 브랜치
                  <BranchSelect branches={branches} value={base} onChange={setBase} />
                </label>
              </>
            )}
          </>
        ) : (
          <button type="button" className="link muted small" onClick={() => setMore(true)}>
            새 브랜치 {branch} · 기준 {base} — 변경
          </button>
        )}
        {agentMissing && (
          <p className="err small">
            {cli?.agents.find((c) => c.id === agent)?.message || "쓸 수 있는 에이전트가 아니에요"} 설정 → 에이전트에서 설치·업데이트하거나 켜 주세요.
          </p>
        )}
        {!agentMissing && cli?.agents.find((c) => c.id === agent)?.status === "loginRequired" && (
          <p className="muted small">{agentLabel(agent)}에 로그인돼 있지 않은 것 같아요. 시작이 안 되면 설정 → 에이전트에서 로그인해 주세요.</p>
        )}
        {err && <p className="err wrap">{err}</p>}
        <div className="modal-actions">
          <button type="button" disabled={busy} onClick={onClose}>취소</button>
          <button type="submit" className="primary" disabled={busy || !(existing || branch.trim()) || agentMissing}>{busy ? "시작하는 중…" : "만들고 실행"}</button>
        </div>
      </form>
    </div>
  );
}
