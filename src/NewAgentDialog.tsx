import { useEffect, useState } from "react";
import { ipc } from "./ipc";
import { isImeComposing } from "./ime";
import { useBodyKeys } from "./ui/useBodyKeys";
import { useSettings } from "./settings";
import { useStore, type NewAgentTarget } from "./store";
import { ChevronDown, GitBranch, MaestroIcon, Plus } from "./icons";
import { AGENTS, AgentLogo, BranchSelect, STATUS_LABEL, agentBlocker, agentLabel, defaultAgentOf, permissionFor, permissionOf, permissionsFor, useModels } from "./agents";
import { branchTaken, maestroBaseArg, nameFromTask, nextAgentBranch } from "./agentForm";
import type { Branch, MergeMode, Repo } from "./types";

/**
 * The new agent dialog (docs/features/worktrees/create-worktree.md): one dialog, three modes picked by where it
 * opens: an agent of the project, a worker of `parentPath`, or the project's Maestro. Opened through the store
 * (`openNewAgent`), drawn by the window's `NewAgentHost` (Workspace.tsx).
 *
 * Fields: 할 일 (optional, focused first) → agent · permission · model on one row → 고급 folded (name,
 * description, branch). The 할 일 is never sent: once made, the agent opens in a lasting tab and the 할 일 waits in
 * its chat composer (`seedComposer`), so one Enter sends it. Ctrl+Enter makes it.
 */
