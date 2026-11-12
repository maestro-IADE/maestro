import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { AGENTS, AgentLogo, agentDef, agentLabel, agentLaunchable, agentUsable, defaultAgentOf, permissionFor, permissionOf, permissionsFor, useModels } from "../agents";
import { useSettings } from "../settings";
import { ipc } from "../ipc";
import { XTerm } from "../nodes/XTerm";
import { Spinner } from "../LogoMotion";
import { useStore, type SettingsTarget } from "../store";
import { ChevronDown, RefreshIcon } from "../icons";
import type { AcpInstallEvent, AcpStatus, CliAgent, PtyExitPayload } from "../types";
import { acpAction, acpLine, acpShort, agentRowState, chatRuntimeNeeded } from "./agentState";
import { Group, Row, Seg, Switch } from "./parts";

/**
 * 에이전트: the defaults for new agents (the one place to pick the default agent), then one row per agent of the
 * catalog, installed ones first. `target.agent` opens that agent's row at 관리 and marks it for the deep link.
 */
export function AgentsSection({ target }: { target: SettingsTarget }) {
  const s = useSettings();
  const cli = useStore((st) => st.cli);
  const installed = (id: string) => agentLaunchable(id) && !!cli?.agents.find((a) => a.id === id)?.installed;
  const order = [...AGENTS.filter((a) => installed(a.id)), ...AGENTS.filter((a) => !installed(a.id))];
  return (
    <>
      {cli && !cli.agents.some((a) => a.installed) && (
        <div className="set-alert">에이전트 CLI가 하나도 설치돼 있지 않아요. 아래 목록에서 하나를 설치해야 에이전트를 만들 수 있어요</div>
      )}
      <Group title="기본값">
        <Row label="기본 에이전트" desc="새 에이전트 창에서 미리 골라 둬요. 자동은 쓸 수 있는 첫 에이전트예요">
          <Seg
            value={s.defaultAgent}
            options={[
              ["auto", "자동"],
              ...AGENTS.filter((a) => agentUsable(a.id, cli, s.disabledAgents)).map((a) => [a.id, <><AgentLogo id={a.id} />{agentLabel(a.id)}</>] as const),
            ]}
            onChange={(v) => s.set({ defaultAgent: v })}
          />
        </Row>
        <DefaultPermissionRow />
        <AssistantRow />
      </Group>

      <Group
        title="에이전트"
        head={<button className="link muted small set-recheck" onClick={() => useStore.getState().loadCli(true).catch(() => {})}><RefreshIcon />다시 확인</button>}
      >
        {order.map((a) => <AgentRow key={a.id} id={a.id} target={target} />)}
      </Group>
    </>
  );
}

/**
 * 권한 for new agents: the modes the default agent takes (Antigravity has no 매번 묻기). A saved mode that agent
 * doesn't take stays saved (another agent may take it); the new agent dialog falls back per agent (`permissionFor`).
 */
function DefaultPermissionRow() {
  const s = useSettings();
  const cli = useStore((st) => st.cli);
  const list = permissionsFor(defaultAgentOf(s.defaultAgent, cli, s.disabledAgents));
  const cur = list.find((p) => p.id === s.defaultPermission);
  return (
    <Row
      label="새 에이전트의 권한"
      desc={cur
        ? "새로 만드는 에이전트에만 적용돼요. 이미 있는 에이전트는 에이전트 설정에서 바꿔요"
        : "기본 에이전트가 받지 않는 권한이에요. 새 에이전트 창에서 파일 수정 자동 허용으로 시작해요"}
    >
      <select aria-label="새 에이전트의 권한" title={cur?.hint} value={s.defaultPermission} onChange={(e) => s.set({ defaultPermission: e.target.value })}>
        {!cur && <option value={s.defaultPermission}>{permissionOf(s.defaultPermission).label}</option>}
        {list.map((p) => <option key={p.id} value={p.id}>{p.label}</option>)}
      </select>
    </Row>
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
    <Row label="보조 모델" desc="프로젝트 캔버스의 답변 요약처럼 가벼운 일을 맡아요. 빠르고 싼 모델이 좋아요">
      <div className="set-inline">
        <select aria-label="보조 모델 에이전트" value={s.assistantAgent} onChange={(e) => s.set({ assistantAgent: e.target.value, assistantModel: "" })}>
          {choices.map((a) => <option key={a.id} value={a.id}>{agentLabel(a.id)}</option>)}
          <option value="off">끄기</option>
        </select>
        {s.assistantAgent !== "off" && (
          <select aria-label="보조 모델" value={s.assistantModel} disabled={!models} onChange={(e) => s.set({ assistantModel: e.target.value })}>
            <option value="">{defaultModel ? `${defaultModel} (기본)` : "CLI 기본값"}</option>
            {models?.filter(([v]) => v !== defaultModel).map(([v, label]) => <option key={v} value={v}>{label}</option>)}
          </select>
        )}
      </div>
    </Row>
  );
}

