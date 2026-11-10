//! Finding the agent CLIs of the catalog: the executable's path, its version, whether it's logged in, and the
//! status that adds up to. Plus the command that installs + first-runs one in a setup terminal.
//! Cheap on purpose: one `--version` run per CLI, login read from files (no network, nothing written).

use crate::catalog::{self, AgentDef, Login, Models};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Generous: the first run after boot can be slow (a virus scan of a fresh exe); usually under a second.
const VERSION_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Serialize, Debug, PartialEq, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    /// 미설치
    Missing,
    /// 버전 낮음
    TooOld,
    /// 로그인 필요
    LoginRequired,
    /// 준비됨
    Ready,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCli {
    id: &'static str,
    status: Status,
    installed: bool,
    /// the executable that runs (after skipping an npm shim), None when missing
    path: Option<String>,
    /// None also when `--version` failed or said nothing parseable; that doesn't block
    version: Option<String>,
    min_version: Option<&'static str>,
    /// None = couldn't tell from files (macOS keychain, a keyring store)
    logged_in: Option<bool>,
    install_cmd: String,
    /// why it isn't ready, in Korean; "" when ready
    message: String,
    /// the chat runtime (ACP adapter) for an agent that has one; None for the others
    acp: Option<crate::acp_setup::AcpStatus>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliStatus {
    agents: Vec<AgentCli>,
    /// npm (Node.js) present, so the install button can work
    npm: bool,
    /// the Node.js version the chat runtime would run on; None = no node.exe found
    node: Option<String>,
}

pub fn quiet(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

fn home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

/// `bin` in `dirs`, in order. Windows: `.exe` first, then npm's `.cmd`/`.bat` shims; never the extensionless
/// sh shim npm drops beside them (not a PE file) or `.ps1` (Nimbalyst claudeExecutableResolver.ts does the same).
pub(crate) fn find_in(bin: &str, dirs: impl IntoIterator<Item = PathBuf>, windows: bool) -> Option<PathBuf> {
    let exts: &[&str] = if windows { &[".exe", ".cmd", ".bat"] } else { &[""] };
    dirs.into_iter().find_map(|d| exts.iter().map(|e| d.join(format!("{bin}{e}"))).find(|p| p.is_file()))
}

/// Where `def`'s CLI is: PATH first (what a terminal would run), then its usual install folders, since an app
/// started before an install (or from Finder) can have a PATH without them.
pub fn find(def: &AgentDef) -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let extra = home().map(|h| def.dirs.iter().map(|d| h.join(d)).collect::<Vec<_>>()).unwrap_or_default();
    let dirs = std::env::split_paths(&path).filter(|d| !d.as_os_str().is_empty()).chain(extra);
    find_in(def.bin, dirs, cfg!(windows))
}

/// The argv prefix that starts `exe`. An npm `.cmd` shim only runs the package's own exe next to it, so that is
/// started directly when present (T3 Code ClaudeExecutable.ts); anything else goes through `cmd.exe /c`, since
/// CreateProcess can't start a `.cmd`. `call` first: a line after `/c` that starts with a quote (a shim path
/// with a space) loses its first and last quote once a later argument is quoted too.
/// shortcut: `call` doubles any `^` in the arguments; none of ours have one.
fn launcher(exe: &Path, bin: &str, npm_package: Option<&str>) -> Vec<String> {
    let ext = exe.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if ext != "cmd" && ext != "bat" {
        return vec![exe.to_string_lossy().into()];
    }
    if let (Some(dir), Some(pkg)) = (exe.parent(), npm_package) {
        let inner = pkg.split('/').fold(dir.join("node_modules"), |p, s| p.join(s)).join("bin").join(format!("{bin}.exe"));
        if inner.is_file() {
            return vec![inner.to_string_lossy().into()];
        }
    }
    vec!["cmd.exe".into(), "/c".into(), "call".into(), exe.to_string_lossy().into()]
}

/// What starts `def`'s CLI, or why it can't: the classified "executable missing" launch failure.
pub fn command(def: &AgentDef) -> Result<Vec<String>, String> {
    let exe = find(def).ok_or_else(|| format!("{} 실행 파일({})을 찾지 못했어요. 설정 → 에이전트에서 설치하거나 다시 확인해 주세요.", def.name, def.bin))?;
    Ok(launcher(&exe, def.bin, def.npm_package()))
}

/// The first `x.y.z` in the output.
pub(crate) fn parse_version(s: &str) -> Option<String> {
    s.split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .map(|t| t.trim_matches('.'))
        .find(|t| t.split('.').count() >= 3 && t.split('.').take(3).all(|p| !p.is_empty()))
        .map(|t| t.split('.').take(3).collect::<Vec<_>>().join("."))
}

fn numbers(v: &str) -> Vec<u64> {
    v.split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

pub(crate) fn older(found: &str, min: &str) -> bool {
    numbers(found) < numbers(min)
}

/// `<cli> --version`, killed after `VERSION_TIMEOUT`. Prints and exits; doesn't touch the network or config.
pub(crate) fn probe_version(argv: &[String], args: &[&str]) -> Option<String> {
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..]).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = quiet(&mut cmd).spawn().ok()?;
    let deadline = Instant::now() + VERSION_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                return None;
            }
        }
    }
    let out = child.wait_with_output().ok()?;
    parse_version(&String::from_utf8_lossy(&out.stdout)).or_else(|| parse_version(&String::from_utf8_lossy(&out.stderr)))
}

