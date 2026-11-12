//! Merge requests (D4): the conductor asks (`maestro merge-request`, control.rs), the user approves or rejects on a
//! card, and the app merges (`worktree::merge_into_base`, `git merge --no-ff` in the project folder). Agents never
//! merge or push themselves (guard.rs blocks it for Claude Code). Kept apart from tool approvals (approval.rs): a
//! request isn't in the runtime stream and doesn't make its agent busy. Pending requests are kept on disk
//! (`~/.maestro/merge-requests.json`), one per worker, so a restart doesn't lose a card.

use crate::worktree::{self, Block, MergePlan, Outcome};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

/// The longest verification text kept (characters).
const MAX_VERIFIED: usize = 4000;
/// A side of the diff dialog larger than this isn't sent (bytes).
const MAX_DIFF: usize = 2 << 20;

/// A conductor's request to merge a worker's branch, as the card shows it and the approval merges it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MergeRequest {
    pub id: u64,
    /// the project folder, where the merge happens
    pub root: String,
    /// the Maestro that asked (its card shows the request; the result note goes to it)
    pub maestro_path: String,
    pub worker_path: String,
    /// its name, or its branch when it has none
    pub worker_name: String,
    /// what the conductor says it checked and how that went (trimmed, no control characters, capped)
    pub verified: Option<String>,
    /// ms since the Unix epoch
    pub requested_at: u64,
    #[serde(flatten)]
    pub plan: MergePlan,
    /// PR mode: what the approval has done so far (written before each outside call, R6)
    #[serde(default)]
    pub pr: Option<crate::pr::PrProgress>,
    /// PR mode: the title and body the card starts from (the user can edit them)
    #[serde(default)]
    pub pr_draft: Option<crate::pr::PrDraft>,
}

/// The answer to an approved or rejected request, for the card. outcome: "merged" | "rejected" | "stale" |
/// "conflict" | "failed", and in PR mode "pr" | "recovered" | "pr-updated".
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MergeResult {
    pub outcome: &'static str,
    pub commit: Option<String>,
    pub message: String,
    /// PR mode: the pull request the approval made, found or updated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr: Option<crate::pr::PrRecord>,
}

/// One file of a request for the diff dialog: at the merge base and at the requested tip (None: not there).
#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct MergeFileDiff {
    pub original: Option<String>,
    pub modified: Option<String>,
    pub binary: bool,
    pub too_large: bool,
}

#[derive(Default)]
struct Inner {
    next_id: u64,
    pending: BTreeMap<u64, MergeRequest>,
}

pub struct Merges {
    inner: Mutex<Inner>,
    /// what is busy right now: "root:<project folder>" while the app merges into it (one merge at a time, the guard's
    /// `merging`), "pr:<worker>" while a PR action for that worker runs (one at a time per worker)
    busy: Mutex<HashSet<String>>,
    /// worker folders the app is committing in (PR mode, the user ticked "커밋하고 진행"): the guard blocks git writes there
    /// (worker folder, its project folder), both keys
    committing: Mutex<HashSet<(String, String)>>,
    file: PathBuf,
}

/// Held while a project folder is merged into; dropping it frees the folder.
pub struct BusyGuard<'a> {
    merges: &'a Merges,
    key: String,
}

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.merges.busy.lock().unwrap_or_else(|e| e.into_inner()).remove(&self.key);
    }
}

/// One spelling of a path for comparisons.
pub(crate) fn key(path: &str) -> String {
    crate::util::path_key(path)
}

impl Merges {
    /// The pending requests kept in `~/.maestro/merge-requests.json`.
    pub fn load(home: &Path) -> Merges {
        Merges::at(home.join(".maestro").join("merge-requests.json"))
    }

    pub(crate) fn at(file: PathBuf) -> Merges {
        let list: Vec<MergeRequest> = std::fs::read_to_string(&file).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let next_id = list.iter().map(|r| r.id).max().unwrap_or(0) + 1;
        let pending = list.into_iter().map(|r| (r.id, r)).collect();
        Merges { inner: Mutex::new(Inner { next_id, pending }), busy: Mutex::default(), committing: Mutex::default(), file }
    }

    /// Written under the lock on every change.
    fn save(&self, inner: &Inner) {
        let list: Vec<&MergeRequest> = inner.pending.values().collect();
        let text = serde_json::to_string_pretty(&list).unwrap_or_else(|_| "[]".into());
        if let Err(e) = crate::util::atomic_write(&self.file, &text) {
            eprintln!("[maestro] merge requests not saved: {e}");
        }
    }