/** The command a new agent of this kind would run, as the backend builds it (agent.rs `launch_preview`). */
function useCommandPreview(id: string, perm: string, model?: string) {
  const [cmd, setCmd] = useState("");
  useEffect(() => void ipc.launchPreview(id, perm, model ?? "").then(setCmd).catch(() => setCmd(id)), [id, perm, model]);
  return cmd;
}

/**
 * The chat runtime (ACP adapter) install of one agent: Maestro's own npm install into app data, never in the
 * background. `lines`: npm's last lines while it runs (null when nothing runs), `failed`: the last failure.
 */
function useAcpInstall(id: string) {
  const loadCli = useStore((s) => s.loadCli);
  const setError = useStore((s) => s.setError);
  const [lines, setLines] = useState<string[] | null>(null);
  const [failed, setFailed] = useState("");
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
  const install = () => {
    setFailed("");
    setLines([]);
    ipc.acpInstall(id).catch((e) => {
      setLines(null);
      setFailed(String(e));
    });
  };
  const cancel = () => ipc.acpInstallCancel(id).catch(setError);
  return { lines, failed, install, cancel };
}
type AcpInstall = ReturnType<typeof useAcpInstall>;

/**
 * One agent as a 56px row: logo, name and its state (the CLI's and the chat runtime's together, `agentRowState`), the
 * launch command, and for an agent with a chat runtime one more line with its state and the one thing to do about it.
 * On the right the "기본" badge, the on/off switch and 관리, which opens 상태·로그인 → 채팅 런타임 → 기본 모델 → 채팅 연결.
 */
function AgentRow({ id, target }: { id: string; target: SettingsTarget }) {
  const cli = useStore((st) => st.cli);
  const s = useSettings();
  const hit = target.agent === id;
  const [open, setOpen] = useState(hit);
  // a deep link to this agent while settings is open opens it again
  useEffect(() => void (hit && setOpen(true)), [target]);
  const st = cli?.agents.find((a) => a.id === id);
  const soon = !agentLaunchable(id);
  const installed = !!st?.installed && !soon;
  // usable as well as switched on; a too-old CLI can't be switched on until it's updated
  const on = agentUsable(id, cli, s.disabledAgents);
  // the agent new agents start with (자동 = the first usable one)
  const isDefault = on && defaultAgentOf(s.defaultAgent, cli, s.disabledAgents) === id;
  const state = agentRowState(st, !soon, on);
  const preview = useCommandPreview(id, permissionFor(id, s.defaultPermission), s.agentModels[id]);
  const inst = useAcpInstall(id);
  const acp = installed ? st?.acp ?? null : null;
  // a failed chat runtime install shows why in 관리
  useEffect(() => void (inst.failed && setOpen(true)), [inst.failed]);

  return (
    <div className="agent-row" data-open={open || undefined} data-off={!on || undefined} data-target={hit || undefined}>
      <div className="agent-row-head">
        <AgentLogo id={id} size={24} />
        <div className="agent-row-text">
          <div className="agent-row-name">
            <b>{agentLabel(id)}</b>
            <span className="agent-state" data-tone={state.tone} title={st?.message || undefined}>{state.label}</span>
          </div>
          <code className="agent-row-cmd" title={installed ? (st?.path ?? undefined) : undefined}>
            {soon ? "연결 준비 중이에요" : installed ? preview : (st?.installCmd || "앱과 함께 설치돼요")}
          </code>
          {acp && <ChatRuntimeLine acp={acp} inst={inst} />}
        </div>
        {isDefault && <span className="agent-badge" title="새 에이전트 창에서 미리 골라 둬요">기본</span>}
        <Switch
          label={`${agentLabel(id)} 사용`}
          on={on}
          disabled={!installed || st?.status === "tooOld"}
          onChange={(v) => s.set({ disabledAgents: v ? s.disabledAgents.filter((x) => x !== id) : [...s.disabledAgents, id] })}
        />
        <button className="agent-manage" disabled={soon || !st} aria-expanded={open} onClick={() => setOpen(!open)}>
          관리
          <ChevronDown size={12} />
        </button>
      </div>
      {open && st && cli && (
        <div className="agent-row-body">
          {!st.installed ? (
            <Sub title="상태·로그인"><InstallHelp id={id} st={st} npm={cli.npm} /></Sub>
          ) : st.status === "tooOld" ? (
            // a too-old npm install updates with the same command that installs it
            <Sub title="상태·로그인">
              <div className="set-desc">{st.message}</div>
              <InstallHelp id={id} st={st} npm={cli.npm} />
            </Sub>
          ) : (
            <InstalledDetails id={id} st={st} acp={acp} inst={inst} />
          )}
        </div>
      )}
    </div>
  );
}