fn env_set(k: &str) -> bool {
    std::env::var_os(k).is_some_and(|v| !v.is_empty())
}

fn read_json(p: &Path) -> Option<serde_json::Value> {
    serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()
}

/// Claude Code: an API key / token / cloud provider in the env, OAuth tokens in `.credentials.json` (Windows,
/// Linux), or the account `~/.claude.json` keeps once logged in (macOS keeps the tokens in the keychain).
fn claude_logged_in(home: &Path, env: &dyn Fn(&str) -> bool) -> Option<bool> {
    let keys = ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "CLAUDE_CODE_OAUTH_TOKEN", "CLAUDE_CODE_USE_BEDROCK", "CLAUDE_CODE_USE_VERTEX"];
    if keys.iter().any(|k| env(k)) {
        return Some(true);
    }
    let dir = std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).unwrap_or_else(|| home.join(".claude"));
    if read_json(&dir.join(".credentials.json")).is_some_and(|c| c["claudeAiOauth"].is_object()) {
        return Some(true);
    }
    // with CLAUDE_CONFIG_DIR the config file moves into it
    let config = [dir.join(".claude.json"), home.join(".claude.json")].into_iter().find_map(|p| read_json(&p));
    if config.is_some_and(|c| c["oauthAccount"].is_object() || c["primaryApiKey"].is_string()) {
        return Some(true);
    }
    Some(false)
}

