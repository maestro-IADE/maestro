import { useEffect, useState } from "react";
import { ipc } from "./ipc";
import { useSettings } from "./settings";
import { useStore } from "./store";
import type { AgentDef, Branch, CliAgentStatus, CliStatus, Repo } from "./types";

// Brand marks: Claude and OpenAI from simple-icons, Antigravity from the icon its installer ships.
const LOGOS = import.meta.glob<string>("./assets/agents/*", { eager: true, import: "default" });

/** The backend's agent catalog (catalog.rs), loaded once before the first render (main.tsx). */
export let AGENTS: AgentDef[] = [];
export async function loadAgents() {
  AGENTS = await ipc.agentCatalog();
}

export const agentDef = (id?: string) => AGENTS.find((a) => a.id === id);

/** Can run as a worktree agent now (has a runtime); Codex doesn't yet. */
export const agentLaunchable = (id?: string) => !!agentDef(id)?.runtimes.length;

export const agentLabel = (id?: string) => agentDef(id)?.name ?? id ?? "";

export const AgentLogo = ({ id, size = 16 }: { id?: string; size?: number }) => {
  const a = agentDef(id);
  const src = a && LOGOS[`./assets/agents/${a.logo}`];
  return src ? <img className="agent-logo" src={src} width={size} height={size} alt="" /> : null;
};

/** 미설치 / 버전 낮음 / 로그인 필요 / 준비됨 */
export const STATUS_LABEL: Record<CliAgentStatus, string> = { missing: "설치 필요", tooOld: "버전 낮음", loginRequired: "로그인 필요", ready: "준비됨" };

/**
 * Can be picked for a new worktree: launchable, installed in a version that works, and not switched off in
 * settings. A login that looks missing doesn't block (it's read from files and can be wrong); the start says so.
 */
export const agentUsable = (id: string, cli: CliStatus | null, disabled: string[]) => {
  const st = cli?.agents.find((c) => c.id === id)?.status;
  return agentLaunchable(id) && (st === "ready" || st === "loginRequired") && !disabled.includes(id);
};

export const PERMISSIONS = [
  { id: "acceptEdits", label: "파일 수정만 자동 허용", hint: "명령 실행 등 나머지 권한이 필요한 도구는 거부돼요." },
  { id: "auto", label: "자동 판단", hint: "Claude가 안전하다고 판단한 작업을 자동으로 허용해요." },
  { id: "bypassPermissions", label: "모두 허용 (주의)", hint: "확인 없이 모든 명령을 실행해요. 격리된 워크트리라도 컴퓨터 전체에 영향을 줄 수 있어요." },
  { id: "plan", label: "계획만", hint: "파일을 바꾸지 않고 계획만 세워요." },
];

/**
 * The repo's branches (local, then remote-only). `pickable`: for checking a branch out, so branches already
 * checked out somewhere are disabled (git refuses them). A value the list doesn't have still shows.
 */
export function BranchSelect({ branches, value, onChange, pickable, className }: {
  branches: Branch[] | null;
  value: string;
  onChange: (name: string) => void;
  pickable?: boolean;
  className?: string;
}) {
  const list = branches ?? [];
  return (
    <select className={className} value={value} disabled={!branches} onChange={(e) => onChange(e.target.value)}>
      {!list.some((b) => b.name === value) && <option value={value}>{branches ? value : "브랜치 읽는 중…"}</option>}
      {list.map((b) => (
        <option key={b.name} value={b.name} disabled={pickable && b.inUse}>
          {b.name}{b.remote ? " · 원격" : ""}{b.inUse ? " · 사용 중" : ""}
        </option>
      ))}
    </select>
  );
}

/** New agent block: pick an agent; Maestro makes its worktree and starts it. */
export function NewAgentDialog({ repo, maestro, parentPath, onClose }: {
  repo: Repo;
  /** create the project's Maestro (coordinator on the project folder) instead of a worktree */
  maestro?: boolean;
  /** make the new worktree a worker of this one */
  parentPath?: string;
  onClose: () => void;
}) {
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
        <h3>{maestro ? `♛ 마에스트로 · ${repo.name}` : parentPath ? `새 워커 · ${repo.name}` : `새 워크트리 · ${repo.name}`}</h3>
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
            {cli?.agents.find((c) => c.id === agent)?.message || "쓸 수 있는 에이전트가 아니에요."} 설정 → 에이전트에서 설치·업데이트하거나 켜 주세요.
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
