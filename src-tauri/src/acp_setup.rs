//! What the ACP chat runtime needs before it can start, and the decision whether it does (`plan`):
//! - Node.js (`find_node`), the user's own CLI as a real executable (`cli_executable`), the adapter (`locate_adapter`:
//!   the managed install under app data, or `MAESTRO_ACP_CLAUDE_DIR` for development)
//! - the managed install itself (`install`, run from Settings only: nothing is downloaded silently)
//! - the user's choice per agent (`~/.maestro/runtimes.json`: auto, or the terminal forced), checks cached for a
//!   minute (`probe`), and starts that failed for the whole agent (`health`) so a broken setup doesn't loop
//! - the adapter's environment (`acp_env`)
//! Every reason the chat falls back to the terminal is a `FallbackReason`, shown as a small note.

use crate::acp::Client;
use crate::catalog::{Acp, AgentDef};
use crate::cli;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, LazyLock, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

/// How long npm may take for the managed install.
const INSTALL_LIMIT: Duration = Duration::from_secs(600);
const PROBE_TTL: Duration = Duration::from_secs(60);
/// Two crashes of a running adapter within this long block the agent's chat runtime until "다시 확인".
const CRASH_WINDOW: Duration = Duration::from_secs(600);
const RECEIPT: &str = ".install-complete.json";

/// Who asked for the agent: `Cli` = the CLI view (its terminal, no note).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Want {
    Auto,
    Cli,
}

/// The user's choice for an agent's chat (Settings → 에이전트).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Pref {
    #[default]
    Auto,
    Pty,
}

/// Why the chat runs on the terminal (PTY) instead of ACP.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum FallbackReason {
    /// the user forced the terminal in Settings
    Forced,
    /// the agent has no ACP runtime (Antigravity, R4)
    NotSupported,
    /// the CLI view asked for the terminal
    CliView,
    /// the CLI is missing, or not something the adapter can start (`CLAUDE_CODE_EXECUTABLE`)
    CliNotExecutable,
    CliTooOld { found: String, need: String },
    /// the transcript's folder name is hashed by Claude (a folder path over 200 characters): where it goes can't be
    /// known before it's there, and the adapter reports nothing
    TranscriptUnreachable,
    NodeMissing { need: String },
    NodeTooOld { found: String, need: String },
    AdapterMissing,
    AdapterBroken { why: String },
    LoggedOut,
    StartFailed { message: String },
    LoginRequired,
    /// `session/load` failed for this worktree's conversation
    LoadFailed { message: String },
    /// the permission mode couldn't be set: never run in the adapter's default instead
    ModeRejected { mode: String },
    /// the worktree's model couldn't be set: never run the user's global model instead (the CLI view runs `--model`)
    ModelRejected { model: String },
    /// the adapter crashed twice within ten minutes
    Crashed,
}

impl FallbackReason {
    /// The muted note shown with the agent; None when there's nothing to say (not an ACP agent, the CLI view).
    pub fn note(&self) -> Option<String> {
        use FallbackReason::*;
        let s = match self {
            NotSupported | CliView => return None,
            Forced => "설정에서 채팅을 터미널 방식으로 고정했어요".into(),
            CliNotExecutable => "Claude Code 실행 파일을 채팅 런타임에서 쓸 수 없어 터미널 방식으로 실행 중이에요".into(),
            CliTooOld { found, need } => format!("Claude Code {need} 이상이 필요해 터미널 방식으로 실행 중이에요 (지금 {found})"),
            TranscriptUnreachable => "폴더 경로가 너무 길어 대화 기록 위치를 찾을 수 없어 터미널 방식으로 실행 중이에요".into(),
            NodeMissing { need } => format!("Node.js {} 이상이 없어 터미널 방식으로 실행 중이에요", major(need)),
            NodeTooOld { found, need } => format!("Node.js {} 이상이 필요해 터미널 방식으로 실행 중이에요 (지금 {found})", major(need)),
            AdapterMissing => "채팅 런타임 어댑터가 설치되지 않았어요 · 설정 → 에이전트에서 설치".into(),
            AdapterBroken { why } => format!("채팅 런타임 어댑터 설치가 깨졌어요 ({why}) · 설정 → 에이전트에서 다시 설치"),
            LoggedOut => "로그인돼 있지 않아 터미널 방식으로 실행 중이에요".into(),
            StartFailed { message } => format!("채팅 런타임을 시작하지 못해 터미널 방식으로 실행 중이에요: {message}"),
            LoginRequired => "로그인이 필요해 터미널 방식으로 실행 중이에요".into(),
            LoadFailed { message } => format!("이 대화를 채팅 런타임으로 열지 못해 터미널 방식으로 실행 중이에요: {message}"),
            ModeRejected { mode } => format!("권한 모드({mode})를 채팅 런타임에 적용하지 못해 터미널 방식으로 실행 중이에요"),
            ModelRejected { model } => format!("모델({model})을 채팅 런타임에 적용하지 못해 터미널 방식으로 실행 중이에요"),
            Crashed => "채팅 런타임이 잇달아 꺼져 터미널 방식으로 실행 중이에요 · 설정에서 다시 확인".into(),
        };
        Some(s)
    }

    /// A failure of the agent's setup, not of one worktree: later starts skip ACP until "다시 확인" or an install.
    pub fn blocks_agent(&self) -> bool {
        matches!(self, FallbackReason::StartFailed { .. } | FallbackReason::LoginRequired | FallbackReason::Crashed)
    }

    /// For the UI: the reason's fields plus `message` (its note).
    pub fn view(&self) -> Value {
        let mut v = serde_json::to_value(self).unwrap_or_default();
        v["message"] = self.note().map_or(Value::Null, Value::String);
        v
    }
}

fn major(v: &str) -> &str {
    v.split('.').next().unwrap_or(v)
}