    pub fn list(&self) -> Vec<MergeRequest> {
        self.inner.lock().unwrap().pending.values().cloned().collect()
    }

    pub fn get(&self, id: u64) -> Option<MergeRequest> {
        self.inner.lock().unwrap().pending.get(&id).cloned()
    }

    /// Changes a pending request in place (kept on disk at once) and returns it as it is now; None when it's gone.
    pub fn update(&self, id: u64, f: impl FnOnce(&mut MergeRequest)) -> Option<MergeRequest> {
        let mut inner = self.inner.lock().unwrap();
        let r = inner.pending.get_mut(&id)?;
        f(r);
        let out = r.clone();
        self.save(&inner);
        Some(out)
    }

    /// The pending requests of one project folder.
    pub fn of_root(&self, root: &str) -> Vec<MergeRequest> {
        let k = key(root);
        self.list().into_iter().filter(|r| key(&r.root) == k).collect()
    }

    /// The worker's pending request, if any.
    pub fn of_worker(&self, worker_path: &str) -> Option<MergeRequest> {
        let k = key(worker_path);
        self.list().into_iter().find(|r| key(&r.worker_path) == k)
    }

    /// Records a request under a new id; the worker's earlier one, if any, is replaced and returned. A PR create the
    /// earlier one ran and never found (PR mode) carries over when the branch is the same, so approving the new one
    /// looks for that PR before it makes another (`PrProgress::carried`).
    pub(crate) fn add(&self, mut req: MergeRequest) -> (MergeRequest, Option<MergeRequest>) {
        let mut inner = self.inner.lock().unwrap();
        req.id = inner.next_id;
        inner.next_id += 1;
        let w = key(&req.worker_path);
        let old = inner.pending.iter().find(|(_, r)| key(&r.worker_path) == w).map(|(id, _)| *id);
        let old = old.and_then(|id| inner.pending.remove(&id));
        if let Some(carried) = old.as_ref().filter(|o| o.plan.branch == req.plan.branch && req.plan.mode.is_pr()).and_then(|o| o.pr.as_ref()).and_then(|g| g.carried()) {
            req.pr = Some(carried);
        }
        inner.pending.insert(req.id, req.clone());
        self.save(&inner);
        (req, old)
    }

    pub(crate) fn remove(&self, id: u64) -> Option<MergeRequest> {
        let mut inner = self.inner.lock().unwrap();
        let r = inner.pending.remove(&id);
        if r.is_some() {
            self.save(&inner);
        }
        r
    }

    fn remove_worker(&self, worker_path: &str) -> Vec<MergeRequest> {
        let w = key(worker_path);
        self.remove_where(|r| key(&r.worker_path) == w)
    }

    fn remove_maestro(&self, maestro_path: &str) -> Vec<MergeRequest> {
        let m = key(maestro_path);
        self.remove_where(|r| key(&r.maestro_path) == m)
    }

    fn remove_where(&self, f: impl Fn(&MergeRequest) -> bool) -> Vec<MergeRequest> {
        let mut inner = self.inner.lock().unwrap();
        let ids: Vec<u64> = inner.pending.iter().filter(|(_, r)| f(r)).map(|(id, _)| *id).collect();
        let gone: Vec<MergeRequest> = ids.iter().filter_map(|id| inner.pending.remove(id)).collect();
        if !gone.is_empty() {
            self.save(&inner);
        }
        gone
    }

    /// A new request: kept, shown (`maestro://merge`), and the worker's earlier card withdrawn ("replaced").
    /// Returns its id and the replaced one's.
    pub fn insert(&self, app: &AppHandle, req: MergeRequest) -> (u64, Option<u64>) {
        let (req, old) = self.add(req);
        if let Some(old) = &old {
            done_event(app, old, "replaced");
        }
        let _ = app.emit("maestro://merge", &req);
        (req.id, old.map(|o| o.id))
    }

    /// Every pending request of the project goes ("withdrawn", no note): the merge mode changed, and a plan made for
    /// the other mode must not be approved.
    pub fn withdraw_root(&self, host: &dyn crate::pr::Host, root: &str) -> usize {
        let k = key(root);
        let gone = self.remove_where(|r| key(&r.root) == k);
        for r in &gone {
            host.emit("maestro://merge-done", done_payload(r, "withdrawn"));
        }
        gone.len()
    }

