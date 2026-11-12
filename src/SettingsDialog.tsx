import { getVersion } from "@tauri-apps/api/app";
import { check } from "@tauri-apps/plugin-updater";
import { useEffect, useState } from "react";
import { AGENTS, AgentLogo, PERMISSIONS, STATUS_LABEL, agentDef, agentLabel, agentLaunchable, agentUsable } from "./NewAgentDialog";
import { AWAY_CHOICES, EASINGS, CANVAS_BGS, CANVAS_GAPS, PANEL_STYLES, SPEEDS, THEMES, useSettings } from "./settings";
import { ipc } from "./ipc";
import { listen } from "@tauri-apps/api/event";
import { XTerm } from "./nodes/XTerm";
import { Spinner } from "./LogoMotion";
import type { AcpInstallEvent, AcpStatus, AgentStatePayload, CliAgent } from "./types";
import { useStore } from "./store";
import { offerUpdate } from "./UpdateNotice";
import { DefaultIcon, RefreshIcon } from "./icons";
import { MergeModeRow } from "./MergeModeRow";

// gear outline (Lucide "settings", ISC)
export const GearIcon = () => (
  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
    <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
    <circle cx="12" cy="12" r="3" />
  </svg>
);

/** An on/off setting: a toggle switch (a checkbox underneath, so keyboard and screen readers work as usual). */
function Switch({ on, disabled, onChange }: { on: boolean; disabled?: boolean; onChange: (on: boolean) => void }) {
  return (
    <label className="switch" title={on ? "끄기" : "켜기"}>
      <input type="checkbox" role="switch" checked={on} disabled={disabled} onChange={(e) => onChange(e.target.checked)} />
      <span />
    </label>
  );
}

const SECTIONS = [
  { id: "agents", label: "에이전트", desc: "쓸 에이전트를 켜고 끄고, 새 워크트리에 쓸 기본 에이전트와 모델을 정해요" },
  { id: "run", label: "실행", desc: "에이전트 CLI를 언제 켜 두고 언제 끌지 정해요" },
  { id: "files", label: "파일", desc: "코드 편집기에서 파일을 다루는 방식을 정해요" },
  { id: "projects", label: "프로젝트", desc: "프로젝트마다 병합 방식(로컬 / PR)을 정해요" },
  { id: "style", label: "스타일", desc: "테마와 화면 움직임의 느낌을 정해요" },
  { id: "about", label: "정보", desc: "설치된 Maestro의 버전을 보고, 새 버전이 있는지 확인해요" },
] as const;

/** 정보: the installed version and a manual update check. A found update opens the card (UpdateNotice.tsx). */
function AboutSection() {
  const [version, setVersion] = useState("");
  const [state, setState] = useState<{ kind: "idle" | "checking" | "latest" } | { kind: "found"; version: string } | { kind: "failed"; message: string }>({ kind: "idle" });
  useEffect(() => void getVersion().then(setVersion).catch(() => {}), []);
  const run = () => {
    setState({ kind: "checking" });
    check()
      .then((u) => {
        if (!u) return setState({ kind: "latest" });
        setState({ kind: "found", version: u.version });
        offerUpdate(u);
      })
      .catch((e) => setState({ kind: "failed", message: String(e) }));
  };
  const line =
    state.kind === "checking" ? "확인하는 중…"
    : state.kind === "latest" ? "최신 버전이에요"
    : state.kind === "found" ? `새 버전 v${state.version}이 있어요. 오른쪽 아래 카드에서 업데이트할 수 있어요.`
    : state.kind === "failed" ? `확인하지 못했어요: ${state.message}`
    : "켜진 지 10초 뒤와 그 뒤 6시간마다 저절로 확인해요";
  return (
    <>
      <h4>버전</h4>
      <section className="set-card">
        <div className="set-row">
          <div>
            <div>Maestro {version ? `v${version}` : ""}</div>
            <div className="muted small">{line}</div>
          </div>
          <button className="primary" disabled={state.kind === "checking"} onClick={run}>업데이트 확인</button>
        </div>
      </section>
    </>
  );
}

/** The command a new worktree with this agent would run, as the backend builds it (agent.rs `launch_preview`). */
function useCommandPreview(id: string, perm: string, model?: string) {
  const [cmd, setCmd] = useState("");
  useEffect(() => void ipc.launchPreview(id, perm, model ?? "").then(setCmd).catch(() => setCmd(id)), [id, perm, model]);
  return cmd;
}