/// Codex: `OPENAI_API_KEY`, or `auth.json` in `CODEX_HOME` (`~/.codex`). Unknown when it keeps them in the keyring.
fn codex_logged_in(home: &Path, env: &dyn Fn(&str) -> bool) -> Option<bool> {
    if env("OPENAI_API_KEY") {
        return Some(true);
    }
    let dir = std::env::var_os("CODEX_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".codex"));
    if read_json(&dir.join("auth.json")).is_some_and(|a| a["tokens"].is_object() || a["OPENAI_API_KEY"].is_string()) {
        return Some(true);
    }
    let keyring = std::fs::read_to_string(dir.join("config.toml")).is_ok_and(|c| c.contains("keyring"));
    if keyring { None } else { Some(false) }
}

/// Antigravity keeps its Google token in the Windows Credential Manager (`gemini:antigravity`), elsewhere in
/// the keyring or `antigravity-oauth-token` (where Orca's antigravity-host-acp.ts looks).
fn antigravity_logged_in(home: &Path) -> Option<bool> {
    #[cfg(windows)]
    {
        let _ = home;
        Some(windows_credential_exists("gemini:antigravity"))
    }
    #[cfg(not(windows))]
    {
        // a keyring entry can't be checked without asking for keychain access
        home.join(".gemini").join("antigravity-cli").join("antigravity-oauth-token").is_file().then_some(true)
    }
}

/// Whether a generic credential exists. Reads it only to free it again; the secret is never looked at.
#[cfg(windows)]
fn windows_credential_exists(target: &str) -> bool {
    use std::ffi::c_void;
    #[link(name = "advapi32")]
    extern "system" {
        fn CredReadW(target: *const u16, kind: u32, flags: u32, credential: *mut *mut c_void) -> i32;
        fn CredFree(buffer: *mut c_void);
    }
    const CRED_TYPE_GENERIC: u32 = 1;
    let wide: Vec<u16> = target.encode_utf16().chain([0]).collect();
    let mut cred = std::ptr::null_mut();
    // SAFETY: a NUL-terminated target, an out pointer CredReadW fills only on success, freed once
    unsafe {
        if CredReadW(wide.as_ptr(), CRED_TYPE_GENERIC, 0, &mut cred) == 0 {
            return false;
        }
        CredFree(cred);
    }
    true
}

pub fn logged_in(login: Login) -> Option<bool> {
    let home = home()?;
    match login {
        Login::Claude => claude_logged_in(&home, &env_set),
        Login::Codex => codex_logged_in(&home, &env_set),
        Login::Antigravity => antigravity_logged_in(&home),
    }
}

/// The status decision. An unknown version or login state doesn't block: only what is known to be wrong does.
pub fn decide(found: bool, version: Option<&str>, min: Option<&str>, logged_in: Option<bool>) -> Status {
    if !found {
        return Status::Missing;
    }
    if let (Some(v), Some(m)) = (version, min) {
        if older(v, m) {
            return Status::TooOld;
        }
    }
    if logged_in == Some(false) {
        return Status::LoginRequired;
    }
    Status::Ready
}

fn message(def: &AgentDef, status: Status, version: Option<&str>) -> String {
    match status {
        Status::Missing => format!("{} 실행 파일({})이 없어요.", def.name, def.bin),
        Status::TooOld => format!("{} {} 이상이 필요해요. 지금은 {}이에요.", def.name, def.min_version.unwrap_or("?"), version.unwrap_or("?")),
        Status::LoginRequired => format!("{}에 로그인돼 있지 않아요. 터미널에서 로그인해 주세요.", def.name),
        Status::Ready => String::new(),
    }
}

fn check(def: &'static AgentDef) -> AgentCli {
    let exe = find(def);
    let version = exe.as_ref().and_then(|e| probe_version(&launcher(e, def.bin, def.npm_package()), def.version_args));
    let logged_in = exe.as_ref().and_then(|_| logged_in(def.login));
    let status = decide(exe.is_some(), version.as_deref(), def.min_version, logged_in);
    AgentCli {
        id: def.id,
        status,
        installed: exe.is_some(),
        path: exe.map(|e| launcher(&e, def.bin, def.npm_package()).pop().unwrap_or_default()),
        message: message(def, status, version.as_deref()),
        version,
        min_version: def.min_version,
        logged_in,
        install_cmd: def.npm_package().map(|p| format!("npm install -g {p}")).unwrap_or_default(),
        acp: crate::acp_setup::status(def),
    }
}

/// Every catalog agent, checked in parallel (each `--version` is a process start). The chat runtime's cached checks
/// are dropped first; `recheck` ("다시 확인") also lifts a blocked start (a crash loop, a failed start), which a
/// plain look (Settings opening, the app starting) leaves as it is.
pub fn status(recheck: bool) -> CliStatus {
    if recheck {
        crate::acp_setup::invalidate();
    } else {
        crate::acp_setup::invalidate_probe();
    }
    let agents = std::thread::scope(|s| {
        let jobs: Vec<_> = catalog::AGENTS.iter().map(|d| s.spawn(move || check(d))).collect();
        jobs.into_iter().map(|j| j.join().unwrap()).collect()
    });
    let npm = find_in("npm", std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()), cfg!(windows)).is_some();
    CliStatus { agents, npm, node: crate::acp_setup::find_node().map(|(_, v)| v) }
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelChoice {
    /// what `--model` / `/model` takes
    id: String,
    /// for the settings' model picker
    label: String,
    /// for the chat's `/model` list
    description: String,
}

/// Models `def` offers: its fixed list, or asked from the CLI (so it follows whatever the account has).
pub fn models(def: &AgentDef) -> Result<Vec<ModelChoice>, String> {
    match &def.models {
        None => Ok(vec![]),
        Some(Models::Fixed(list)) => Ok(list.iter().map(|&(id, d)| ModelChoice { id: id.into(), label: format!("{id} · {d}"), description: d.into() }).collect()),
        Some(Models::Command(args)) => {
            let exe = command(def)?;
            let mut cmd = Command::new(&exe[0]);
            cmd.args(&exe[1..]).args(*args).stdin(Stdio::null());
            let out = quiet(&mut cmd).output().map_err(|e| format!("{} 실행 실패: {e}", def.bin))?;
            Ok(parse_models(&String::from_utf8_lossy(&out.stdout)))
        }
    }
}

/// One tab-separated `id<TAB>label` per line (`agy models`); other lines are progress text.
fn parse_models(s: &str) -> Vec<ModelChoice> {
    s.lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(id, label)| ModelChoice { id: id.trim().into(), label: label.trim().into(), description: label.trim().into() })
        .collect()
}