    /// The worker was discarded: its request goes without a note.
    pub fn withdraw_worker(&self, app: &AppHandle, worker_path: &str) {
        for r in self.remove_worker(worker_path) {
            done_event(app, &r, "withdrawn");
        }
    }

    /// The Maestro was discarded: every request it made goes without a note (no block would show the cards, and a
    /// later Maestro on the same folder never asked for them).
    pub fn withdraw_maestro(&self, app: &AppHandle, maestro_path: &str) {
        for r in self.remove_maestro(maestro_path) {
            done_event(app, &r, "withdrawn");
        }
    }

    /// Marks `k` busy; None while it already is.
    pub fn lock_key(&self, k: String) -> Option<BusyGuard<'_>> {
        if !self.busy.lock().unwrap_or_else(|e| e.into_inner()).insert(k.clone()) {
            return None;
        }
        Some(BusyGuard { merges: self, key: k })
    }

    /// Marks the project folder as being merged into; Err while another merge there runs.
    pub fn lock_root(&self, root: &str) -> Result<BusyGuard<'_>, String> {
        self.lock_key(format!("root:{}", key(root))).ok_or_else(|| "이 프로젝트는 지금 합치는 중이에요".into())
    }

    /// One PR action (approve, merge, base update) per worker at a time.
    pub fn lock_pr(&self, worker_path: &str) -> Result<BusyGuard<'_>, String> {
        self.lock_key(format!("pr:{}", key(worker_path))).ok_or_else(|| "이 워커의 PR 작업이 진행 중이에요".into())
    }

    /// The app is merging into this project folder now (the guard blocks git writes there meanwhile).
    pub fn merging(&self, root: &str) -> bool {
        self.busy.lock().unwrap_or_else(|e| e.into_inner()).contains(&format!("root:{}", key(root)))
    }

    /// A PR action for this worker runs now.
    pub fn pr_busy(&self, worker_path: &str) -> bool {
        self.busy.lock().unwrap_or_else(|e| e.into_inner()).contains(&format!("pr:{}", key(worker_path)))
    }

    /// A PR action runs for a worker of `root` (by the requests it knows), or a request is unresolved (committing,
    /// pushing, creating, or a create whose PR wasn't found yet): the merge mode can't change and the Maestro can't
    /// be discarded now, since either would drop the request unseen.
    pub fn pr_active_in(&self, root: &str) -> bool {
        self.of_root(root).iter().any(|r| self.pr_busy(&r.worker_path) || r.pr.as_ref().is_some_and(crate::pr::PrProgress::unresolved))
    }

    /// The worker's request is unresolved, or a PR action runs for it: it can't be discarded now.
    pub fn pr_unresolved(&self, worker_path: &str) -> bool {
        self.pr_busy(worker_path) || self.of_worker(worker_path).is_some_and(|r| r.pr.as_ref().is_some_and(crate::pr::PrProgress::unresolved))
    }

    /// Marks a worker folder of project `root` as being committed in by the app until the guard is dropped.
    pub fn commit_lock(&self, worker_path: &str, root: &str) -> CommitGuard<'_> {
        let k = (key(worker_path), key(root));
        self.committing.lock().unwrap_or_else(|e| e.into_inner()).insert(k.clone());
        CommitGuard { merges: self, key: k }
    }

    /// The worker folders the app commits in right now (for the guard): those of project `root`, or of every
    /// project when the agent's project is unknown.
    pub fn committing(&self, root: Option<&str>) -> Vec<String> {
        let root = root.map(key);
        self.committing.lock().unwrap_or_else(|e| e.into_inner()).iter().filter(|(_, r)| root.as_ref().is_none_or(|k| k == r)).map(|(w, _)| w.clone()).collect()
    }
}

/// Held while the app commits in a worker folder; dropping it frees the folder.
pub struct CommitGuard<'a> {
    merges: &'a Merges,
    key: (String, String),
}

impl Drop for CommitGuard<'_> {
    fn drop(&mut self) {
        self.merges.committing.lock().unwrap_or_else(|e| e.into_inner()).remove(&self.key);
    }
}

pub(crate) fn done_payload(r: &MergeRequest, outcome: &str) -> serde_json::Value {
    json!({ "id": r.id, "maestroPath": r.maestro_path, "outcome": outcome })
}

fn done_event(app: &AppHandle, r: &MergeRequest, outcome: &str) {
    let _ = app.emit("maestro://merge-done", done_payload(r, outcome));
}