/** (value, label) models an agent offers (catalog: Claude's aliases, or Antigravity's live list). null while loading. */
function useModels(id: string) {
  const [models, setModels] = useState<Array<[string, string]> | null>(null);
  useEffect(() => {
    setModels(null);
    if (!agentDef(id)) return setModels([]);
    ipc.agentModels(id).then((m) => setModels(m.map((x) => [x.id, x.label]))).catch(() => setModels([]));
  }, [id]);
  return models;
}

/** The tile's state pill: 확인 중 / 준비 중 (no runtime) / 설치 필요 / 버전 낮음 / 로그인 필요 / 사용 중 / 꺼짐. */
function tileState(st: CliAgent | undefined, launchable: boolean, on: boolean) {
  if (!st) return "확인 중";
  if (!launchable) return "준비 중";
  if (st.status !== "ready") return STATUS_LABEL[st.status];
  return on ? "사용 중" : "꺼짐";
}

/** One agent as a tile: logo, name, state, on/off switch, launch command; opens up for its model and setup terminal. */
function AgentTile({ id }: { id: string }) {
  const cli = useStore((st) => st.cli);
  const s = useSettings();
  const [open, setOpen] = useState(false);
  const st = cli?.agents.find((a) => a.id === id);
  const soon = !agentLaunchable(id);
  const installed = !!st?.installed && !soon;
  // usable as well as switched on; a too-old CLI can't be switched on until it's updated
  const on = agentUsable(id, cli, s.disabledAgents);
  const isDefault = s.defaultAgent === id;
  const state = tileState(st, !soon, on);
  const preview = useCommandPreview(id, s.defaultPermission, s.agentModels[id]);

  return (
    <div className="agent-tile" data-open={open || undefined} data-off={!on || undefined}>
      <div className="agent-tile-top">
        <AgentLogo id={id} size={28} />
        <div className="agent-tile-name">
          <b>{agentLabel(id)}</b>
          <span className="agent-pill" data-state={state}>{state}</span>
        </div>
        <span className="spacer" />
        <Switch on={on} disabled={!installed || st?.status === "tooOld"} onChange={(v) => s.set({ disabledAgents: v ? s.disabledAgents.filter((x) => x !== id) : [...s.disabledAgents, id] })} />
      </div>
      <code className="agent-tile-cmd" title={st?.path ?? undefined}>{installed ? preview : (st?.installCmd || "앱과 함께 설치돼요")}</code>
      {st?.message && !soon && <div className="muted small">{st.message}{st.version && st.status !== "tooOld" ? ` (${st.version})` : ""}</div>}
      <div className="agent-tile-actions">
        <button className="link small" disabled={!on || isDefault} onClick={() => s.set({ defaultAgent: id })}>
          {isDefault ? <><DefaultIcon />기본 에이전트</> : "기본으로"}
        </button>
        <span className="spacer" />
        <button className="link small" disabled={soon} onClick={() => setOpen(!open)}>{open ? "접기" : installed ? "모델 · 로그인" : "설치"}</button>
      </div>
      {open && st && <div className="agent-tile-body">{st.installed ? <InstalledDetails id={id} st={st} npm={cli!.npm} /> : <InstallHelp id={id} st={st} npm={cli!.npm} />}</div>}
    </div>
  );
}

/** 보조 모델: one agent + model for light background work (project-canvas summaries), or off. */
function AssistantRow() {
  const s = useSettings();
  const cli = useStore((st) => st.cli);
  const models = useModels(s.assistantAgent);
  // agents that can answer one-off questions (catalog `assistant`) and are installed
  const choices = AGENTS.filter((a) => a.features.assistant !== null && cli?.agents.find((c) => c.id === a.id)?.installed);
  const defaultModel = agentDef(s.assistantAgent)?.features.assistant;
  return (
    <div className="set-row">
      <div>
        <div>보조 모델</div>
        <div className="muted small">프로젝트 캔버스의 답변 요약처럼 가벼운 일을 맡아요. 빠르고 싼 모델이 좋아요.</div>
      </div>
      <div className="set-inline">
        <select value={s.assistantAgent} onChange={(e) => s.set({ assistantAgent: e.target.value, assistantModel: "" })}>
          {choices.map((a) => <option key={a.id} value={a.id}>{agentLabel(a.id)}</option>)}
          <option value="off">끄기</option>
        </select>
        {s.assistantAgent !== "off" && (
          <select value={s.assistantModel} disabled={!models} onChange={(e) => s.set({ assistantModel: e.target.value })}>
            <option value="">{defaultModel ? `${defaultModel} (기본)` : "CLI 기본값"}</option>
            {models?.filter(([v]) => v !== defaultModel).map(([v, label]) => <option key={v} value={v}>{label}</option>)}
          </select>
        )}
      </div>
    </div>
  );
}