/// Everything needed to start the adapter.
#[derive(Clone, Debug, PartialEq)]
pub struct AcpPlan {
    pub node: PathBuf,
    pub entry: PathBuf,
    /// the user's CLI, for `CLAUDE_CODE_EXECUTABLE`
    pub cli_exe: PathBuf,
    pub adapter_version: String,
    /// the `MAESTRO_ACP_CLAUDE_DIR` install, not the managed one
    pub dev: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Adapter {
    Found { dir: PathBuf, entry: PathBuf, version: String, dev: bool },
    Missing,
    Broken(String),
}

/// The checks `plan` reads, cached for a minute (`probe`).
#[derive(Clone, Debug)]
pub struct Probe {
    pub node: Option<(PathBuf, String)>,
    pub cli: Result<PathBuf, FallbackReason>,
    /// only asked when the adapter is there (a `--version` run); None = unknown, never blocks
    pub cli_version: Option<String>,
    pub logged_in: Option<bool>,
    pub adapter: Adapter,
}

impl Default for Probe {
    fn default() -> Probe {
        Probe { node: None, cli: Err(FallbackReason::CliNotExecutable), cli_version: None, logged_in: None, adapter: Adapter::Missing }
    }
}

pub struct PlanInput<'a> {
    pub acp: Option<&'a Acp>,
    pub want: Want,
    pub pref: Pref,
    pub probe: &'a Probe,
    pub cwd: &'a str,
    /// the agent's start blocked earlier this run (`health`)
    pub health: Option<&'a FallbackReason>,
}

/// ACP or why not. The first failing check wins, in this order: the user's choice, support, the CLI view, the CLI,
/// the transcript's place, Node, the adapter, the login, earlier failures.
pub fn plan(i: &PlanInput) -> Result<AcpPlan, FallbackReason> {
    use FallbackReason::*;
    if i.pref == Pref::Pty {
        return Err(Forced);
    }
    let Some(acp) = i.acp else { return Err(NotSupported) };
    if i.want == Want::Cli {
        return Err(CliView);
    }
    let cli_exe = i.probe.cli.clone()?;
    if let Some(v) = &i.probe.cli_version {
        if cli::older(v, acp.min_version) {
            return Err(CliTooOld { found: v.clone(), need: acp.min_version.into() });
        }
    }
    if crate::claude::slug_hashed(i.cwd) {
        return Err(TranscriptUnreachable);
    }
    let Some((node, node_version)) = &i.probe.node else { return Err(NodeMissing { need: acp.node_min.into() }) };
    if cli::older(node_version, acp.node_min) {
        return Err(NodeTooOld { found: node_version.clone(), need: acp.node_min.into() });
    }
    let (entry, adapter_version, dev) = match &i.probe.adapter {
        Adapter::Missing => return Err(AdapterMissing),
        Adapter::Broken(why) => return Err(AdapterBroken { why: why.clone() }),
        Adapter::Found { entry, version, dev, .. } => (entry.clone(), version.clone(), *dev),
    };
    if i.probe.logged_in == Some(false) {
        return Err(LoggedOut);
    }
    if let Some(h) = i.health {
        return Err(h.clone());
    }
    Ok(AcpPlan { node: node.clone(), entry, cli_exe, adapter_version, dev })
}

// ---------- app data, preference ----------

static APP_DATA: OnceLock<PathBuf> = OnceLock::new();

/// Where managed installs and adapter logs go (`app_local_data_dir`), set once at startup.
pub fn set_app_data(dir: PathBuf) {
    let _ = APP_DATA.set(dir);
}

pub fn app_data() -> Option<&'static Path> {
    APP_DATA.get().map(PathBuf::as_path)
}

fn prefs_file(home: &Path) -> PathBuf {
    home.join(".maestro").join("runtimes.json")
}

/// The format of `runtimes.json` this build writes (and the newest it reads; durable.rs).
const PREFS_VERSION: u64 = 1;

/// The chat runtime the user chose for agent `id` (missing = auto).
pub fn pref(home: &Path, id: &str) -> Pref {
    let v: Value = crate::durable::read_or_default(&prefs_file(home), PREFS_VERSION);
    serde_json::from_value(v["chat"][id].clone()).unwrap_or_default()
}

/// Err when the file is a newer Maestro's (it's left as it is).
pub fn set_pref(home: &Path, id: &str, p: Pref) -> Result<(), String> {
    let file = prefs_file(home);
    let mut v: Value = crate::durable::read_for_write(&file, PREFS_VERSION)?;
    if !v.is_object() {
        v = json!({});
    }
    if !v["chat"].is_object() {
        v["chat"] = json!({});
    }
    v["chat"][id] = serde_json::to_value(p).unwrap();
    crate::durable::write_json(&file, &v, PREFS_VERSION)
}

// ---------- Node, the CLI, the adapter ----------

/// `node.exe` (never a `.cmd` shim: volta/nvm put those first on PATH) and its version.
pub fn find_node() -> Option<(PathBuf, String)> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut dirs: Vec<PathBuf> = std::env::split_paths(&path).filter(|d| !d.as_os_str().is_empty()).collect();
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    dirs.extend(env("ProgramFiles").map(|p| p.join("nodejs")));
    dirs.extend(env("LOCALAPPDATA").map(|p| p.join("Programs").join("nodejs")));
    dirs.extend(env("NVM_SYMLINK"));
    dirs.extend(env("LOCALAPPDATA").map(|p| p.join("Volta").join("bin")));
    let exe = if cfg!(windows) { "node.exe" } else { "node" };
    let node = dirs.into_iter().map(|d| d.join(exe)).find(|p| p.is_file())?;
    let version = cli::probe_version(&[node.to_string_lossy().into()], &["--version"])?;
    Some((node, version))
}

/// `v22.11.0` → `22.11.0`.
#[cfg(test)]
pub fn parse_node_version(s: &str) -> Option<String> {
    cli::parse_version(s)
}

