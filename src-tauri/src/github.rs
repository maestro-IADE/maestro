//! GitHub through the user's own `gh` (PR mode, R6/D7): finding gh, checking the project's remote and login,
//! classifying gh's failures, and the batched status query. The app never logs in, never stores a token and never
//! asks gh for one: `gh auth status` is the only auth command it runs. Every call goes through `exec::run`.

use crate::exec::{self, Cmd, Out};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

/// The oldest gh PR mode runs with (`--json` fields, `auth status --active`, `--match-head-commit`).
pub const MIN_GH: (u32, u32, u32) = (2, 40, 0);
const MISSING_GH: &str = "gh(GitHub CLI)가 없어요. cli.github.com에서 설치한 뒤 '다시 확인'을 누르세요. 설치했는데도 이렇게 나오면 앱을 다시 시작하세요 (실행 중인 앱은 설치 뒤 바뀐 PATH를 보지 못해요).";
/// Readiness is checked again after this long (ms), or at once with "다시 확인".
const READY_TTL: u64 = 5 * 60 * 1000;
const TRANSIENT_TTL: u64 = 20 * 1000;

/// Seconds each kind of call may take, and the pauses between recovery lookups (tests shrink them).
#[derive(Clone, Debug)]
pub struct Timeouts {
    pub git_local: f64,
    pub ls_remote: f64,
    pub fetch: f64,
    pub commit: f64,
    pub push: f64,
    pub gh_read: f64,
    pub graphql: f64,
    pub auth: f64,
    pub version: f64,
    pub create: f64,
    pub edit: f64,
    pub merge: f64,
    pub logs: f64,
    /// `ssh -G` (reads the ssh config only, never connects)
    pub ssh_config: f64,
    /// before each lookup after an unclear `gh pr create` (0 s, +2 s, +5 s)
    pub recover: Vec<Duration>,
    /// between re-reads of a mergeable state GitHub hasn't computed yet
    pub unknown_pause: Duration,
    /// before each retry of a read
    pub retries: Vec<Duration>,
}

impl Default for Timeouts {
    fn default() -> Self {
        Timeouts {
            git_local: 30.0,
            ls_remote: 60.0,
            fetch: 120.0,
            // the user's commit hooks run (lint, tests): they get time
            commit: 300.0,
            push: 180.0,
            gh_read: 30.0,
            graphql: 45.0,
            auth: 20.0,
            version: 10.0,
            create: 60.0,
            edit: 60.0,
            merge: 90.0,
            logs: 60.0,
            ssh_config: 10.0,
            recover: vec![Duration::ZERO, Duration::from_secs(2), Duration::from_secs(3)],
            unknown_pause: Duration::from_secs(2),
            retries: exec::READ_RETRIES.to_vec(),
        }
    }
}

/// Where gh is, the PATH children get, and the time limits.
#[derive(Clone, Debug)]
pub struct Tools {
    pub gh: Option<PathBuf>,
    /// the ssh that resolves host aliases (`ssh -G`); None: `ssh` on the PATH
    pub ssh: Option<PathBuf>,
    pub path_var: Option<OsString>,
    pub t: Timeouts,
}

impl Tools {
    /// The user's gh on the app's PATH (`MAESTRO_GH` overrides, for manual use), else where its installers put it:
    /// a running app never sees the PATH an installer changed after it started.
    pub fn system() -> Tools {
        let gh = std::env::var_os("MAESTRO_GH").map(PathBuf::from).filter(|p| p.is_file()).or_else(|| find_gh(&std::env::var_os("PATH").unwrap_or_default())).or_else(installed_gh);
        Tools { gh, ssh: None, path_var: None, t: Timeouts::default() }
    }

    pub fn git(&self, dir: &Path, args: &[&str], secs: f64) -> Cmd {
        Cmd::git(dir, args).timeout(secs).path(self.path_var.as_deref())
    }

    /// gh with `args`, from a neutral folder (every call names its repo with -R); None: gh isn't there.
    pub fn gh(&self, args: &[&str], secs: f64) -> Option<Cmd> {
        let gh = self.gh.as_ref()?;
        let mut c = Cmd::new(gh, args).timeout(secs).path(self.path_var.as_deref());
        c.cwd = Some(std::env::temp_dir());
        Some(c)
    }

    /// A read-only gh call, retried on transient failures.
    pub fn gh_read(&self, args: &[&str], secs: f64) -> Out {
        match self.gh(args, secs) {
            Some(c) => exec::run_read(c, &self.t.retries),
            None => missing_gh(),
        }
    }

    /// A gh call that writes: run once, never retried.
    pub fn gh_write(&self, args: &[&str], secs: f64) -> Out {
        match self.gh(args, secs) {
            Some(c) => exec::run(c),
            None => missing_gh(),
        }
    }
}

/// gh where the Windows installers put it (MSI per machine or per user, winget, scoop, chocolatey).
fn installed_gh() -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let places = [
        env("ProgramFiles").map(|p| p.join("GitHub CLI")),
        env("ProgramFiles(x86)").map(|p| p.join("GitHub CLI")),
        env("LOCALAPPDATA").map(|p| p.join("Programs").join("GitHub CLI")),
        env("LOCALAPPDATA").map(|p| p.join("Microsoft").join("WinGet").join("Links")),
        env("USERPROFILE").map(|p| p.join("scoop").join("shims")),
        env("ProgramData").map(|p| p.join("chocolatey").join("bin")),
    ];
    places.into_iter().flatten().map(|d| d.join("gh.exe")).find(|p| p.is_file())
}

fn missing_gh() -> Out {
    Out { spawn_err: Some(std::io::ErrorKind::NotFound), stderr: "gh not found".into(), ..Default::default() }
}