/** Default model for new worktrees with this agent; an update when it's too old. */
function InstalledDetails({ id, st, npm }: { id: string; st: CliAgent; npm: boolean }) {
  const s = useSettings();
  const models = useModels(id);
  if (!agentLaunchable(id)) return <div className="muted small">{agentLabel(id)} 연결은 준비 중이에요.</div>;
  // a too-old npm install updates with the same command that installs it
  if (st.status === "tooOld") return <InstallHelp id={id} st={st} npm={npm} />;
  return (
    <>
      <div className="set-row">
        <div>
          <div>기본 모델</div>
          <div className="muted small">새 워크트리를 만들 때 이 모델로 시작해요. 이미 있는 워크트리는 그대로예요.</div>
        </div>
        <select value={s.agentModels[id] ?? ""} disabled={!models} onChange={(e) => s.set({ agentModels: { ...s.agentModels, [id]: e.target.value } })}>
          <option value="">CLI 기본값</option>
          {models?.map(([v, label]) => <option key={v} value={v}>{label}</option>)}
        </select>
      </div>
      {!models && <div className="muted small">모델 목록 불러오는 중…</div>}
      {agentDef(id)?.runtimes.includes("acp") && st.acp && <ChatRuntimeRow id={id} acp={st.acp} />}
      <SetupTerminal id={id} installed />
    </>
  );
}

/** One line on the chat runtime's state (acp_setup.rs `AcpStatus`). */
function acpLine(a: AcpStatus) {
  if (a.state !== "ready") return a.message;
  const where = a.dev ? " (개발용 경로)" : "";
  return `준비됨 · 어댑터 ${a.adapterVersion ?? "?"}${where}${a.node ? ` · Node.js ${a.node}` : ""}`;
}

/**
 * The chat runtime (ACP adapter) of an agent that has one: its state, auto / terminal, and the adapter install
 * (Maestro's own npm install into app data, never in the background), with the npm output as it goes.
 */
function ChatRuntimeRow({ id, acp }: { id: string; acp: AcpStatus }) {
  const loadCli = useStore((s) => s.loadCli);
  const setError = useStore((s) => s.setError);
  const [mode, setMode] = useState(acp.mode);
  const [changed, setChanged] = useState(false);
  // the install going on: npm's last lines; failed: its error
  const [lines, setLines] = useState<string[] | null>(null);
  const [failed, setFailed] = useState("");
  useEffect(() => setMode(acp.mode), [acp.mode]);

  useEffect(() => {
    const un = listen<AcpInstallEvent>("acp://install", (e) => {
      const p = e.payload;
      if (p.id !== id) return;
      if (p.kind === "line") return setLines((ls) => [...(ls ?? []), p.text].slice(-6));
      setLines(null);
      setFailed(p.kind === "failed" ? p.text || "설치하지 못했어요" : "");
      loadCli().catch(() => {});
    });
    return () => void un.then((f) => f());
  }, [id]);

  const pick = (m: "auto" | "pty") => {
    setMode(m);
    ipc.setChatRuntime(id, m)
      .then(() => {
        setChanged(true);
        return loadCli();
      })
      .catch((e) => {
        setMode(acp.mode);
        setError(e);
      });
  };
  const install = () => {
    setFailed("");
    setLines([]);
    ipc.acpInstall(id).catch((e) => {
      setLines(null);
      setFailed(String(e));
    });
  };
  const noNode = acp.state === "nodeMissing" || acp.state === "nodeTooOld";
  const canInstall = acp.state === "notInstalled" || acp.state === "broken" || noNode;
  const nodeMajor = acp.nodeMin.split(".")[0];

  return (
    <div className="acp-row">
      <div className="set-row">
        <div>
          <div>채팅 방식</div>
          <div className="muted small">
            자동은 채팅 런타임(ACP)이 준비되면 그걸로, 아니면 터미널로 채팅해요.{changed ? " 다음 시작부터 적용돼요" : ""}
          </div>
        </div>
        <div className="seg">
          <button data-on={mode === "auto" || undefined} onClick={() => pick("auto")}>자동</button>
          <button data-on={mode === "pty" || undefined} onClick={() => pick("pty")}>터미널(PTY)</button>
        </div>
      </div>
      {acp.state === "blocked" ? (
        // failed starts or a crash loop: blocked until the user asks again (opening Settings doesn't lift it)
        <div className="set-row">
          <div className="small">채팅 런타임: {acpLine(acp)}</div>
          <button onClick={() => loadCli(true).catch(setError)}>다시 확인</button>
        </div>
      ) : (
        <div className={`small ${acp.state === "ready" ? "muted" : ""}`}>채팅 런타임: {acpLine(acp)}</div>
      )}
      {lines ? (
        <>
          <div className="set-row">
            <div className="muted small"><Spinner /> 채팅 런타임 설치 중…</div>
            <button onClick={() => ipc.acpInstallCancel(id).catch(setError)}>취소</button>
          </div>
          {!!lines.length && <pre className="acp-log nowheel">{lines.join("\n")}</pre>}
        </>
      ) : (
        canInstall && (
          <div className="set-row">
            <div className="muted small">
              {noNode
                ? `Node.js ${nodeMajor} 이상 필요 · winget install OpenJS.NodeJS.LTS`
                : `@agentclientprotocol/claude-agent-acp${acp.pinned ? `@${acp.pinned}` : ""}를 앱 데이터 폴더에 설치해요.`}
            </div>
            <button className="primary" disabled={noNode} onClick={install}>{acp.state === "broken" ? "다시 설치" : "채팅 런타임 설치"}</button>
          </div>
        )
      )}
      {failed && <div className="err small wrap">{failed}</div>}
    </div>
  );
}

