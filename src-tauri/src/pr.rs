//! PR mode (R6/D7): when a project's merge mode is "pr", approving a merge request makes the app (not an agent)
//! commit the worker's changes if the user ticked it, push the recorded commit to the worker's branch on the remote
//! (never forced), check once more right before creating, and open the pull request with the user's own `gh`. An
//! unclear outcome (timeout, network, an error after the request may have gone through) is never retried blind: the
//! app looks for the branch's open PR and carries on with it. Every step is written down before the call it guards
//! (`PrProgress` on the request), so a restart reconciles with reads only. Merging the PR (merge or squash, D6) is a
//! separate click; afterwards the project folder is fast-forwarded only when it is clean and on the base branch.
//! CI and review state are polled in batches (one GraphQL call per host and 25 PRs) and pause near the rate limit.

use crate::exec::{self, Out};
use crate::github::{self, Checks, GhErr, PrLive, PrReadiness, Target, Tools};
use crate::merge::{self, key, MergeRequest, MergeResult, Merges};
use crate::project::{self, MergeMode};
use crate::util::sha7;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// What the flows need from the app, behind a trait so they run in tests without one.
pub trait Host: Send + Sync {
    fn tools(&self) -> Tools;
    fn home(&self) -> PathBuf;
    /// ms since the epoch
    fn now(&self) -> u64;
    fn emit(&self, event: &str, payload: Value);
    /// a `[maestro]` note for an agent (kept until it can take it; one per key)
    fn note(&self, root: &str, to: &str, key: &str, text: &str);
    /// the worker's agent is in a turn
    fn worker_busy(&self, worker: &str) -> bool;
    fn base_branch(&self, root: &str) -> String;
    /// the project has a Maestro to tell
    fn has_maestro(&self, _root: &str) -> bool {
        true
    }
    fn wake(&self) {}
    /// an approval, merge or base update runs for this worker now (the poller leaves its PR alone meanwhile)
    fn pr_busy(&self, _worker: &str) -> bool {
        false
    }
}

pub struct AppHost {
    app: AppHandle,
    home: PathBuf,
}

impl AppHost {
    pub fn new(app: &AppHandle) -> Result<AppHost, String> {
        Ok(AppHost { app: app.clone(), home: crate::home(app)? })
    }
}

impl Host for AppHost {
    fn tools(&self) -> Tools {
        Tools::system()
    }
    fn home(&self) -> PathBuf {
        self.home.clone()
    }
    fn now(&self) -> u64 {
        merge::now_ms()
    }
    fn emit(&self, event: &str, payload: Value) {
        let _ = self.app.emit(event, payload);
    }
    fn note(&self, root: &str, to: &str, key: &str, text: &str) {
        // a discarded Maestro takes its traffic with it: nothing is kept on disk for a later Maestro in the folder,
        // which never asked for this work
        if merge::key(to) == merge::key(root) && !self.has_maestro(root) {
            eprintln!("[maestro] note '{key}' dropped: no Maestro in {root}");
            return;
        }
        // delivering waits for the agent's prompt
        let (app, root, to, key, text) = (self.app.clone(), root.to_string(), to.to_string(), key.to_string(), text.to_string());
        std::thread::spawn(move || {
            if let Err(e) = crate::control::tell(&app, &root, &to, &key, &text) {
                eprintln!("[maestro] note for {to} not kept: {e}");
            }
        });
    }
    fn worker_busy(&self, worker: &str) -> bool {
        self.app.state::<crate::AppState>().get(worker).filter(|a| a.alive()).is_some_and(|a| *a.state.lock().unwrap() != crate::agent::AgentState::Idle)
    }
    fn base_branch(&self, root: &str) -> String {
        crate::base_branch(&self.app, root)
    }
    fn has_maestro(&self, root: &str) -> bool {
        crate::files(&self.app, root, root).ok().and_then(|f| f.read_meta()).is_some()
    }
    fn wake(&self) {
        wake_poller();
    }
    fn pr_busy(&self, worker: &str) -> bool {
        self.app.state::<Merges>().pr_busy(worker)
    }
}

// ---------- the request's progress (write-ahead) ----------

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Step {
    #[default]
    None,
    Committing,
    Committed,
    Pushing,
    Pushed,
    Creating,
    /// `gh pr create` may or may not have made the PR, and no lookup found one yet
    Unconfirmed,
}

impl Step {
    /// an outside call may be under way (or was, when the app went down)
    pub fn mid_way(self) -> bool {
        matches!(self, Step::Committing | Step::Pushing | Step::Creating)
    }
}

/// What an approval in PR mode has done so far, kept on the request before each outside call.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct PrProgress {
    pub step: Step,
    pub commit_message: Option<String>,
    /// the worker's tip before the app's commit
    pub commit_parent: Option<String>,
    pub app_commit: Option<String>,
    /// the app's commit holds other files than the card showed (a pre-commit hook added or reverted some): kept so it
    /// is never made twice, never pushed; the request has to be made again, after the worker looked at it
    pub commit_mismatch: Option<String>,
    /// what goes to the remote: the app's commit or the requested tip
    pub push_sha: Option<String>,
    /// ms; when `gh pr create` was last run
    pub create_attempt_at: Option<u64>,
    pub title: String,
    pub body: String,
    pub draft: bool,
    /// ms; when a lookup last found no open PR
    pub last_empty_lookup_at: Option<u64>,
    pub error: Option<String>,
    /// what a lookup found, not a failure ("기존 PR #N을 찾았어요 …"): shown muted on the card
    pub notice: Option<String>,
}

impl PrProgress {
    /// a `gh pr create` ran and no lookup has found its PR yet: it may be open on GitHub, so the next approval only
    /// looks first (and creates again only 30 s on, by "다시 만들기"). `create_attempt_at` stays set until the request
    /// ends with a PR or a create that certainly made nothing.
    pub fn create_unresolved(&self) -> bool {
        self.create_attempt_at.is_some() || matches!(self.step, Step::Creating | Step::Unconfirmed)
    }

    /// an outside call may be under way, or a create may have made a PR the app has no record of: the merge mode
    /// can't change and the worker or its Maestro can't be discarded (that would drop the request unseen)
    pub fn unresolved(&self) -> bool {
        self.step.mid_way() || self.create_unresolved()
    }

    /// What a request replacing this one (the worker's next `maestro merge-request`) keeps: an unresolved create on the
    /// same branch, so the new approval looks for that PR instead of making a second one.
    pub fn carried(&self) -> Option<PrProgress> {
        self.create_unresolved().then(|| PrProgress {
            step: Step::Unconfirmed,
            create_attempt_at: self.create_attempt_at.or(Some(0)),
            last_empty_lookup_at: self.last_empty_lookup_at,
            push_sha: self.push_sha.clone(),
            error: Some("이전 요청에서 만든 PR이 있는지 아직 확인하지 못했어요: '다시 확인'으로 먼저 찾아요".into()),
            ..Default::default()
        })
    }
}

/// The title and body the card starts from.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct PrDraft {
    pub title: String,
    pub body: String,
    pub draft: bool,
}

/// What the card sends with an approval in PR mode.
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PrOptions {
    /// the user ticked "커밋하고 진행" and wrote this message
    pub commit_message: Option<String>,
    pub title: String,
    pub body: String,
    pub draft: bool,
    /// rewrite the title and body of the worker's PR (only one the app made)
    pub update_body: bool,
    /// after an unclear create: make it again if a fresh lookup (30 s after the attempt) finds none
    pub recreate: bool,
    /// "다시 확인": only look for the branch's PR. Nothing is committed, pushed or created
    pub check_only: bool,
}

// ---------- pull request records (~/.maestro/pull-requests.json) ----------

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct MergeAttempt {
    pub method: String,
    pub seen_head: String,
    pub at: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Merged {
    /// "merge" | "squash" | "github" (merged outside the app)
    pub method: String,
    pub commit: Option<String>,
    pub head: String,
    pub at: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AfterMerge {
    /// the project folder's base after the fast-forward; None: left as it was
    pub base_updated: Option<String>,
    pub message: String,
}

/// One worker's pull request as the app knows it. Made once a number is known (created, adopted or recovered).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct PrRecord {
    pub root: String,
    pub maestro_path: String,
    pub worker_path: String,
    pub worker_name: String,
    pub branch: String,
    pub base: String,
    pub remote: String,
    pub host: String,
    pub owner: String,
    pub repo: String,
    pub number: u64,
    pub url: String,
    /// made by the app (only such a PR's title and body are ever rewritten)
    pub app_created: bool,
    /// "OPEN" | "CLOSED" | "MERGED"
    pub state: String,
    pub is_draft: bool,
    pub head_sha: String,
    pub review_decision: Option<String>,
    pub mergeable: String,
    pub merge_state_status: String,
    pub auto_merge: bool,
    /// in a merge queue or set to merge automatically after the user's click
    pub queued: bool,
    pub checks: Checks,
    /// the head a CI failure note was sent for
    pub ci_noted_sha: Option<String>,
    pub merge_attempt: Option<MergeAttempt>,
    pub merged: Option<Merged>,
    /// every head merged for this worker: commits up to them are on the base (discard, plan)
    pub merged_heads: Vec<String>,
    pub after_merge: Option<AfterMerge>,
    pub error: Option<String>,
    /// what the last approval did, when the card that asked is gone by then ("기존 PR #N을 찾아 이어 썼어요", a body
    /// that couldn't be updated)
    pub notice: Option<String>,
    pub checked_at: u64,
    pub hidden: bool,
    /// merged by the user's click and the conductor not told yet (the folder's fast-forward runs in between): told
    /// on the next start if the app went down meanwhile
    pub note_owed: bool,
}

impl PrRecord {
    pub fn slug(&self) -> String {
        if self.host == "github.com" {
            format!("{}/{}", self.owner, self.repo)
        } else {
            format!("{}/{}/{}", self.host, self.owner, self.repo)
        }
    }

    fn ready(&self) -> PrReadiness {
        PrReadiness { ok: true, remote: Some(self.remote.clone()), host: self.host.clone(), owner: self.owner.clone(), repo: self.repo.clone(), slug: self.slug(), ..Default::default() }
    }

    fn polled(&self) -> bool {
        !self.hidden && (self.state == "OPEN" || self.merge_attempt.is_some())
    }
}

pub struct Prs {
    list: Mutex<Vec<PrRecord>>,
    file: PathBuf,
}

impl Prs {
    pub fn load(home: &Path) -> Prs {
        Prs::at(home.join(".maestro").join("pull-requests.json"))
    }

    pub fn at(file: PathBuf) -> Prs {
        let list = std::fs::read_to_string(&file).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        Prs { list: Mutex::new(list), file }
    }

    fn save(&self, list: &[PrRecord]) {
        let text = serde_json::to_string_pretty(list).unwrap_or_else(|_| "[]".into());
        if let Err(e) = crate::util::atomic_write(&self.file, &text) {
            eprintln!("[maestro] pull requests not saved: {e}");
        }
    }

    pub fn list(&self) -> Vec<PrRecord> {
        self.list.lock().unwrap().clone()
    }

    pub fn get(&self, worker: &str) -> Option<PrRecord> {
        let k = key(worker);
        self.list.lock().unwrap().iter().find(|r| key(&r.worker_path) == k).cloned()
    }

    /// Adds or replaces the worker's record. A different PR (after the last was merged or closed) replaces the live
    /// fields and keeps what was merged before.
    pub fn upsert(&self, mut rec: PrRecord) -> PrRecord {
        let mut list = self.list.lock().unwrap();
        let k = key(&rec.worker_path);
        if let Some(old) = list.iter_mut().find(|r| key(&r.worker_path) == k) {
            for h in &old.merged_heads {
                if !rec.merged_heads.contains(h) {
                    rec.merged_heads.insert(0, h.clone());
                }
            }
            if old.number == rec.number {
                rec.app_created |= old.app_created;
                rec.ci_noted_sha = rec.ci_noted_sha.or(old.ci_noted_sha.clone());
            }
            *old = rec.clone();
        } else {
            list.push(rec.clone());
        }
        self.save(&list);
        rec
    }

    /// Changes the worker's record in place; returns it as it is now.
    pub fn update(&self, worker: &str, f: impl FnOnce(&mut PrRecord)) -> Option<PrRecord> {
        let mut list = self.list.lock().unwrap();
        let k = key(worker);
        let r = list.iter_mut().find(|r| key(&r.worker_path) == k)?;
        f(r);
        let out = r.clone();
        self.save(&list);
        Some(out)
    }

    pub fn remove_worker(&self, worker: &str) -> Option<PrRecord> {
        let mut list = self.list.lock().unwrap();
        let k = key(worker);
        let i = list.iter().position(|r| key(&r.worker_path) == k)?;
        let r = list.remove(i);
        self.save(&list);
        Some(r)
    }

    /// The Maestro was discarded: its PRs keep their records (their workers still show them) but belong to no
    /// Maestro any more (`maestro_path` empty), so no note and no Maestro card reaches a later one in the folder.
    pub fn orphan_maestro(&self, maestro: &str) -> Vec<PrRecord> {
        let mut list = self.list.lock().unwrap();
        let k = key(maestro);
        let mut changed = vec![];
        for r in list.iter_mut().filter(|r| !r.maestro_path.is_empty() && key(&r.maestro_path) == k) {
            r.maestro_path.clear();
            changed.push(r.clone());
        }
        if !changed.is_empty() {
            self.save(&list);
        }
        changed
    }

    /// Heads of the worker's merged PRs on `branch` (empty when the record is for another branch).
    pub fn merged_heads(&self, worker: &str, branch: &str) -> Vec<String> {
        self.get(worker).filter(|r| branch.is_empty() || r.branch == branch).map(|r| r.merged_heads).unwrap_or_default()
    }

}

// ---------- notes ----------

fn pr_key(worker: &str) -> String {
    format!("pr:{}", key(worker))
}

/// A note about the PR for the Maestro that asked for it; none once that Maestro was discarded (a later Maestro in
/// the folder never asked for this work).
fn note_pr(host: &dyn Host, r: &PrRecord, text: &str) {
    if r.maestro_path.is_empty() {
        return;
    }
    host.note(&r.root, &r.maestro_path, &pr_key(&r.worker_path), text);
}

/// A pushed branch is never rewritten: the app never force-pushes, so the next approval would be refused.
const NO_REWRITE: &str = "PR이 열린(푸시된) 워커 브랜치는 rebase·amend·reset으로 다시 쓰지 말고, 기준 브랜치는 merge로 받게 하세요. 앱은 강제 푸시를 하지 않아서 다음 승인이 거절돼요.";

/// "[maestro] PR 생성 …" or, for a PR found and carried on with, "[maestro] 기존 PR #N을 찾아 이어 썼어요 …".
pub fn note_opened(rec: &PrRecord, commits: usize, how: &str) -> String {
    let draft = if rec.is_draft { ", 초안" } else { "" };
    let tail = "CI·리뷰 결과는 [maestro] 메모로 알려요. 병합은 사용자가 앱에서 해요. 워커가 더 고치면 커밋하게 한 뒤 다시 maestro merge-request로 요청하세요 (같은 PR이 갱신돼요). push·gh pr 명령은 직접 쓰지 마세요.";
    let tail = format!("{tail} {NO_REWRITE}");
    let what = format!("워커 '{}' ({} → {}) PR #{} {}{draft}. 커밋 {commits}개.", rec.worker_name, rec.branch, rec.base, rec.number, rec.url);
    match how {
        "pr" => format!("[maestro] PR 생성: {what} {tail}"),
        "pr-updated" => format!("[maestro] PR 갱신: {what} {tail}"),
        _ => format!("[maestro] 기존 PR #{}을 찾아 이어 썼어요: {what} {tail}", rec.number),
    }
}

pub fn note_ci_failed(rec: &PrRecord) -> String {
    let jobs: Vec<&str> = rec.checks.failed_jobs.iter().map(|j| j.name.as_str()).collect();
    format!(
        "[maestro] PR #{} CI 실패 (head {}): 작업 {}. 사용자가 실패 로그를 붙여 주거나 지시할 때까지 기다리세요. 고친 뒤 워커가 커밋하면 maestro merge-request로 다시 요청하세요 (같은 PR이 갱신돼요). {NO_REWRITE}",
        rec.number,
        sha7(&rec.checks.head_sha),
        if jobs.is_empty() { "(이름 없음)".to_string() } else { jobs.join(", ") }
    )
}

pub fn note_closed(rec: &PrRecord) -> String {
    format!("[maestro] PR #{}이 병합 없이 닫혔어요. 사용자 지시를 기다리세요.", rec.number)
}

pub fn note_github_merged(rec: &PrRecord, commit: Option<&str>) -> String {
    format!(
        "[maestro] PR #{}이 GitHub에서 합쳐졌어요 (커밋 {}). 프로젝트 폴더는 그대로예요: 사용자가 앱에서 '기준 브랜치 받기'를 누를 수 있어요.",
        rec.number,
        commit.map(sha7).unwrap_or("?")
    )
}

/// After the user's merge click: the method, the commit, and what happened to the project folder. Only a folder
/// that was fast-forwarded (`updated`) has the merge to build and test.
pub fn note_merged(rec: &PrRecord, method: &str, commit: Option<&str>, folder: &str, updated: bool) -> String {
    let check = if updated {
        "프로젝트 폴더에서 빌드·테스트로 확인하세요."
    } else {
        "프로젝트 폴더는 아직 이전 기준이에요: 사용자가 '기준 브랜치 받기'를 누른 뒤 빌드·테스트하세요."
    };
    let mut s = format!(
        "[maestro] PR #{} 병합 완료 ({method}, 커밋 {}): 워커 '{}' → {}. 프로젝트 폴더: {folder}. {check}",
        rec.number,
        commit.map(sha7).unwrap_or("?"),
        rec.worker_name,
        rec.base
    );
    if method == "squash" {
        s.push_str(" squash라 워커 브랜치 커밋은 기준에 그대로 있지 않아요. 이 워커를 계속 쓰려면 원격 기준을 받아 합치거나 새 워커로 시작하세요.");
    }
    s
}

pub fn note_mode(mode: MergeMode) -> String {
    match mode {
        MergeMode::Pr => "[maestro] 병합 방식이 PR로 바뀌었어요: merge-request를 승인하면 앱이 (확인 시 커밋하고) 브랜치를 푸시하고 PR을 만들어요. 대기 중이던 요청은 거뒀으니 필요하면 다시 요청하세요.".into(),
        MergeMode::Local => "[maestro] 병합 방식이 로컬로 바뀌었어요: merge-request를 승인하면 Maestro가 프로젝트 폴더에서 --no-ff로 합쳐요. 대기 중이던 요청은 거뒀으니 필요하면 다시 요청하세요.".into(),
    }
}

// ---------- title and body ----------

const TEMPLATES: [&str; 6] = [".github/pull_request_template.md", ".github/PULL_REQUEST_TEMPLATE.md", "pull_request_template.md", "PULL_REQUEST_TEMPLATE.md", "docs/pull_request_template.md", "docs/PULL_REQUEST_TEMPLATE.md"];
const LOCAL: &str = "<로컬 경로>";
const MAX_BODY: usize = 60_000;

/// The local paths a PR must not show: the project folder, the worker, home, temp and ~/.maestro. A `X:` path also
/// in the spellings Git Bash, WSL and Cygwin print (`/x/…`, `/mnt/x/…`, `/cygdrive/x/…`).
pub fn local_paths(root: &str, worker: &str, home: &Path) -> Vec<String> {
    let mut v = vec![root.to_string(), worker.to_string(), home.join(".maestro").to_string_lossy().to_string(), home.to_string_lossy().to_string(), std::env::temp_dir().to_string_lossy().to_string()];
    v.retain(|p| p.trim_matches(['/', '\\']).len() > 2);
    let mut more = vec![];
    for p in &v {
        let b = p.as_bytes();
        if b.len() > 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'/' || b[2] == b'\\') {
            let (d, rest) = ((b[0] as char).to_ascii_lowercase(), p[3..].replace('\\', "/"));
            more.extend([format!("/{d}/{rest}"), format!("/mnt/{d}/{rest}"), format!("/cygdrive/{d}/{rest}")]);
        }
    }
    v.extend(more);
    v
}