/** A part of an agent's 관리: its small title (12/500, muted), then its rows. */
function Sub({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="agent-sub">
      <div className="agent-sub-title">{title}</div>
      {children}
    </div>
  );
}

/** An installed agent's 관리: its state and the login terminal, the chat runtime, the default model, the chat connection. */
function InstalledDetails({ id, st, acp, inst }: { id: string; st: CliAgent; acp: AcpStatus | null; inst: AcpInstall }) {
  const s = useSettings();
  const models = useModels(id);
  return (
    <>
      <Sub title="상태·로그인">
        <div className="set-desc">{st.message || "준비됐어요"}{st.version ? ` · v${st.version}` : ""}</div>
        <SetupTerminal id={id} installed />
      </Sub>
      {acp && (
        <Sub title="채팅 런타임">
          <ChatRuntimeRow acp={acp} inst={inst} />
        </Sub>
      )}
      <Sub title="기본 모델">
        <Row label="새 에이전트의 모델" desc={models ? "새 에이전트를 만들 때 이 모델로 시작해요. 이미 있는 에이전트는 에이전트 설정에서 바꿔요" : "모델 목록 불러오는 중…"}>
          <select aria-label="기본 모델" value={s.agentModels[id] ?? ""} disabled={!models} onChange={(e) => s.set({ agentModels: { ...s.agentModels, [id]: e.target.value } })}>
            <option value="">CLI 기본값</option>
            {models?.map(([v, label]) => <option key={v} value={v}>{label}</option>)}
          </select>
        </Row>
      </Sub>
      {acp && (
        <Sub title="채팅 연결">
          <ChatConnectionRow id={id} acp={acp} />
        </Sub>
      )}
    </>
  );
}

/**
 * The chat runtime in one line under an agent's row, shown without opening 관리 ("채팅 런타임 · 설치되지 않았어요
 * [설치]"): its state and at most one button (설치 / 다시 설치 / 다시 확인, 취소 while it installs).
 */
function ChatRuntimeLine({ acp, inst }: { acp: AcpStatus; inst: AcpInstall }) {
  const loadCli = useStore((s) => s.loadCli);
  const setError = useStore((s) => s.setError);
  const action = acpAction(acp);
  const warn = !inst.lines && (chatRuntimeNeeded(acp) || !!inst.failed);
  return (
    <div className="agent-row-acp" data-warn={warn || undefined}>
      <span className="ellipsis" title={acpLine(acp)}>
        채팅 런타임 · {inst.lines ? <><Spinner /> 설치하는 중…</> : inst.failed ? "설치하지 못했어요" : acpShort(acp)}
        {acp.mode === "pty" ? " · 터미널 경유로 채팅해요" : ""}
      </span>
      {inst.lines ? (
        <button className="agent-acp-btn" onClick={inst.cancel}>취소</button>
      ) : action === "install" || action === "reinstall" ? (
        <button className="agent-acp-btn primary" onClick={inst.install}>{action === "install" ? "설치" : "다시 설치"}</button>
      ) : action === "recheck" ? (
        // failed starts or a crash loop: blocked until the user asks again (opening settings doesn't lift it)
        <button className="agent-acp-btn" onClick={() => loadCli(true).catch(setError)}>다시 확인</button>
      ) : null}
    </div>
  );
}