/**
 * A terminal right in settings: installs the CLI if needed, then runs it so its first-run screens
 * (theme, login, trust) are finished here instead of in some other window.
 */
function SetupTerminal({ id, installed }: { id: string; installed: boolean }) {
  const loadCli = useStore((s) => s.loadCli);
  const [open, setOpen] = useState(false);
  const [done, setDone] = useState(false);
  const key = `setup:${id}`;

  // the CLI exiting (login finished, or the user quit) → re-detect what's installed now
  useEffect(() => {
    if (!open) return;
    const un = listen<AgentStatePayload>("agent://state", (e) => {
      if (e.payload.worktreePath === key && e.payload.state.kind === "TERMINATED") {
        setDone(true);
        loadCli().catch(() => {});
      }
    });
    return () => void un.then((f) => f());
  }, [open]);

  const close = () => {
    ipc.setupStop(id).catch(() => {});
    setOpen(false);
    setDone(false);
    loadCli().catch(() => {});
  };

  if (!open)
    return (
      <div className="set-row">
        <div className="muted small">
          {installed ? "로그인하거나 첫 설정을 마치려면 여기서 바로 실행하세요" : "설치부터 첫 실행, 로그인까지 이 안의 터미널에서 이어서 해요"}
        </div>
        <button className="primary" onClick={() => setOpen(true)}>{installed ? "터미널에서 로그인" : "설치하고 로그인"}</button>
      </div>
    );
  return (
    <div className="setup-term">
      <div className="setup-term-bar small">
        <span className="muted">{done ? "끝났어요. 설치 상태를 다시 확인했어요." : "로그인까지 마치면 CLI를 종료하거나 닫기를 누르세요"}</span>
        <span className="spacer" />
        <button onClick={close}>닫기</button>
      </div>
      <XTerm path={key} start={() => ipc.setupStart(id)} />
    </div>
  );
}

function InstallHelp({ id, st, npm }: { id: string; st: CliAgent; npm: boolean }) {
  const install = agentDef(id)?.install;
  if (!st.installCmd) return <div className="muted small">{install?.kind === "guide" ? install.text : "설치 방법을 확인해 주세요"}</div>;
  return (
    <>
      <code className="set-cmd">{st.installCmd}</code>
      {npm ? <SetupTerminal id={id} installed={false} /> : <div className="err small">npm이 없어요. Node.js(nodejs.org)를 먼저 설치해 주세요.</div>}
    </>
  );
}

/** 프로젝트: each project's merge mode (R6). */
function ProjectsSection() {
  const repos = useStore((s) => s.repos);
  return (
    <>
      <h4>병합 방식</h4>
      {repos.length ? (
        <section className="set-card">
          {repos.map((r) => <MergeModeRow key={r.root} repo={r} />)}
        </section>
      ) : (
        <p className="muted small">프로젝트가 없어요</p>
      )}
      <p className="muted small">
        PR 모드는 GitHub 원격과 로그인된 gh(2.40 이상)가 있어야 써요. 앱은 로그인 정보를 보관하지 않고, 강제 푸시나 원격 브랜치 삭제는 하지 않아요. squash는 PR 모드에서만 골라요.
      </p>
    </>
  );
}