/// gh on `path_var`, directory by directory: gh.exe, gh.cmd, gh.bat on Windows, gh elsewhere.
pub fn find_gh(path_var: &OsStr) -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) { &["gh.exe", "gh.cmd", "gh.bat"] } else { &["gh"] };
    for dir in std::env::split_paths(path_var) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        for n in names {
            let p = dir.join(n);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// "gh version 2.102.0 (2026-09-30)" → (2, 102, 0)
pub fn parse_gh_version(text: &str) -> Option<(u32, u32, u32)> {
    let rest = text.lines().next()?.split("version").nth(1)?.trim();
    let v = rest.split_whitespace().next()?;
    let mut it = v.split('.').map(|p| p.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse::<u32>().ok());
    Some((it.next()??, it.next()??, it.next().flatten().unwrap_or(0)))
}

// ---------- remotes ----------

/// A remote URL as far as PR mode cares.
#[derive(Debug, PartialEq, Clone)]
pub enum RemoteUrl {
    Repo { host: String, owner: String, repo: String },
    /// a path or file:// URL: no host to make a PR on
    Local,
    Unknown,
}

/// https://HOST/OWNER/REPO(.git)(/), git@HOST:OWNER/REPO(.git), ssh://git@HOST(:port)/OWNER/REPO(.git); a path or
/// file:// is Local. The raw configured value is parsed (insteadOf not applied).
pub fn parse_remote_url(url: &str) -> RemoteUrl {
    let u = url.trim();
    let lower = u.to_lowercase();
    let bytes = u.as_bytes();
    let drive = bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && (bytes[2] == b'/' || bytes[2] == b'\\');
    if lower.starts_with("file://") || drive || u.starts_with('/') || u.starts_with('.') || u.starts_with('\\') || u.starts_with('~') {
        return RemoteUrl::Local;
    }
    let (authority, path) = if let Some(rest) = ["https://", "http://", "ssh://", "git+ssh://", "ssh+git://"].iter().find_map(|s| lower.starts_with(s).then(|| &u[s.len()..])) {
        match rest.split_once('/') {
            Some((a, p)) => (a.to_string(), p.to_string()),
            None => return RemoteUrl::Unknown,
        }
    } else if u.contains("://") {
        return RemoteUrl::Unknown;
    } else {
        // scp-like: [user@]host:path, with no slash before the colon
        match u.split_once(':') {
            Some((a, p)) if !a.contains('/') && !a.is_empty() => (a.to_string(), p.to_string()),
            _ => return RemoteUrl::Local,
        }
    };
    let host = authority.rsplit('@').next().unwrap_or("");
    let host = host.split(':').next().unwrap_or("").to_lowercase();
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let parts: Vec<&str> = path.split('/').collect();
    if host.is_empty() || parts.len() != 2 || parts.iter().any(|p| p.is_empty()) {
        return RemoteUrl::Unknown;
    }
    let host = if host == "ssh.github.com" { "github.com".to_string() } else { host };
    RemoteUrl::Repo { host, owner: parts[0].to_string(), repo: parts[1].to_string() }
}

/// The URL reaches its host over ssh: `ssh://…`, `git+ssh://…` or scp-like `[user@]host:path`.
pub fn is_ssh_url(url: &str) -> bool {
    let lower = url.trim().to_lowercase();
    if ["ssh://", "git+ssh://", "ssh+git://"].iter().any(|s| lower.starts_with(s)) {
        return true;
    }
    !lower.contains("://") && matches!(parse_remote_url(url), RemoteUrl::Repo { .. })
}

/// The `hostname` line of `ssh -G` output, lowercased; ssh.github.com is github.com (as in `parse_remote_url`).
pub fn ssh_hostname(stdout: &str) -> Option<String> {
    let h = stdout.lines().find_map(|l| l.trim().strip_prefix("hostname ").map(|h| h.trim().to_lowercase())).filter(|h| !h.is_empty())?;
    Some(if h == "ssh.github.com" { "github.com".into() } else { h })
}

/// An ssh host alias (`Host github-work` → `HostName github.com` in ~/.ssh/config, common with several GitHub
/// accounts) resolved the way gh does it: `ssh -G <alias>`, which prints the configuration and never connects. The
/// host itself when ssh isn't there, fails, or has no mapping.
pub fn resolve_ssh_host(tools: &Tools, host: &str) -> String {
    if host.is_empty() || host.starts_with('-') || is_github(host) {
        return host.to_string();
    }
    let program: &OsStr = tools.ssh.as_deref().map(Path::as_os_str).unwrap_or(OsStr::new("ssh"));
    let mut c = Cmd::new(program, &["-G", "-o", "BatchMode=yes", host]).timeout(tools.t.ssh_config).path(tools.path_var.as_deref());
    c.cwd = Some(std::env::temp_dir());
    let out = exec::run(c);
    if !out.ok() {
        return host.to_string();
    }
    ssh_hostname(&out.stdout).unwrap_or_else(|| host.to_string())
}

/// github.com or GitHub Enterprise Cloud with data residency (*.ghe.com). Other hosts count only when gh is
/// logged in to them (GitHub Enterprise Server).
pub fn is_github(host: &str) -> bool {
    host == "github.com" || host.ends_with(".ghe.com")
}

/// The remote PR mode pushes to: the base branch's own remote, else origin, else the only one.
pub fn pick_remote(tools: &Tools, root: &Path, base: &str) -> Option<String> {
    let cfg = exec::run(tools.git(root, &["config", "--get", &format!("branch.{base}.remote")], tools.t.git_local));
    if cfg.ok() && !cfg.text().is_empty() && cfg.text() != "." {
        return Some(cfg.text().to_string());
    }
    let list = exec::run(tools.git(root, &["remote"], tools.t.git_local));
    if !list.ok() {
        return None;
    }
    let names: Vec<&str> = list.stdout.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    if names.contains(&"origin") {
        return Some("origin".into());
    }
    (names.len() == 1).then(|| names[0].to_string())
}

// ---------- readiness ----------

/// Whether PR mode can run for a project, and why not.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PrReadiness {
    pub ok: bool,
    pub remote: Option<String>,
    pub host: String,
    pub owner: String,
    pub repo: String,
    /// "O/R" on github.com, "HOST/O/R" elsewhere: every gh call gets `-R <slug>`
    pub slug: String,
    /// "missing" | "loggedOut" | "unknown" | "ready" ("" before gh was looked at)
    pub gh: String,
    pub gh_version: Option<String>,
    pub merge_allowed: bool,
    pub squash_allowed: bool,
    /// the first failure, in Korean ("" when ok)
    pub message: String,
    pub checked_at: u64,
    /// the failure may pass by itself (network): cached only briefly
    #[serde(skip)]
    pub transient: bool,
}

impl PrReadiness {
    fn fail(mut self, msg: impl Into<String>) -> PrReadiness {
        self.ok = false;
        self.message = msg.into();
        self
    }

    /// `--hostname H` for `gh api` off github.com
    pub fn host_args(&self) -> Vec<String> {
        if self.host == "github.com" {
            vec![]
        } else {
            vec!["--hostname".into(), self.host.clone()]
        }
    }
}

static READY: Mutex<Option<HashMap<String, PrReadiness>>> = Mutex::new(None);

/// The full check (remote, URL, gh, login, repo, base on the remote), cached 5 minutes per project unless `recheck`.
pub fn readiness(tools: &Tools, root: &str, base: &str, recheck: bool, now: u64) -> PrReadiness {
    let k = format!("{}|{base}", crate::util::path_key(root));
    if !recheck {
        if let Some(r) = READY.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).get(&k) {
            if now.saturating_sub(r.checked_at) < if r.transient { TRANSIENT_TTL } else { READY_TTL } {
                return r.clone();
            }
        }
    }
    let mut r = check(tools, root, base, true);
    r.checked_at = now;
    READY.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(k, r.clone());
    r
}