/// The executable the adapter can start for the user's CLI: the `.exe` behind an npm shim, else the package's
/// `cli.js` next to the shim (the SDK runs `.js` with node). A `cmd.exe /c` line can't be handed over.
fn exe_from_argv(argv: &[String], npm_package: Option<&str>) -> Option<PathBuf> {
    match argv {
        [exe] => Some(PathBuf::from(exe)).filter(|p| !cfg!(windows) || p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"))),
        [cmd, c, call, shim] if cmd == "cmd.exe" && c == "/c" && call == "call" => {
            let pkg = npm_package?;
            let js = pkg.split('/').fold(Path::new(shim).parent()?.join("node_modules"), |p, s| p.join(s)).join("cli.js");
            js.is_file().then_some(js)
        }
        _ => None,
    }
}

pub fn cli_executable(def: &AgentDef) -> Result<PathBuf, FallbackReason> {
    let argv = cli::command(def).map_err(|_| FallbackReason::CliNotExecutable)?;
    exe_from_argv(&argv, def.npm_package()).ok_or(FallbackReason::CliNotExecutable)
}

/// `<base>/node_modules/<package>`
fn package_dir(base: &Path, package: &str) -> PathBuf {
    package.split('/').fold(base.join("node_modules"), |p, s| p.join(s))
}

/// Where the managed install of `acp` lives: `<app data>/acp/<name>@<version>`.
pub fn managed_dir(app_data: &Path, acp: &Acp) -> Option<PathBuf> {
    let name = acp.package?.rsplit('/').next()?;
    Some(app_data.join("acp").join(format!("{name}@{}", acp.version?)))
}

fn package_version(pkg_dir: &Path) -> Option<String> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(pkg_dir.join("package.json")).ok()?).ok()?;
    v["version"].as_str().map(String::from)
}

/// The adapter to run: the dev folder when given (any version, no receipt), else the managed install, which
/// counts only with its receipt, the pinned version and its entry file.
pub fn locate_adapter(acp: &Acp, app_data: Option<&Path>, env_dir: Option<PathBuf>) -> Adapter {
    let Some(package) = acp.package else { return Adapter::Missing };
    if let Some(dir) = env_dir {
        let pkg = package_dir(&dir, package);
        let entry = pkg.join(acp.entry);
        return match package_version(&pkg) {
            Some(version) if entry.is_file() => Adapter::Found { dir, entry, version, dev: true },
            _ => Adapter::Broken(format!("{}에 어댑터가 없어요", pkg.display())),
        };
    }
    let Some(base) = app_data.and_then(|a| managed_dir(a, acp)) else { return Adapter::Missing };
    if !base.exists() {
        return Adapter::Missing;
    }
    if !base.join(RECEIPT).is_file() {
        return Adapter::Broken("설치가 끝나지 않았어요".into());
    }
    let pkg = package_dir(&base, package);
    let entry = pkg.join(acp.entry);
    match package_version(&pkg) {
        Some(v) if Some(v.as_str()) == acp.version && entry.is_file() => Adapter::Found { dir: base, entry, version: v, dev: false },
        Some(v) if Some(v.as_str()) != acp.version => Adapter::Broken(format!("버전이 달라요: {v}")),
        _ => Adapter::Broken("어댑터 파일이 없어요".into()),
    }
}

fn dev_dir(acp: &Acp) -> Option<PathBuf> {
    acp.dev_dir_env.and_then(std::env::var_os).filter(|v| !v.is_empty()).map(PathBuf::from)
}

// ---------- caches ----------

static PROBE: LazyLock<Mutex<HashMap<&'static str, (Instant, Probe)>>> = LazyLock::new(Mutex::default);

#[derive(Default)]
struct Health {
    blocked: Option<FallbackReason>,
    crashes: Vec<Instant>,
}

impl Health {
    fn record(&mut self, reason: &FallbackReason) {
        if reason.blocks_agent() {
            self.blocked = Some(reason.clone());
        }
    }

    fn crash(&mut self, now: Instant) {
        self.crashes.retain(|t| now.saturating_duration_since(*t) < CRASH_WINDOW);
        self.crashes.push(now);
        if self.crashes.len() >= 2 {
            self.blocked = Some(FallbackReason::Crashed);
        }
    }
}

static HEALTH: LazyLock<Mutex<HashMap<String, Health>>> = LazyLock::new(Mutex::default);

/// Forgets the cached checks and the blocked starts ("다시 확인", an install, a changed preference).
pub fn invalidate() {
    invalidate_probe();
    clear_health();
}

/// Forgets the cached checks only (Settings opening: what's installed may have changed, a block stays).
pub fn invalidate_probe() {
    PROBE.lock().unwrap().clear();
}

/// Lifts the blocked starts: only what the user asked for ("다시 확인", an install, a changed preference).
pub fn clear_health() {
    HEALTH.lock().unwrap().clear();
}

/// The agent's checks, at most a minute old.
pub fn probe(def: &'static AgentDef) -> Probe {
    if let Some((t, p)) = PROBE.lock().unwrap().get(def.id) {
        if t.elapsed() < PROBE_TTL {
            return p.clone();
        }
    }
    let Some(acp) = def.acp_adapter() else { return Probe::default() };
    let adapter = locate_adapter(acp, app_data(), dev_dir(acp));
    let cli = cli_executable(def);
    // a `--version` run of the CLI only when ACP could start at all
    let cli_version = match (&adapter, &cli) {
        (Adapter::Found { .. }, Ok(_)) => cli::command(def).ok().and_then(|argv| cli::probe_version(&argv, def.version_args)),
        _ => None,
    };
    let p = Probe { node: find_node(), cli, cli_version, logged_in: cli::logged_in(def.login), adapter };
    PROBE.lock().unwrap().insert(def.id, (Instant::now(), p.clone()));
    p
}

/// The reason the agent's chat runtime is blocked this run, if any.
pub fn health(id: &str) -> Option<FallbackReason> {
    HEALTH.lock().unwrap().get(id).and_then(|h| h.blocked.clone())
}

/// A failed start of the whole agent: blocked until `invalidate` (other reasons are per worktree: ignored).
pub fn record(id: &str, reason: &FallbackReason) {
    HEALTH.lock().unwrap().entry(id.into()).or_default().record(reason);
}

/// A running adapter went away by itself; the second time within `CRASH_WINDOW` blocks the agent.
pub fn record_crash(id: &str) {
    HEALTH.lock().unwrap().entry(id.into()).or_default().crash(Instant::now());
}

/// `plan` for a real start: the saved choice, the cached checks, this run's failures.
pub fn plan_for(def: &'static AgentDef, home: &Path, cwd: &str, want: Want) -> Result<AcpPlan, FallbackReason> {
    let pref = pref(home, def.id);
    let acp = def.acp_adapter();
    // the cheap answers first, without running anything
    let probe = if pref == Pref::Pty || acp.is_none() || want == Want::Cli { Probe::default() } else { probe(def) };
    let health = health(def.id);
    plan(&PlanInput { acp, want, pref, probe: &probe, cwd, health: health.as_ref() })
}