/// The command the settings' setup terminal runs: install (if missing or too old), then the CLI itself so its
/// first-run screens — theme, login, trust — happen right there.
pub fn setup_argv(id: &str) -> Result<Vec<String>, String> {
    let def = catalog::require(id)?;
    let usable = find(def).is_some_and(|e| {
        let version = probe_version(&launcher(&e, def.bin, def.npm_package()), def.version_args);
        decide(true, version.as_deref(), def.min_version, None) != Status::TooOld
    });
    let line = match (usable, def.npm_package()) {
        (true, _) => def.login_cmd.to_string(),
        (false, Some(pkg)) => format!("npm install -g {pkg} && {}", def.login_cmd),
        (false, None) => return Err("npm으로 설치·업데이트할 수 없어요. 앱으로 설치하거나 업데이트해 주세요.".into()),
    };
    Ok(if cfg!(windows) { vec!["cmd.exe".into(), "/c".into(), line] } else { vec!["sh".into(), "-c".into(), line] })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("maestro-cli-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn status_lists_catalog_agents() {
        let s = status(false);
        assert_eq!(s.agents.iter().map(|a| a.id).collect::<Vec<_>>(), ["claude-code", "antigravity", "codex"]);
        assert_eq!(s.agents[0].install_cmd, "npm install -g @anthropic-ai/claude-code");
        assert_eq!(s.agents[1].install_cmd, "");
        assert!(s.agents.iter().all(|a| (a.status == Status::Missing) != a.installed));
        assert!(setup_argv("nope").is_err());
    }

    /// What this PC's CLIs come out as: `cargo test -- --ignored cli_status_live --nocapture`
    #[test]
    #[ignore]
    fn cli_status_live() {
        let t = Instant::now();
        println!("{}\n{:?}", serde_json::to_string_pretty(&status(false)).unwrap(), t.elapsed());
    }

    #[test]
    fn lists_models_per_agent() {
        let got = parse_models("Fetching available models...\ngemini-4-argon-high\tGemini 4 Argon (High)\nclaude-opus-5-5-low\tClaude Opus 5.5 (Low)\n");
        assert_eq!(got.iter().map(|m| (m.id.as_str(), m.label.as_str())).collect::<Vec<_>>(), [("gemini-4-argon-high", "Gemini 4 Argon (High)"), ("claude-opus-5-5-low", "Claude Opus 5.5 (Low)")]);
        let claude = models(catalog::get("claude-code").unwrap()).unwrap();
        assert_eq!((claude[2].id.as_str(), claude[2].label.as_str(), claude[2].description.as_str()), ("opus", "opus · Opus · 어렵고 긴 작업", "Opus · 어렵고 긴 작업"));
        assert!(models(catalog::get("codex").unwrap()).unwrap().is_empty());
    }

    #[test]
    fn decides_status() {
        assert_eq!(decide(false, None, Some("2.0.54"), Some(true)), Status::Missing);
        assert_eq!(decide(true, Some("2.0.9"), Some("2.0.54"), Some(true)), Status::TooOld);
        assert_eq!(decide(true, Some("1.9.99"), Some("2.0.0"), Some(false)), Status::TooOld); // too old wins over login
        assert_eq!(decide(true, Some("2.1.302"), Some("2.0.54"), Some(false)), Status::LoginRequired);
        assert_eq!(decide(true, Some("2.1.302"), Some("2.0.54"), Some(true)), Status::Ready);
        // what can't be known doesn't block
        assert_eq!(decide(true, None, Some("2.0.54"), None), Status::Ready);
        assert_eq!(decide(true, Some("0.1.0"), None, Some(true)), Status::Ready);
    }

    #[test]
    fn parses_and_compares_versions() {
        assert_eq!(parse_version("2.1.302 (Claude Code)").as_deref(), Some("2.1.302"));
        assert_eq!(parse_version("1.5.4\n").as_deref(), Some("1.5.4"));
        assert_eq!(parse_version("codex-cli 0.160.0-alpha.3").as_deref(), Some("0.160.0"));
        assert_eq!(parse_version("version 12 of 3.4"), None);
        assert!(older("2.0.9", "2.0.54") && older("1.4.12", "2.0.0") && !older("2.0.54", "2.0.54") && !older("2.10.0", "2.9.9"));
    }

    #[test]
    fn finds_windows_executables_before_shims() {
        let d = temp("find");
        let (a, b) = (d.join("a"), d.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("claude"), "#!/bin/sh").unwrap(); // npm's sh shim: never picked on Windows
        std::fs::write(a.join("claude.ps1"), "").unwrap();
        assert_eq!(find_in("claude", [a.clone(), b.clone()], true), None);
        std::fs::write(a.join("claude.cmd"), "").unwrap();
        std::fs::write(b.join("claude.exe"), "").unwrap();
        // the first folder wins, like PATH; within it .exe beats .cmd
        assert_eq!(find_in("claude", [a.clone(), b.clone()], true), Some(a.join("claude.cmd")));
        std::fs::write(a.join("claude.exe"), "").unwrap();
        assert_eq!(find_in("claude", [a.clone(), b.clone()], true), Some(a.join("claude.exe")));
        assert_eq!(find_in("claude", [a.clone()], false), Some(a.join("claude")));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn skips_npm_shim_for_package_exe() {
        let d = temp("shim");
        let shim = d.join("claude.cmd");
        std::fs::write(&shim, "").unwrap();
        let s = |p: &Path| p.to_string_lossy().to_string();
        assert_eq!(launcher(&shim, "claude", Some("@anthropic-ai/claude-code")), ["cmd.exe".to_string(), "/c".into(), "call".into(), s(&shim)]);
        let exe = d.join("node_modules").join("@anthropic-ai").join("claude-code").join("bin").join("claude.exe");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, "").unwrap();
        assert_eq!(launcher(&shim, "claude", Some("@anthropic-ai/claude-code")), [s(&exe)]);
        assert_eq!(launcher(&d.join("agy.exe"), "agy", None), [s(&d.join("agy.exe"))]);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    #[cfg(windows)]
    fn runs_shim_with_spaces_and_quoted_args() {
        // a home folder with a space: both the shim path and a later argument get quoted
        let d = temp("sp ace");
        let shim = d.join("echo.cmd");
        std::fs::write(&shim, "@echo off\r\necho [%~1] [%~2] [%~3]\r\n").unwrap();
        let a = launcher(&shim, "echo", None);
        let out = std::process::Command::new(&a[0]).args(&a[1..]).args(["--settings", r"C:\a b\s.json", ""]).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), r"[--settings] [C:\a b\s.json] []");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn reads_login_state_from_files() {
        let home = temp("login");
        let none = |_: &str| false;
        // CLAUDE_CONFIG_DIR / CODEX_HOME are assumed unset in the test environment
        assert_eq!(claude_logged_in(&home, &none), Some(false));
        assert_eq!(claude_logged_in(&home, &|k| k == "ANTHROPIC_API_KEY"), Some(true));
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        std::fs::write(home.join(".claude/.credentials.json"), r#"{"mcpOAuth":{}}"#).unwrap();
        assert_eq!(claude_logged_in(&home, &none), Some(false));
        std::fs::write(home.join(".claude.json"), r#"{"oauthAccount":{"emailAddress":"x"}}"#).unwrap();
        assert_eq!(claude_logged_in(&home, &none), Some(true));
        std::fs::remove_file(home.join(".claude.json")).unwrap();
        std::fs::write(home.join(".claude/.credentials.json"), r#"{"claudeAiOauth":{"refreshToken":"t"}}"#).unwrap();
        assert_eq!(claude_logged_in(&home, &none), Some(true));

        assert_eq!(codex_logged_in(&home, &none), Some(false));
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        std::fs::write(home.join(".codex/config.toml"), "cli_auth_credentials_store = \"keyring\"").unwrap();
        assert_eq!(codex_logged_in(&home, &none), None);
        std::fs::write(home.join(".codex/auth.json"), r#"{"tokens":{"id_token":"t"}}"#).unwrap();
        assert_eq!(codex_logged_in(&home, &none), Some(true));
        let _ = std::fs::remove_dir_all(home);
    }
}