/// a) remote b) its URL c) gh and its version d) gh's login for the host; with `full` also e) the repo through gh
/// and f) the base branch on the remote. Stops at the first failure.
pub fn check(tools: &Tools, root: &str, base: &str, full: bool) -> PrReadiness {
    let dir = Path::new(root);
    let r = PrReadiness::default();
    let Some(remote) = pick_remote(tools, dir, base) else {
        return r.fail("원격 저장소가 없어요. GitHub 저장소를 origin으로 연결해야 PR 모드를 쓸 수 있어요.");
    };
    let mut r = PrReadiness { remote: Some(remote.clone()), ..r };
    // every value of a multi-valued key, in order (`config --get` gives the last one; fetch, ls-remote and
    // `remote get-url` use the first, and `git push` pushes to all of them)
    let all = |key: String| -> Vec<String> {
        let o = exec::run(tools.git(dir, &["config", "--get-all", &key], tools.t.git_local));
        if o.ok() {
            o.stdout.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()
        } else {
            vec![]
        }
    };
    let urls = all(format!("remote.{remote}.url"));
    let url = urls.first().cloned().unwrap_or_default();
    // an ssh host alias still named as such after `ssh -G` (no ssh on the PATH, an alias only in a config ssh -G
    // doesn't read): gh can't log in to it, so the message says so instead of `gh auth login --hostname <alias>`
    let mut alias_kept = false;
    let (host, owner, repo) = match parse_remote_url(&url) {
        // an ssh host alias names the real host only in the ssh config
        RemoteUrl::Repo { host, owner, repo } if is_ssh_url(&url) => {
            let real = resolve_ssh_host(tools, &host);
            alias_kept = !is_github(&real) && real.eq_ignore_ascii_case(&host);
            (real, owner, repo)
        }
        RemoteUrl::Repo { host, owner, repo } => (host, owner, repo),
        RemoteUrl::Local => return r.fail("원격이 로컬 경로라 PR을 만들 수 없어요."),
        RemoteUrl::Unknown => return r.fail(format!("원격 주소를 읽지 못했어요 ({remote}). GitHub 저장소 주소(https 또는 ssh)여야 해요.")),
    };
    let same_repo = |u: &str| match parse_remote_url(u) {
        RemoteUrl::Repo { host: h, owner: o, repo: p } => {
            let h = if is_ssh_url(u) { resolve_ssh_host(tools, &h) } else { h };
            h.eq_ignore_ascii_case(&host) && o.eq_ignore_ascii_case(&owner) && p.eq_ignore_ascii_case(&repo)
        }
        _ => false,
    };
    // several `url`s: the push goes to each of them, the PR only to the first's repo
    if urls.iter().skip(1).any(|u| !same_repo(u)) {
        return r.fail(format!("원격 {remote}에 주소(url)가 여러 개이고 서로 다른 저장소를 가리켜요. 푸시가 그 모든 주소로 가요. PR을 만드는 저장소({owner}/{repo}) 하나만 남도록 `remote.{remote}.url`을 정리한 뒤 '다시 확인'을 누르세요."));
    }
    // `git push` uses `remote.<r>.pushurl` when set: a push to another repo (a fork, `no_push`) would never show on
    // the PR's repo
    let get = |push: bool| {
        let mut args = vec!["remote", "get-url"];
        if push {
            args.push("--push");
        }
        args.push(&remote);
        exec::run(tools.git(dir, &args, tools.t.git_local))
    };
    let (fetch_url, push_url) = (get(false), get(true));
    // every pushurl (a push goes to each), and the first as git rewrites it (`pushInsteadOf`)
    let other_push = all(format!("remote.{remote}.pushurl")).iter().any(|u| !same_repo(u));
    if other_push || (fetch_url.ok() && push_url.ok() && fetch_url.text() != push_url.text()) {
        let same = !other_push && (push_url.text() == fetch_url.text() || same_repo(push_url.text()));
        if !same {
            return r.fail(format!("푸시 주소가 다른 저장소예요 ({remote}의 pushurl). PR을 만드는 저장소({owner}/{repo})로 푸시하도록 `remote.{remote}.pushurl`을 고친 뒤 '다시 확인'을 누르세요."));
        }
    }
    r.slug = if host == "github.com" { format!("{owner}/{repo}") } else { format!("{host}/{owner}/{repo}") };
    (r.host, r.owner, r.repo) = (host.clone(), owner, repo);
    let Some(_) = &tools.gh else {
        r.gh = "missing".into();
        return r.fail(MISSING_GH);
    };
    let v = tools.gh_read(&["--version"], tools.t.version);
    if v.spawn_err.is_some() {
        r.gh = "missing".into();
        return r.fail(MISSING_GH);
    }
    let ver = parse_gh_version(&v.stdout);
    r.gh_version = ver.map(|(a, b, c)| format!("{a}.{b}.{c}"));
    match ver {
        Some(ver) if ver >= MIN_GH => {}
        _ => {
            r.gh = "unknown".into();
            let shown = r.gh_version.clone().unwrap_or_else(|| "알 수 없음".into());
            return r.fail(format!("gh 버전이 낮아요 ({shown}). 2.40 이상으로 올려 주세요."));
        }
    }
    match auth_status(tools, &host) {
        Auth::Ready => r.gh = "ready".into(),
        Auth::LoggedOut => {
            r.gh = "loggedOut".into();
            let msg = if is_github(&host) {
                "gh에 로그인돼 있지 않아요. 터미널에서 직접 `gh auth login`을 실행한 뒤 '다시 확인'을 누르세요 (앱은 로그인 정보를 보관하지 않아요).".to_string()
            } else if alias_kept {
                ssh_alias_message(&host, &r.owner, &r.repo)
            } else {
                format!("GitHub 저장소가 아니에요 ({host}). PR 모드는 GitHub에서만 써요. GitHub Enterprise라면 터미널에서 직접 `gh auth login --hostname {host}`을 실행한 뒤 '다시 확인'을 누르세요.")
            };
            return r.fail(msg);
        }
        Auth::Expired if alias_kept => {
            r.gh = "loggedOut".into();
            let msg = ssh_alias_message(&host, &r.owner, &r.repo);
            return r.fail(msg);
        }
        Auth::Expired => {
            r.gh = "loggedOut".into();
            return r.fail(format!("gh 로그인이 만료됐거나 토큰이 올바르지 않아요 ({host}). 터미널에서 직접 `gh auth login --hostname {host}`을 실행한 뒤 '다시 확인'을 누르세요 (앱은 로그인 정보를 보관하지 않아요)."));
        }
        Auth::Unknown => {
            r.gh = "unknown".into();
            r.transient = true;
            return r.fail("gh 로그인 상태를 확인하지 못했어요 (네트워크를 확인해 주세요)");
        }
    }
    if !full {
        r.ok = true;
        return r;
    }
    let view = tools.gh_read(&["repo", "view", &r.slug, "--json", "nameWithOwner,viewerPermission,mergeCommitAllowed,squashMergeAllowed,isArchived"], tools.t.gh_read);
    if !view.ok() {
        return match classify(&view) {
            GhErr::NotFound => r.fail("gh로 이 저장소를 찾지 못했어요."),
            GhErr::Auth => r.fail("gh에 로그인돼 있지 않아요. 터미널에서 직접 `gh auth login`을 실행한 뒤 '다시 확인'을 누르세요 (앱은 로그인 정보를 보관하지 않아요)."),
            _ => {
                r.transient = true;
                r.fail(format!("저장소 정보를 확인하지 못했어요: {}", view.tail()))
            }
        };
    }
    let v: Value = serde_json::from_str(view.text()).unwrap_or(Value::Null);
    if v.is_null() {
        return r.fail("gh로 이 저장소를 찾지 못했어요.");
    }
    if v["isArchived"].as_bool() == Some(true) {
        return r.fail("보관된 저장소예요.");
    }
    if matches!(v["viewerPermission"].as_str(), Some("READ" | "TRIAGE") | None) {
        return r.fail("이 저장소에 푸시 권한이 없어요.");
    }
    r.merge_allowed = v["mergeCommitAllowed"].as_bool().unwrap_or(true);
    r.squash_allowed = v["squashMergeAllowed"].as_bool().unwrap_or(true);
    let ls = exec::run(tools.git(dir, &["ls-remote", "--heads", &remote, &format!("refs/heads/{base}")], tools.t.ls_remote).network());
    if !ls.ok() {
        r.transient = true;
        return r.fail(format!("원격({remote})에 연결하지 못했어요: {}", ls.tail()));
    }
    if ls.text().is_empty() {
        return r.fail(format!("원격에 기준 브랜치({base})가 없어요. 먼저 기준 브랜치를 푸시해 주세요."));
    }
    r.ok = true;
    r
}