export function NewAgentDialog({ repo, mode, parentPath, onClose }: {
  repo: Repo;
  mode: NewAgentTarget["mode"];
  /** worker mode: the new agent becomes a worker of this one */
  parentPath?: string;
  onClose: () => void;
}) {
  const maestro = mode === "maestro";
  const count = useStore((s) => s.worktrees.filter((w) => w.root === repo.root && !w.isMaestro).length);
  const parentName = useStore((s) => (parentPath ? s.worktrees.find((w) => w.path === parentPath)?.name : undefined));
  const foundMaestro = useStore((s) => (maestro ? s.worktrees.find((w) => w.root === repo.root && w.isMaestro) : undefined));
  const cli = useStore((s) => s.cli);
  const { defaultAgent, defaultPermission, disabledAgents, agentModels } = useSettings();

  const [task, setTask] = useState("");
  const [agent, setAgentState] = useState(() => defaultAgentOf(defaultAgent, cli, disabledAgents));
  const [perm, setPerm] = useState(() => permissionFor(agent, defaultPermission));
  const [model, setModel] = useState(""); // "" = the default (settings' model for this agent, else the CLI's)
  const setAgent = (id: string) => {
    setAgentState(id);
    setPerm((p) => permissionFor(id, p));
    setModel("");
  };
  const models = useModels(agent);
  const perms = permissionsFor(agent);
  const blocker = cli ? agentBlocker(agent, cli, disabledAgents) : ""; // cli null = still checking
  const login = !blocker && cli?.agents.find((c) => c.id === agent)?.status === "loginRequired";

  // 고급: name, description and the branch (worktree modes)
  const [more, setMore] = useState(false);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [branches, setBranches] = useState<Branch[] | null>(null);
  useEffect(() => void ipc.listBranches(repo.root).then(setBranches).catch(() => setBranches([])), [repo.root]);
  // the next free agent-N until the user types their own
  const [typedBranch, setTypedBranch] = useState<string | null>(null);
  const branch = typedBranch ?? nextAgentBranch(branches, count + 1);
  const [existing, setExisting] = useState(""); // "" = a new branch
  // workers branch off the Maestro's base branch unless picked otherwise
  const maestroBase = useStore((s) => s.worktrees.find((w) => w.root === repo.root && w.isMaestro)?.baseBranch);
  const [base, setBase] = useState(maestroBase ?? repo.defaultBranch);
  const taken = !existing && branchTaken(branches, branch);

  // Maestro: how finished workers are merged (the project's setting, set right after it's made)
  const [merge, setMerge] = useState<MergeMode>("local");
  useEffect(() => {
    if (!maestro) return;
    let live = true;
    ipc.projectMergeMode(repo.root).then((m) => live && setMerge(m)).catch(() => {});
    return () => {
      live = false;
    };
  }, [maestro, repo.root]);

  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");
  // the project has a Maestro already: the dialog only opens it (not while making one: that one appears meanwhile)
  const existingMaestro = busy ? undefined : foundMaestro;

  const openMaestro = () => {
    if (existingMaestro) useStore.getState().select(existingMaestro.path, true);
    onClose();
  };

  const canMake = !busy && !blocker && (maestro || (!!(existing || branch.trim()) && !taken));
  const submit = async () => {
    if (existingMaestro) return openMaestro();
    if (!canMake) return;
    setBusy(true);
    setErr("");
    const s = useStore.getState();
    const todo = task.trim();
    try {
      // the dialog's own model, else the settings' default for this agent, else the CLI's
      const chosenModel = model || (agentModels[agent] ?? "");
      let info;
      if (maestro) {
        info = await ipc.createMaestro({ projectRoot: repo.root, adapterId: agent, permissionMode: perm, model: chosenModel, baseBranch: maestroBaseArg(base, repo.defaultBranch) });
        s.upsert(repo.root, info);
        // PR mode right after it's made; refused (no remote, no gh login…): it stays local and says why
        if (merge === "pr") {
          await ipc.setMergeMode(repo.root, "pr").catch((e) => s.setError(`병합 방식을 PR로 바꾸지 못해 로컬로 뒀어요\n${e}`));
          await s.refreshWorktrees(repo.root).catch(() => {});
        }
      } else {
        info = await ipc.createAgent({
          projectRoot: repo.root,
          adapterId: agent,
          permissionMode: perm,
          branchName: existing || branch.trim(),
          baseBranch: base.trim() || "HEAD",
          existingBranch: !!existing,
          // an empty name and description come from the 할 일
          name: name.trim() || nameFromTask(todo),
          description: description.trim() || todo,
          model: chosenModel,
        });
        s.upsert(repo.root, info);
        if (parentPath) {
          // made as a worker (its menu, the canvas's + 워커, a line dragged out of a card): linked under its parent
          await ipc.linkWorktrees(repo.root, parentPath, info.path);
          await s.refreshWorktrees(repo.root);
        }
      }
      // opened, in a lasting tab on its role's first screen; the 할 일 waits in its composer (never sent)
      if (todo) s.seedComposer(info.path, todo);
      s.select(info.path, true);
      onClose();
    } catch (e) {
      setErr(String(e));
      setBusy(false);
    }
  };

  const onKey = (e: React.KeyboardEvent | KeyboardEvent) => {
    if (isImeComposing(e)) return;
    if (e.key === "Escape" && !busy) {
      e.stopPropagation();
      return onClose();
    }
    if (e.key !== "Enter") return;
    // Ctrl+Enter makes it; a plain Enter in a one-line field doesn't (the 할 일 takes new lines)
    if (e.ctrlKey || e.metaKey) {
      e.preventDefault();
      void submit();
    } else if (e.target instanceof HTMLInputElement) e.preventDefault();
  };
  // focus fell to the body (the clicked button got disabled while busy): Esc and Ctrl+Enter still work
  useBodyKeys(onKey);

  const permNow = perms.find((p) => p.id === perm) ?? permissionOf(perm);
  const title = maestro ? `Lead 만들기 · ${repo.name}` : parentPath ? `새 워커 · ${parentName ?? repo.name}` : `새 에이전트 · ${repo.name}`;
  const settingsLink = blocker === "꺼 둠" ? "설정에서 켜기" : blocker === "준비 중" ? "" : "설정에서 설치";

  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && !busy && onClose()}>
      <form className="modal form-dialog new-agent" onSubmit={(e) => (e.preventDefault(), void submit())} onKeyDown={onKey} aria-label={title}>
        <h3 className="modal-title">
          {maestro ? <MaestroIcon size={16} /> : <Plus size={16} />}
          <span className="ellipsis">{title}</span>
        </h3>
        {maestro && (
          <p className="dialog-lead">
            프로젝트 폴더에서 바로 일하는 지휘 에이전트예요. 일을 나눠 워커에게 맡기고, 끝난 결과를 기준 브랜치에 합쳐 달라고 요청해요
          </p>
        )}
        {existingMaestro ? (
          <p className="field-hint">이 프로젝트에는 Lead가 이미 있어요. 권한과 모델은 Lead 설정에서 바꿔요</p>
        ) : (
          <>
            <label className="field">
              <span className="field-label">할 일 <span className="field-opt">선택</span></span>
              <textarea
                autoFocus
                rows={3}
                value={task}
                disabled={busy}
                placeholder={maestro ? "Lead에게 맡길 일" : "이 에이전트가 할 일"}
                onChange={(e) => setTask(e.target.value)}
              />
              <span className="field-hint">열린 입력창에 넣어 둬요. Enter로 보내요</span>
            </label>
            <div className="field-row new-agent-row">
              <label className="field">
                <span className="field-label">에이전트</span>
                <span className="agent-select">
                  <AgentLogo id={agent} size={18} />
                  <select value={agent} disabled={busy} onChange={(e) => setAgent(e.target.value)}>
                    {AGENTS.map((a) => {
                      const why = cli ? agentBlocker(a.id, cli, disabledAgents) : "";
                      const st = cli?.agents.find((c) => c.id === a.id)?.status;
                      return (
                        <option key={a.id} value={a.id}>
                          {a.name}{why ? ` · ${why}` : st === "loginRequired" ? ` · ${STATUS_LABEL[st]}` : ""}
                        </option>
                      );
                    })}
                  </select>
                </span>
              </label>
              <label className="field">
                <span className="field-label">권한</span>
                <select value={perm} disabled={busy} onChange={(e) => setPerm(e.target.value)}>
                  {perms.map((p) => <option key={p.id} value={p.id}>{p.label}</option>)}
                </select>
              </label>
              <label className="field">
                <span className="field-label">모델</span>
                <select value={model} disabled={busy || !models} onChange={(e) => setModel(e.target.value)}>
                  <option value="">기본 ({agentModels[agent] ? (models?.find(([v]) => v === agentModels[agent])?.[1] ?? agentModels[agent]) : "CLI 기본값"})</option>
                  {models?.map(([v, label]) => <option key={v} value={v}>{label}</option>)}
                </select>
              </label>
            </div>
            <p className="field-hint" data-warn={permNow.warn || undefined}>{permNow.hint}</p>
            {maestro && perm === "bypassPermissions" && <p className="field-hint" data-warn>Lead가 만드는 워커도 이 권한을 물려받아요</p>}
            {maestro ? (
              <div className="field-row">
                <label className="field">
                  <span className="field-label">워커 기준 브랜치</span>
                  <BranchSelect branches={branches} value={base} onChange={setBase} disabled={busy} />
                </label>
                <div className="field">
                  <span className="field-label">병합 방식</span>
                  <span className="seg" role="radiogroup" aria-label="병합 방식">
                    <button type="button" role="radio" aria-checked={merge === "local"} data-on={merge === "local" || undefined} disabled={busy} onClick={() => setMerge("local")}>로컬</button>
                    <button type="button" role="radio" aria-checked={merge === "pr"} data-on={merge === "pr" || undefined} disabled={busy} onClick={() => setMerge("pr")}>PR</button>
                  </span>
                </div>
              </div>
            ) : (
              <div className="advanced" data-open={more || undefined}>
                <button type="button" className="advanced-toggle" aria-expanded={more} onClick={() => setMore(!more)}>
                  <GitBranch size={12} />
                  <span className="ellipsis">
                    고급 · {existing ? `기존 브랜치 ${existing}` : `새 브랜치 ${branch.trim() || "(이름 없음)"} · 기준 ${base}`}
                  </span>
                  <ChevronDown size={12} />
                </button>
                {more && (
                  <div className="advanced-body">
                    <label className="field">
                      <span className="field-label">이름 <span className="field-opt">선택</span></span>
                      <input
                        value={name}
                        maxLength={60}
                        disabled={busy}
                        placeholder={nameFromTask(task) || `비우면 브랜치 이름(${existing || branch.trim()})`}
                        onChange={(e) => setName(e.target.value)}
                      />
                    </label>
                    <label className="field">
                      <span className="field-label">설명 <span className="field-opt">선택</span></span>
                      <textarea rows={2} value={description} disabled={busy} placeholder={task.trim() ? "비우면 할 일" : "이 에이전트가 맡은 일"} onChange={(e) => setDescription(e.target.value)} />
                    </label>
                    <div className="seg branch-mode">
                      <button type="button" data-on={!existing || undefined} disabled={busy} onClick={() => setExisting("")}>새 브랜치</button>
                      <button
                        type="button"
                        data-on={!!existing || undefined}
                        disabled={busy || !branches?.some((b) => !b.inUse)}
                        onClick={() => setExisting(branches?.find((b) => !b.inUse)?.name ?? "")}
                      >
                        기존 브랜치
                      </button>
                    </div>
                    {existing ? (
                      <label className="field">
                        <span className="field-label">체크아웃할 브랜치</span>
                        <BranchSelect branches={branches} value={existing} onChange={setExisting} pickable disabled={busy} />
                        <span className="field-hint">이 브랜치에서 이어서 일해요. 에이전트를 폐기해도 브랜치는 남아요</span>
                      </label>
                    ) : (
                      <div className="field-row">
                        <label className="field">
                          <span className="field-label">새 브랜치 이름</span>
                          <input value={branch} disabled={busy} aria-invalid={taken || undefined} onChange={(e) => setTypedBranch(e.target.value)} />
                          {taken && <span className="field-hint" data-err>이미 있는 브랜치예요 · 다른 이름을 쓰거나 기존 브랜치를 골라요</span>}
                        </label>
                        <label className="field">
                          <span className="field-label">기준 브랜치</span>
                          <BranchSelect branches={branches} value={base} onChange={setBase} disabled={busy} />
                        </label>
                      </div>
                    )}
                  </div>
                )}
                {!more && taken && <span className="field-hint" data-err>이미 있는 브랜치 이름이에요 · 고급에서 바꿔요</span>}
              </div>
            )}
          </>
        )}
        {!existingMaestro && blocker && (
          <div className="dialog-error" role="alert">
            <span className="wrap">
              {agentLabel(agent)}
              {blocker === "꺼 둠" ? "은(는) 설정에서 꺼 두었어요" : blocker === "준비 중" ? " 연결은 준비 중이에요" : ` · ${cli?.agents.find((c) => c.id === agent)?.message || blocker}`}
            </span>
            {settingsLink && (
              <button type="button" className="link" onClick={() => useStore.getState().openSettings({ section: "agents", agent })}>{settingsLink}</button>
            )}
          </div>
        )}
        {!existingMaestro && login && (
          <p className="field-hint">{agentLabel(agent)}에 로그인돼 있지 않은 것 같아요. 시작이 안 되면 설정 → 에이전트에서 로그인해 주세요</p>
        )}
        {err && <div className="dialog-error" role="alert"><span className="wrap">{err}</span></div>}
        <div className="dialog-foot">
          <span className="spacer" />
          <button type="button" disabled={busy} onClick={onClose}>취소</button>
          <button type="submit" className="primary" disabled={existingMaestro ? false : !canMake}>
            {existingMaestro ? "Lead 열기" : busy ? "만드는 중…" : "만들고 열기"}
            {!busy && <kbd className="keycap">Ctrl ↵</kbd>}
          </button>
        </div>
      </form>
    </div>
  );
}