/** The chat runtime in full (관리): its state with versions, what an install does or what it needs, npm's output. */
function ChatRuntimeRow({ acp, inst }: { acp: AcpStatus; inst: AcpInstall }) {
  const noNode = acp.state === "nodeMissing" || acp.state === "nodeTooOld";
  const canInstall = acp.state === "notInstalled" || acp.state === "broken";
  const nodeMajor = acp.nodeMin.split(".")[0];
  return (
    <>
      <div className={`small ${acp.state === "ready" ? "muted" : ""}`}>{acpLine(acp)}</div>
      {noNode && <div className="set-desc">Node.js {nodeMajor} 이상 필요 · winget install OpenJS.NodeJS.LTS</div>}
      {canInstall && !inst.lines && (
        <div className="set-desc">@agentclientprotocol/claude-agent-acp{acp.pinned ? `@${acp.pinned}` : ""}를 앱 데이터 폴더에 설치해요. 위 줄의 설치 버튼으로 시작해요</div>
      )}
      {!!inst.lines?.length && <pre className="acp-log nowheel">{inst.lines.join("\n")}</pre>}
      {inst.failed && <div className="err small wrap">{inst.failed}</div>}
    </>
  );
}

/** 채팅 연결: 자동 (the chat runtime when it's ready, else through the terminal) or 터미널 경유 (always the terminal). */
function ChatConnectionRow({ id, acp }: { id: string; acp: AcpStatus }) {
  const loadCli = useStore((s) => s.loadCli);
  const setError = useStore((s) => s.setError);
  const [mode, setMode] = useState(acp.mode);
  const [changed, setChanged] = useState(false);
  useEffect(() => setMode(acp.mode), [acp.mode]);
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
  return (
    <Row
      label="채팅 연결"
      desc={`자동은 채팅 런타임이 준비되면 그걸로, 아니면 터미널을 거쳐 채팅해요${changed ? ". 다음 시작부터 적용돼요" : ""}`}
    >
      <Seg value={mode} options={[["auto", "자동"], ["pty", "터미널 경유"]]} onChange={pick} />
    </Row>
  );
}

/**
 * A terminal right in settings (340px): installs the CLI if needed, then runs it so its first-run screens (theme,
 * login, trust) are finished here instead of in some other window. The process ending with 0 ("pty://exit") closes
 * it and leaves "끝났어요"; any other code keeps it open with the code, so the output can be read.
 */
function SetupTerminal({ id, installed }: { id: string; installed: boolean }) {
  const loadCli = useStore((s) => s.loadCli);
  const [open, setOpen] = useState(false);
  // the exit code while it's still open (not 0), and whether it closed itself after a 0
  const [code, setCode] = useState<number | null>(null);
  const [finished, setFinished] = useState(false);
  const key = `setup:${id}`;

  // the CLI exiting (login finished, or the user quit) → re-detect what's installed now
  useEffect(() => {
    if (!open) return;
    const un = listen<PtyExitPayload>("pty://exit", (e) => {
      if (e.payload.worktreePath !== key) return;
      loadCli().catch(() => {});
      if (e.payload.code !== 0) return setCode(e.payload.code);
      ipc.setupStop(id).catch(() => {});
      setOpen(false);
      setFinished(true);
    });
    return () => void un.then((f) => f());
  }, [open]);

  const start = () => {
    setCode(null);
    setFinished(false);
    setOpen(true);
  };
  const close = () => {
    ipc.setupStop(id).catch(() => {});
    setOpen(false);
    setCode(null);
    loadCli().catch(() => {});
  };

  if (!open)
    return (
      <Row
        label={installed ? "로그인과 첫 설정" : "설치와 로그인"}
        desc={finished ? "끝났어요" : installed ? "로그인하거나 첫 설정을 마치려면 여기서 바로 실행해요" : "설치부터 첫 실행, 로그인까지 이 안의 터미널에서 이어서 해요"}
      >
        <button className="primary" onClick={start}>{installed ? "터미널에서 로그인" : "설치하고 로그인"}</button>
      </Row>
    );
  return (
    <div className="setup-term">
      <div className="setup-term-bar small">
        <span className={code === null ? "muted" : "err"}>
          {code === null ? "로그인까지 마치고 CLI를 끝내면 저절로 닫혀요" : `끝났지만 오류가 있었어요 · 종료 코드 ${code}`}
        </span>
        <span className="spacer" />
        <button onClick={close}>닫기</button>
      </div>
      <XTerm path={key} start={() => ipc.setupStart(id)} />
    </div>
  );
}

function InstallHelp({ id, st, npm }: { id: string; st: CliAgent; npm: boolean }) {
  const install = agentDef(id)?.install;
  if (!st.installCmd) return <div className="set-desc">{install?.kind === "guide" ? install.text : "설치 방법을 확인해 주세요"}</div>;
  return (
    <>
      <code className="set-cmd">{st.installCmd}</code>
      {npm ? <SetupTerminal id={id} installed={false} /> : <div className="err small">npm이 없어요. Node.js(nodejs.org)를 먼저 설치해 주세요</div>}
    </>
  );
}