/// The ssh remote's host is not GitHub as far as the app can tell: an Enterprise server, or an ssh host alias whose
/// real host `ssh -G` couldn't read (no ssh on the PATH, an alias only in a config given by `core.sshCommand -F`, a
/// PuTTY session).
fn ssh_alias_message(host: &str, owner: &str, repo: &str) -> String {
    format!(
        "원격의 ssh 호스트({host})가 GitHub로 확인되지 않아요. ssh 호스트 별칭이라면 앱이 `ssh -G {host}`로 실제 호스트를 읽지 못한 거예요 (ssh가 PATH에 없거나, 별칭이 core.sshCommand의 -F 설정 등에만 있을 때). 원격을 https 주소(https://github.com/{owner}/{repo}.git)로 바꾸거나, ~/.ssh/config에 `Host {host}`와 `HostName github.com`을 적은 뒤 '다시 확인'을 누르세요. GitHub Enterprise 서버라면 터미널에서 직접 `gh auth login --hostname {host}`을 실행하세요."
    )
}

enum Auth {
    Ready,
    /// no account for the host
    LoggedOut,
    /// an account whose token GitHub refuses (401)
    Expired,
    /// can't tell (network, timeout): never reported as logged out
    Unknown,
}

/// `gh auth status --hostname H --active` (without --active on a gh that doesn't know it). Reads only: no token
/// is shown or kept. Logged out only when gh says no account is set up. "Failed to log in … token … is invalid"
/// is also what gh prints offline (it can't validate the token), so `gh api rate_limit` (no quota, no token output)
/// then tells the network apart from a token GitHub refuses.
fn auth_status(tools: &Tools, host: &str) -> Auth {
    let mut out = tools.gh_read(&["auth", "status", "--hostname", host, "--active"], tools.t.auth);
    if out.stderr.to_lowercase().contains("unknown flag") {
        out = tools.gh_read(&["auth", "status", "--hostname", host], tools.t.auth);
    }
    if out.timed_out || out.spawn_err.is_some() {
        return Auth::Unknown;
    }
    if out.ok() {
        return Auth::Ready;
    }
    let text = format!("{}\n{}", out.stderr, out.stdout).to_lowercase();
    if network_text(&text) {
        return Auth::Unknown;
    }
    if ["not logged in", "not logged into", "no accounts"].iter().any(|k| text.contains(k)) {
        return Auth::LoggedOut;
    }
    if !(text.contains("failed to log in") || text.contains("invalid") || out.code == Some(4)) {
        return Auth::Unknown;
    }
    let mut args = vec!["api", "rate_limit"];
    if host != "github.com" {
        args.extend(["--hostname", host]);
    }
    let probe = tools.gh_read(&args, tools.t.auth);
    let ptext = format!("{}\n{}", probe.stderr, probe.stdout).to_lowercase();
    if probe.ok() {
        Auth::Ready
    } else if probe.timed_out || probe.spawn_err.is_some() || network_text(&ptext) {
        Auth::Unknown
    } else if ptext.contains("http 401") || ptext.contains("bad credentials") {
        Auth::Expired
    } else {
        Auth::Unknown
    }
}

fn network_text(t: &str) -> bool {
    let t = t.to_lowercase();
    ["error connecting", "dial tcp", "no such host", "i/o timeout", "connection refused", "connection reset", "tls handshake", "network is unreachable", "timeout", "could not resolve host"].iter().any(|k| t.contains(k))
}

// ---------- failures ----------

/// What a failed gh call means, checked in this order.
#[derive(Debug, Clone, PartialEq)]
pub enum GhErr {
    /// gh didn't start: certainly nothing happened
    SpawnFailed,
    Auth,
    RateLimited,
    /// a PR for the head exists; its number when gh printed the URL
    AlreadyExists(Option<u64>),
    Validation,
    NotFound,
    /// GitHub got the request and said no (`GraphQL: …` with no sign of a network or server fault): nothing happened
    Refused,
    /// may or may not have happened: a write is reconciled with a read
    Ambiguous,
}

pub fn classify(out: &Out) -> GhErr {
    if out.spawn_err.is_some() {
        return GhErr::SpawnFailed;
    }
    let raw = format!("{}\n{}", out.stderr, out.stdout);
    let t = raw.to_lowercase();
    if out.timed_out {
        return GhErr::Ambiguous;
    }
    if out.code == Some(4) || ["not logged", "authentication", "gh auth login", "http 401", "bad credentials"].iter().any(|k| t.contains(k)) {
        return GhErr::Auth;
    }
    if rate_limited(&t) {
        return GhErr::RateLimited;
    }
    if t.contains("already exists") {
        return GhErr::AlreadyExists(pr_number_in(&raw));
    }
    if ["validation failed", "http 422", "no commits between"].iter().any(|k| t.contains(k)) {
        return GhErr::Validation;
    }
    if ["no pull requests found", "could not resolve", "http 404"].iter().any(|k| t.contains(k)) && !network_text(&t) {
        return GhErr::NotFound;
    }
    // `pull request create failed: GraphQL: Draft pull requests are not supported …`: an answer, unless GitHub or the
    // way there failed while answering
    if t.contains("graphql:") && !network_text(&t) && !["something went wrong", "timed out", "http 5", "try again", "server error"].iter().any(|k| t.contains(k)) {
        return GhErr::Refused;
    }
    GhErr::Ambiguous
}

/// `api rate limit (already )exceeded`, `secondary rate limit`, `abuse detection`, `http 429` (checked before a 403
/// is anything else).
fn rate_limited(lower: &str) -> bool {
    lower.contains("api rate limit exceeded") || lower.contains("api rate limit already exceeded") || lower.contains("secondary rate limit") || lower.contains("abuse detection") || lower.contains("http 429")
}

/// The number of the first `https?://host/o/r/pull/N` in `text`.
pub fn pr_number_in(text: &str) -> Option<u64> {
    pr_url_in(text).map(|(_, n)| n)
}

/// The first `https?://HOST/O/R/pull/N` in `text`, with N.
pub fn pr_url_in(text: &str) -> Option<(String, u64)> {
    for word in text.split_whitespace() {
        let w = word.trim_matches(|c: char| c == '"' || c == '\'' || c == '<' || c == '>' || c == '(' || c == ')' || c == ',');
        let Some(rest) = w.strip_prefix("https://").or_else(|| w.strip_prefix("http://")) else { continue };
        let parts: Vec<&str> = rest.split('/').collect();
        if parts.len() >= 5 && parts[3] == "pull" && parts[..3].iter().all(|p| !p.is_empty()) {
            let digits: String = parts[4].chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = digits.parse() {
                let url = format!("{}{}/{}/{}/pull/{n}", if w.starts_with("https") { "https://" } else { "http://" }, parts[0], parts[1], parts[2]);
                return Some((url, n));
            }
        }
    }
    None
}

// ---------- pull requests ----------

/// A row of `gh pr list --json …`.
#[derive(Deserialize, Serialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct PrRow {
    pub number: u64,
    pub url: String,
    pub state: String,
    pub is_draft: bool,
    pub head_ref_name: String,
    pub head_ref_oid: String,
    pub base_ref_name: String,
    pub is_cross_repository: bool,
}

const LIST_FIELDS: &str = "number,url,state,isDraft,headRefName,headRefOid,baseRefName,isCrossRepository,headRepositoryOwner";

/// Open PRs from this repo's `branch` (forks with the same branch name left out), optionally into `base`.
pub fn open_prs(tools: &Tools, ready: &PrReadiness, branch: &str, base: Option<&str>) -> Result<Vec<PrRow>, (GhErr, String)> {
    let mut args = vec!["pr", "list", "-R", &ready.slug, "--head", branch, "--state", "open", "--limit", "5", "--json", LIST_FIELDS];
    if let Some(b) = base {
        args.extend(["--base", b]);
    }
    let out = tools.gh_read(&args, tools.t.gh_read);
    if !out.ok() {
        return Err((classify(&out), out.tail()));
    }
    let rows: Vec<PrRow> = serde_json::from_str(out.text()).map_err(|_| (GhErr::Ambiguous, "gh pr list의 답을 읽지 못했어요".to_string()))?;
    Ok(rows.into_iter().filter(|r| !r.is_cross_repository && r.head_ref_name == branch).collect())
}