/// The chat runtime's state for Settings.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AcpStatus {
    /// "ready" | "notInstalled" | "broken" | "nodeMissing" | "nodeTooOld" | "cliUnsupported" | "loggedOut" | "blocked"
    pub state: &'static str,
    /// why not ready, in Korean; "" when ready
    pub message: String,
    pub node: Option<String>,
    pub node_min: &'static str,
    /// what's installed (managed or dev)
    pub adapter_version: Option<String>,
    /// what the managed install puts there
    pub pinned: Option<&'static str>,
    /// the `MAESTRO_ACP_CLAUDE_DIR` folder is used
    pub dev: bool,
    /// "auto" | "pty"
    pub mode: Pref,
}

/// `cli_status`'s chat runtime part for one agent; None when it has no ACP runtime.
pub fn status(def: &'static AgentDef) -> Option<AcpStatus> {
    let acp = def.acp_adapter()?;
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    let p = probe(def);
    let mode = home.as_deref().map(|h| pref(h, def.id)).unwrap_or_default();
    let (adapter_version, dev) = match &p.adapter {
        Adapter::Found { version, dev, .. } => (Some(version.clone()), *dev),
        _ => (None, false),
    };
    let reason = match (&p.node, &p.adapter) {
        (None, _) => Some(FallbackReason::NodeMissing { need: acp.node_min.into() }),
        (Some((_, v)), _) if cli::older(v, acp.node_min) => Some(FallbackReason::NodeTooOld { found: v.clone(), need: acp.node_min.into() }),
        (_, Adapter::Missing) => Some(FallbackReason::AdapterMissing),
        (_, Adapter::Broken(why)) => Some(FallbackReason::AdapterBroken { why: why.clone() }),
        _ => match &p.cli {
            Err(r) => Some(r.clone()),
            Ok(_) => match &p.cli_version {
                Some(v) if cli::older(v, acp.min_version) => Some(FallbackReason::CliTooOld { found: v.clone(), need: acp.min_version.into() }),
                _ if p.logged_in == Some(false) => Some(FallbackReason::LoggedOut),
                _ => health(def.id),
            },
        },
    };
    let state = match &reason {
        None => "ready",
        Some(FallbackReason::NodeMissing { .. }) => "nodeMissing",
        Some(FallbackReason::NodeTooOld { .. }) => "nodeTooOld",
        Some(FallbackReason::AdapterMissing) => "notInstalled",
        Some(FallbackReason::AdapterBroken { .. }) => "broken",
        Some(FallbackReason::CliNotExecutable) | Some(FallbackReason::CliTooOld { .. }) => "cliUnsupported",
        Some(FallbackReason::LoggedOut) => "loggedOut",
        Some(_) => "blocked",
    };
    Some(AcpStatus {
        state,
        message: reason.and_then(|r| r.note()).unwrap_or_default(),
        node: p.node.map(|(_, v)| v),
        node_min: acp.node_min,
        adapter_version,
        pinned: acp.version,
        dev,
        mode,
    })
}

// ---------- environment ----------

/// Always taken out of the adapter's environment: a host Claude session's identity (Maestro run from one, Claude
/// Desktop), anything that would make node or the adapter behave differently, and the values set below.
const STRIP: [&str; 9] = [
    "CLAUDECODE",
    "CLAUDE_PID",
    // the SDK fills it only when unset; left set, the session would count as the host's
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_EXECUTABLE",
    "CLAUDE_CODE_SSE_PORT",
    "CLAUDE_AGENT_ACP_PROTOCOL",
    "CLAUDE_AGENT_LOGS",
    "NODE_OPTIONS",
    "ELECTRON_RUN_AS_NODE",
];

/// (vars to remove, vars to set after the removals) for the adapter. `ANTHROPIC_*` and the user's own
/// `CLAUDE_CODE_*` settings stay; a host session's are dropped (`host_session_env`). The PTY env isn't touched.
pub fn acp_env(vars: impl Iterator<Item = String>, exe: &Path, maestro: &[(String, String)], logs: Option<PathBuf>) -> (Vec<String>, Vec<(String, String)>) {
    let vars: Vec<String> = vars.collect();
    let mut remove: Vec<String> = STRIP.iter().map(|s| s.to_string()).collect();
    for k in crate::agent::host_session_env(vars.into_iter()) {
        if !remove.contains(&k) {
            remove.push(k);
        }
    }
    let mut set = vec![("CLAUDE_CODE_EXECUTABLE".to_string(), exe.to_string_lossy().to_string()), ("CLAUDE_AGENT_ACP_PROTOCOL".into(), "v1".into())];
    set.extend(maestro.iter().cloned());
    if let Some(l) = logs {
        set.push(("CLAUDE_AGENT_LOGS".into(), l.to_string_lossy().into()));
    }
    (remove, set)
}

/// The adapter's command line, with its environment and working folder.
pub fn adapter_command(node: &Path, entry: &Path, cwd: &Path, env: &(Vec<String>, Vec<(String, String)>)) -> Command {
    let mut cmd = Command::new(node);
    cmd.arg(entry).current_dir(cwd);
    for k in &env.0 {
        cmd.env_remove(k);
    }
    for (k, v) in &env.1 {
        cmd.env(k, v);
    }
    cmd
}

/// Starts the adapter and checks it speaks ACP v1 (`initialize`). The client is returned live; ignored messages.
pub fn handshake(cmd: Command, limit: Duration) -> Result<(std::sync::Arc<Client>, Value), String> {
    let client = Client::spawn(cmd, |c, inc| {
        if let crate::acp::Incoming::Request { id, .. } = inc {
            c.respond_error(&id, -32601, "unsupported");
        }
    }, |_| {})
    .map_err(|e| format!("어댑터를 실행하지 못했어요: {e}"))?;
    let init = client.request("initialize", initialize_params(), Some(limit)).map_err(|e| {
        client.kill();
        format!("어댑터가 응답하지 않아요: {}", e.message)
    })?;
    if init["protocolVersion"] != 1 {
        client.close(Duration::from_millis(500));
        return Err("프로토콜이 맞지 않아요".into());
    }
    Ok((client, init))
}