/** Settings popup: category list on the left, the chosen category's options on the right. */
export function SettingsDialog({ onClose }: { onClose: () => void }) {
  const s = useSettings();
  const [sec, setSec] = useState<(typeof SECTIONS)[number]["id"]>("agents");
  const [flip, setFlip] = useState(false);
  const cur = SECTIONS.find((x) => x.id === sec)!;
  const cli = useStore((st) => st.cli);
  const isInstalled = (id: string) => !!cli?.agents.find((a) => a.id === id)?.installed;
  const installedAgents = AGENTS.filter((a) => agentLaunchable(a.id) && isInstalled(a.id));
  const availableAgents = AGENTS.filter((a) => !installedAgents.includes(a));
  // re-check on open: a CLI may have been installed from a terminal meanwhile
  useEffect(() => void useStore.getState().loadCli().catch(() => {}), []);

  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal settings" onKeyDown={(e) => e.key === "Escape" && onClose()}>
        <nav className="set-nav">
          <div className="set-nav-title muted small">설정</div>
          {SECTIONS.map((x) => (
            <button key={x.id} data-on={x.id === sec || undefined} onClick={() => setSec(x.id)}>{x.label}</button>
          ))}
        </nav>

        <div className="set-body">
          <button className="icon set-close" title="닫기" autoFocus onClick={onClose}>×</button>
          <h3>{cur.label}</h3>
          <p className="muted small">{cur.desc}</p>

          {sec === "agents" ? (
            <>
              {cli && !cli.agents.some((a) => a.installed) && (
                <div className="set-alert">에이전트 CLI가 하나도 설치돼 있지 않아요. 아래 "설치 가능"에서 하나를 설치해야 워크트리를 만들 수 있어요.</div>
              )}
              <h4>기본값</h4>
              <section className="set-card">
                <div className="set-row">
                  <div>
                    <div>기본 에이전트</div>
                    <div className="muted small">새 워크트리 창에서 미리 골라 둬요. 자동은 쓸 수 있는 첫 에이전트예요.</div>
                  </div>
                  <div className="seg seg-logos">
                    <button data-on={s.defaultAgent === "auto" || undefined} onClick={() => s.set({ defaultAgent: "auto" })}>자동</button>
                    {AGENTS.filter((a) => agentUsable(a.id, cli, s.disabledAgents)).map((a) => (
                      <button key={a.id} data-on={s.defaultAgent === a.id || undefined} onClick={() => s.set({ defaultAgent: a.id })}>
                        <AgentLogo id={a.id} />
                        {agentLabel(a.id)}
                      </button>
                    ))}
                  </div>
                </div>
                <div className="set-row">
                  <div>
                    <div>권한</div>
                    <div className="muted small">{PERMISSIONS.find((p) => p.id === s.defaultPermission)?.hint}</div>
                  </div>
                  <select value={s.defaultPermission} onChange={(e) => s.set({ defaultPermission: e.target.value })}>
                    {PERMISSIONS.map((p) => <option key={p.id} value={p.id}>{p.label}</option>)}
                  </select>
                </div>
                <AssistantRow />
              </section>

              <div className="set-group-head">
                <h4>에이전트</h4>
                <span className="spacer" />
                <button className="link muted small" onClick={() => useStore.getState().loadCli(true).catch(() => {})}><RefreshIcon />다시 확인</button>
              </div>
              <div className="agent-grid">
                {[...installedAgents, ...availableAgents].map((a) => <AgentTile key={a.id} id={a.id} />)}
              </div>
            </>
          ) : sec === "projects" ? (
            <ProjectsSection />
          ) : sec === "about" ? (
            <AboutSection />
          ) : sec === "files" ? (
            <>
              <h4>파일 편집</h4>
              <section className="set-card">
                <div className="set-row">
                  <div>
                    <div>자동 저장</div>
                    <div className="muted small">입력을 멈추고 1초 뒤에 저장해요. 그 사이 에이전트가 같은 파일을 바꿨으면 덮어쓰지 않고 알려 줘요.</div>
                  </div>
                  <Switch on={s.autoSave} onChange={(v) => s.set({ autoSave: v })} />
                </div>
              </section>
            </>
          ) : sec === "run" ? (
            <>
              <h4>자리 비움</h4>
              <section className="set-card">
                <div className="set-row">
                  <div>
                    <div>자리 비운 프로젝트 정리</div>
                    <div className="muted small">다른 프로젝트로 옮긴 뒤 이만큼 지나면 쉬고 있는 에이전트를 꺼요. 작업 중, 승인 대기, 백그라운드 셸이 도는 에이전트는 남겨요. 돌아오면 대화를 이어서 다시 켜요.</div>
                  </div>
                  <select value={s.awayMinutes} onChange={(e) => s.set({ awayMinutes: Number(e.target.value) })}>
                    {AWAY_CHOICES.map((c) => <option key={c.minutes} value={c.minutes}>{c.label}</option>)}
                  </select>
                </div>
              </section>
            </>
          ) : (
            <>
              <h4>테마</h4>
              <div className="theme-pick">
                {THEMES.map((t) => (
                  <button key={t.id} data-theme-swatch={t.id} data-on={t.id === s.theme || undefined} onClick={() => s.set({ theme: t.id })}>
                    <span className="swatch" />
                    {t.label}
                  </button>
                ))}
              </div>

              <h4>패널 구분</h4>
              <section className="set-card">
                <div className="set-row">
                  <div className="muted small">{PANEL_STYLES[s.panelStyle]?.hint}</div>
                  <div className="seg">
                    {Object.entries(PANEL_STYLES).map(([id, p]) => (
                      <button key={id} data-on={id === s.panelStyle || undefined} onClick={() => s.set({ panelStyle: id as keyof typeof PANEL_STYLES })}>{p.label}</button>
                    ))}
                  </div>
                </div>
              </section>

              <h4>캔버스 배경</h4>
              <section className="set-card">
                <div className="set-row">
                  <div>
                    <div>무늬</div>
                    <div className="muted small">{(CANVAS_BGS[s.canvasBg] ?? CANVAS_BGS.dots).hint}</div>
                  </div>
                  <div className="seg">
                    {Object.entries(CANVAS_BGS).map(([id, b]) => (
                      <button key={id} data-on={id === (s.canvasBg ?? "dots") || undefined} onClick={() => s.set({ canvasBg: id as keyof typeof CANVAS_BGS })}>{b.label}</button>
                    ))}
                  </div>
                </div>
                <div className="set-row">
                  <div>간격</div>
                  <div className="seg">
                    {CANVAS_GAPS.map((g) => (
                      <button key={g.px} disabled={s.canvasBg === "none"} data-on={g.px === (s.canvasGap ?? 24) || undefined} title={`${g.px}px`} onClick={() => s.set({ canvasGap: g.px })}>{g.label}</button>
                    ))}
                  </div>
                </div>
                <div className="set-row">
                  <div>진하기</div>
                  <div className="set-range">
                    <input
                      type="range"
                      aria-label="무늬 진하기"
                      min={10}
                      max={100}
                      step={5}
                      disabled={s.canvasBg === "none"}
                      value={s.canvasAlpha ?? 100}
                      onChange={(e) => s.set({ canvasAlpha: Number(e.target.value) })}
                    />
                    <span className="muted small">{s.canvasAlpha ?? 100}%</span>
                  </div>
                </div>
              </section>

              <h4>모션</h4>
              <section className="set-card">
                <div className="set-row">
                  <div>
                    <div>모션 사용</div>
                    <div className="muted small">패널 접기·펴기, 미니맵 확대, 창 열기, 블록으로 이동</div>
                  </div>
                  <Switch on={s.motion} onChange={(v) => s.set({ motion: v })} />
                </div>
                <div className="set-row">
                  <div>이징</div>
                  <div className="seg">
                    {Object.entries(EASINGS).map(([id, e]) => (
                      <button key={id} disabled={!s.motion} data-on={id === s.easing || undefined} onClick={() => s.set({ easing: id as keyof typeof EASINGS })}>{e.label}</button>
                    ))}
                  </div>
                </div>
                <div className="set-row">
                  <div>속도</div>
                  <div className="seg">
                    {Object.entries(SPEEDS).map(([id, sp]) => (
                      <button key={id} disabled={!s.motion} data-on={id === s.speed || undefined} onClick={() => s.set({ speed: id as keyof typeof SPEEDS })}>{sp.label}</button>
                    ))}
                  </div>
                </div>
                <div className="set-row">
                  <div>미리보기</div>
                  <button className="motion-preview" onClick={() => setFlip(!flip)} title="눌러서 미리보기">
                    <span className="knob" data-flip={flip || undefined} />
                  </button>
                </div>
              </section>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