/// `gh pr view N --json <fields>` as JSON.
pub fn view_pr(tools: &Tools, ready: &PrReadiness, number: u64, fields: &str) -> Result<Value, (GhErr, String)> {
    let n = number.to_string();
    let out = tools.gh_read(&["pr", "view", &n, "-R", &ready.slug, "--json", fields], tools.t.gh_read);
    if !out.ok() {
        return Err((classify(&out), out.tail()));
    }
    serde_json::from_str(out.text()).map_err(|_| (GhErr::Ambiguous, "gh pr view의 답을 읽지 못했어요".to_string()))
}

/// The argv of `gh pr create`: never --fill, --web, --editor, --attach or a template flag.
pub fn create_args(slug: &str, base: &str, head: &str, title: &str, body_file: &str, draft: bool) -> Vec<String> {
    let mut a: Vec<String> = ["pr", "create", "-R", slug, "--base", base, "--head", head, "--title", title, "--body-file", body_file].iter().map(|s| s.to_string()).collect();
    if draft {
        a.push("--draft".into());
    }
    a
}

/// The argv of `gh pr merge`: merge or squash only, pinned to the head the user saw; never --delete-branch (the
/// worktree has the branch checked out), --auto, --admin or --rebase.
pub fn merge_args(slug: &str, number: u64, squash: bool, head: &str) -> Vec<String> {
    ["pr", "merge", &number.to_string(), "-R", slug, if squash { "--squash" } else { "--merge" }, "--match-head-commit", head].iter().map(|s| s.to_string()).collect()
}

// ---------- batched status (one GraphQL call per host and 25 PRs) ----------

pub const CHUNK: usize = 25;

/// A PR to poll: (owner, repo, number).
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub owner: String,
    pub repo: String,
    pub number: u64,
}

/// One query for `targets` (aliases rI / pJ), with GitHub's rate limit asked along. String literals are JSON-escaped.
pub fn build_query(targets: &[Target]) -> (String, Vec<(usize, usize)>) {
    let mut repos: Vec<(String, String, Vec<(usize, u64)>)> = vec![];
    for (k, t) in targets.iter().enumerate() {
        match repos.iter_mut().find(|(o, r, _)| *o == t.owner && *r == t.repo) {
            Some((_, _, list)) => list.push((k, t.number)),
            None => repos.push((t.owner.clone(), t.repo.clone(), vec![(k, t.number)])),
        }
    }
    let mut q = String::from("query { rateLimit { limit remaining resetAt cost }");
    // (repo alias index, pr alias index) per target, in target order
    let mut map = vec![(0, 0); targets.len()];
    for (i, (owner, repo, list)) in repos.iter().enumerate() {
        q.push_str(&format!(" r{i}: repository(owner: {}, name: {}) {{", serde_json::to_string(owner).unwrap(), serde_json::to_string(repo).unwrap()));
        for (j, (k, n)) in list.iter().enumerate() {
            map[*k] = (i, j);
            q.push_str(&format!(" p{j}: pullRequest(number: {n}) {{ {PR_FIELDS} }}"));
        }
        q.push_str(" }");
    }
    q.push_str(" }");
    (q, map)
}

const PR_FIELDS: &str = "number url state isDraft headRefName headRefOid baseRefName mergeable mergeStateStatus reviewDecision mergedAt mergeCommit { oid } autoMergeRequest { enabledAt mergeMethod } isInMergeQueue \
commits(last: 1) { nodes { commit { oid statusCheckRollup { state contexts(first: 100) { nodes { __typename \
... on CheckRun { databaseId name status conclusion detailsUrl checkSuite { workflowRun { databaseId } } } \
... on StatusContext { context state targetUrl } } } } } } }";

/// GitHub's rate limit as the query reported it.
#[derive(Debug, Clone, PartialEq)]
pub struct RateLimit {
    pub limit: u64,
    pub remaining: u64,
    /// ms since the epoch
    pub reset_at: u64,
}

/// A failed CI job: its name, the job id (CheckRun.databaseId) and its workflow run.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct FailedJob {
    pub name: String,
    pub job_id: Option<u64>,
    pub run_id: Option<u64>,
}

/// The checks of a PR's head: state "none" | "pending" | "success" | "failure".
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Checks {
    pub state: String,
    pub passed: u32,
    pub failed: u32,
    pub pending: u32,
    pub failed_jobs: Vec<FailedJob>,
    pub head_sha: String,
}

impl Default for Checks {
    fn default() -> Self {
        Checks { state: "none".into(), passed: 0, failed: 0, pending: 0, failed_jobs: vec![], head_sha: String::new() }
    }
}

/// A PR as the status query sees it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PrLive {
    pub number: u64,
    pub url: String,
    pub state: String,
    pub is_draft: bool,
    pub head_ref_name: String,
    pub head_ref_oid: String,
    pub base_ref_name: String,
    pub mergeable: String,
    pub merge_state_status: String,
    pub review_decision: Option<String>,
    pub merged_at: Option<String>,
    pub merge_commit: Option<String>,
    pub auto_merge: bool,
    /// in GitHub's merge queue (`isInMergeQueue`, GraphQL only: `gh pr view --json` doesn't have it)
    pub in_queue: bool,
    pub checks: Checks,
}

impl PrLive {
    /// set to merge without another click: auto-merge on, or in the merge queue
    pub fn queued(&self) -> bool {
        self.auto_merge || self.in_queue
    }
}

const FAILED: &[&str] = &["FAILURE", "TIMED_OUT", "CANCELLED", "STARTUP_FAILURE", "ACTION_REQUIRED", "STALE"];

/// failure: a failing conclusion or status (or the rollup says so); else pending: anything not finished, or the
/// rollup still waiting (only the first 100 contexts are read; the rollup covers the rest); else success; nothing
/// at all: none. A null node (one the token can't read) counts as nothing.
pub fn ci_summary(rollup: Option<&str>, contexts: &[Value], head: &str) -> Checks {
    let mut c = Checks { head_sha: head.to_string(), ..Default::default() };
    for n in contexts {
        if n.is_null() {
            continue;
        }
        let is_run = n["__typename"].as_str() == Some("CheckRun") || n.get("conclusion").is_some();
        if is_run {
            let status = n["status"].as_str().unwrap_or("");
            let conclusion = n["conclusion"].as_str().unwrap_or("");
            if FAILED.contains(&conclusion) {
                c.failed += 1;
                if c.failed_jobs.len() < 5 {
                    c.failed_jobs.push(FailedJob { name: n["name"].as_str().unwrap_or("").to_string(), job_id: n["databaseId"].as_u64(), run_id: n["checkSuite"]["workflowRun"]["databaseId"].as_u64() });
                }
            } else if status != "COMPLETED" {
                c.pending += 1;
            } else {
                c.passed += 1;
            }
        } else {
            match n["state"].as_str().unwrap_or("") {
                "FAILURE" | "ERROR" => {
                    c.failed += 1;
                    if c.failed_jobs.len() < 5 {
                        c.failed_jobs.push(FailedJob { name: n["context"].as_str().unwrap_or("").to_string(), job_id: None, run_id: None });
                    }
                }
                "PENDING" | "EXPECTED" => c.pending += 1,
                _ => c.passed += 1,
            }
        }
    }
    c.state = if c.failed > 0 || matches!(rollup, Some("FAILURE" | "ERROR")) {
        "failure"
    } else if c.pending > 0 || matches!(rollup, Some("PENDING" | "EXPECTED")) {
        "pending"
    } else if c.passed > 0 || rollup == Some("SUCCESS") {
        "success"
    } else {
        "none"
    }
    .into();
    c
}