/// `needle` in `text` ignoring ASCII case, both slash spellings, replaced by "<로컬 경로>".
fn replace_path(text: &str, needle: &str) -> String {
    let mut out = text.to_string();
    for n in [needle.replace('\\', "/"), needle.replace('/', "\\")] {
        let n = n.trim_end_matches(['/', '\\']);
        if n.is_empty() {
            continue;
        }
        let mut s = String::with_capacity(out.len());
        let (hay, nb) = (out.as_bytes(), n.as_bytes());
        let mut i = 0;
        while i < hay.len() {
            if out.is_char_boundary(i) && hay.len() - i >= nb.len() && hay[i..i + nb.len()].eq_ignore_ascii_case(nb) && out.is_char_boundary(i + nb.len()) {
                s.push_str(LOCAL);
                i += nb.len();
                // the rest of the path goes with it
                while i < hay.len() && !hay[i].is_ascii_whitespace() && !b")]>\"'`".contains(&hay[i]) {
                    i += 1;
                }
                while i < hay.len() && !out.is_char_boundary(i) {
                    i += 1;
                }
            } else {
                let c = out[i..].chars().next().unwrap();
                s.push(c);
                i += c.len_utf8();
            }
        }
        out = s;
    }
    out
}

/// Paths that look local whoever's they are: `C:\…`, `/Users/…`, `/home/…`, Git Bash, WSL and Cygwin drives (`/c/…`,
/// `/mnt/c/…`, `/cygdrive/c/…`), `\\server\…`, `file://…`.
fn replace_path_like(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let boundary = |k: usize| k == 0 || chars[k - 1].is_whitespace() || "([<\"'`=:,".contains(chars[k - 1]);
    let starts = |k: usize, s: &str| chars[k..].iter().take(s.chars().count()).collect::<String>().eq_ignore_ascii_case(s);
    while i < chars.len() {
        let drive = i + 2 < chars.len() && chars[i].is_ascii_alphabetic() && chars[i + 1] == ':' && (chars[i + 2] == '\\' || chars[i + 2] == '/') && boundary(i) && (i + 3 >= chars.len() || chars[i + 3] != '/');
        let letter = |k: usize| k + 1 < chars.len() && chars[k].is_ascii_alphabetic() && chars[k + 1] == '/';
        let msys = chars[i] == '/' && boundary(i) && (letter(i + 1) || (starts(i, "/mnt/") && letter(i + 5)) || (starts(i, "/cygdrive/") && letter(i + 10)));
        let unix = chars[i] == '/' && (boundary(i) || (i >= 7 && starts(i - 7, "file://"))) && (starts(i, "/Users/") || starts(i, "/home/"));
        let unc = chars[i] == '\\' && i + 2 < chars.len() && chars[i + 1] == '\\' && !chars[i + 2].is_whitespace() && boundary(i);
        let file = starts(i, "file://") && boundary(i);
        if drive || msys || unix || unc || file {
            out.push_str(LOCAL);
            while i < chars.len() && !chars[i].is_whitespace() && !")]>\"'`".contains(chars[i]) {
                i += 1;
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// `@name` outside code, put in backticks so nobody is pinged.
fn inert_mentions(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let mut in_code = false;
    while i < chars.len() {
        let c = chars[i];
        if c == '`' {
            in_code = !in_code;
        }
        let starts_name = |k: usize| k < chars.len() && chars[k].is_ascii_alphanumeric();
        if c == '@' && !in_code && (i == 0 || chars[i - 1].is_whitespace() || "([".contains(chars[i - 1])) && starts_name(i + 1) {
            let mut j = i + 1;
            while j < chars.len() && (chars[j].is_ascii_alphanumeric() || "-_/".contains(chars[j])) {
                j += 1;
            }
            out.push('`');
            out.extend(&chars[i..j]);
            out.push('`');
            i = j;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// The body as it may go out: local paths replaced, control characters dropped, mentions made inert outside code
/// fences, images reduced to their text, at most 60 000 characters.
pub fn sanitize_body(text: &str, locals: &[String]) -> String {
    let mut t: String = text.replace("\r\n", "\n").chars().filter(|c| !c.is_control() || *c == '\n' || *c == '\t').collect();
    let mut sorted: Vec<&String> = locals.iter().collect();
    sorted.sort_by_key(|p| std::cmp::Reverse(p.len()));
    for p in sorted {
        t = replace_path(&t, p);
    }
    let mut fence = false;
    let lines: Vec<String> = t
        .lines()
        .map(|l| {
            if l.trim_start().starts_with("```") {
                fence = !fence;
                return replace_path_like(l);
            }
            let l = replace_path_like(l);
            if fence {
                l
            } else {
                inert_mentions(&no_images(&l))
            }
        })
        .collect();
    let out = lines.join("\n");
    match out.char_indices().nth(MAX_BODY) {
        Some((i, _)) => format!("{}\n…", &out[..i]),
        None => out,
    }
}

/// `![alt](url)` → `alt`: no images or attachments in a PR the app writes.
fn no_images(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(i) = rest.find("![") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        match after.find("](").and_then(|j| after[j + 2..].find(')').map(|k| (j, j + 2 + k))) {
            Some((j, end)) => {
                out.push_str(&after[..j]);
                rest = &after[end + 1..];
            }
            None => {
                out.push_str("![");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// A title: one line, no control characters, local paths replaced, at most 200 characters.
pub fn sanitize_title(text: &str, locals: &[String]) -> String {
    let one = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let mut t = one.to_string();
    for p in locals {
        t = replace_path(&t, p);
    }
    let t: String = replace_path_like(&t).chars().filter(|c| !c.is_control()).collect();
    t.chars().take(200).collect::<String>().trim().to_string()
}

/// A fence longer than any run of backticks in `text`.
fn fence_for(text: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for c in text.chars() {
        if c == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    "`".repeat((longest + 1).max(3))
}

/// The repo's PR template at `rev` (Orca's six places, any case).
fn template(tools: &Tools, root: &Path, rev: &str) -> Option<String> {
    let ls = exec::run(tools.git(root, &["ls-tree", "-r", "--name-only", "-z", rev], tools.t.git_local));
    if !ls.ok() {
        return None;
    }
    let names: Vec<&str> = ls.stdout.split('\0').filter(|n| !n.is_empty()).collect();
    for want in TEMPLATES {
        if let Some(n) = names.iter().find(|n| n.eq_ignore_ascii_case(want)) {
            let show = exec::run(tools.git(root, &["show", &format!("{rev}:{n}")], tools.t.git_local));
            if show.ok() && !show.text().trim().is_empty() {
                return Some(show.text().to_string());
            }
        }
    }
    None
}

fn subject(line: &str) -> &str {
    line.split_once(' ').map(|(_, s)| s).unwrap_or(line)
}

/// The card's starting title and body for a PR-mode request: the commits' subject(s), the repo's template if it
/// has one, then a "Maestro 요약" (commits, files, what the conductor checked in a text fence, what wasn't checked).
/// Draft when the conductor reported no verification.
pub fn draft(tools: &Tools, req: &MergeRequest, home: &Path) -> PrDraft {
    let p = &req.plan;
    let locals = local_paths(&req.root, &req.worker_path, home);
    let title = match (p.commit_count, p.commits.first()) {
        (1, Some(c)) if p.worker_dirty.is_empty() => subject(c).to_string(),
        (0, _) | (_, None) => format!("{}: 작업 커밋", req.worker_name),
        (n, Some(c)) => {
            let more = n - 1 + usize::from(!p.worker_dirty.is_empty());
            if more == 0 {
                subject(c).to_string()
            } else {
                format!("{}: {} 외 {more}개", req.worker_name, subject(c))
            }
        }
    };
    let mut s = String::from("## Maestro 요약\n\n");
    s.push_str(&format!("`{}` → `{}` · 커밋 {}개 · 파일 {}개 (+{} −{})\n\n", p.branch, p.base, p.commit_count, p.file_count, p.adds, p.dels));
    for c in p.commits.iter().take(20) {
        s.push_str(&format!("- {c}\n"));
    }
    if p.commit_count > 20 {
        s.push_str(&format!("- 외 {}개\n", p.commit_count - 20));
    }
    if !p.files.is_empty() {
        s.push_str("\n변경 파일:\n\n");
        for f in p.files.iter().take(30) {
            s.push_str(&format!("- `{}`\n", f.path));
        }
        if p.file_count > 30 {
            s.push_str(&format!("- 외 {}개\n", p.file_count - 30));
        }
    }
    s.push_str("\n### 확인\n\n");
    match &req.verified {
        Some(v) => {
            let f = fence_for(v);
            s.push_str(&format!("{f}text\n{v}\n{f}\n"));
        }
        None => s.push_str("지휘자가 보고한 확인 내용이 없어요.\n"),
    }
    s.push_str("\n### 확인하지 않은 것\n\n- CI가 아직 돌지 않았어요.\n");
    if p.conflicts_checked {
        s.push_str(&format!("- 충돌 검사는 마지막으로 받아 온 기준(`{}`)으로 했어요.\n", p.cmp_ref.trim_start_matches("refs/remotes/").trim_start_matches("refs/heads/")));
    } else {
        s.push_str("- 충돌 검사를 하지 못했어요 (git 2.38 미만).\n");
    }
    if p.worker_untracked > 0 {
        s.push_str(&format!("- 워커의 추적하지 않는 파일 {}개는 포함되지 않았어요.\n", p.worker_untracked));
    }
    let body = match template(tools, Path::new(&req.root), &p.tip_sha) {
        Some(t) => format!("{}\n\n{s}", t.trim_end()),
        None => s,
    };
    PrDraft { title: sanitize_title(&title, &locals), body: sanitize_body(&body, &locals), draft: req.verified.is_none() }
}

// ---------- git through exec ----------

fn git(tools: &Tools, dir: &Path, args: &[&str]) -> Out {
    exec::run(tools.git(dir, args, tools.t.git_local))
}

fn rev(tools: &Tools, dir: &Path, r: &str) -> Option<String> {
    let o = git(tools, dir, &["rev-parse", "--verify", "--quiet", &format!("{r}^{{commit}}")]);
    (o.ok() && !o.text().is_empty()).then(|| o.text().to_string())
}

fn is_ancestor(tools: &Tools, dir: &Path, a: &str, b: &str) -> bool {
    git(tools, dir, &["merge-base", "--is-ancestor", a, b]).ok()
}

fn has_object(tools: &Tools, dir: &Path, sha: &str) -> bool {
    git(tools, dir, &["cat-file", "-e", &format!("{sha}^{{commit}}")]).ok()
}

fn head_branch(tools: &Tools, dir: &Path) -> Option<String> {
    let o = git(tools, dir, &["symbolic-ref", "-q", "HEAD"]);
    o.ok().then(|| o.text().strip_prefix("refs/heads/").map(String::from)).flatten()
}

fn dirty_paths(tools: &Tools, dir: &Path) -> Result<Vec<String>, String> {
    let o = git(tools, dir, &["status", "--porcelain=v1", "-z", "--untracked-files=no"]);
    if !o.ok() {
        return Err(format!("git status 실패: {}", o.tail()));
    }
    Ok(crate::worktree::parse_status_z(&o.stdout))
}

fn in_progress(tools: &Tools, dir: &Path) -> Result<Option<&'static str>, String> {
    let mut args = vec!["rev-parse"];
    for (p, _) in crate::worktree::OPERATIONS {
        args.extend(["--git-path", p]);
    }
    let o = git(tools, dir, &args);
    if !o.ok() {
        return Err(o.tail());
    }
    Ok(crate::worktree::OPERATIONS.iter().zip(o.stdout.lines()).find(|(_, l)| crate::worktree::git_path_exists(dir, l)).map(|((_, w), _)| *w))
}

/// The branch's tip on the remote; None: no such branch there.
fn ls_remote(tools: &Tools, dir: &Path, remote: &str, branch: &str) -> Result<Option<String>, String> {
    Ok(ls_remote_all(tools, dir, remote, &[branch])?.remove(0))
}

/// Several branches' tips on the remote in one call, in the order asked.
fn ls_remote_all(tools: &Tools, dir: &Path, remote: &str, branches: &[&str]) -> Result<Vec<Option<String>>, String> {
    let refs: Vec<String> = branches.iter().map(|b| format!("refs/heads/{b}")).collect();
    let mut args = vec!["ls-remote", "--heads", remote];
    args.extend(refs.iter().map(String::as_str));
    let o = exec::run(tools.git(dir, &args, tools.t.ls_remote).network());
    if !o.ok() {
        return Err(format!("원격({remote})에 연결하지 못했어요: {}", o.tail()));
    }
    Ok(refs.iter().map(|want| o.stdout.lines().find_map(|l| l.split_once('\t').filter(|(_, r)| r == want).map(|(s, _)| s.to_string()))).collect())
}

/// `fetch` of the base into its remote-tracking ref only (never the working tree).
fn fetch_base(tools: &Tools, root: &Path, remote: &str, base: &str) -> Result<(), String> {
    let spec = format!("+refs/heads/{base}:refs/remotes/{remote}/{base}");
    let o = exec::run(tools.git(root, &["fetch", "--no-tags", "--no-write-fetch-head", remote, &spec], tools.t.fetch).network());
    if o.ok() {
        Ok(())
    } else {
        Err(format!("원격 기준 브랜치({remote}/{base})를 받지 못했어요: {}", o.tail()))
    }
}

/// The argv after `git … -C <worker>` for the push: the recorded commit to the branch, no `+`, no force, no -u.
pub fn push_args(remote: &str, sha: &str, branch: &str) -> Vec<String> {
    ["-c", "push.followTags=false", "-c", "push.recurseSubmodules=no", "push", "--porcelain", "--no-progress", remote, &format!("{sha}:refs/heads/{branch}")].iter().map(|s| s.to_string()).collect()
}

/// After the app's own git write in `dir` was killed at its time limit: git's `index.lock` it left behind (killing the
/// process skips git's cleanup, so every later git write there would fail with "index.lock: File exists") is removed
/// only when it is evidently the app's: made after `started`, and `held` (the write never got past its locked phase:
/// HEAD didn't move). git releases the lock before post-commit/post-merge hooks and `gc --auto` run, so once HEAD
/// moved a lock there may be someone else's (the user's terminal or IDE runs git in the project folder freely, and
/// an agent may refresh a worker's index): it is only reported. A line for the message ("" when there was none).
fn own_index_lock(tools: &Tools, dir: &Path, started: std::time::SystemTime, held: bool) -> String {
    let o = git(tools, dir, &["rev-parse", "--git-path", "index.lock"]);
    if !o.ok() || o.text().is_empty() {
        return String::new();
    }
    let p = PathBuf::from(o.text());
    let path = if p.is_absolute() { p } else { dir.join(p) };
    let Ok(meta) = std::fs::metadata(&path) else { return String::new() };
    if !held || !meta.modified().is_ok_and(|m| m >= started) {
        return format!(" (index.lock이 남아 있어요: 다른 git 작업이 없는지 확인한 뒤 {}을 지워 주세요)", path.display());
    }
    // the killed process may hold it a moment longer
    for k in 0..10 {
        if std::fs::remove_file(&path).is_ok() || !path.exists() {
            return " (앱의 git이 남긴 index.lock은 지웠어요)".into();
        }
        std::thread::sleep(Duration::from_millis(100 * (k + 1)));
    }
    format!(" (index.lock을 지우지 못했어요: 다른 git 작업이 없는지 확인한 뒤 {}을 지워 주세요)", path.display())
}

/// A commit message as git stored it holds the one given: git's cleanup only trims whitespace and blank lines, and a
/// commit-msg or prepare-commit-msg hook may add around it (a ticket prefix, a Change-Id or Signed-off-by trailer).
fn same_message(stored: &str, given: &str) -> bool {
    let s: Vec<&str> = stored.split_whitespace().collect();
    let g: Vec<&str> = given.split_whitespace().collect();
    !g.is_empty() && s.windows(g.len()).any(|w| w == g.as_slice())
}

/// The files HEAD's commit changed, sorted.
fn head_files(tools: &Tools, dir: &Path) -> Vec<String> {
    let files = git(tools, dir, &["diff-tree", "--no-commit-id", "--name-only", "-r", "-z", "--no-renames", "HEAD"]);
    let mut got: Vec<String> = files.stdout.split('\0').filter(|f| !f.is_empty()).map(String::from).collect();
    got.sort();
    got
}

/// The app's commit (on the requested tip) against the files the card showed: None when they are the same, else
/// what differs, for the card.
fn file_mismatch(head: Option<&str>, got: &[String], want: &[String]) -> Option<String> {
    if got == want {
        return None;
    }
    let extra: Vec<String> = got.iter().filter(|f| !want.contains(f)).cloned().collect();
    let missing: Vec<String> = want.iter().filter(|f| !got.contains(f)).cloned().collect();
    let mut parts = vec![];
    if !extra.is_empty() {
        parts.push(format!("카드에 없던 파일이 들어갔어요: {}", crate::worktree::short_list(&extra)));
    }
    if !missing.is_empty() {
        parts.push(format!("빠진 파일: {}", crate::worktree::short_list(&missing)));
    }
    Some(format!(
        "앱 커밋({})의 파일이 카드와 달라요 (커밋 훅이 바꿨을 수 있어요). {}. 이 커밋은 푸시하지 않아요: 거부하고, 워커가 커밋을 확인한 뒤 다시 요청하게 하세요",
        head.map(sha7).unwrap_or(""),
        parts.join(" / ")
    ))
}

fn identity(tools: &Tools, dir: &Path) -> Vec<&'static str> {
    let has = |k: &str| {
        let o = git(tools, dir, &["config", k]);
        o.ok() && !o.text().is_empty()
    };
    if has("user.name") && has("user.email") {
        vec![]
    } else {
        vec!["-c", "user.name=Maestro", "-c", "user.email=maestro@localhost"]
    }
}

// ---------- approve ----------

#[cfg(test)]
thread_local! {
    /// runs between the push and the check before create (a test moves the remote branch there)
    static BEFORE_CREATE: std::cell::RefCell<Option<Box<dyn Fn()>>> = std::cell::RefCell::new(None);
}

fn before_create() {
    #[cfg(test)]
    BEFORE_CREATE.with(|h| {
        if let Some(f) = h.borrow().as_ref() {
            f();
        }
    });
}

/// Writes the request's progress (on disk before the call it guards) and shows it on the card.
fn progress(host: &dyn Host, merges: &Merges, id: u64, f: impl FnOnce(&mut PrProgress)) -> Result<PrProgress, String> {
    let req = merges.update(id, |r| f(r.pr.get_or_insert_with(Default::default))).ok_or("이미 처리된 요청이에요")?;
    host.emit("maestro://merge", json!(req));
    Ok(req.pr.unwrap_or_default())
}

/// The card's approval in PR mode. Each step is skipped when its result is already recorded, so a second click
/// carries on where the first stopped. Err: the Korean reason; the request stays.
pub fn approve(host: &dyn Host, merges: &Merges, prs: &Prs, id: u64, opts: PrOptions) -> Result<MergeResult, String> {
    let req = merges.get(id).ok_or("이미 처리된 요청이에요")?;
    let _one = merges.lock_pr(&req.worker_path)?;
    let req = merges.get(id).ok_or("이미 처리된 요청이에요")?;
    match run_approve(host, merges, prs, &req, &opts) {
        Ok(r) => Ok(r),
        Err(e) => {
            let _ = progress(host, merges, id, |p| p.error = Some(e.clone()));
            Err(e)
        }
    }
}

const APP_COMMITTED: &str = "앱이 사용자 확인으로 커밋했어요";
const UNSURE_PUSH: &str = "푸시가 끝났는지 확인하지 못했어요. 다시 누르면 같은 커밋으로 안전하게 다시 시도해요";

/// Why a push didn't put the commit on the remote, from git's `--porcelain` status line (`!\t<src>:<dst>\t[rejected]
/// (non-fast-forward)`) and stderr. Only a push that may still have gone through (cut off by the time limit, killed,
/// or exited 0 with the tip not seen after) is "unsure"; any other failure shows git's own words.
pub fn push_failure(out: &Out) -> String {
    if out.spawn_err.is_some() {
        return "git을 실행하지 못했어요".into();
    }
    if out.timed_out || out.code.is_none() || out.ok() {
        return UNSURE_PUSH.into();
    }
    let status = out.stdout.lines().filter_map(|l| l.split('\t').nth(2)).find(|s| s.starts_with('['));
    if let Some(st) = status {
        let reason = st.split_once('(').map(|(_, r)| r.trim_end().trim_end_matches(')')).unwrap_or("");
        if st.starts_with("[rejected]") && ["non-fast-forward", "fetch first", "stale info"].iter().any(|k| reason.contains(k)) {
            return DIVERGED.into();
        }
        if st.starts_with("[remote rejected]") {
            let remote: Vec<&str> = out.stderr.lines().map(str::trim).filter_map(|l| l.strip_prefix("remote:")).map(str::trim).filter(|l| !l.is_empty()).collect();
            let said = crate::util::tail_bytes(&remote.join("\n"), 600).trim().to_string();
            return if said.is_empty() { format!("원격이 푸시를 거부했어요 ({reason})") } else { format!("원격이 푸시를 거부했어요 ({reason}):\n{said}") };
        }
    }
    // credentials: the phrases git and its helpers print, never a bare number (stdout has the pushed SHA)
    let err = out.stderr.to_lowercase();
    let denied = err.split_once("permission to ").is_some_and(|(_, r)| r.contains(" denied"));
    if denied || ["returned error: 403", "authentication failed", "could not read username", "could not read password", "terminal prompts disabled", "permission denied (publickey"].iter().any(|k| err.contains(k)) {
        return "git 자격 증명이 없거나 권한이 없어 푸시하지 못했어요. 터미널에서 한 번 직접 푸시해 자격 증명을 확인해 주세요".into();
    }
    format!("푸시하지 못했어요: {}", out.tail())
}
const DIVERGED: &str = "원격에 같은 이름의 브랜치가 다른 내용으로 있어요. 덮어쓰지 않아요 (강제 푸시 안 함)";

fn run_approve(host: &dyn Host, merges: &Merges, prs: &Prs, req: &MergeRequest, opts: &PrOptions) -> Result<MergeResult, String> {
    let tools = host.tools();
    let (id, p) = (req.id, &req.plan);
    let (root, worker) = (Path::new(&req.root), Path::new(&req.worker_path));
    // S0: remote, gh and login now; the worker still on the requested branch
    let ready = github::check(&tools, &req.root, &p.base, false);
    if !ready.ok {
        return Err(ready.message);
    }
    let remote = ready.remote.clone().unwrap_or_default();
    if head_branch(&tools, worker).as_deref() != Some(p.branch.as_str()) {
        return Err(format!("워커가 다른 브랜치로 옮겼어요 (요청 때 {})", p.branch));
    }
    // one record per worker: a PR of another branch still open (or waiting to merge) is never replaced by this one,
    // which would stop it being polled and noted. Read once more first: it may be merged or closed by now
    if let Some(old) = prs.get(&req.worker_path).filter(|r| r.branch != p.branch && r.state == "OPEN") {
        refresh_record(host, prs, &tools, &old)?;
        if let Some(old) = prs.get(&req.worker_path).filter(|r| r.branch != p.branch && r.state == "OPEN") {
            return Err(format!("이 워커의 다른 브랜치({})에 열린 PR #{}이 있어요. 그 PR이 병합되거나 닫힌 뒤 다시 요청하게 하세요", old.branch, old.number));
        }
    }
    let locals = local_paths(&req.root, &req.worker_path, &host.home());
    let start = req.pr.clone().unwrap_or_default();
    let fallback = req.pr_draft.clone().unwrap_or_default();
    let title = sanitize_title(if opts.title.trim().is_empty() { if start.title.is_empty() { &fallback.title } else { &start.title } } else { &opts.title }, &locals);
    if title.is_empty() {
        return Err("PR 제목을 적어 주세요".into());
    }
    let body_in = if opts.body.trim().is_empty() { if start.body.is_empty() { fallback.body.clone() } else { start.body.clone() } } else { opts.body.clone() };
    let body = sanitize_body(&body_in, &locals);
    let mut prog = progress(host, merges, id, |g| {
        g.title = title.clone();
        g.body = body.clone();
        g.draft = opts.draft;
        g.error = None;
        g.notice = None;
    })?;
    // a commit cut off by a crash (no approval runs while this one holds the worker): did it land?
    if prog.step == Step::Committing {
        prog = reconcile_commit(host, merges, &tools, req, &prog)?;
    }
    if opts.check_only {
        return check_created(host, merges, prs, req, &tools, &ready, &prog);
    }

    // S1: the app commits the worker's tracked changes, only with the user's tick and message
    if !p.worker_dirty.is_empty() && prog.app_commit.is_none() {
        prog = commit(host, merges, &tools, req, opts)?;
    }
    // the app's commit holds files the card didn't show (or lacks some): never pushed by a plain "계속"
    if let Some(why) = &prog.commit_mismatch {
        return Err(why.clone());
    }
    let push_sha = prog.app_commit.clone().unwrap_or_else(|| p.tip_sha.clone());
    // where a step that didn't happen goes back to: an earlier create not found yet stays unconfirmed
    let back = if prog.create_unresolved() {
        Step::Unconfirmed
    } else if prog.app_commit.is_some() {
        Step::Committed
    } else {
        Step::None
    };
    // the worker's PR as last polled may be merged or closed on GitHub by now (the poller is slow, paused, or hasn't
    // run since a restart): recorded and told first, so neither the squash check nor the lookup below goes by it
    if let Some(old) = prs.get(&req.worker_path).filter(|r| r.state == "OPEN" && r.branch == p.branch) {
        refresh_record(host, prs, &tools, &old)?;
    }
    let heads = prs.merged_heads(&req.worker_path, &p.branch);
    // the PR says so when the app made a commit (the card's text never has the line)
    let body = match prog.app_commit.as_ref().and(prog.commit_message.as_deref()) {
        Some(m) if !body.contains(APP_COMMITTED) => {
            let line = sanitize_body(&format!("- {APP_COMMITTED}: {}", m.lines().next().unwrap_or("")), &locals);
            let b = format!("{}\n{line}", body.trim_end());
            progress(host, merges, id, |g| g.body = b.clone())?;
            b
        }
        _ => body,
    };

    // S2: the base on the remote; a PR must not carry local base commits the remote lacks
    fetch_base(&tools, root, &remote, &p.base)?;
    // the worker's last PR went in as one new commit: its old commits would come back (the plan may not have had
    // that commit yet: a merge seen by the poller, a folder that wasn't fast-forwarded)
    if let Some((n, commit)) = prs.get(&req.worker_path).filter(|r| r.branch == p.branch).and_then(|r| squashed(&tools, root, &r)) {
        if has_object(&tools, root, &commit) && !is_ancestor(&tools, root, &commit, &push_sha) {
            return Err(crate::worktree::squash_refusal(n));
        }
    }
    // the commits the PR would carry that came from the local base (where the worker branched off or merged it
    // in) and that the remote base lacks
    let tracking = format!("refs/remotes/{remote}/{}", p.base);
    let mb = git(&tools, root, &["merge-base", &push_sha, &format!("refs/heads/{}", p.base)]);
    if mb.ok() && !mb.text().is_empty() {
        let mut range = vec!["rev-list".to_string(), mb.text().to_string(), format!("^{tracking}")];
        range.extend(heads.iter().filter(|h| has_object(&tools, root, h)).map(|h| format!("^{h}")));
        let extra: Vec<String> = git(&tools, root, &range.iter().map(String::as_str).collect::<Vec<_>>()).stdout.lines().map(|l| sha7(l).to_string()).collect();
        if !extra.is_empty() {
            let shown = extra.iter().take(5).cloned().collect::<Vec<_>>().join(" ");
            return Err(format!("로컬 기준 브랜치에 원격에 없는 커밋 {}개가 이 PR에 같이 들어가요 ({shown} …). 먼저 기준 브랜치를 푸시하거나 정리해 주세요.", extra.len()));
        }
    }

    // S3: push the recorded commit, never forced
    let remote_tip = ls_remote(&tools, worker, &remote, &p.branch)?;
    if remote_tip.as_deref() != Some(push_sha.as_str()) {
        if let Some(rt) = &remote_tip {
            if !(has_object(&tools, worker, rt) && is_ancestor(&tools, worker, rt, &push_sha)) {
                return Err(DIVERGED.into());
            }
        }
        progress(host, merges, id, |g| {
            g.step = Step::Pushing;
            g.push_sha = Some(push_sha.clone());
        })?;
        let args = push_args(&remote, &push_sha, &p.branch);
        let out = exec::run(tools.git(worker, &args.iter().map(String::as_str).collect::<Vec<_>>(), tools.t.push).network());
        let after = ls_remote(&tools, worker, &remote, &p.branch).ok().flatten();
        if after.as_deref() != Some(push_sha.as_str()) {
            progress(host, merges, id, |g| g.step = back)?;
            return Err(push_failure(&out));
        }
    }
    prog = progress(host, merges, id, |g| {
        if !matches!(g.step, Step::Creating | Step::Unconfirmed) {
            g.step = Step::Pushed;
        }
        g.push_sha = Some(push_sha.clone());
    })?;
    before_create();

    // S4: the last check right before create (D7)
    let again = github::check(&tools, &req.root, &p.base, false);
    if !again.ok {
        return Err(again.message);
    }
    let mut tips = ls_remote_all(&tools, worker, &remote, &[&p.branch, &p.base])?;
    let (now_tip, base_tip) = (tips.remove(0), tips.remove(0));
    if now_tip.as_deref() != Some(push_sha.as_str()) {
        if prog.step == Step::Pushed {
            progress(host, merges, id, |g| g.step = back)?;
        }
        return Err(format!("원격 브랜치 끝이 승인한 커밋과 달라요 ({} ≠ {})", now_tip.as_deref().map(sha7).unwrap_or("없음"), sha7(&push_sha)));
    }
    if base_tip.is_none() {
        return Err(format!("원격에 기준 브랜치({})가 없어요", p.base));
    }
    if p.branch.eq_ignore_ascii_case(&p.base) {
        return Err("워커 브랜치가 기준 브랜치와 같아요".into());
    }
    // a create ran (in this request or the one it replaced) and its PR wasn't found yet
    let attempted = prog.create_unresolved();
    let rows = match github::open_prs(&tools, &ready, &p.branch, None) {
        Ok(rows) => rows,
        Err((GhErr::RateLimited, _)) => return Err(rate_message(&tools, &ready, host.now())),
        Err((GhErr::Auth, _)) => return Err("gh에 로그인돼 있지 않아요. 터미널에서 직접 `gh auth login`을 실행한 뒤 다시 누르세요".into()),
        Err(_) => return Err("PR이 이미 있는지 확인하지 못해 만들지 않았어요. 다시 누르세요".into()),
    };
    if rows.len() >= 2 {
        let nums: Vec<String> = rows.iter().map(|r| format!("#{}", r.number)).collect();
        return Err(format!("이 브랜치에 열린 PR이 여러 개예요 ({}). 직접 정리해 주세요", nums.join(", ")));
    }
    if let Some(row) = rows.first() {
        if row.base_ref_name != p.base {
            return Err(format!("열린 PR #{}이 다른 기준({})을 향해요", row.number, row.base_ref_name));
        }
        let known = prs.get(&req.worker_path).filter(|r| r.number == row.number);
        let mut note = String::new();
        if opts.update_body && known.as_ref().is_some_and(|r| r.app_created) {
            note = edit_pr(&tools, &ready, row.number, &title, &body, id);
        }
        let how = if known.is_some() { "pr-updated" } else { "recovered" };
        let rec = record(req, &ready, row.number, &row.url, &push_sha, attempted || known.as_ref().is_some_and(|r| r.app_created), row.is_draft);
        return Ok(opened(host, merges, prs, req, rec, how, prog.app_commit.is_some(), &note));
    }
    if attempted {
        let now = host.now();
        let waited = prog.create_attempt_at.is_some_and(|t| now >= t + 30_000);
        if !(opts.recreate && waited) {
            progress(host, merges, id, |g| {
                g.step = Step::Unconfirmed;
                g.last_empty_lookup_at = Some(now);
            })?;
            return Err(if opts.recreate {
                "PR을 만든 지 30초가 지나지 않았어요. 조금 뒤 '다시 만들기'를 누르세요 (그때 한 번 더 찾아봐요)".into()
            } else {
                "아직 PR이 보이지 않아요. 잠시 뒤 '다시 확인'하거나 '다시 만들기'를 누르세요".into()
            });
        }
    }

    // S5: create, once; anything unclear is reconciled with a lookup
    let now = host.now();
    // a create that certainly made nothing goes back to this: an earlier unclear one is still unclear
    let prior = prog.create_attempt_at;
    let undo = move |g: &mut PrProgress| {
        g.step = if prior.is_some() { Step::Unconfirmed } else { Step::Pushed };
        g.create_attempt_at = prior;
    };
    progress(host, merges, id, |g| {
        g.step = Step::Creating;
        g.create_attempt_at = Some(now);
    })?;
    let dir = crate::util::unique_temp_dir(&format!("maestro-pr-{id}"));
    let file = dir.join("body.md");
    let written = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&file, &body));
    if let Err(e) = written {
        let _ = std::fs::remove_dir_all(&dir);
        progress(host, merges, id, undo)?;
        return Err(format!("PR 본문 파일을 쓰지 못했어요: {e}"));
    }
    let args = github::create_args(&ready.slug, &p.base, &p.branch, &title, &file.to_string_lossy(), opts.draft);
    let out = tools.gh_write(&args.iter().map(String::as_str).collect::<Vec<_>>(), tools.t.create);
    let _ = std::fs::remove_dir_all(&dir);
    if out.ok() {
        if let Some((url, n)) = github::pr_url_in(&out.stdout) {
            let mut rec = record(req, &ready, n, &url, &push_sha, true, opts.draft);
            if let Ok(v) = github::view_pr(&tools, &ready, n, "number,url,state,isDraft,headRefOid,headRefName,baseRefName") {
                let live = github::live_from_view(&v);
                rec.url = if live.url.is_empty() { rec.url } else { live.url };
                rec.is_draft = live.is_draft;
                if !live.head_ref_oid.is_empty() {
                    rec.head_sha = live.head_ref_oid;
                }
            }
            return Ok(opened(host, merges, prs, req, rec, "pr", prog.app_commit.is_some(), ""));
        }
    }
    match github::classify(&out) {
        GhErr::Auth | GhErr::Validation | GhErr::Refused | GhErr::RateLimited | GhErr::SpawnFailed if !out.ok() => {
            progress(host, merges, id, undo)?;
            let why = match github::classify(&out) {
                GhErr::RateLimited => rate_message(&tools, &ready, host.now()),
                GhErr::Auth => "gh에 로그인돼 있지 않아요. 터미널에서 직접 `gh auth login`을 실행한 뒤 다시 누르세요".into(),
                GhErr::SpawnFailed => "gh를 실행하지 못했어요".into(),
                _ if opts.draft && out.tail().to_lowercase().contains("draft pull requests are not supported") => {
                    "이 저장소는 초안 PR을 지원하지 않아요 (GitHub Free의 비공개 저장소 등). 카드에서 '초안으로'를 끄고 다시 누르세요".into()
                }
                _ => format!("GitHub가 PR을 만들지 않았어요: {}", out.tail()),
            };
            Err(why)
        }
        _ => {
            // maybe made: look for it (0 s, +2 s, +5 s), never create again here
            for pause in tools.t.recover.clone() {
                std::thread::sleep(pause);
                if let Ok(rows) = github::open_prs(&tools, &ready, &p.branch, Some(&p.base)) {
                    if let [row] = rows.as_slice() {
                        let rec = record(req, &ready, row.number, &row.url, &push_sha, true, row.is_draft);
                        return Ok(opened(host, merges, prs, req, rec, "recovered", prog.app_commit.is_some(), ""));
                    }
                }
            }
            // the request was replaced meanwhile: the new one carried the attempt over (`Merges::add`)
            if progress(host, merges, id, |g| g.step = Step::Unconfirmed).is_err() {
                eprintln!("[maestro] unclear create for {}: the request went meanwhile", req.worker_path);
            }
            Err("PR이 만들어졌는지 확인하지 못했어요. '다시 확인'은 찾기만 하고, '다시 만들기'는 30초 뒤 다시 찾아 없을 때만 만들어요".into())
        }
    }
}

/// The recorded open PR read once more: merged or closed outside the app is recorded (merged heads included) and
/// the conductor told, as the poller would have. Err: it couldn't be read, so nothing is decided on a stale record.
fn refresh_record(host: &dyn Host, prs: &Prs, tools: &Tools, old: &PrRecord) -> Result<(), String> {
    let live = view_fields(tools, old, "state,mergeCommit,headRefOid").map_err(|e| format!("기존 PR #{}을 확인하지 못했어요: {e}", old.number))?;
    match live.state.as_str() {
        "MERGED" => {
            let (method, head) = match &old.merge_attempt {
                Some(a) => (a.method.clone(), a.seen_head.clone()),
                None => ("github".to_string(), live.head_ref_oid.clone()),
            };
            if let Some((r, first)) = mark_merged(prs, old, &method, live.merge_commit.clone(), &head, host.now()) {
                host.emit("maestro://pr", json!(r));
                if r.note_owed {
                    note_merge_owed(host, prs, &r, "그대로예요(대기열·자동 병합으로 합쳐짐: 사용자가 '기준 브랜치 받기'를 누를 수 있어요)", false);
                } else if first {
                    note_pr(host, &r, &note_github_merged(&r, live.merge_commit.as_deref()));
                }
            }
        }
        "CLOSED" => {
            let now = host.now();
            let mut closed = false;
            if let Some(r) = prs.update(&old.worker_path, |r| {
                if r.state == "OPEN" {
                    closed = true;
                    r.state = "CLOSED".into();
                    r.merge_attempt = None;
                    r.queued = false;
                    r.checked_at = r.checked_at.max(now);
                }
            }) {
                if closed {
                    host.emit("maestro://pr", json!(r));
                    note_pr(host, &r, &note_closed(&r));
                }
            }
        }
        _ => {}
    }
    Ok(())
}

/// S1: `git commit --only` of exactly the files the card showed, with the user's message. Untracked files are never
/// added. The guard blocks the agent's git writes in that folder meanwhile.
fn commit(host: &dyn Host, merges: &Merges, tools: &Tools, req: &MergeRequest, opts: &PrOptions) -> Result<PrProgress, String> {
    let p = &req.plan;
    let worker = Path::new(&req.worker_path);
    let n = p.worker_dirty.len();
    let message = opts.commit_message.as_deref().map(|m| merge::clean_text(m, 2000)).filter(|m| !m.is_empty()).ok_or_else(|| {
        format!("워커에 커밋하지 않은 변경이 {n}개 있어요: 카드에서 '커밋하고 진행'을 고르고 메시지를 적거나, 워커가 커밋한 뒤 다시 요청하게 하세요")
    })?;
    if rev(tools, worker, &format!("refs/heads/{}", p.branch)).as_deref() != Some(p.tip_sha.as_str()) {
        return Err("요청 뒤 워커 브랜치에 새 커밋이 생겼어요: 다시 요청하게 하세요".into());
    }
    let now = dirty_paths(tools, worker)?;
    let mut want = p.worker_dirty.clone();
    want.sort();
    if now != want {
        return Err(format!("카드에 보인 뒤 변경 파일이 달라졌어요: {}", crate::worktree::short_list(&now)));
    }
    if let Some(what) = in_progress(tools, worker)? {
        return Err(format!("워커에서 다른 git 작업({what})이 진행 중이에요"));
    }
    if host.worker_busy(&req.worker_path) {
        return Err("워커가 작업 중이에요. 끝난 뒤 누르세요".into());
    }
    progress(host, merges, req.id, |g| {
        g.step = Step::Committing;
        g.commit_parent = Some(p.tip_sha.clone());
        g.commit_message = Some(message.clone());
    })?;
    let _lock = merges.commit_lock(&req.worker_path, &req.root);
    let mut args: Vec<&str> = vec!["--literal-pathspecs"];
    let id = identity(tools, worker);
    args.extend(id.iter().copied());
    // hooks run, as for any commit
    args.extend(["commit", "--only", "-m", &message, "--pathspec-from-file=-", "--pathspec-file-nul"]);
    let input = want.join("\0").into_bytes();
    let started = std::time::SystemTime::now();
    let out = exec::run(tools.git(worker, &args, tools.t.commit).input(input));
    let head = rev(tools, worker, "HEAD");
    let parent = rev(tools, worker, "HEAD^");
    let got = head_files(tools, worker);
    let on_tip = parent.as_deref() == Some(p.tip_sha.as_str());
    let made = on_tip && got == want;
    // the app's commit is there whatever git's exit said: a post-commit hook or `gc --auto` past the limit is killed
    // after the commit and its ref were written
    let ours = on_tip && same_message(git(tools, worker, &["log", "-1", "--format=%B", "HEAD"]).text(), &message);
    let landed = made && ours;
    // HEAD still at the tip: git never got past its locked phase, so a lock left is the app's
    let lock = if out.timed_out { own_index_lock(tools, worker, started, head.as_deref() == Some(p.tip_sha.as_str())) } else { String::new() };
    if !out.ok() || !made {
        // the app's commit on the tip (git said so, or its message is there) whose files aren't the card's
        let mismatch = if (out.ok() || ours) && on_tip { file_mismatch(head.as_deref(), &got, &want) } else { None };
        let why = if let Some(m) = &mismatch {
            format!("{m}{lock}")
        } else if out.timed_out && landed {
            format!("커밋은 됐지만({}) 커밋 뒤 훅(post-commit 등)이 시간 제한({}초)을 넘겨 멈췄어요. 다시 누르면 이 커밋으로 이어서 푸시해요{lock}", head.as_deref().map(sha7).unwrap_or(""), tools.t.commit.round())
        } else if out.timed_out {
            format!("커밋이 시간 제한({}초)을 넘겨 멈췄어요: 커밋 훅(pre-commit 등)이 너무 오래 걸렸어요{lock}", tools.t.commit.round())
        } else if out.ok() {
            "앱 커밋을 확인하지 못했어요".to_string()
        } else {
            format!("커밋하지 못했어요: {}", out.tail())
        };
        // a commit that did land on top of the tip stays recorded as made, so it's never made twice; one whose files
        // aren't the card's is never pushed (`commit_mismatch`)
        if landed || mismatch.is_some() {
            progress(host, merges, req.id, |g| {
                g.step = Step::Committed;
                g.app_commit = head.clone();
                g.commit_mismatch = mismatch;
            })?;
        } else {
            progress(host, merges, req.id, |g| g.step = Step::None)?;
        }
        return Err(why);
    }
    progress(host, merges, req.id, |g| {
        g.step = Step::Committed;
        g.app_commit = head.clone();
    })
}

/// A request left in `Committing` (the app went down during its commit): the commit counts as made when it sits on the
/// recorded parent with the user's message; HEAD still at the parent means it never landed; anything else is a
/// branch that moved meanwhile. Reads only. Run by `resume` and by the next approval (which may come first).
fn reconcile_commit(host: &dyn Host, merges: &Merges, tools: &Tools, req: &MergeRequest, g: &PrProgress) -> Result<PrProgress, String> {
    let worker = Path::new(&req.worker_path);
    let parent = g.commit_parent.clone().unwrap_or_default();
    let head = rev(tools, worker, "HEAD");
    let head_parent = rev(tools, worker, "HEAD^");
    let stored = git(tools, worker, &["log", "-1", "--format=%B"]).text().to_string();
    let same_msg = g.commit_message.as_deref().is_some_and(|m| same_message(&stored, m));
    if head_parent.as_deref() == Some(parent.as_str()) && same_msg {
        let mut want = req.plan.worker_dirty.clone();
        want.sort();
        let mismatch = file_mismatch(head.as_deref(), &head_files(tools, worker), &want);
        progress(host, merges, req.id, |x| {
            x.step = Step::Committed;
            x.app_commit = head.clone();
            if mismatch.is_some() {
                x.error = mismatch.clone();
            }
            x.commit_mismatch = mismatch;
        })
    } else if head.as_deref() == Some(parent.as_str()) {
        progress(host, merges, req.id, |x| {
            x.step = Step::None;
            x.error = Some("앱이 커밋 도중 꺼졌어요".into());
        })
    } else {
        progress(host, merges, req.id, |x| {
            x.step = Step::None;
            x.error = Some("워커 브랜치가 바뀌었어요: 다시 요청하게 하세요".into());
        })
    }
}

/// `gh pr edit` of a PR the app made, only when the user asked. Returns a line for the message ("" when done).
fn edit_pr(tools: &Tools, ready: &PrReadiness, number: u64, title: &str, body: &str, id: u64) -> String {
    let dir = crate::util::unique_temp_dir(&format!("maestro-pr-edit-{id}"));
    let file = dir.join("body.md");
    if std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&file, body)).is_err() {
        return " (본문은 갱신하지 못했어요)".into();
    }
    let n = number.to_string();
    let f = file.to_string_lossy().to_string();
    let out = tools.gh_write(&["pr", "edit", &n, "-R", &ready.slug, "--title", title, "--body-file", &f], tools.t.edit);
    let _ = std::fs::remove_dir_all(&dir);
    if out.ok() {
        String::new()
    } else {
        format!(" (본문은 갱신하지 못했어요: {})", out.tail())
    }
}

fn record(req: &MergeRequest, ready: &PrReadiness, number: u64, url: &str, head: &str, app_created: bool, draft: bool) -> PrRecord {
    PrRecord {
        root: req.root.clone(),
        maestro_path: req.maestro_path.clone(),
        worker_path: req.worker_path.clone(),
        worker_name: req.worker_name.clone(),
        branch: req.plan.branch.clone(),
        base: req.plan.base.clone(),
        remote: ready.remote.clone().unwrap_or_default(),
        host: ready.host.clone(),
        owner: ready.owner.clone(),
        repo: ready.repo.clone(),
        number,
        url: url.to_string(),
        app_created,
        state: "OPEN".into(),
        is_draft: draft,
        head_sha: head.to_string(),
        mergeable: "UNKNOWN".into(),
        ..Default::default()
    }
}

/// S6: the PR is known. Kept, shown, the request finished, the conductor told, the poller woken.
#[allow(clippy::too_many_arguments)]
fn opened(host: &dyn Host, merges: &Merges, prs: &Prs, req: &MergeRequest, mut rec: PrRecord, how: &'static str, app_commit: bool, extra: &str) -> MergeResult {
    rec.checked_at = host.now();
    // a new PR starts with no merge of its own and no CI note; CI results are only ever those of the head shown
    match prs.get(&req.worker_path).filter(|old| old.number == rec.number) {
        None => {
            rec.merged = None;
            rec.after_merge = None;
            rec.checks = pending_checks(&rec.head_sha);
        }
        Some(old) => {
            rec.checks = if old.checks.head_sha == rec.head_sha { old.checks } else { pending_checks(&rec.head_sha) };
            rec.review_decision = old.review_decision;
            rec.merged = old.merged;
        }
    }
    let message = match how {
        "pr" => format!("PR #{}을 만들었어요{}", rec.number, if rec.is_draft { " (초안)" } else { "" }),
        "pr-updated" => format!("푸시해서 PR #{}을 갱신했어요{extra}", rec.number),
        _ => format!("기존 PR #{}을 찾아 이어 썼어요{extra}", rec.number),
    };
    // the card that asked is gone once the request is finished: what it should have said stays on the PR
    rec.notice = (how != "pr" || !extra.is_empty()).then(|| message.clone());
    let rec = prs.upsert(rec);
    host.emit("maestro://pr", json!(rec));
    let commits = req.plan.commit_count + usize::from(app_commit);
    merge::finish_host(host, merges, req, how, &note_opened(&rec, commits, how));
    poll_soon(&rec.worker_path);
    host.wake();
    MergeResult { outcome: how, commit: Some(rec.head_sha.clone()), message, pr: Some(rec) }
}

/// Checks not known yet for `head` (a new PR, a push): pending until the next poll.
fn pending_checks(head: &str) -> Checks {
    Checks { state: "pending".into(), head_sha: head.to_string(), ..Default::default() }
}

/// The worker's last PR went in as one new commit, so the branch's own commits are not on the base: a squash by the
/// app, or a merge outside the app whose commit doesn't have the head in its history (squash or rebase on GitHub,
/// judged only when both commits are here). (number, that commit)
pub fn squashed(tools: &Tools, root: &Path, rec: &PrRecord) -> Option<(u64, String)> {
    let m = rec.merged.as_ref()?;
    let commit = m.commit.clone()?;
    let one = match m.method.as_str() {
        "squash" => true,
        "github" => !m.head.is_empty() && has_object(tools, root, &commit) && has_object(tools, root, &m.head) && !is_ancestor(tools, root, &m.head, &commit),
        _ => false,
    };
    one.then_some((rec.number, commit))
}

/// "GitHub 요청 한도에 걸렸어요 (약 N분 뒤 다시)", from `gh api rate_limit` when it answers.
fn rate_message(tools: &Tools, ready: &PrReadiness, now: u64) -> String {
    let mut args: Vec<String> = vec!["api".into(), "rate_limit".into()];
    args.extend(ready.host_args());
    let out = tools.gh_read(&args.iter().map(String::as_str).collect::<Vec<_>>(), tools.t.gh_read);
    match github::graphql_reset(&out.stdout).filter(|r| *r > now) {
        Some(r) => format!("GitHub 요청 한도에 걸렸어요 (약 {}분 뒤 다시)", (r - now).div_ceil(60_000)),
        None => "GitHub 요청 한도에 걸렸어요 (잠시 뒤 다시)".into(),
    }
}

/// The card's reject in PR mode: like R5's, and the note says a branch already pushed stays on the remote. A request
/// whose create may have made a PR looks for it first (read only): one found is recorded, shown and polled like any
/// other, and the note says it stays open.
pub fn reject(host: &dyn Host, merges: &Merges, prs: &Prs, id: u64, reason: Option<String>) -> Result<MergeResult, String> {
    let req = merges.get(id).ok_or("이미 처리된 요청이에요")?;
    let _one = merges.lock_pr(&req.worker_path)?;
    let req = merges.get(id).ok_or("이미 처리된 요청이에요")?;
    let reason = reason.map(|r| merge::clean_text(&r, 500)).filter(|r| !r.is_empty());
    let mut note = merge::merge_note(&req, None, reason.as_deref());
    let mut message = "병합 요청을 거부했어요".to_string();
    if req.pr.as_ref().is_some_and(|g| g.push_sha.is_some() && g.step != Step::None) {
        note.push_str(&format!(" 이미 푸시한 원격 브랜치({})는 남아 있어요.", req.plan.branch));
    }
    let mut found = None;
    if req.pr.as_ref().is_some_and(PrProgress::create_unresolved) {
        match find_created(host, prs, &req) {
            Ok(Some(rec)) => {
                note.push_str(&format!(" 열린 PR #{}은 남아 있어요 ({}).", rec.number, rec.url));
                message.push_str(&format!(": 열린 PR #{}은 남아 있어요", rec.number));
                found = Some(rec);
            }
            Ok(None) => {}
            Err(e) => {
                note.push_str(" 앱이 만든 PR이 있는지 확인하지 못했어요: 사용자가 GitHub에서 확인해야 해요.");
                message.push_str(&format!(". 앱이 만든 PR이 있는지 확인하지 못했어요 ({e}): GitHub에서 직접 확인해 주세요"));
            }
        }
    }
    // the branch's open PR as recorded ("다시 확인" or a restart found it and may have told nobody): it stays open,
    // polled and noted, so the conductor hears of it here
    if found.is_none() {
        if let Some(rec) = prs.get(&req.worker_path).filter(|r| r.state == "OPEN" && r.branch == req.plan.branch) {
            note.push_str(&format!(" 열린 PR #{}은 남아 있어요 ({}).", rec.number, rec.url));
            message.push_str(&format!(": 열린 PR #{}은 남아 있어요", rec.number));
        }
    }
    merge::finish_host(host, merges, &req, "rejected", &note);
    Ok(MergeResult { outcome: "rejected", commit: None, message, pr: found })
}

/// The open PR an unclear create may have made, looked up (read only) and recorded so it is shown and polled.
/// Ok(None): none open for the branch now.
fn find_created(host: &dyn Host, prs: &Prs, req: &MergeRequest) -> Result<Option<PrRecord>, String> {
    let tools = host.tools();
    let ready = github::check(&tools, &req.root, &req.plan.base, false);
    if !ready.ok {
        return Err(ready.message);
    }
    let rows = github::open_prs(&tools, &ready, &req.plan.branch, Some(&req.plan.base)).map_err(|(_, m)| m)?;
    match rows.as_slice() {
        [] => Ok(None),
        // the worker's PR of another branch still open keeps its record: this one is only named
        [row] if prs.get(&req.worker_path).is_some_and(|r| r.branch != req.plan.branch && r.state == "OPEN") => Ok(Some(record(req, &ready, row.number, &row.url, &row.head_ref_oid, true, row.is_draft))),
        [row] => Ok(Some(keep_found(host, prs, req, &ready, row))),
        _ => Err(format!("이 브랜치에 열린 PR이 {}개예요", rows.len())),
    }
}

/// A PR found for the request's branch, recorded as GitHub has it (its own head) so it is shown and polled. Nothing
/// is pushed and the request isn't finished.
fn keep_found(host: &dyn Host, prs: &Prs, req: &MergeRequest, ready: &PrReadiness, row: &github::PrRow) -> PrRecord {
    let head = if row.head_ref_oid.is_empty() { req.pr.as_ref().and_then(|g| g.push_sha.clone()).unwrap_or_else(|| req.plan.tip_sha.clone()) } else { row.head_ref_oid.clone() };
    let known = prs.get(&req.worker_path).filter(|old| old.number == row.number);
    let attempted = req.pr.as_ref().is_some_and(PrProgress::create_unresolved);
    let mut rec = record(req, ready, row.number, &row.url, &head, attempted || known.as_ref().is_some_and(|r| r.app_created), row.is_draft);
    rec.checked_at = host.now();
    match known {
        Some(old) => {
            rec.checks = if old.checks.head_sha == rec.head_sha { old.checks } else { pending_checks(&rec.head_sha) };
            rec.review_decision = old.review_decision;
            rec.merged = old.merged;
        }
        None => rec.checks = pending_checks(&rec.head_sha),
    }
    let rec = prs.upsert(rec);
    host.emit("maestro://pr", json!(rec));
    poll_soon(&rec.worker_path);
    host.wake();
    rec
}

/// The branch's PR was found, but what this request would push isn't on the remote yet (the request replaced an
/// unclear one and the worker committed since): the PR is recorded, the unclear create is over, and the request waits
/// for the user's approval, which pushes the new commits to that PR. The conductor isn't told anything yet.
fn found_unpushed(host: &dyn Host, merges: &Merges, prs: &Prs, req: &MergeRequest, ready: &PrReadiness, row: &github::PrRow) -> Result<MergeResult, String> {
    let rec = keep_found(host, prs, req, ready, row);
    let message = format!("기존 PR #{}을 찾았어요. 새 커밋은 승인하면 푸시해요", rec.number);
    let req = merges
        .update(req.id, |r| {
            r.plan.existing_pr = Some(rec.number);
            let g = r.pr.get_or_insert_with(Default::default);
            g.step = if g.app_commit.is_some() { Step::Committed } else { Step::None };
            g.create_attempt_at = None;
            g.last_empty_lookup_at = None;
            g.error = None;
            g.notice = Some(message.clone());
        })
        .ok_or("이미 처리된 요청이에요")?;
    host.emit("maestro://merge", json!(req));
    Ok(MergeResult { outcome: "found", commit: None, message, pr: Some(rec) })
}

/// `sha` is in the remote branch's history up to `tip` (fetched into the object store only when it isn't here; no
/// ref is written: an empty `--refmap` stops git updating `refs/remotes/<remote>/<branch>` through the remote's
/// configured fetch refspec).
fn remote_contains(tools: &Tools, dir: &Path, remote: &str, branch: &str, tip: &str, sha: &str) -> bool {
    if !has_object(tools, dir, tip) {
        let spec = format!("refs/heads/{branch}");
        exec::run(tools.git(dir, &["fetch", "--no-tags", "--no-write-fetch-head", "--refmap=", remote, &spec], tools.t.fetch).network());
    }
    has_object(tools, dir, tip) && is_ancestor(tools, dir, sha, tip)
}

/// "다시 확인" on a request whose create is unclear: looks for the branch's PR with reads only. Nothing is committed,
/// pushed or created. A PR whose head on the remote is what this request would push finishes the request as the
/// approval would have; a PR still missing this request's commits is recorded and the request waits for an approval.
fn check_created(host: &dyn Host, merges: &Merges, prs: &Prs, req: &MergeRequest, tools: &Tools, ready: &PrReadiness, prog: &PrProgress) -> Result<MergeResult, String> {
    let p = &req.plan;
    let remote = ready.remote.clone().unwrap_or_default();
    let push_sha = prog.app_commit.clone().unwrap_or_else(|| p.tip_sha.clone());
    if let Some(old) = prs.get(&req.worker_path).filter(|r| r.state == "OPEN" && r.branch == p.branch) {
        refresh_record(host, prs, tools, &old)?;
    }
    let tip = ls_remote(tools, Path::new(&req.worker_path), &remote, &p.branch)?;
    let rows = match github::open_prs(tools, ready, &p.branch, None) {
        Ok(rows) => rows,
        Err((GhErr::RateLimited, _)) => return Err(rate_message(tools, ready, host.now())),
        Err((GhErr::Auth, _)) => return Err("gh에 로그인돼 있지 않아요. 터미널에서 직접 `gh auth login`을 실행한 뒤 다시 누르세요".into()),
        Err(_) => return Err("PR이 있는지 확인하지 못했어요. 잠시 뒤 다시 누르세요".into()),
    };
    if rows.len() >= 2 {
        let nums: Vec<String> = rows.iter().map(|r| format!("#{}", r.number)).collect();
        return Err(format!("이 브랜치에 열린 PR이 여러 개예요 ({}). 직접 정리해 주세요", nums.join(", ")));
    }
    if let Some(row) = rows.first() {
        if row.base_ref_name != p.base {
            return Err(format!("열린 PR #{}이 다른 기준({})을 향해요", row.number, row.base_ref_name));
        }
        // done already only when what this request would push is on the PR's branch (its tip, or under commits added
        // on top since: a bot's fix, GitHub's "Update branch") and nothing is left to commit (a request made for
        // uncommitted changes has no commit of its own yet: its approval commits and pushes)
        let nothing_to_commit = p.worker_dirty.is_empty() || prog.app_commit.is_some();
        let pushed = match tip.as_deref() {
            Some(t) if t == push_sha => true,
            Some(t) => remote_contains(tools, Path::new(&req.worker_path), &remote, &p.branch, t, &push_sha),
            None => false,
        };
        if !(nothing_to_commit && pushed) {
            return found_unpushed(host, merges, prs, req, ready, row);
        }
        let head = tip.unwrap_or(push_sha);
        let known = prs.get(&req.worker_path).filter(|r| r.number == row.number);
        let rec = record(req, ready, row.number, &row.url, &head, prog.create_unresolved() || known.as_ref().is_some_and(|r| r.app_created), row.is_draft);
        // a look pushes nothing: "found", even when the PR was recorded already (a create that went through meanwhile)
        return Ok(opened(host, merges, prs, req, rec, "recovered", prog.app_commit.is_some(), ""));
    }
    if !prog.create_unresolved() {
        return Err("열린 PR이 없어요. 승인하면 푸시하고 PR을 만들어요".into());
    }
    let now = host.now();
    progress(host, merges, req.id, |g| {
        g.step = Step::Unconfirmed;
        g.last_empty_lookup_at = Some(now);
    })?;
    Err("아직 PR이 보이지 않아요. 잠시 뒤 '다시 확인'하거나 '다시 만들기'를 누르세요".into())
}

// ---------- restart ----------

/// After a restart: every request left mid-way and every merge click is reconciled with reads only. Writes still
/// needed wait for the user's next click.
pub fn resume(host: &dyn Host, merges: &Merges, prs: &Prs) {
    let tools = host.tools();
    for seen in merges.list().into_iter().filter(|r| r.plan.mode.is_pr()) {
        let Some(step) = seen.pr.as_ref().map(|g| g.step).filter(|s| matches!(s, Step::Committing | Step::Pushing | Step::Creating | Step::Unconfirmed)) else { continue };
        // the card is clickable already: an approval running for the worker now reconciles by itself (a commit left
        // `Committing` too: `run_approve` starts with it), and one that ran meanwhile moved the request on (or
        // finished it)
        let Ok(_one) = merges.lock_pr(&seen.worker_path) else { continue };
        let Some(req) = merges.get(seen.id) else { continue };
        let Some(g) = req.pr.clone().filter(|g| g.step == step) else { continue };
        let worker = Path::new(&req.worker_path);
        match g.step {
            Step::Committing => {
                let _ = reconcile_commit(host, merges, &tools, &req, &g);
            }
            Step::Pushing => {
                let remote = req.plan.remote.clone().or_else(|| github::pick_remote(&tools, Path::new(&req.root), &req.plan.base)).unwrap_or_default();
                let tip = ls_remote(&tools, worker, &remote, &req.plan.branch).ok().flatten();
                let back = if g.app_commit.is_some() { Step::Committed } else { Step::None };
                let _ = if tip.is_some() && tip == g.push_sha {
                    progress(host, merges, req.id, |x| x.step = Step::Pushed)
                } else {
                    progress(host, merges, req.id, |x| {
                        x.step = back;
                        x.error = Some("푸시가 끝났는지 몰라요: 다시 누르면 같은 커밋으로 안전하게 시도해요".into());
                    })
                };
            }
            Step::Creating | Step::Unconfirmed => {
                let ready = github::check(&tools, &req.root, &req.plan.base, false);
                let rows = if ready.ok { github::open_prs(&tools, &ready, &req.plan.branch, Some(&req.plan.base)).map_err(|_| ()) } else { Err(()) };
                match rows {
                    Ok(rows) if rows.len() == 1 => {
                        let row = &rows[0];
                        // only what this request would push was approved; a request that replaced an unclear one
                        // carries the old request's push, and its own commits wait for the user's click
                        let want = g.app_commit.clone().unwrap_or_else(|| req.plan.tip_sha.clone());
                        // and uncommitted changes the request was made for still wait for the user's commit
                        let nothing_to_commit = req.plan.worker_dirty.is_empty() || g.app_commit.is_some();
                        if g.push_sha.as_deref() == Some(want.as_str()) && nothing_to_commit {
                            let rec = record(&req, &ready, row.number, &row.url, &want, true, row.is_draft);
                            opened(host, merges, prs, &req, rec, "recovered", g.app_commit.is_some(), "");
                        } else {
                            let _ = found_unpushed(host, merges, prs, &req, &ready, row);
                        }
                    }
                    Ok(rows) if rows.is_empty() => {
                        let now = host.now();
                        let _ = progress(host, merges, req.id, |x| {
                            x.step = Step::Unconfirmed;
                            x.last_empty_lookup_at = Some(now);
                        });
                    }
                    _ => {
                        let _ = progress(host, merges, req.id, |x| x.step = Step::Unconfirmed);
                    }
                }
            }
            _ => {}
        }
    }
    for seen in prs.list().into_iter().filter(|r| r.merge_attempt.is_some()) {
        // a merge click running now reads its own result; one that ran meanwhile recorded a newer attempt
        let Ok(_one) = merges.lock_pr(&seen.worker_path) else { continue };
        let Some(rec) = prs.get(&seen.worker_path).filter(|r| r.merge_attempt.is_some() && r.merge_attempt == seen.merge_attempt) else { continue };
        let Ok(live) = read_back(&tools, &rec) else { continue };
        let attempt = rec.merge_attempt.clone().unwrap_or_default();
        match live.state.as_str() {
            "MERGED" => {
                if let Some((r, _)) = mark_merged(prs, &rec, &attempt.method, live.merge_commit.clone(), &attempt.seen_head, host.now()) {
                    host.emit("maestro://pr", json!(r));
                    if r.note_owed {
                        note_merge_owed(host, prs, &r, "그대로예요(앱이 꺼진 사이 합쳐짐: '기준 브랜치 받기'를 누를 수 있어요)", false);
                    }
                }
            }
            "OPEN" if live.queued() => {
                if let Some(r) = prs.update(&rec.worker_path, |r| r.queued = true) {
                    host.emit("maestro://pr", json!(r));
                }
            }
            // GitHub shows a queue entry only a moment after the click: the poller decides once the grace is over
            "OPEN" if host.now().saturating_sub(attempt.at) <= ATTEMPT_GRACE => {}
            "OPEN" => {
                if let Some(r) = prs.update(&rec.worker_path, |r| {
                    r.merge_attempt = None;
                    r.notice = None;
                    r.error = Some("병합이 끝나지 않았어요".into());
                }) {
                    host.emit("maestro://pr", json!(r));
                }
            }
            _ => {}
        }
    }
    // merged by a click whose note didn't go out (the app went down during the folder's fast-forward)
    for r in prs.list().into_iter().filter(|r| r.note_owed && r.state == "MERGED") {
        let (folder, updated) = match &r.after_merge {
            Some(a) => (a.message.clone(), a.base_updated.is_some()),
            None => ("그대로예요(앱이 꺼진 사이 합쳐짐: '기준 브랜치 받기'를 누를 수 있어요)".to_string(), false),
        };
        note_merge_owed(host, prs, &r, &folder, updated);
    }
    host.wake();
}

/// The conductor's note for a merge the user clicked, sent once: the record stops owing it afterwards.
fn note_merge_owed(host: &dyn Host, prs: &Prs, r: &PrRecord, folder: &str, updated: bool) {
    let m = r.merged.clone().unwrap_or_default();
    note_pr(host, r, &note_merged(r, &m.method, m.commit.as_deref(), folder, updated));
    prs.update(&r.worker_path, |x| x.note_owed = false);
}

/// The record as merged; true when this call made it so (only that one tells the conductor). A merge by the user's
/// click (not "github") owes the conductor its note until `note_merge_owed` sends it. `checked_at` moves on, so a
/// poll asked before this ends doesn't write its older answer over it.
fn mark_merged(prs: &Prs, rec: &PrRecord, method: &str, commit: Option<String>, head: &str, now: u64) -> Option<(PrRecord, bool)> {
    let mut first = false;
    let r = prs.update(&rec.worker_path, |r| {
        first = r.state != "MERGED";
        if first && method != "github" {
            r.note_owed = true;
        }
        r.checked_at = r.checked_at.max(now);
        r.state = "MERGED".into();
        r.merged = Some(Merged { method: method.to_string(), commit: commit.clone(), head: head.to_string(), at: now });
        if !head.is_empty() && !r.merged_heads.iter().any(|h| h == head) {
            r.merged_heads.push(head.to_string());
        }
        r.merge_attempt = None;
        r.queued = false;
        r.error = None;
        r.notice = None;
    })?;
    Some((r, first))
}

// ---------- polling ----------

static WAKE: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());
/// project folders (path keys; "*" for all) whose PRs "다시 확인" asked to poll now
static FORCE: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// workers (path keys) whose PR just changed under the app (a push, a new head seen): due on the next round
static SOON: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// The worker's PR is due on the poller's next round (the caller wakes it).
pub fn poll_soon(worker: &str) {
    let mut s = SOON.lock().unwrap_or_else(|e| e.into_inner());
    s.push(key(worker));
    let extra = s.len().saturating_sub(64);
    s.drain(..extra);
}

pub fn wake_poller() {
    let (m, cv) = &WAKE;
    *m.lock().unwrap_or_else(|e| e.into_inner()) = true;
    cv.notify_all();
}

/// "다시 확인": the project's PRs (all with None) are due now, past a rate-limit pause once.
pub fn refresh(root: Option<&str>) {
    FORCE.lock().unwrap_or_else(|e| e.into_inner()).push(root.map(key).unwrap_or_else(|| "*".into()));
    wake_poller();
}

const FAST: u64 = 20_000;
const SLOW: u64 = 120_000;
const MAX_BACKOFF: u64 = 300_000;
const BUSY_WAIT: u64 = 5_000;
/// a merge click whose PR is still open and not set to merge automatically after this long has left the queue
const ATTEMPT_GRACE: u64 = 60_000;

/// The poller's memory: when each PR is due, errors and pauses per host.
#[derive(Default, Debug)]
pub struct Sched {
    pub due: HashMap<String, u64>,
    pub errors: HashMap<String, u32>,
    /// host → (paused until, why)
    pub paused: HashMap<String, (u64, String)>,
}

/// 20 s while something is about to change (checks running, mergeability not computed, a merge under way), else 2 min.
pub fn interval(rec: &PrRecord) -> u64 {
    if rec.checks.state == "pending" || rec.mergeable == "UNKNOWN" || rec.queued || rec.merge_attempt.is_some() {
        FAST
    } else {
        SLOW
    }
}

/// Up to the next 5 s mark, so PRs due close together go in one call.
pub fn align(t: u64) -> u64 {
    t.div_ceil(5_000) * 5_000
}

/// base × 2^(errors-1), at most 5 minutes.
pub fn backoff(base: u64, errors: u32) -> u64 {
    if errors == 0 {
        return base;
    }
    base.saturating_mul(1u64 << (errors - 1).min(16)).min(MAX_BACKOFF)
}

fn minutes(until: u64, now: u64) -> u64 {
    until.saturating_sub(now).div_ceil(60_000).max(1)
}

/// One round: polls what is due, host by host in chunks of 25, and returns the ms until the next round.
pub fn poll_once(host: &dyn Host, prs: &Prs, s: &mut Sched) -> u64 {
    let now = host.now();
    let all: Vec<PrRecord> = prs.list().into_iter().filter(PrRecord::polled).collect();
    // the "다시 확인" asks this store's projects are concerned by (the app has one poller; tests run several)
    let forced: Vec<String> = {
        let mut f = FORCE.lock().unwrap_or_else(|e| e.into_inner());
        let (mine, rest): (Vec<String>, Vec<String>) = f.drain(..).partition(|k| k == "*" || all.iter().any(|r| key(&r.root) == *k));
        *f = rest.into_iter().rev().take(32).rev().collect();
        mine
    };
    let soon: Vec<String> = {
        let mut f = SOON.lock().unwrap_or_else(|e| e.into_inner());
        let (mine, rest): (Vec<String>, Vec<String>) = f.drain(..).partition(|k| all.iter().any(|r| key(&r.worker_path) == *k));
        *f = rest.into_iter().rev().take(32).rev().collect();
        mine
    };
    // a pause that is over is said to be over, whether or not a poll of that host follows (an error, no PRs left)
    let over: Vec<String> = s.paused.iter().filter(|(_, (until, _))| *until <= now).map(|(h, _)| h.clone()).collect();
    for h in over {
        s.paused.remove(&h);
        host.emit("maestro://pr-poll", json!({ "host": h, "pausedUntil": 0, "message": "" }));
    }
    if all.is_empty() {
        return u64::MAX;
    }
    let mut bypass = std::collections::HashSet::new();
    for r in &all {
        if forced.iter().any(|f| f == "*" || *f == key(&r.root)) {
            s.due.insert(key(&r.worker_path), now);
            bypass.insert(r.host.clone());
        } else if soon.contains(&key(&r.worker_path)) {
            s.due.insert(key(&r.worker_path), now);
        }
    }
    // a PR action for the worker runs now (it reads the PR itself): looked at again shortly after
    for r in all.iter().filter(|r| host.pr_busy(&r.worker_path)) {
        s.due.insert(key(&r.worker_path), align(now + BUSY_WAIT));
    }
    let due: Vec<(String, PrRecord)> = all.iter().filter(|r| s.due.get(&key(&r.worker_path)).is_none_or(|t| *t <= now)).map(|r| (r.host.clone(), r.clone())).collect();
    let tools = host.tools();
    for (h, chunk) in github::chunks(&due) {
        if let Some((until, _)) = s.paused.get(&h) {
            if *until > now && !bypass.contains(&h) {
                continue;
            }
        }
        poll_chunk(host, prs, s, &tools, &h, &chunk, now);
    }
    // the next round: the earliest due PR, past any pause on its host
    let all: Vec<PrRecord> = prs.list().into_iter().filter(PrRecord::polled).collect();
    let next = all
        .iter()
        .map(|r| {
            let due = s.due.get(&key(&r.worker_path)).copied().unwrap_or(now);
            let paused = s.paused.get(&r.host).map(|(u, _)| *u).unwrap_or(0);
            due.max(paused)
        })
        .min()
        .unwrap_or(u64::MAX);
    next.saturating_sub(now).max(1_000)
}

fn poll_chunk(host: &dyn Host, prs: &Prs, s: &mut Sched, tools: &Tools, h: &str, chunk: &[PrRecord], now: u64) {
    let ready = chunk[0].ready();
    let targets: Vec<Target> = chunk.iter().map(|r| Target { owner: r.owner.clone(), repo: r.repo.clone(), number: r.number }).collect();
    let (out, map) = github::query_status(tools, &ready.host_args(), &targets);
    let parsed = github::parse_status(&out.stdout, &map);
    let Ok((rate, lives)) = parsed else {
        let kind = github::classify(&out);
        let (until, msg) = match kind {
            GhErr::RateLimited => {
                let mut args: Vec<String> = vec!["api".into(), "rate_limit".into()];
                args.extend(ready.host_args());
                let probe = tools.gh_read(&args.iter().map(String::as_str).collect::<Vec<_>>(), tools.t.gh_read);
                let until = github::graphql_reset(&probe.stdout).map(|r| r + 1_000).filter(|u| *u > now).unwrap_or(now + 300_000);
                (until, format!("GitHub 요청 한도에 걸려 약 {}분 동안 상태 확인을 쉬어요", minutes(until, now)))
            }
            GhErr::Auth => (now + 600_000, "gh 로그인이 필요해요 (터미널에서 gh auth login)".to_string()),
            _ => {
                let e = s.errors.entry(h.to_string()).or_insert(0);
                *e += 1;
                let wait = backoff(FAST, *e);
                for r in chunk {
                    s.due.insert(key(&r.worker_path), align(now + wait));
                }
                return;
            }
        };
        s.paused.insert(h.to_string(), (until, msg.clone()));
        host.emit("maestro://pr-poll", json!({ "host": h, "pausedUntil": until, "message": msg }));
        return;
    };
    s.errors.remove(h);
    if let Some(r) = rate.filter(github::low_budget) {
        let until = r.reset_at.max(now) + 1_000;
        let msg = format!("GitHub 요청 한도에 가까워 약 {}분 동안 상태 확인을 쉬어요", minutes(until, now));
        s.paused.insert(h.to_string(), (until, msg.clone()));
        host.emit("maestro://pr-poll", json!({ "host": h, "pausedUntil": until, "message": msg }));
    } else if s.paused.remove(h).is_some() {
        host.emit("maestro://pr-poll", json!({ "host": h, "pausedUntil": 0, "message": "" }));
    }
    for (rec, live) in chunk.iter().zip(lives) {
        // the merge click (or an approval) started while this was asked: what it reads back wins
        // ...or one finished meanwhile and wrote a newer record (a push's new head): this answer is older than that
        if host.pr_busy(&rec.worker_path) || prs.get(&rec.worker_path).is_some_and(|c| c.checked_at > now) {
            s.due.insert(key(&rec.worker_path), align(now + BUSY_WAIT));
            continue;
        }
        let updated = match live {
            Some(live) => apply(host, prs, rec, &live, now),
            None => prs.update(&rec.worker_path, |r| {
                r.error = Some("PR을 찾지 못했어요".into());
                r.checked_at = now;
            }),
        };
        let next = updated.as_ref().map(|u| if u.error.as_deref() == Some("PR을 찾지 못했어요") { SLOW } else { interval(u) }).unwrap_or(SLOW);
        s.due.insert(key(&rec.worker_path), align(now + next));
    }
}

/// What a poll saw, onto the record: transitions send their notes; only a changed record is emitted.
fn apply(host: &dyn Host, prs: &Prs, rec: &PrRecord, live: &PrLive, now: u64) -> Option<PrRecord> {
    let before = rec.clone();
    let mut notes: Vec<String> = vec![];
    let updated = prs.update(&rec.worker_path, |r| {
        // merged is final: an answer asked before the merge was recorded is older than the record
        if r.state == "MERGED" && live.state != "MERGED" {
            return;
        }
        r.url = if live.url.is_empty() { r.url.clone() } else { live.url.clone() };
        r.is_draft = live.is_draft;
        if !live.head_ref_oid.is_empty() {
            r.head_sha = live.head_ref_oid.clone();
        }
        r.review_decision = live.review_decision.clone();
        r.mergeable = live.mergeable.clone();
        r.merge_state_status = live.merge_state_status.clone();
        r.auto_merge = live.auto_merge;
        r.checks = live.checks.clone();
        if r.error.as_deref() == Some("PR을 찾지 못했어요") {
            r.error = None;
        }
        // transitions from the record as it is now (a merge click may have recorded the merge meanwhile), not from
        // the copy this round started with
        let was_open = r.state == "OPEN";
        match live.state.as_str() {
            "MERGED" if r.state != "MERGED" => {
                let (method, head) = match &r.merge_attempt {
                    Some(a) => (a.method.clone(), a.seen_head.clone()),
                    None => ("github".to_string(), live.head_ref_oid.clone()),
                };
                r.merged = Some(Merged { method: method.clone(), commit: live.merge_commit.clone(), head: head.clone(), at: now });
                if !head.is_empty() && !r.merged_heads.contains(&head) {
                    r.merged_heads.push(head);
                }
                r.merge_attempt = None;
                r.queued = false;
                r.state = "MERGED".into();
                // a merge click's "대기열에 들어갔어요" and the like are over
                r.error = None;
                r.notice = None;
                notes.push(if method == "github" {
                    note_github_merged(r, live.merge_commit.as_deref())
                } else {
                    note_merged(r, &method, live.merge_commit.as_deref(), "그대로예요(대기열·자동 병합으로 합쳐짐: 사용자가 '기준 브랜치 받기'를 누를 수 있어요)", false)
                });
            }
            "CLOSED" if was_open && r.state != "MERGED" => {
                r.state = "CLOSED".into();
                r.merge_attempt = None;
                r.queued = false;
                r.error = None;
                r.notice = None;
                notes.push(note_closed(r));
            }
            "OPEN" if r.state != "MERGED" => {
                r.state = "OPEN".into();
                r.queued = live.queued();
                // auto-merge turned off or the PR left the merge queue: the click is over, the button works again
                if r.merge_attempt.as_ref().is_some_and(|a| !live.queued() && now.saturating_sub(a.at) > ATTEMPT_GRACE) {
                    r.merge_attempt = None;
                    r.notice = None;
                    r.error = Some("병합 대기열/자동 병합에서 빠졌어요".into());
                } else if r.merge_attempt.is_none() {
                    // a merge click's error ("병합이 끝나지 않았어요", "GitHub가 병합을 거부했어요: …", the last round's
                    // "빠졌어요") is no longer true once the PR is read fine with no click under way
                    r.error = None;
                }
                let head = r.checks.head_sha.clone();
                if r.checks.state == "failure" && r.ci_noted_sha.as_deref() != Some(head.as_str()) {
                    notes.push(note_ci_failed(r));
                    r.ci_noted_sha = Some(head);
                }
            }
            _ => {}
        }
        r.checked_at = now;
    })?;
    let mut cmp = updated.clone();
    cmp.checked_at = before.checked_at;
    if cmp != before {
        host.emit("maestro://pr", json!(updated));
    }
    for n in notes {
        note_pr(host, &updated, &n);
    }
    Some(updated)
}

/// The app's poller thread: woken by approvals, merges, "다시 확인" and startup; sleeps when nothing is open.
pub fn start_poller(app: AppHandle) {
    std::thread::spawn(move || {
        let mut s = Sched::default();
        loop {
            let Ok(host) = AppHost::new(&app) else { return };
            let wait = poll_once(&host, &app.state::<Prs>(), &mut s);
            let (m, cv) = &WAKE;
            let mut woke = m.lock().unwrap_or_else(|e| e.into_inner());
            if !*woke {
                woke = cv.wait_timeout(woke, Duration::from_millis(wait.min(600_000))).map(|(g, _)| g).unwrap_or_else(|e| e.into_inner().0);
            }
            *woke = false;
        }
    });
}

// ---------- merging the PR (a separate click) ----------

const NO_ACCESS: &str = "gh 토큰에 이 저장소의 PR을 읽을 권한이 없어요 (Resource not accessible). 토큰 권한(Pull requests 읽기)을 확인해 주세요";

/// A failed read of a PR, in Korean.
fn read_error(kind: GhErr, text: &str) -> String {
    if text.to_lowercase().contains("resource not accessible") {
        return NO_ACCESS.into();
    }
    match kind {
        GhErr::Auth => "gh에 로그인돼 있지 않아요. 터미널에서 직접 `gh auth login`을 실행한 뒤 다시 누르세요".to_string(),
        GhErr::RateLimited => "GitHub 요청 한도에 걸렸어요 (잠시 뒤 다시)".to_string(),
        _ => format!("PR 상태를 읽지 못했어요: {text}"),
    }
}

fn view_fields(tools: &Tools, rec: &PrRecord, fields: &str) -> Result<PrLive, String> {
    github::view_pr(tools, &rec.ready(), rec.number, fields).map(|v| github::live_from_view(&v)).map_err(|(k, m)| read_error(k, &m))
}

/// The PR after a merge click: `gh pr view`, and for one still open with no auto-merge the merge queue too, which
/// only the GraphQL query has (`isInMergeQueue`; a failed read of it leaves the PR as not queued).
fn read_back(tools: &Tools, rec: &PrRecord) -> Result<PrLive, String> {
    let mut v = view_fields(tools, rec, "state,mergedAt,mergeCommit,autoMergeRequest,headRefOid")?;
    if v.state == "OPEN" && !v.auto_merge {
        if let Ok(l) = live_status(tools, rec) {
            v.in_queue = l.state == "OPEN" && l.in_queue;
        }
    }
    Ok(v)
}

/// The PR with its head's checks, read through the poller's own query (one target) rather than `gh pr view --json
/// statusCheckRollup`: a token that can't read Actions (a fine-grained token, an app token) gets a partial answer
/// there, which gh turns into a failure with no JSON, while the query's partial `data` still has the PR.
fn live_status(tools: &Tools, rec: &PrRecord) -> Result<PrLive, String> {
    let target = Target { owner: rec.owner.clone(), repo: rec.repo.clone(), number: rec.number };
    let (out, map) = github::query_status(tools, &rec.ready().host_args(), &[target]);
    match github::parse_status(&out.stdout, &map) {
        Ok((_, lives)) => match lives.into_iter().next().flatten() {
            Some(live) => Ok(live),
            None if out.ok() => Err(format!("GitHub에서 PR #{}을 찾지 못했어요", rec.number)),
            None => Err(read_error(github::classify(&out), &out.tail())),
        },
        Err(_) => Err(read_error(github::classify(&out), &out.tail())),
    }
}

/// What the user confirmed with "그래도 병합": the CI state the card showed when it wasn't green ("failure" |
/// "none" | "pending") and a change request. The merge goes only when GitHub's state now is green, or exactly what was
/// confirmed; anything else (worse, or just different) is refused and shown.
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct MergeAck {
    pub ci: Option<String>,
    pub changes_requested: bool,
}

/// The PrCard's "PR 병합": checks the PR once more (open, not a draft, its base, the head the user saw, mergeable,
/// reviews, checks), records the attempt, runs `gh pr merge --merge|--squash --match-head-commit`, and always reads
/// the result back. Merged in this click: the project folder is fast-forwarded when it can be (`sync_base`).
pub fn merge_pr(host: &dyn Host, merges: &Merges, prs: &Prs, worker: &str, method: &str, seen_head: &str, ack: &MergeAck) -> Result<PrRecord, String> {
    let tools = host.tools();
    prs.get(worker).ok_or("이 워커의 PR 기록이 없어요")?;
    let _one = merges.lock_pr(worker)?;
    let rec = prs.get(worker).ok_or("이 워커의 PR 기록이 없어요")?;
    // a click sent moments ago: a second `gh pr merge` could fail ("already queued") before GitHub shows the queue
    // entry and erase the first one
    if rec.merge_attempt.as_ref().is_some_and(|a| host.now().saturating_sub(a.at) <= ATTEMPT_GRACE) {
        return Err("병합 요청을 이미 보냈어요. 상태를 확인하는 중이에요 (잠시 뒤 다시 확인)".into());
    }
    let squash = match method {
        "merge" => false,
        "squash" => true,
        _ => return Err(format!("알 수 없는 병합 방식: {method}")),
    };
    let ready = github::readiness(&tools, &rec.root, &rec.base, false, host.now());
    if !ready.ok {
        return Err(ready.message);
    }
    if squash && !ready.squash_allowed {
        return Err("이 저장소는 squash 병합을 허용하지 않아요".into());
    }
    if !squash && !ready.merge_allowed {
        return Err("이 저장소는 병합 커밋을 허용하지 않아요".into());
    }
    let mut live = live_status(&tools, &rec)?;
    for _ in 0..2 {
        if live.mergeable != "UNKNOWN" {
            break;
        }
        std::thread::sleep(tools.t.unknown_pause);
        live = live_status(&tools, &rec)?;
    }
    let refuse = |msg: String| -> Result<PrRecord, String> {
        let mut moved = false;
        if let Some(r) = prs.update(worker, |r| {
            // the checks as GitHub has them now (the card shows what was refused); a new head with none read yet:
            // the checks shown were another commit's
            if !live.head_ref_oid.is_empty() && live.checks.head_sha == live.head_ref_oid {
                moved = r.checks.head_sha != live.head_ref_oid;
                r.checks = live.checks.clone();
            } else if !live.head_ref_oid.is_empty() && r.checks.head_sha != live.head_ref_oid {
                r.checks = pending_checks(&live.head_ref_oid);
                moved = true;
            }
            r.head_sha = if live.head_ref_oid.is_empty() { r.head_sha.clone() } else { live.head_ref_oid.clone() };
            r.mergeable = live.mergeable.clone();
            r.merge_state_status = live.merge_state_status.clone();
            r.review_decision = live.review_decision.clone();
            r.is_draft = live.is_draft;
        }) {
            host.emit("maestro://pr", json!(r));
        }
        if moved {
            poll_soon(worker);
            host.wake();
        }
        Err(msg)
    };
    if live.state != "OPEN" {
        return refuse("이미 합쳐졌거나 닫혔어요".into());
    }
    if live.is_draft {
        return refuse("초안 PR이에요. GitHub에서 준비 완료로 바꾼 뒤 누르세요".into());
    }
    if live.base_ref_name != rec.base {
        return refuse(format!("PR의 기준이 {}로 바뀌었어요 (앱은 {}로 알고 있어요)", live.base_ref_name, rec.base));
    }
    if live.head_ref_oid != seen_head {
        return refuse(format!("본 뒤 PR에 새 커밋이 생겼어요 ({} → {}). 다시 확인해 주세요", sha7(seen_head), sha7(&live.head_ref_oid)));
    }
    if live.mergeable == "CONFLICTING" || live.merge_state_status == "DIRTY" {
        return refuse("기준 브랜치와 충돌해요".into());
    }
    if live.mergeable == "UNKNOWN" {
        return refuse("GitHub가 아직 병합 가능 여부를 계산 중이에요. 잠시 뒤 다시 누르세요".into());
    }
    if live.auto_merge {
        return refuse("자동 병합이 이미 켜져 있어요".into());
    }
    if live.in_queue {
        return refuse("이미 병합 대기열에 있어요".into());
    }
    if live.review_decision.as_deref() == Some("REVIEW_REQUIRED") || live.merge_state_status == "BLOCKED" {
        return refuse("리뷰나 보호 규칙 때문에 지금은 병합할 수 없어요".into());
    }
    // the checks of the head being merged: as read just now, else the poller's when they are that head's
    let ci = if live.checks.head_sha == seen_head { Some(live.checks.clone()) } else { Some(rec.checks.clone()).filter(|c| c.head_sha == seen_head) };
    let Some(ci) = ci else {
        poll_soon(worker);
        host.wake();
        return refuse(format!("이 커밋({})의 CI 결과를 아직 몰라요. 잠시 뒤 다시 누르세요", sha7(seen_head)));
    };
    // only green, or exactly the red the user confirmed for this head
    if ci.state != "success" && ack.ci.as_deref() != Some(ci.state.as_str()) {
        return refuse(match ci.state.as_str() {
            "pending" => "CI가 아직 진행 중이에요".into(),
            "failure" => "CI가 실패했어요. 그래도 병합하려면 한 번 더 확인해 주세요".into(),
            _ => "CI 결과가 없어요. 그래도 병합하려면 한 번 더 확인해 주세요".into(),
        });
    }
    if live.review_decision.as_deref() == Some("CHANGES_REQUESTED") && !ack.changes_requested {
        return refuse("변경 요청이 있어요. 그래도 병합하려면 한 번 더 확인해 주세요".into());
    }
    let now = host.now();
    prs.update(worker, |r| {
        r.merge_attempt = Some(MergeAttempt { method: method.into(), seen_head: seen_head.into(), at: now });
        r.error = None;
        r.checked_at = r.checked_at.max(now);
    });
    let args = github::merge_args(&rec.slug(), rec.number, squash, seen_head);
    let out = tools.gh_write(&args.iter().map(String::as_str).collect::<Vec<_>>(), tools.t.merge);
    // always read back: exit 0 may be a queue entry, a failure may have merged
    let mut after = None;
    for k in 0..3 {
        if k > 0 {
            std::thread::sleep(tools.t.unknown_pause);
        }
        if let Ok(v) = read_back(&tools, &rec) {
            let done = v.state == "MERGED" || v.queued();
            after = Some(v);
            if done {
                break;
            }
        }
    }
    let Some(after) = after else {
        if let Some(r) = prs.update(worker, |r| r.error = Some("병합 결과를 확인하지 못했어요: 다시 확인을 눌러 주세요".into())) {
            host.emit("maestro://pr", json!(r));
        }
        host.wake();
        return Err("병합 결과를 확인하지 못했어요. 상태를 다시 확인할게요".into());
    };
    match after.state.as_str() {
        "MERGED" => {
            // recorded before the fetch: the poller sees it merged and has nothing to add
            let (r, first) = mark_merged(prs, &rec, method, after.merge_commit.clone(), seen_head, host.now()).ok_or("PR 기록이 사라졌어요")?;
            host.emit("maestro://pr", json!(r));
            let folder = match sync_base(&tools, merges, &rec) {
                Ok(head) => AfterMerge { base_updated: Some(head.clone()), message: format!("빨리감기 했어요 ({})", sha7(&head)) },
                Err(why) => AfterMerge { base_updated: None, message: format!("그대로예요({why})") },
            };
            let r = prs.update(worker, |r| r.after_merge = Some(folder.clone())).unwrap_or(r);
            host.emit("maestro://pr", json!(r));
            host.emit("maestro://worktrees", json!(rec.root));
            if first && r.note_owed {
                note_merge_owed(host, prs, &r, &folder.message, folder.base_updated.is_some());
            }
            Ok(prs.get(worker).unwrap_or(r))
        }
        "OPEN" if after.queued() => {
            let at = host.now();
            let r = prs
                .update(worker, |r| {
                    // the grace counts from when the click's answer is known, not from before a slow `gh pr merge`
                    if let Some(a) = r.merge_attempt.as_mut() {
                        a.at = at;
                    }
                    r.queued = true;
                    r.error = None;
                    r.notice = Some("병합 대기열/자동 병합에 들어갔어요".into());
                    r.checked_at = r.checked_at.max(at);
                })
                .ok_or("PR 기록이 사라졌어요")?;
            host.emit("maestro://pr", json!(r));
            host.wake();
            Ok(r)
        }
        // gh said yes and the PR is still open, not seen in a queue (yet): a merge-queue base enqueues instead of merging,
        // and GitHub may show it a moment later. Not refused: the click stays recorded and the poller follows it (it
        // ends after ATTEMPT_GRACE when no queue or auto-merge shows up)
        "OPEN" if out.ok() => {
            let at = host.now();
            let r = prs
                .update(worker, |r| {
                    if let Some(a) = r.merge_attempt.as_mut() {
                        a.at = at;
                    }
                    r.error = None;
                    r.notice = Some("gh는 병합 요청을 받았지만 PR이 아직 열려 있어요 (병합 대기열에 들어갔을 수 있어요). 상태를 다시 확인할게요".into());
                    r.checked_at = r.checked_at.max(at);
                })
                .ok_or("PR 기록이 사라졌어요")?;
            host.emit("maestro://pr", json!(r));
            poll_soon(worker);
            host.wake();
            Ok(r)
        }
        _ => {
            let why = merge_refusal(&out);
            let at = host.now();
            if let Some(r) = prs.update(worker, |r| {
                r.merge_attempt = None;
                r.error = Some(why.clone());
                r.checked_at = r.checked_at.max(at);
            }) {
                host.emit("maestro://pr", json!(r));
            }
            Err(why)
        }
    }
}

/// Why `gh pr merge` failed, for the card. A moved head is GitHub's "head branch was modified" or an HTTP 409, never a
/// bare "409" (gh names the PR, e.g. "o/r#1409 is not mergeable").
pub fn merge_refusal(out: &Out) -> String {
    let t = format!("{}\n{}", out.stderr, out.stdout).to_lowercase();
    if t.contains("head branch was modified") || t.contains("http 409") {
        "새 커밋이 생겼어요. 다시 확인해 주세요".to_string()
    } else if t.contains("approving review") {
        "승인 리뷰가 필요해요".to_string()
    } else if github::classify(out) == GhErr::Auth {
        "gh에 로그인돼 있지 않아요. 터미널에서 직접 `gh auth login`을 실행한 뒤 다시 누르세요".to_string()
    } else if github::classify(out) == GhErr::RateLimited {
        "GitHub 요청 한도에 걸렸어요 (잠시 뒤 다시)".to_string()
    } else {
        format!("GitHub가 병합을 거부했어요: {}", out.tail())
    }
}

/// The project folder takes the merged base by fast-forward, only when it's on the base, clean (untracked files
/// too), nothing is under way and the local base has nothing the remote lacks. Err: why it was left alone.
pub fn sync_base(tools: &Tools, merges: &Merges, rec: &PrRecord) -> Result<String, String> {
    let root = Path::new(&rec.root);
    let _busy = merges.lock_root(&rec.root)?;
    let cur = head_branch(tools, root).unwrap_or_else(|| "HEAD".into());
    if cur != rec.base {
        return Err(format!("프로젝트 폴더가 {cur}에 있어요 → 직접 받아 주세요"));
    }
    let st = git(tools, root, &["status", "--porcelain=v1"]);
    if !st.ok() || !st.text().is_empty() {
        return Err("커밋하지 않은 변경이 있어요 → 직접 받아 주세요".into());
    }
    if let Some(what) = in_progress(tools, root)? {
        return Err(format!("다른 git 작업({what}) 중 → 직접 받아 주세요"));
    }
    fetch_base(tools, root, &rec.remote, &rec.base)?;
    let tracking = format!("refs/remotes/{}/{}", rec.remote, rec.base);
    let local = format!("refs/heads/{}", rec.base);
    if !is_ancestor(tools, root, &local, &tracking) {
        return Err("로컬 기준 브랜치에 원격에 없는 커밋이 있어요 → 직접 받아 주세요".into());
    }
    let target = rev(tools, root, &tracking).ok_or("원격 기준 브랜치를 찾지 못했어요")?;
    let before = rev(tools, root, "HEAD");
    if before.as_deref() == Some(target.as_str()) {
        return Ok(target);
    }
    let started = std::time::SystemTime::now();
    let out = exec::run(tools.git(root, &["merge", "--ff-only", "--no-edit", &tracking], tools.t.commit));
    if out.timed_out {
        // a post-merge hook or `gc --auto` past the limit runs after the fast-forward itself
        let head = rev(tools, root, "HEAD");
        let moved = head.as_deref() == Some(target.as_str());
        // HEAD where it was: git never got past its locked phase, so a lock left is the app's
        let lock = own_index_lock(tools, root, started, head.is_some() && head == before);
        if moved {
            return Ok(target);
        }
        return Err(format!("빨리감기가 시간 제한을 넘겨 멈췄어요{lock} → 직접 받아 주세요"));
    }
    if !out.ok() || rev(tools, root, "HEAD").as_deref() != Some(target.as_str()) {
        return Err(format!("빨리감기를 하지 못했어요: {}", out.tail()));
    }
    Ok(target)
}

/// "기준 브랜치 받기" on the PrCard, after a merge the app didn't fast-forward (queue, GitHub, a dirty folder then).
pub fn pr_sync_base(host: &dyn Host, merges: &Merges, prs: &Prs, worker: &str) -> Result<String, String> {
    let rec = prs.get(worker).ok_or("이 워커의 PR 기록이 없어요")?;
    if rec.state != "MERGED" {
        return Err("PR이 아직 합쳐지지 않았어요".into());
    }
    let _one = merges.lock_pr(worker)?;
    let tools = host.tools();
    let (after, result) = match sync_base(&tools, merges, &rec) {
        Ok(head) => (AfterMerge { base_updated: Some(head.clone()), message: format!("빨리감기 했어요 ({})", sha7(&head)) }, Ok(format!("기준 브랜치를 받았어요 ({})", sha7(&head)))),
        Err(why) => (AfterMerge { base_updated: None, message: format!("그대로예요({why})") }, Err(why)),
    };
    if let Some(r) = prs.update(worker, |r| r.after_merge = Some(after)) {
        host.emit("maestro://pr", json!(r));
    }
    host.emit("maestro://worktrees", json!(rec.root));
    result
}

// ---------- failed CI logs ----------

/// the most of one job's log read (its end)
const LOG_READ: usize = 8 << 20;
const LOG_JOB: usize = 16 << 10;
const LOG_ALL: usize = 32 << 10;

fn format_char(c: char) -> bool {
    matches!(c as u32, 0x00AD | 0x061C | 0x180E | 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x2064 | 0x2066..=0x206F | 0xFEFF | 0xFFF9..=0xFFFB)
}

fn strip_ansi(line: &str) -> String {
    let mut out = String::new();
    let mut it = line.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\u{1b}' {
            if it.peek() == Some(&'[') {
                it.next();
                for d in it.by_ref() {
                    if d.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        if (c.is_control() && c != '\t') || format_char(c) {
            continue;
        }
        out.push(c);
    }
    out
}

/// "2026-11-11T12:34:56.1234567Z rest" → "rest"
fn drop_timestamp(s: &str) -> &str {
    let b = s.as_bytes();
    if b.len() > 20 && b[4] == b'-' && b[7] == b'-' && b[10] == b'T' {
        if let Some(sp) = s.find(' ') {
            if s[..sp].ends_with('Z') {
                return &s[sp + 1..];
            }
        }
    }
    s
}

fn is_error_line(l: &str) -> bool {
    let low = l.to_lowercase();
    l.contains("##[error]") || l.contains("::error") || low.contains("error:") || l.contains("FAILED") || low.contains("exit code") || l.contains("panic:") || l.contains("AssertionError") || l.contains("ENOENT") || l.contains("EACCES")
}

/// A failed job's log for an agent: the job and step columns and timestamps dropped (each step named once), ANSI and
/// invisible characters gone, the last 100 lines plus up to 30 earlier error lines (±2 around each), at most 16 KiB.
pub fn trim_log(raw: &str) -> String {
    // each line with the step it belongs to
    let mut lines: Vec<String> = vec![];
    let mut steps: Vec<String> = vec![];
    for l in raw.replace("\r\n", "\n").split('\n') {
        let mut cols = l.splitn(3, '\t');
        let (a, b, c) = (cols.next(), cols.next(), cols.next());
        let (step, text) = match (a, b, c) {
            (Some(_job), Some(s), Some(rest)) => (strip_ansi(s), rest),
            _ => (steps.last().cloned().unwrap_or_default(), l),
        };
        steps.push(step);
        lines.push(strip_ansi(drop_timestamp(text)));
    }
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
        steps.pop();
    }
    let tail_from = lines.len().saturating_sub(100);
    let mut keep = vec![false; lines.len()];
    let mut picked = 0;
    for i in 0..tail_from {
        if picked >= 30 {
            break;
        }
        if is_error_line(&lines[i]) {
            picked += 1;
            for k in i.saturating_sub(2)..(i + 3).min(tail_from) {
                keep[k] = true;
            }
        }
    }
    let mut out = String::new();
    let mut gap = false;
    let mut shown_step: Option<&str> = None;
    for (i, l) in lines.iter().enumerate() {
        if i >= tail_from || keep[i] {
            if gap && !out.is_empty() {
                out.push_str("…\n");
            }
            gap = false;
            // the step's name once, before its first line shown
            if !steps[i].is_empty() && shown_step != Some(steps[i].as_str()) {
                out.push_str(&format!("── {} ──\n", steps[i]));
                shown_step = Some(steps[i].as_str());
            }
            out.push_str(l);
            out.push('\n');
        } else {
            gap = true;
        }
        if i + 1 == tail_from && keep[..tail_from].iter().any(|k| *k) {
            out.push_str("… 앞쪽 오류 …\n");
            gap = false;
        }
    }
    crate::util::tail_bytes(&out, LOG_JOB).to_string()
}

/// The text the log dialog starts from (never stored): the failed jobs' trimmed logs, each in its own fence.
pub fn failed_log(host: &dyn Host, prs: &Prs, worker: &str, job_id: Option<u64>) -> Result<String, String> {
    let rec = prs.get(worker).ok_or("이 워커의 PR 기록이 없어요")?;
    let tools = host.tools();
    let jobs: Vec<_> = rec.checks.failed_jobs.iter().filter(|j| job_id.is_none_or(|id| j.job_id == Some(id))).filter(|j| j.job_id.is_some() || j.run_id.is_some()).take(5).cloned().collect();
    if jobs.is_empty() {
        return Err("받을 수 있는 실패 로그가 없어요 (GitHub Actions 작업이 아니거나 이미 다시 돌았어요)".into());
    }
    let slug = rec.slug();
    let mut parts = vec![];
    let mut total = 0;
    for j in &jobs {
        let mut out = None;
        // a failure is at the log's end: a long log keeps its last 8 MiB
        if let Some(id) = j.job_id {
            let id = id.to_string();
            let o = tools.gh(&["run", "view", "--job", &id, "-R", &slug, "--log-failed"], tools.t.logs).map(|c| exec::run(c.tail_cap(LOG_READ)));
            out = o.filter(|o| o.ok() && !o.text().is_empty());
        }
        if out.is_none() {
            if let Some(run) = j.run_id {
                let run = run.to_string();
                let o = tools.gh(&["run", "view", &run, "-R", &slug, "--log-failed"], tools.t.logs).map(|c| exec::run(c.tail_cap(LOG_READ)));
                out = o.filter(|o| o.ok() && !o.text().is_empty());
            }
        }
        let (body, cut) = match out {
            Some(o) => (trim_log(&o.stdout), o.truncated),
            None => ("(실패 로그를 받지 못했어요 (진행 중이거나 만료))".into(), false),
        };
        let room = LOG_ALL.saturating_sub(total);
        if room == 0 {
            break;
        }
        let body = crate::util::tail_bytes(&body, room).to_string();
        total += body.len();
        let f = fence_for(&body);
        let name = if cut { format!("{} (로그가 길어 마지막 {} MiB만 받았어요: 앞쪽 오류는 빠졌을 수 있어요)", j.name, LOG_READ >> 20) } else { j.name.clone() };
        parts.push((name,format!("{f}text\n{}\n{f}", body.trim_end())));
    }
    if parts.iter().all(|(_, b)| b.contains("실패 로그를 받지 못했어요")) {
        return Err("실패 로그를 받지 못했어요 (진행 중이거나 만료)".into());
    }
    let names: Vec<String> = jobs.iter().take(parts.len()).map(|j| format!("'{}'", j.name)).collect();
    let mut text = format!("PR #{} ({}, head {})의 CI 작업 {}가 실패했어요. 아래 로그를 보고 원인을 고친 뒤 커밋하고 알려 주세요.", rec.number, rec.branch, sha7(&rec.head_sha), names.join(", "));
    for (n, b) in parts {
        text.push_str(&format!("\n\n{n}:\n{b}"));
    }
    Ok(text)
}

/// Opens the recorded PR in the browser, only a URL of that PR's own form.
pub fn open_pr(prs: &Prs, worker: &str) -> Result<(), String> {
    let rec = prs.get(worker).ok_or("이 워커의 PR 기록이 없어요")?;
    if !valid_pr_url(&rec) {
        return Err("PR 주소가 올바르지 않아요".into());
    }
    let mut cmd = if cfg!(windows) {
        let mut c = std::process::Command::new("rundll32");
        c.args(["url.dll,FileProtocolHandler", &rec.url]);
        c
    } else if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg(&rec.url);
        c
    } else {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(&rec.url);
        c
    };
    crate::cli::quiet(&mut cmd).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// `https://<host>/<owner>/<repo>/pull/<n>` and nothing else.
pub fn valid_pr_url(rec: &PrRecord) -> bool {
    let want = format!("https://{}/{}/{}/pull/", rec.host, rec.owner, rec.repo);
    rec.url.len() > want.len() && rec.url[..want.len()].eq_ignore_ascii_case(&want) && rec.url[want.len()..].chars().all(|c| c.is_ascii_digit())
}

// ---------- the project's merge mode ----------

/// Why the merge mode can't change, or a worker or the Maestro can't be discarded, now.
pub const PR_UNRESOLVED: &str = "PR 작업 중이거나, 앱이 만든 PR을 아직 찾지 못한 병합 요청이 있어요. 병합 요청 카드에서 '다시 확인'으로 PR을 찾거나 요청을 거부한 뒤 다시 해 주세요";
/// A worker whose PR waits to merge (the user's merge click under way, or in a merge queue / auto-merge): discarding
/// it would drop the record before GitHub merges, and nothing would note the merge.
pub const PR_MERGE_PENDING: &str = "이 워커의 PR이 병합을 기다리고 있어요(병합 중이거나 병합 대기열·자동 병합). 병합되거나 대기열에서 빠진 뒤 지워 주세요";

impl PrRecord {
    /// The user's merge click is under way, or GitHub will merge it later (merge queue, auto-merge).
    pub fn merge_pending(&self) -> bool {
        self.state == "OPEN" && (self.merge_attempt.is_some() || self.queued)
    }
}

/// The 프로젝트 setting. To "pr" only when the full readiness check passes (else Err with the reason, the mode
/// stays). Refused while a PR action runs. Pending requests are withdrawn (a plan made for the other mode must not
/// be approved) and the conductor is told, since its running brief doesn't change.
pub fn set_merge_mode(host: &dyn Host, merges: &Merges, root: &str, mode: MergeMode) -> Result<(), String> {
    let home = host.home();
    if project::merge_mode(&home, root) == mode {
        return Ok(());
    }
    if merges.pr_active_in(root) {
        return Err(PR_UNRESOLVED.into());
    }
    if mode.is_pr() {
        let ready = github::readiness(&host.tools(), root, &host.base_branch(root), true, host.now());
        if !ready.ok {
            return Err(ready.message);
        }
    }
    project::write_mode(&home, root, mode)?;
    merges.withdraw_root(host, root);
    if host.has_maestro(root) {
        host.note(root, root, "mode", &note_mode(mode));
    }
    host.emit("maestro://worktrees", json!(root));
    Ok(())
}

/// What `maestro merge-request` plans with in PR mode: the remote, merged heads, the last squash, the open PR.
pub fn plan_opts(tools: &Tools, prs: &Prs, root: &str, base: &str, worker: &str, mode: MergeMode) -> crate::worktree::PlanOpts {
    if !mode.is_pr() {
        return crate::worktree::PlanOpts::default();
    }
    let rec = prs.get(worker);
    // the record's PR is this request's only when it is on the branch the worker is on now
    let branch = head_branch(tools, Path::new(worker));
    crate::worktree::PlanOpts {
        mode,
        remote: github::pick_remote(tools, Path::new(root), base),
        merged_heads: rec.as_ref().map(|r| r.merged_heads.clone()).unwrap_or_default(),
        squashed: rec.as_ref().and_then(|r| squashed(tools, Path::new(root), r)),
        existing_pr: rec.as_ref().filter(|r| r.state == "OPEN" && branch.as_deref() == Some(r.branch.as_str())).map(|r| r.number),
    }
}

#[cfg(test)]
mod tests;