/// What Maestro tells the adapter about itself: no file system or terminal of its own (the CLI does those).
pub fn initialize_params() -> Value {
    json!({
        "protocolVersion": 1,
        "clientCapabilities": { "fs": { "readTextFile": false, "writeTextFile": false }, "terminal": false },
        "clientInfo": { "name": "maestro", "version": env!("CARGO_PKG_VERSION") },
    })
}

// ---------- the managed install ----------

#[derive(Debug, Clone, PartialEq)]
pub enum InstallEvent {
    /// one line of npm's output (or a step of ours)
    Line(String),
}

/// npm through the node.exe that will run the adapter: `<node dir>/node_modules/npm/bin/npm-cli.js`, else the
/// `npm.cmd` on PATH.
fn npm_command(node: &Path) -> Command {
    let cli = node.parent().map(|d| d.join("node_modules").join("npm").join("bin").join("npm-cli.js"));
    match cli.filter(|c| c.is_file()) {
        Some(js) => {
            let mut c = Command::new(node);
            c.arg(js);
            c
        }
        None if cfg!(windows) => {
            let mut c = Command::new("cmd.exe");
            c.args(["/c", "call", "npm.cmd"]);
            c
        }
        None => Command::new("npm"),
    }
}

/// Runs a command to its end, every output line to `progress`; killed on `cancel` or after `limit`.
fn run_logged(mut cmd: Command, cancel: &AtomicBool, limit: Duration, progress: &dyn Fn(InstallEvent)) -> Result<(), String> {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cli::quiet(&mut cmd).spawn().map_err(|e| format!("npm을 실행하지 못했어요: {e}"))?;
    let (tx, rx) = mpsc::channel::<String>();
    for out in [child.stdout.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>), child.stderr.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>)].into_iter().flatten() {
        let tx = tx.clone();
        // bytes, not `lines()`: a line that isn't UTF-8 (CP949 from cmd.exe or a lifecycle script) would end the
        // reader, and npm, writing into a pipe nobody reads, would die of EPIPE. Read to EOF, whatever comes.
        thread::spawn(move || {
            let mut r = BufReader::new(out);
            let mut buf = vec![];
            while r.read_until(b'\n', &mut buf).is_ok_and(|n| n > 0) {
                let _ = tx.send(String::from_utf8_lossy(&buf).trim_end_matches(['\r', '\n']).to_string());
                buf.clear();
            }
        });
    }
    drop(tx);
    let start = Instant::now();
    let mut last = String::new();
    let mut take = |l: String| {
        if !l.trim().is_empty() {
            last = l.clone();
            progress(InstallEvent::Line(l));
        }
    };
    loop {
        while let Ok(l) = rx.try_recv() {
            take(l);
        }
        if cancel.load(Ordering::SeqCst) || start.elapsed() > limit {
            crate::proc::kill_tree(child.id());
            let _ = child.kill();
            return Err(if cancel.load(Ordering::SeqCst) { "설치를 취소했어요".into() } else { "설치가 너무 오래 걸려 멈췄어요".into() });
        }
        match child.try_wait() {
            Ok(Some(s)) => {
                // every line the readers still hold, up to their EOF (both senders gone): the last one is the
                // error shown. Bounded: a process npm left behind may keep a pipe open.
                let until = Instant::now() + Duration::from_secs(2);
                while let Some(left) = until.checked_duration_since(Instant::now()) {
                    match rx.recv_timeout(left) {
                        Ok(l) => take(l),
                        Err(_) => break,
                    }
                }
                drop(take);
                return if s.success() { Ok(()) } else { Err(format!("npm이 실패했어요: {last}")) };
            }
            Ok(None) => thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

fn unix_secs() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// The receipt that makes a managed install count (`locate_adapter`).
fn write_receipt(dir: &Path, acp: &Acp, node_version: &str) -> Result<(), String> {
    let r = json!({
        "package": acp.package,
        "version": acp.version,
        "entry": acp.entry,
        "node": node_version,
        "omitOptional": true,
        "installedAt": unix_secs(),
    });
    std::fs::write(dir.join(RECEIPT), serde_json::to_string_pretty(&r).unwrap()).map_err(|e| e.to_string())
}

/// Moves a verified staging folder into place. A valid install already there wins (the staging is dropped); a
/// broken one is removed first, which fails while an adapter from it still runs.
fn finalize(staging: &Path, target: &Path, acp: &Acp) -> Result<&'static str, String> {
    let app_data = target.parent().and_then(Path::parent);
    if target.exists() {
        if matches!(locate_adapter(acp, app_data, None), Adapter::Found { .. }) {
            let _ = std::fs::remove_dir_all(staging);
            return Ok("이미 설치돼 있어요");
        }
        std::fs::remove_dir_all(target).map_err(|_| "실행 중인 채팅 에이전트를 끄고 다시 시도해 주세요".to_string())?;
    }
    // Windows: the checked adapter's files may stay open for a moment after it exits
    let mut tries = 0;
    loop {
        match std::fs::rename(staging, target) {
            Ok(()) => return Ok("설치했어요"),
            Err(_) if tries < 20 => {
                tries += 1;
                thread::sleep(Duration::from_millis(250));
            }
            Err(e) => return Err(format!("설치 폴더를 옮기지 못했어요: {e}")),
        }
    }
}

/// Installs `def`'s adapter into app data with the user's Node and npm (exact version, optional packages left out:
/// the user's own CLI runs, not the SDK's bundled one), verifies it, then moves it into place. Any failure leaves
/// nothing behind. Returns what to say when done.
pub fn install(def: &'static AgentDef, app_data: &Path, cancel: &AtomicBool, progress: &dyn Fn(InstallEvent)) -> Result<&'static str, String> {
    let acp = def.acp_adapter().ok_or("채팅 런타임이 없는 에이전트예요")?;
    let (Some(package), Some(version)) = (acp.package, acp.version) else { return Err("설치할 어댑터가 없어요".into()) };
    let (node, node_version) = find_node().ok_or(format!("Node.js {} 이상이 필요해요", major(acp.node_min)))?;
    if cli::older(&node_version, acp.node_min) {
        return Err(format!("Node.js {} 이상이 필요해요 (지금 {node_version})", major(acp.node_min)));
    }
    let cli_exe = cli_executable(def).map_err(|_| format!("{} 실행 파일을 찾지 못했어요. 먼저 설치해 주세요.", def.name))?;
    let target = managed_dir(app_data, acp).ok_or("설치 위치를 정할 수 없어요")?;
    let parent = target.parent().unwrap().to_path_buf();
    let nanos = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
    let staging = parent.join(format!(".staging-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let result = (|| {
        std::fs::write(staging.join("package.json"), r#"{"name":"maestro-acp-host","private":true}"#).map_err(|e| e.to_string())?;
        progress(InstallEvent::Line(format!("npm install {package}@{version}")));
        let mut npm = npm_command(&node);
        npm.args(["install", "--prefix"]).arg(&staging).args(["--omit=optional", "--no-audit", "--no-fund", "--save-exact"]).arg(format!("{package}@{version}"));
        npm.env("npm_config_update_notifier", "false").current_dir(&staging);
        run_logged(npm, cancel, INSTALL_LIMIT, progress)?;
        let pkg = package_dir(&staging, package);
        if package_version(&pkg).as_deref() != Some(version) {
            return Err("설치된 어댑터 버전이 달라요".to_string());
        }
        let entry = pkg.join(acp.entry);
        let said = cli::probe_version(&[node.to_string_lossy().into(), entry.to_string_lossy().into()], &["--version"]);
        if said.as_deref() != Some(version) {
            return Err(format!("어댑터가 실행되지 않아요 (--version: {})", said.unwrap_or_default()));
        }
        progress(InstallEvent::Line("initialize 확인 중…".into()));
        let env = acp_env(std::env::vars().map(|(k, _)| k), &cli_exe, &[], None);
        // not from inside the staging folder: a process working there would keep it from being renamed
        let (client, _) = handshake(adapter_command(&node, &entry, &std::env::temp_dir(), &env), Duration::from_secs(30))?;
        client.close(Duration::from_secs(2));
        write_receipt(&staging, acp, &node_version)?;
        finalize(&staging, &target, acp)
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    invalidate();
    result
}

/// Installs running now, per agent id, with their cancel flags.
static INSTALLS: LazyLock<Mutex<HashMap<String, std::sync::Arc<AtomicBool>>>> = LazyLock::new(Mutex::default);

/// Starts an install on its own thread (one per agent). `progress` gets ("line" | "done" | "failed", text).
pub fn start_install(def: &'static AgentDef, app_data: PathBuf, progress: impl Fn(&str, &str) + Send + 'static) -> Result<(), String> {
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    {
        let mut running = INSTALLS.lock().unwrap();
        if running.contains_key(def.id) {
            return Err("이미 설치 중이에요".into());
        }
        running.insert(def.id.into(), cancel.clone());
    }
    thread::spawn(move || {
        let r = install(def, &app_data, &cancel, &|InstallEvent::Line(l)| progress("line", &l));
        INSTALLS.lock().unwrap().remove(def.id);
        match r {
            Ok(done) => progress("done", done),
            Err(e) => progress("failed", &e),
        }
    });
    Ok(())
}

pub fn cancel_install(id: &str) {
    if let Some(c) = INSTALLS.lock().unwrap().get(id) {
        c.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;
    use FallbackReason::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("maestro-acpsetup-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn claude_acp() -> &'static Acp {
        catalog::get("claude-code").unwrap().acp_adapter().unwrap()
    }

    fn ready_probe() -> Probe {
        Probe {
            node: Some(("C:/node/node.exe".into(), "22.11.0".into())),
            cli: Ok("C:/npm/claude.exe".into()),
            cli_version: Some("2.1.302".into()),
            logged_in: Some(true),
            adapter: Adapter::Found { dir: "C:/a".into(), entry: "C:/a/index.js".into(), version: "0.102.0".into(), dev: false },
        }
    }

    fn input<'a>(probe: &'a Probe, health: Option<&'a FallbackReason>) -> PlanInput<'a> {
        PlanInput { acp: Some(claude_acp()), want: Want::Auto, pref: Pref::Auto, probe, cwd: "C:/w/agent-1", health }
    }

    #[test]
    fn plans_acp_or_says_why_not_in_order() {
        let p = ready_probe();
        let ok = plan(&input(&p, None)).unwrap();
        assert_eq!((ok.node.to_string_lossy().as_ref(), ok.cli_exe.to_string_lossy().as_ref(), ok.adapter_version.as_str(), ok.dev), ("C:/node/node.exe", "C:/npm/claude.exe", "0.102.0", false));
        // each check, alone
        let with = |f: &dyn Fn(&mut Probe)| {
            let mut p = ready_probe();
            f(&mut p);
            plan(&input(&p, None)).unwrap_err()
        };
        assert_eq!(with(&|p| p.cli = Err(CliNotExecutable)), CliNotExecutable);
        assert_eq!(with(&|p| p.cli_version = Some("2.1.100".into())), CliTooOld { found: "2.1.100".into(), need: "2.1.226".into() });
        assert_eq!(with(&|p| p.node = None), NodeMissing { need: "22.0.0".into() });
        assert_eq!(with(&|p| p.node = Some(("n".into(), "20.18.0".into()))), NodeTooOld { found: "20.18.0".into(), need: "22.0.0".into() });
        assert_eq!(with(&|p| p.adapter = Adapter::Missing), AdapterMissing);
        assert_eq!(with(&|p| p.adapter = Adapter::Broken("x".into())), AdapterBroken { why: "x".into() });
        assert_eq!(with(&|p| p.logged_in = Some(false)), LoggedOut);
        // what can't be known doesn't block
        assert!(plan(&input(&Probe { cli_version: None, logged_in: None, ..ready_probe() }, None)).is_ok());
        // the order: the user's choice before everything, then support, the CLI view, the CLI, Node, the adapter
        let broken = Probe { node: None, cli: Err(CliNotExecutable), adapter: Adapter::Missing, logged_in: Some(false), ..ready_probe() };
        assert_eq!(plan(&PlanInput { pref: Pref::Pty, acp: None, ..input(&broken, None) }).unwrap_err(), Forced);
        assert_eq!(plan(&PlanInput { acp: None, want: Want::Cli, ..input(&broken, None) }).unwrap_err(), NotSupported);
        assert_eq!(plan(&PlanInput { want: Want::Cli, ..input(&broken, None) }).unwrap_err(), CliView);
        assert_eq!(plan(&input(&broken, None)).unwrap_err(), CliNotExecutable);
        let no_node = Probe { node: None, adapter: Adapter::Missing, ..ready_probe() };
        assert_eq!(plan(&input(&no_node, None)).unwrap_err(), NodeMissing { need: "22.0.0".into() });
        // a folder name Claude hashes (over SLUG_MAX), checked before Node
        let at = |n: usize| format!("C:/{}", "a".repeat(n - 3));
        assert!(plan(&PlanInput { cwd: &at(crate::claude::SLUG_MAX), ..input(&p, None) }).is_ok());
        assert_eq!(plan(&PlanInput { cwd: &at(crate::claude::SLUG_MAX + 1), ..input(&no_node, None) }).unwrap_err(), TranscriptUnreachable);
        // a failure earlier this run blocks last
        assert_eq!(plan(&input(&p, Some(&Crashed))).unwrap_err(), Crashed);
        assert_eq!(plan(&input(&Probe { logged_in: Some(false), ..ready_probe() }, Some(&Crashed))).unwrap_err(), LoggedOut);
    }

    #[test]
    fn reasons_say_why_in_korean() {
        assert_eq!(NotSupported.note(), None);
        assert_eq!(CliView.note(), None);
        assert!(NodeMissing { need: "22.0.0".into() }.note().unwrap().contains("Node.js 22 이상"));
        assert!(AdapterMissing.note().unwrap().contains("설정 → 에이전트에서 설치"));
        assert!(StartFailed { message: "x".into() }.blocks_agent() && LoginRequired.blocks_agent() && Crashed.blocks_agent());
        assert!(!LoadFailed { message: "x".into() }.blocks_agent() && !ModeRejected { mode: "plan".into() }.blocks_agent() && !ModelRejected { model: "opus".into() }.blocks_agent());
        let v = NodeTooOld { found: "20.1.0".into(), need: "22.0.0".into() }.view();
        assert_eq!((v["kind"].as_str(), v["found"].as_str()), (Some("nodeTooOld"), Some("20.1.0")));
        assert!(v["message"].as_str().unwrap().contains("지금 20.1.0"));
        assert!(Forced.view()["message"].is_string() && CliView.view()["message"].is_null());
    }

    #[test]
    fn health_blocks_after_a_failed_start_or_two_crashes() {
        let mut h = Health::default();
        h.record(&LoadFailed { message: "x".into() });
        assert_eq!(h.blocked, None);
        h.record(&LoginRequired);
        assert_eq!(h.blocked, Some(LoginRequired));
        let mut h = Health::default();
        let t0 = Instant::now();
        h.crash(t0);
        assert_eq!(h.blocked, None);
        // a crash long after the first doesn't count it
        h.crash(t0 + CRASH_WINDOW + Duration::from_secs(1));
        assert_eq!(h.blocked, None);
        h.crash(t0 + CRASH_WINDOW + Duration::from_secs(5));
        assert_eq!(h.blocked, Some(Crashed));
    }

    #[test]
    fn parses_node_versions() {
        assert_eq!(parse_node_version("v22.11.0\n").as_deref(), Some("22.11.0"));
        assert_eq!(parse_node_version("v24.21.0").as_deref(), Some("24.21.0"));
        assert_eq!(parse_node_version("node"), None);
    }

    #[test]
    fn finds_the_cli_executable_behind_a_shim() {
        let d = temp("exe");
        let s = |p: &Path| p.to_string_lossy().to_string();
        let exe = d.join("claude.exe");
        assert_eq!(exe_from_argv(&[s(&exe)], None), Some(exe.clone()));
        let shim = d.join("claude.cmd");
        let line = |p: &Path| vec!["cmd.exe".to_string(), "/c".into(), "call".into(), s(p)];
        assert_eq!(exe_from_argv(&line(&shim), Some("@anthropic-ai/claude-code")), None);
        let js = d.join("node_modules").join("@anthropic-ai").join("claude-code").join("cli.js");
        std::fs::create_dir_all(js.parent().unwrap()).unwrap();
        std::fs::write(&js, "").unwrap();
        assert_eq!(exe_from_argv(&line(&shim), Some("@anthropic-ai/claude-code")), Some(js));
        assert_eq!(exe_from_argv(&line(&shim), None), None);
        if cfg!(windows) {
            assert_eq!(exe_from_argv(&[s(&d.join("claude.bat"))], None), None);
        }
        assert_eq!(exe_from_argv(&[], None), None);
        let _ = std::fs::remove_dir_all(d);
    }

    fn fake_package(base: &Path, version: &str) {
        let pkg = package_dir(base, "@agentclientprotocol/claude-agent-acp");
        std::fs::create_dir_all(pkg.join("dist")).unwrap();
        std::fs::write(pkg.join("package.json"), json!({"version": version}).to_string()).unwrap();
        std::fs::write(pkg.join("dist/index.js"), "").unwrap();
    }

    #[test]
    fn locates_the_dev_or_managed_adapter() {
        let d = temp("locate");
        let acp = claude_acp();
        // dev folder: any version, no receipt
        let dev = d.join("dev");
        assert!(matches!(locate_adapter(acp, None, Some(dev.clone())), Adapter::Broken(_)));
        fake_package(&dev, "0.200.0");
        assert!(matches!(locate_adapter(acp, None, Some(dev.clone())), Adapter::Found { dev: true, ref version, .. } if version == "0.200.0"));
        // managed: missing, then without its receipt, then the wrong version, then right
        let app = d.join("app");
        assert_eq!(locate_adapter(acp, Some(&app), None), Adapter::Missing);
        assert_eq!(locate_adapter(acp, None, None), Adapter::Missing);
        let base = managed_dir(&app, acp).unwrap();
        assert!(base.ends_with("acp/claude-agent-acp@0.102.0"));
        fake_package(&base, "0.101.0");
        assert_eq!(locate_adapter(acp, Some(&app), None), Adapter::Broken("설치가 끝나지 않았어요".into()));
        write_receipt(&base, acp, "22.0.0").unwrap();
        assert!(matches!(locate_adapter(acp, Some(&app), None), Adapter::Broken(w) if w.contains("0.101.0")));
        fake_package(&base, "0.102.0");
        assert!(matches!(locate_adapter(acp, Some(&app), None), Adapter::Found { dev: false, .. }));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn a_verified_staging_folder_moves_into_place() {
        let d = temp("finalize");
        let acp = claude_acp();
        let app = d.join("app");
        let target = managed_dir(&app, acp).unwrap();
        let stage = |n: &str| {
            let s = target.parent().unwrap().join(n);
            fake_package(&s, "0.102.0");
            write_receipt(&s, acp, "22.0.0").unwrap();
            s
        };
        let s1 = stage(".staging-1");
        assert_eq!(finalize(&s1, &target, acp), Ok("설치했어요"));
        assert!(!s1.exists() && matches!(locate_adapter(acp, Some(&app), None), Adapter::Found { .. }));
        // a valid install already there wins; the staging goes
        let s2 = stage(".staging-2");
        assert_eq!(finalize(&s2, &target, acp), Ok("이미 설치돼 있어요"));
        assert!(!s2.exists());
        // a broken one is replaced
        std::fs::remove_file(target.join(RECEIPT)).unwrap();
        let s3 = stage(".staging-3");
        assert_eq!(finalize(&s3, &target, acp), Ok("설치했어요"));
        assert!(target.join(RECEIPT).is_file());
        let r: Value = serde_json::from_str(&std::fs::read_to_string(target.join(RECEIPT)).unwrap()).unwrap();
        assert_eq!((r["version"].as_str(), r["omitOptional"].as_bool()), (Some("0.102.0"), Some(true)));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn the_adapter_env_drops_host_values_and_keeps_the_users() {
        let vars = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>().into_iter();
        let maestro = [("MAESTRO_URL".to_string(), "u".to_string())];
        // no host session: the entrypoint still goes (the SDK only fills it when unset); ANTHROPIC stays
        let (rm, set) = acp_env(vars(&["CLAUDE_CODE_ENTRYPOINT", "ANTHROPIC_BASE_URL", "CLAUDE_CODE_USE_BEDROCK", "PATH"]), Path::new("C:/c.exe"), &maestro, None);
        assert!(rm.contains(&"CLAUDE_CODE_ENTRYPOINT".to_string()) && rm.contains(&"NODE_OPTIONS".to_string()));
        assert!(!rm.iter().any(|k| k.starts_with("ANTHROPIC") || k == "CLAUDE_CODE_USE_BEDROCK" || k == "PATH"));
        assert_eq!(set[0], ("CLAUDE_CODE_EXECUTABLE".into(), "C:/c.exe".into()));
        assert_eq!(set[1], ("CLAUDE_AGENT_ACP_PROTOCOL".into(), "v1".into()));
        assert!(set.contains(&maestro[0]) && !set.iter().any(|(k, _)| k == "CLAUDE_AGENT_LOGS"));
        // inside a host Claude session its own values go too, user config stays
        let (rm, _) = acp_env(vars(&["CLAUDECODE", "CLAUDE_CODE_SESSION_ID", "CLAUDE_CODE_GIT_BASH_PATH"]), Path::new("x"), &[], Some("C:/logs".into()));
        assert!(rm.contains(&"CLAUDE_CODE_SESSION_ID".to_string()) && !rm.contains(&"CLAUDE_CODE_GIT_BASH_PATH".to_string()));
        // the command applies removals before the values it sets
        let env = acp_env(vars(&["CLAUDE_CODE_EXECUTABLE"]), Path::new("C:/c.exe"), &[], None);
        let cmd = adapter_command(Path::new("node"), Path::new("e.js"), Path::new("."), &env);
        let got: HashMap<String, Option<String>> = cmd.get_envs().map(|(k, v)| (k.to_string_lossy().into(), v.map(|v| v.to_string_lossy().into()))).collect();
        assert_eq!(got.get("CLAUDE_CODE_EXECUTABLE"), Some(&Some("C:/c.exe".to_string())));
        assert_eq!(got.get("CLAUDE_CODE_ENTRYPOINT"), Some(&None));
    }

    #[test]
    fn the_preference_is_saved_per_agent() {
        let home = temp("pref");
        assert_eq!(pref(&home, "claude-code"), Pref::Auto);
        set_pref(&home, "claude-code", Pref::Pty).unwrap();
        set_pref(&home, "other", Pref::Auto).unwrap();
        assert_eq!(pref(&home, "claude-code"), Pref::Pty);
        let v: Value = serde_json::from_str(&std::fs::read_to_string(prefs_file(&home)).unwrap()).unwrap();
        assert_eq!(v, json!({"chat": {"claude-code": "pty", "other": "auto"}, "version": 1}));
        let _ = std::fs::remove_dir_all(home);
    }

    /// The managed install for real (network, ~30 MB): `MAESTRO_ACP_LIVE_INSTALL=1 cargo test -- --ignored acp_live_install --nocapture`
    #[test]
    #[ignore]
    fn acp_live_install() {
        if std::env::var("MAESTRO_ACP_LIVE_INSTALL").as_deref() != Ok("1") {
            println!("skipped: set MAESTRO_ACP_LIVE_INSTALL=1");
            return;
        }
        let app = temp("live-install");
        let def = catalog::get("claude-code").unwrap();
        let r = install(def, &app, &AtomicBool::new(false), &|InstallEvent::Line(l)| println!("npm> {l}"));
        println!("result: {r:?}");
        assert!(r.is_ok());
        let base = managed_dir(&app, def.acp_adapter().unwrap()).unwrap();
        assert!(base.join(RECEIPT).is_file());
        // the SDK's bundled CLI (~331 MB) was left out
        assert!(!package_dir(&base, "@anthropic-ai/claude-agent-sdk-win32-x64").exists());
        assert!(matches!(locate_adapter(def.acp_adapter().unwrap(), Some(&app), None), Adapter::Found { .. }));
        let _ = std::fs::remove_dir_all(app);
    }
}