fn live_from(v: &Value) -> PrLive {
    let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
    let commit = &v["commits"]["nodes"][0]["commit"];
    let head = s("headRefOid");
    let rollup = &commit["statusCheckRollup"];
    let contexts = rollup["contexts"]["nodes"].as_array().cloned().unwrap_or_default();
    PrLive {
        number: v["number"].as_u64().unwrap_or(0),
        url: s("url"),
        state: s("state"),
        is_draft: v["isDraft"].as_bool().unwrap_or(false),
        head_ref_name: s("headRefName"),
        head_ref_oid: head.clone(),
        base_ref_name: s("baseRefName"),
        mergeable: s("mergeable"),
        merge_state_status: s("mergeStateStatus"),
        review_decision: v["reviewDecision"].as_str().map(String::from),
        merged_at: v["mergedAt"].as_str().map(String::from),
        merge_commit: v["mergeCommit"]["oid"].as_str().map(String::from),
        auto_merge: !v["autoMergeRequest"].is_null(),
        in_queue: v["isInMergeQueue"].as_bool().unwrap_or(false),
        checks: ci_summary(rollup["state"].as_str(), &contexts, commit["oid"].as_str().unwrap_or(&head)),
    }
}

/// The answer to `build_query`: the rate limit, and per target its PR (None: GitHub didn't find it). Partial answers
/// (gh exits 1 with data and errors) are read too. Err: nothing readable.
pub fn parse_status(stdout: &str, map: &[(usize, usize)]) -> Result<(Option<RateLimit>, Vec<Option<PrLive>>), String> {
    let v: Value = serde_json::from_str(stdout.trim()).map_err(|_| "상태 응답을 읽지 못했어요".to_string())?;
    let data = &v["data"];
    if data.is_null() {
        return Err("상태 응답에 data가 없어요".into());
    }
    let rl = &data["rateLimit"];
    let rate = rl["limit"].as_u64().map(|limit| RateLimit { limit, remaining: rl["remaining"].as_u64().unwrap_or(limit), reset_at: rl["resetAt"].as_str().and_then(parse_iso_ms).unwrap_or(0) });
    let prs = map
        .iter()
        .map(|(i, j)| {
            let p = &data[format!("r{i}")][format!("p{j}")];
            (!p.is_null() && p["number"].as_u64().is_some()).then(|| live_from(p))
        })
        .collect();
    Ok((rate, prs))
}

/// `gh pr view --json …` output (the same names as GraphQL) as a PrLive. Checks come from `statusCheckRollup` (the
/// head's contexts as a list) when it was asked for; otherwise they have no head (unknown).
pub fn live_from_view(v: &Value) -> PrLive {
    let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
    let checks = match v.get("statusCheckRollup") {
        Some(r) => ci_summary(None, r.as_array().map(Vec::as_slice).unwrap_or(&[]), &s("headRefOid")),
        None => Checks::default(),
    };
    PrLive {
        number: v["number"].as_u64().unwrap_or(0),
        url: s("url"),
        state: s("state"),
        is_draft: v["isDraft"].as_bool().unwrap_or(false),
        head_ref_name: s("headRefName"),
        head_ref_oid: s("headRefOid"),
        base_ref_name: s("baseRefName"),
        mergeable: s("mergeable"),
        merge_state_status: s("mergeStateStatus"),
        review_decision: v["reviewDecision"].as_str().filter(|r| !r.is_empty()).map(String::from),
        merged_at: v["mergedAt"].as_str().filter(|r| !r.is_empty()).map(String::from),
        merge_commit: v["mergeCommit"]["oid"].as_str().map(String::from),
        auto_merge: !v["autoMergeRequest"].is_null(),
        in_queue: v["isInMergeQueue"].as_bool().unwrap_or(false),
        checks,
    }
}

/// `2026-11-11T12:34:56Z` (or with an offset / fraction) → ms since the epoch.
pub fn parse_iso_ms(s: &str) -> Option<u64> {
    let s = s.trim();
    let (date, time) = s.split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>().ok());
    let (y, mo, da) = (d.next()??, d.next()??, d.next()??);
    let (clock, offset) = match time.find(['Z', '+', '-']) {
        Some(k) => (&time[..k], &time[k..]),
        None => (time, ""),
    };
    let mut c = clock.split(':');
    let (h, mi) = (c.next()?.parse::<i64>().ok()?, c.next()?.parse::<i64>().ok()?);
    let sec = c.next().map(|x| x.split('.').next().unwrap_or("0").parse::<i64>().unwrap_or(0)).unwrap_or(0);
    let off = if offset.starts_with('+') || offset.starts_with('-') {
        let sign = if offset.starts_with('-') { -1 } else { 1 };
        let mut o = offset[1..].split(':');
        sign * (o.next()?.parse::<i64>().ok()? * 3600 + o.next().unwrap_or("0").parse::<i64>().ok()? * 60)
    } else {
        0
    };
    // days from the civil date (Howard Hinnant's algorithm)
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = if y2 >= 0 { y2 } else { y2 - 399 } / 400;
    let yoe = y2 - era * 400;
    let doy = (153 * (if mo > 2 { mo - 3 } else { mo + 9 }) + 2) / 5 + da - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let secs = days * 86400 + h * 3600 + mi * 60 + sec - off;
    (secs >= 0).then(|| secs as u64 * 1000)
}

/// `gh api rate_limit`'s GraphQL reset (ms since the epoch).
pub fn graphql_reset(stdout: &str) -> Option<u64> {
    let v: Value = serde_json::from_str(stdout.trim()).ok()?;
    v["resources"]["graphql"]["reset"].as_u64().map(|s| s * 1000)
}

/// Pause polling a host when no more than 40% of its GraphQL budget is left.
pub fn low_budget(r: &RateLimit) -> bool {
    r.remaining * 10 <= r.limit * 4
}

/// The targets grouped by host, in chunks of 25: one GraphQL call each.
pub fn chunks<T: Clone>(items: &[(String, T)]) -> Vec<(String, Vec<T>)> {
    let mut by_host: Vec<(String, Vec<T>)> = vec![];
    for (host, t) in items {
        match by_host.iter_mut().find(|(h, list)| h == host && list.len() < CHUNK) {
            Some((_, list)) => list.push(t.clone()),
            None => by_host.push((host.clone(), vec![t.clone()])),
        }
    }
    by_host
}