/// The conductor's verification text as kept: trimmed, control characters other than newline and tab dropped, capped.
pub fn clean_text(s: &str, max: usize) -> String {
    let s: String = s.chars().filter(|c| !c.is_control() || *c == '\n' || *c == '\t').collect();
    let s = s.trim();
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

use crate::util::sha7;

/// The verification text a request carries, from `--verified`.
pub fn verified(text: &str) -> Option<String> {
    Some(clean_text(text, MAX_VERIFIED)).filter(|t| !t.is_empty())
}

/// The `[maestro] 병합 …` note for the conductor. `outcome` None: the user rejected (with `reason`, if given).
pub fn merge_note(req: &MergeRequest, outcome: Option<&Outcome>, reason: Option<&str>) -> String {
    let (name, p) = (&req.worker_name, &req.plan);
    match outcome {
        None => format!("[maestro] 병합 요청 거부: 워커 '{name}' ({} → {}). 사용자 사유: {}. 사용자 지시를 기다리세요.", p.branch, p.base, reason.filter(|r| !r.is_empty()).unwrap_or("없음")),
        Some(Outcome::Merged { commit, later }) => {
            let mut s = format!("[maestro] 병합 완료: 워커 '{name}' ({}) → {}, 병합 커밋 {} (커밋 {}개). 프로젝트 폴더에서 빌드·테스트로 확인하세요.", p.branch, p.base, sha7(commit), p.commit_count);
            if *later > 0 {
                s = format!("{s} (요청 뒤 워커 브랜치에 커밋 {later}개가 더 생겼어요: 이번 병합에는 들어가지 않았어요)");
            }
            s
        }
        Some(Outcome::Stale { now }) => format!(
            "[maestro] 병합하지 않음: 요청 뒤 기준 브랜치({base})가 바뀌었어요 ({} → {}). 워커가 자기 워크트리에서 최신 기준 브랜치({base})를 합치거나 rebase하고 다시 확인한 뒤 maestro merge-request로 다시 요청하세요.",
            sha7(&p.base_sha),
            sha7(now),
            base = p.base
        ),
        Some(Outcome::Conflict { files }) => format!(
            "[maestro] 병합 충돌로 취소: 워커 '{name}' → {base}. 충돌 파일: {}. 프로젝트 폴더는 원래 상태로 돌렸어요. 워커 워크트리에서 기준 브랜치({base})를 합쳐 충돌을 푼 뒤 다시 요청하세요.",
            worktree::short_list(files),
            base = p.base
        ),
        Some(Outcome::Failed { message, touched: true }) => format!("[maestro] 병합 실패: {message}. 프로젝트 폴더가 합치기 도중 상태로 남았을 수 있어요. 직접 고치지 말고 사용자에게 확인을 부탁하세요."),
        Some(Outcome::Failed { message, touched: false }) => format!("[maestro] 병합 실패: {message}. 프로젝트 폴더는 바뀌지 않았어요. 사용자 지시를 기다리세요."),
    }
}

/// What the card says when an approval can't run now (the request stays).
fn block_message(req: &MergeRequest, b: &Block) -> String {
    match b {
        Block::NotOnBase(cur) => format!("프로젝트 폴더가 기준 브랜치({})가 아니라 {cur}에 있어요", req.plan.base),
        Block::Dirty(files) => format!("프로젝트 폴더에 커밋하지 않은 변경이 있어요: {}. 정리한 뒤 다시 병합을 누르세요", worktree::short_list(files)),
        Block::Busy(what) => format!("프로젝트 폴더에서 다른 git 작업({what})이 진행 중이에요"),
        Block::Git(e) => e.clone(),
    }
}

/// The card's answer. A reject, or an approval that ended (merged, stale, conflict, failed), removes the request and
/// tells the conductor; an approval that can't run now (`Block`) is an Err and the card stays. A request planned in PR
/// mode goes to `pr::approve` (push and pull request) instead.
pub fn answer(app: &AppHandle, id: u64, approve: bool, reason: Option<String>, opts: Option<crate::pr::PrOptions>) -> Result<MergeResult, String> {
    let merges = app.state::<Merges>();
    let req = merges.get(id).ok_or("이미 처리된 요청이에요")?;
    if req.plan.mode.is_pr() {
        let host = crate::pr::AppHost::new(app)?;
        let prs = app.state::<crate::pr::Prs>();
        return if approve { crate::pr::approve(&host, &merges, &prs, id, opts.unwrap_or_default()) } else { crate::pr::reject(&host, &merges, &prs, id, reason) };
    }
    let _busy = merges.lock_root(&req.root)?;
    // answered from another window meanwhile
    if merges.get(id).is_none() {
        return Err("이미 처리된 요청이에요".into());
    }
    if !approve {
        let reason = reason.map(|r| clean_text(&r, 500)).filter(|r| !r.is_empty());
        finish(app, &merges, &req, "rejected", merge_note(&req, None, reason.as_deref()));
        return Ok(MergeResult { outcome: "rejected", commit: None, message: "병합 요청을 거부했어요".into(), pr: None });
    }
    let outcome = worktree::merge_into_base(&req.root, &req).map_err(|b| block_message(&req, &b))?;
    let note = merge_note(&req, Some(&outcome), None);
    let result = match &outcome {
        Outcome::Merged { commit, .. } => MergeResult { outcome: "merged", commit: Some(commit.clone()), message: format!("병합했어요: {}", sha7(commit)), pr: None },
        Outcome::Stale { now } => MergeResult {
            outcome: "stale",
            commit: None,
            message: format!("요청 뒤 기준 브랜치({})가 바뀌어서 ({} → {}) 합치지 않았어요. 지휘자에게 알렸어요", req.plan.base, sha7(&req.plan.base_sha), sha7(now)),
            pr: None,
        },
        Outcome::Conflict { files } => MergeResult { outcome: "conflict", commit: None, message: format!("충돌이 나서 합치지 않았어요: {}. 프로젝트 폴더는 그대로예요", worktree::short_list(files)), pr: None },
        Outcome::Failed { message, touched } => MergeResult {
            outcome: "failed",
            commit: None,
            message: if *touched { format!("병합 실패: {message}. 프로젝트 폴더가 합치는 도중 상태일 수 있어요: 직접 확인해 주세요") } else { format!("병합 실패: {message}") },
            pr: None,
        },
    };
    finish(app, &merges, &req, result.outcome, note);
    if result.outcome == "merged" {
        let _ = app.emit("maestro://worktrees", &req.root);
    }
    Ok(result)
}

/// The request is done: gone from the list and the cards, and the conductor told (on its own thread: delivering
/// waits for the Maestro's prompt).
fn finish(app: &AppHandle, merges: &Merges, req: &MergeRequest, outcome: &str, note: String) {
    match crate::pr::AppHost::new(app) {
        Ok(host) => finish_host(&host, merges, req, outcome, &note),
        Err(e) => eprintln!("[maestro] merge note for {} not kept: {e}", req.maestro_path),
    }
}

/// `finish` through a host (PR mode, tests): the request goes, the cards hear it, the note goes under `merge:<worker>`.
pub fn finish_host(host: &dyn crate::pr::Host, merges: &Merges, req: &MergeRequest, outcome: &str, note: &str) {
    merges.remove(req.id);
    host.emit("maestro://merge-done", done_payload(req, outcome));
    host.note(&req.root, &req.maestro_path, &format!("merge:{}", key(&req.worker_path)), note);
}

/// One file of a request, at the recorded merge base and tip (not the worker's files now). `path` must be one of the
/// request's files.
pub fn file_diff(req: &MergeRequest, path: &str) -> Result<MergeFileDiff, String> {
    let f = req.plan.files.iter().find(|f| f.path == path).ok_or("이 병합 요청에 없는 파일이에요")?;
    let original = worktree::show_at(&req.root, &req.plan.merge_base, f.old_path.as_deref().unwrap_or(&f.path)).ok();
    let modified = worktree::show_at(&req.root, &req.plan.tip_sha, &f.path).ok();
    let sides = [&original, &modified];
    let too_large = sides.iter().any(|s| s.as_ref().is_some_and(|s| s.len() > MAX_DIFF));
    let binary = f.add.is_none() || sides.iter().any(|s| s.as_ref().is_some_and(|s| s.as_bytes()[..s.len().min(8000)].contains(&0)));
    if too_large || binary {
        return Ok(MergeFileDiff { original: None, modified: None, binary, too_large });
    }
    Ok(MergeFileDiff { original, modified, binary, too_large })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(worker: &str) -> MergeRequest {
        MergeRequest {
            id: 0,
            root: "C:/p/repo".into(),
            maestro_path: "C:/p/repo".into(),
            worker_path: worker.into(),
            worker_name: "api".into(),
            verified: verified("  cargo test 통과\u{7}\n"),
            requested_at: 1,
            plan: MergePlan { branch: "feat".into(), base: "main".into(), base_sha: "1111111aaaa".into(), tip_sha: "2222222bbbb".into(), commit_count: 3, ..Default::default() },
            pr: None,
            pr_draft: None,
        }
    }

    #[test]
    fn notes_for_each_outcome() {
        let r = req("C:/w/api");
        assert_eq!(r.verified.as_deref(), Some("cargo test 통과"));
        let merged = merge_note(&r, Some(&Outcome::Merged { commit: "abcdef1234".into(), later: 0 }), None);
        assert_eq!(merged, "[maestro] 병합 완료: 워커 'api' (feat) → main, 병합 커밋 abcdef1 (커밋 3개). 프로젝트 폴더에서 빌드·테스트로 확인하세요.");
        let more = merge_note(&r, Some(&Outcome::Merged { commit: "abcdef1234".into(), later: 2 }), None);
        assert!(more.contains("커밋 2개가 더 생겼어요"));
        assert!(merge_note(&r, None, Some("테스트 부족")).contains("사용자 사유: 테스트 부족."));
        assert!(merge_note(&r, None, None).contains("사용자 사유: 없음."));
        let stale = merge_note(&r, Some(&Outcome::Stale { now: "3333333cccc".into() }), None);
        assert!(stale.starts_with("[maestro] 병합하지 않음") && stale.contains("(1111111 → 3333333)") && stale.contains("maestro merge-request"));
        let conflict = merge_note(&r, Some(&Outcome::Conflict { files: vec!["a.rs".into(), "b.rs".into()] }), None);
        assert!(conflict.contains("충돌 파일: a.rs, b.rs.") && conflict.contains("원래 상태로"));
        assert!(merge_note(&r, Some(&Outcome::Failed { message: "boom".into(), touched: true }), None).contains("도중 상태로 남았을 수"));
        assert!(merge_note(&r, Some(&Outcome::Failed { message: "boom".into(), touched: false }), None).contains("바뀌지 않았어요"));
    }

    #[test]
    fn verified_text_is_cleaned_and_capped() {
        assert_eq!(verified("   "), None);
        assert_eq!(clean_text("a\u{1b}[31mb\tc\n", 100), "a[31mb\tc");
        let long = clean_text(&"가".repeat(5000), MAX_VERIFIED);
        assert_eq!(long.chars().count(), MAX_VERIFIED + 1);
    }

    #[test]
    fn pending_requests_persist_and_replace_per_worker() {
        let dir = std::env::temp_dir().join(format!("maestro-merges-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("merge-requests.json");
        let m = Merges::at(file.clone());
        let (a, none) = m.add(req("C:/w/api"));
        assert!(none.is_none());
        let (b, _) = m.add(req("C:/w/web"));
        // the same worker again (another spelling): the new one replaces it
        let (c, old) = m.add(req("c:\\W\\API\\"));
        assert_eq!(old.map(|o| o.id), Some(a.id));
        assert!(c.id > b.id);
        let ids = |m: &Merges| m.list().iter().map(|r| r.id).collect::<Vec<_>>();
        assert_eq!(ids(&m), vec![b.id, c.id]);
        // after a restart: the same requests, and new ids keep going up
        let again = Merges::at(file.clone());
        assert_eq!(again.list(), m.list());
        let (d, _) = again.add(req("C:/w/db"));
        assert_eq!(d.id, c.id + 1);
        assert_eq!(again.remove_worker("C:/w/web").len(), 1);
        assert!(again.remove(b.id).is_none());
        assert!(again.remove(d.id).is_some());
        let last = Merges::at(file);
        assert_eq!(ids(&last), vec![c.id]);
        // the Maestro discarded: every request it made goes
        assert!(last.remove_maestro("C:/p/other").is_empty());
        assert_eq!(last.remove_maestro("c:\\P\\repo\\").len(), 1);
        assert!(last.list().is_empty());
        // one merge per project folder at a time
        let g = m.lock_root("C:/p/repo").unwrap();
        assert!(m.merging("c:\\P\\repo\\") && m.lock_root("C:/p/repo").is_err());
        drop(g);
        assert!(!m.merging("C:/p/repo"));
        // the app's commit locks a worker folder for its own project's agents (all, when the project is unknown)
        let c = m.commit_lock("C:/w/api", "C:/p/repo");
        assert_eq!(m.committing(Some("c:\\P\\repo\\")), vec![key("C:/w/api")]);
        assert!(m.committing(Some("C:/p/other")).is_empty());
        assert_eq!(m.committing(None).len(), 1);
        drop(c);
        assert!(m.committing(None).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