/// Runs the status query for one chunk: the query goes through a temp file (argv stays small on Windows).
pub fn query_status(tools: &Tools, host_args: &[String], targets: &[Target]) -> (Out, Vec<(usize, usize)>) {
    let (q, map) = build_query(targets);
    let dir = crate::util::unique_temp_dir("maestro-gql");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("query.graphql");
    let out = match std::fs::write(&file, &q) {
        Ok(()) => {
            let qa = format!("query=@{}", file.to_string_lossy());
            let mut args: Vec<&str> = vec!["api", "graphql"];
            args.extend(host_args.iter().map(String::as_str));
            args.extend(["-F", &qa]);
            tools.gh_read(&args, tools.t.graphql)
        }
        Err(e) => Out { stderr: e.to_string(), code: Some(1), ..Default::default() },
    };
    let _ = std::fs::remove_dir_all(&dir);
    (out, map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_remote_urls() {
        let repo = |h: &str, o: &str, r: &str| RemoteUrl::Repo { host: h.into(), owner: o.into(), repo: r.into() };
        assert_eq!(parse_remote_url("https://github.com/o/r.git"), repo("github.com", "o", "r"));
        assert_eq!(parse_remote_url("https://github.com/o/r/"), repo("github.com", "o", "r"));
        assert_eq!(parse_remote_url("https://user@github.com/o/r"), repo("github.com", "o", "r"));
        assert_eq!(parse_remote_url("git@github.com:o/r.git"), repo("github.com", "o", "r"));
        assert_eq!(parse_remote_url("ssh://git@github.com:22/o/r.git"), repo("github.com", "o", "r"));
        assert_eq!(parse_remote_url("ssh://git@ssh.github.com:443/o/r.git"), repo("github.com", "o", "r"));
        assert_eq!(parse_remote_url("https://acme.ghe.com/o/r.git"), repo("acme.ghe.com", "o", "r"));
        assert_eq!(parse_remote_url("https://git.corp.example/team/app.git"), repo("git.corp.example", "team", "app"));
        assert_eq!(parse_remote_url("C:/repos/r.git"), RemoteUrl::Local);
        assert_eq!(parse_remote_url("C:\\repos\\r.git"), RemoteUrl::Local);
        assert_eq!(parse_remote_url("/srv/git/r.git"), RemoteUrl::Local);
        assert_eq!(parse_remote_url("file:///srv/git/r.git"), RemoteUrl::Local);
        assert_eq!(parse_remote_url("../r.git"), RemoteUrl::Local);
        assert_eq!(parse_remote_url("https://github.com/o"), RemoteUrl::Unknown);
        assert_eq!(parse_remote_url("https://github.com/o/r/tree/main"), RemoteUrl::Unknown);
        assert!(is_github("github.com") && is_github("acme.ghe.com") && !is_github("gitlab.com"));
        // ssh forms (an alias in them is resolved with `ssh -G`), https never
        for u in ["git@github-work:o/r.git", "ssh://git@github-work/o/r.git", "git+ssh://git@h:22/o/r"] {
            assert!(is_ssh_url(u), "{u}");
        }
        for u in ["https://github.com/o/r.git", "C:/repos/r.git", "file:///srv/r.git"] {
            assert!(!is_ssh_url(u), "{u}");
        }
        assert_eq!(ssh_hostname("user git\nhostname GitHub.com\nport 22\n").as_deref(), Some("github.com"));
        assert_eq!(ssh_hostname("hostname ssh.github.com\n").as_deref(), Some("github.com"));
        assert_eq!(ssh_hostname("user git\n"), None);
        // a github.com host or an option-like word is never handed to ssh
        let t = Tools { gh: None, ssh: Some(PathBuf::from("no-such-ssh")), path_var: None, t: Timeouts::default() };
        assert_eq!(resolve_ssh_host(&t, "github.com"), "github.com");
        assert_eq!(resolve_ssh_host(&t, "-oProxyCommand=x"), "-oProxyCommand=x");
    }

    #[test]
    fn finds_gh_in_path_order() {
        let base = std::env::temp_dir().join(format!("maestro-findgh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (a, b) = (base.join("a"), base.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let path = std::env::join_paths([a.clone(), b.clone()]).unwrap();
        assert_eq!(find_gh(&path), None);
        if cfg!(windows) {
            std::fs::write(b.join("gh.exe"), "").unwrap();
            std::fs::write(a.join("gh.cmd"), "").unwrap();
            // the first directory wins, whatever the extension
            assert_eq!(find_gh(&path), Some(a.join("gh.cmd")));
            std::fs::write(a.join("gh.exe"), "").unwrap();
            assert_eq!(find_gh(&path), Some(a.join("gh.exe")));
        } else {
            std::fs::write(b.join("gh"), "").unwrap();
            assert_eq!(find_gh(&path), Some(b.join("gh")));
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn versions() {
        assert_eq!(parse_gh_version("gh version 2.102.0 (2026-09-30)\nhttps://github.com/cli/cli/releases/tag/v2.102.0"), Some((2, 102, 0)));
        assert_eq!(parse_gh_version("gh version 2.39.1 (2023-11-14)"), Some((2, 39, 1)));
        assert!(Some((2, 39, 1)) < Some(MIN_GH) && (2, 102, 0) >= MIN_GH);
        assert_eq!(parse_gh_version("nonsense"), None);
    }

    #[test]
    fn classifies_failures() {
        let o = |code: i32, err: &str| Out { code: Some(code), stderr: err.into(), ..Default::default() };
        assert_eq!(classify(&Out { spawn_err: Some(std::io::ErrorKind::NotFound), ..Default::default() }), GhErr::SpawnFailed);
        assert_eq!(classify(&o(4, "")), GhErr::Auth);
        assert_eq!(classify(&o(1, "To get started with GitHub CLI, please run:  gh auth login")), GhErr::Auth);
        assert_eq!(classify(&o(1, "HTTP 401: Bad credentials")), GhErr::Auth);
        // rate limits before a plain 403
        assert_eq!(classify(&o(1, "HTTP 403: API rate limit exceeded for user ID 1.")), GhErr::RateLimited);
        assert_eq!(classify(&o(1, "You have exceeded a secondary rate limit")), GhErr::RateLimited);
        assert_eq!(classify(&o(1, "HTTP 422: Validation Failed")), GhErr::Validation);
        assert_eq!(classify(&o(1, "pull request create failed: GraphQL: No commits between main and feat")), GhErr::Validation);
        assert_eq!(
            classify(&o(1, "a pull request for branch \"feat\" into branch \"main\" already exists:\nhttps://github.com/o/r/pull/42")),
            GhErr::AlreadyExists(Some(42))
        );
        assert_eq!(classify(&o(1, "no pull requests found for branch \"x\"")), GhErr::NotFound);
        assert_eq!(classify(&o(1, "HTTP 502: Bad Gateway")), GhErr::Ambiguous);
        assert_eq!(classify(&Out { timed_out: true, ..Default::default() }), GhErr::Ambiguous);
        assert_eq!(classify(&o(1, "something odd")), GhErr::Ambiguous);
        for refused in ["pull request create failed: GraphQL: Draft pull requests are not supported in this repository. (createPullRequest)", "GraphQL: Resource protected by organization SAML enforcement.", "GraphQL: was submitted too quickly (createPullRequest)", "GraphQL: must be a collaborator (createPullRequest)"] {
            assert_eq!(classify(&o(1, refused)), GhErr::Refused, "{refused}");
        }
        assert_eq!(classify(&o(1, "GraphQL: Something went wrong while executing your query. Please try again.")), GhErr::Ambiguous);
        assert_eq!(classify(&o(1, "Post \"https://api.github.com/graphql\": dial tcp: i/o timeout")), GhErr::Ambiguous);
        assert_eq!(pr_url_in("Creating pull request...\nhttps://gh.test/o/r/pull/7\n"), Some(("https://gh.test/o/r/pull/7".into(), 7)));
        assert_eq!(pr_url_in("https://github.com/o/r/issues/7"), None);
    }

    #[test]
    fn args_never_force_or_delete() {
        let c = create_args("o/r", "main", "feat", "t", "b.md", true);
        for bad in ["--fill", "--web", "-w", "--editor", "-e", "--attach", "--template", "-T", "--dry-run"] {
            assert!(!c.iter().any(|a| a == bad), "{bad}");
        }
        assert!(c.ends_with(&["--draft".to_string()]));
        let m = merge_args("o/r", 3, true, "abc");
        assert_eq!(m, ["pr", "merge", "3", "-R", "o/r", "--squash", "--match-head-commit", "abc"]);
        for bad in ["--delete-branch", "-d", "--auto", "--admin", "--rebase", "-r"] {
            assert!(!m.iter().any(|a| a == bad) && !merge_args("o/r", 3, false, "abc").iter().any(|a| a == bad), "{bad}");
        }
    }

    #[test]
    fn ci_summary_table() {
        let run = |name: &str, status: &str, conclusion: Option<&str>| json!({"__typename":"CheckRun","name":name,"status":status,"conclusion":conclusion,"databaseId":11,"checkSuite":{"workflowRun":{"databaseId":22}}});
        let ctx = |c: &str, s: &str| json!({"__typename":"StatusContext","context":c,"state":s});
        assert_eq!(ci_summary(None, &[], "h").state, "none");
        let ok = ci_summary(Some("SUCCESS"), &[run("build", "COMPLETED", Some("SUCCESS")), ctx("ci/x", "SUCCESS")], "h");
        assert_eq!((ok.state.as_str(), ok.passed), ("success", 2));
        let pend = ci_summary(Some("PENDING"), &[run("build", "IN_PROGRESS", None), run("lint", "COMPLETED", Some("SUCCESS"))], "h");
        assert_eq!((pend.state.as_str(), pend.pending), ("pending", 1));
        let fail = ci_summary(Some("FAILURE"), &[run("build", "COMPLETED", Some("FAILURE")), run("t", "IN_PROGRESS", None), ctx("ci/y", "ERROR")], "h");
        assert_eq!((fail.state.as_str(), fail.failed, fail.pending), ("failure", 2, 1));
        assert_eq!(fail.failed_jobs[0], FailedJob { name: "build".into(), job_id: Some(11), run_id: Some(22) });
        assert_eq!(fail.failed_jobs[1].name, "ci/y");
        for c in ["TIMED_OUT", "CANCELLED", "STARTUP_FAILURE", "ACTION_REQUIRED", "STALE"] {
            assert_eq!(ci_summary(None, &[run("x", "COMPLETED", Some(c))], "h").state, "failure", "{c}");
        }
        assert_eq!(ci_summary(None, &[run("x", "COMPLETED", Some("NEUTRAL")), run("y", "COMPLETED", Some("SKIPPED"))], "h").state, "success");
        let many: Vec<Value> = (0..8).map(|i| run(&format!("j{i}"), "COMPLETED", Some("FAILURE"))).collect();
        assert_eq!(ci_summary(None, &many, "h").failed_jobs.len(), 5);
        // the rollup still waiting wins over finished contexts (only the first 100 are read); a failure still wins
        assert_eq!(ci_summary(Some("PENDING"), &[run("a", "COMPLETED", Some("SUCCESS"))], "h").state, "pending");
        assert_eq!(ci_summary(Some("EXPECTED"), &[ctx("ci/x", "SUCCESS")], "h").state, "pending");
        assert_eq!(ci_summary(Some("PENDING"), &[run("a", "COMPLETED", Some("FAILURE"))], "h").state, "failure");
        let page: Vec<Value> = (0..100).map(|i| run(&format!("j{i}"), "COMPLETED", Some("SUCCESS"))).collect();
        let p = ci_summary(Some("PENDING"), &page, "h");
        assert_eq!((p.state.as_str(), p.passed, p.pending), ("pending", 100, 0));
        assert_eq!(ci_summary(Some("SUCCESS"), &page, "h").state, "success");
        // a node the token can't read is not a pass
        let null = ci_summary(None, &[Value::Null], "h");
        assert_eq!((null.state.as_str(), null.passed), ("none", 0));
        assert_eq!(ci_summary(Some("PENDING"), &[Value::Null, run("a", "COMPLETED", Some("SUCCESS"))], "h").state, "pending");
    }

    #[test]
    fn query_and_parse() {
        let t = |o: &str, r: &str, n: u64| Target { owner: o.into(), repo: r.into(), number: n };
        let targets = [t("o", "r", 1), t("x\"y", "z", 2), t("o", "r", 3)];
        let (q, map) = build_query(&targets);
        assert_eq!(map, [(0, 0), (1, 0), (0, 1)]);
        assert!(q.contains("r0: repository(owner: \"o\", name: \"r\")") && q.contains("p1: pullRequest(number: 3)"));
        assert!(q.contains("owner: \"x\\\"y\""), "{q}");
        assert!(q.contains("rateLimit"));
        let body = json!({"data":{"rateLimit":{"limit":5000,"remaining":1000,"resetAt":"2026-11-11T12:00:00Z","cost":1},
            "r0":{"p0":{"number":1,"url":"https://gh.test/o/r/pull/1","state":"OPEN","isDraft":false,"headRefName":"a","headRefOid":"h1","baseRefName":"main","mergeable":"MERGEABLE","mergeStateStatus":"CLEAN","reviewDecision":"APPROVED","mergedAt":null,"mergeCommit":null,"autoMergeRequest":null,
                "commits":{"nodes":[{"commit":{"oid":"h1","statusCheckRollup":{"state":"FAILURE","contexts":{"nodes":[{"__typename":"CheckRun","name":"build","status":"COMPLETED","conclusion":"FAILURE","databaseId":9,"checkSuite":{"workflowRun":{"databaseId":8}}}]}}}}]}},
                "p1":{"number":3,"state":"MERGED","headRefOid":"h3","mergeCommit":{"oid":"m3"},"autoMergeRequest":null,"commits":{"nodes":[]}}},
            "r1":{"p0":null}},
            "errors":[{"type":"NOT_FOUND","path":["r1","p0"]}]});
        let (rate, prs) = parse_status(&body.to_string(), &map).unwrap();
        let rate = rate.unwrap();
        assert_eq!((rate.limit, rate.remaining), (5000, 1000));
        assert_eq!(rate.reset_at, parse_iso_ms("2026-11-11T12:00:00Z").unwrap());
        assert!(low_budget(&rate) && !low_budget(&RateLimit { limit: 5000, remaining: 2001, reset_at: 0 }) && low_budget(&RateLimit { limit: 5000, remaining: 2000, reset_at: 0 }));
        let a = prs[0].as_ref().unwrap();
        assert_eq!((a.state.as_str(), a.checks.state.as_str(), a.checks.failed_jobs[0].job_id), ("OPEN", "failure", Some(9)));
        assert!(prs[1].is_none(), "NOT_FOUND alias");
        let c = prs[2].as_ref().unwrap();
        assert_eq!((c.state.as_str(), c.merge_commit.as_deref(), c.checks.state.as_str()), ("MERGED", Some("m3"), "none"));
        assert!(parse_status("not json", &map).is_err());
        assert!(parse_status("{\"errors\":[]}", &map).is_err());
    }

    #[test]
    fn iso_and_rate_limit_reset() {
        assert_eq!(parse_iso_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso_ms("2000-03-01T00:00:00Z"), Some(951_868_800_000));
        assert_eq!(parse_iso_ms("2000-03-01T09:00:00+09:00"), Some(951_868_800_000));
        assert_eq!(graphql_reset(r#"{"resources":{"graphql":{"limit":5000,"remaining":0,"reset":1700000000}}}"#), Some(1_700_000_000_000));
    }

    #[test]
    fn chunks_by_host() {
        let items: Vec<(String, u32)> = (0..30).map(|i| ("github.com".to_string(), i)).chain([("ghe.x".to_string(), 99)]).collect();
        let c = chunks(&items);
        assert_eq!(c.len(), 3);
        assert_eq!((c[0].1.len(), c[1].1.len(), c[2].0.as_str()), (25, 5, "ghe.x"));
    }
}
