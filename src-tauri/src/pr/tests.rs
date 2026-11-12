//! PR mode against a temp repo whose `origin` (https://gh.test/o/r.git) is rewritten by `insteadOf` to a local bare
//! repo, and a fake `gh` (a script first on the PATH given to the children) that logs its argv and answers from
//! canned files. Nothing here can reach the network: `.test` never resolves, and the fixture refuses to run unless
//! the remote host is gh.test and the gh it found is the fake.

use super::*;
use crate::github::Timeouts;
use crate::worktree;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

struct TestHost {
    tools: Tools,
    home: PathBuf,
    now: AtomicU64,
    events: Mutex<Vec<(String, Value)>>,
    notes: Mutex<Vec<(String, String)>>,
    busy: AtomicBool,
    pr_busy: AtomicBool,
}

impl Host for TestHost {
    fn tools(&self) -> Tools {
        self.tools.clone()
    }
    fn home(&self) -> PathBuf {
        self.home.clone()
    }
    fn now(&self) -> u64 {
        self.now.load(Ordering::SeqCst)
    }
    fn emit(&self, event: &str, payload: Value) {
        self.events.lock().unwrap().push((event.to_string(), payload));
    }
    fn note(&self, _root: &str, _to: &str, key: &str, text: &str) {
        self.notes.lock().unwrap().push((key.to_string(), text.to_string()));
    }
    fn worker_busy(&self, _worker: &str) -> bool {
        self.busy.load(Ordering::SeqCst)
    }
    fn base_branch(&self, _root: &str) -> String {
        "main".into()
    }
    fn pr_busy(&self, _worker: &str) -> bool {
        self.pr_busy.load(Ordering::SeqCst)
    }
}

impl TestHost {
    fn advance(&self, ms: u64) {
        self.now.fetch_add(ms, Ordering::SeqCst);
    }
    fn notes(&self) -> Vec<(String, String)> {
        self.notes.lock().unwrap().clone()
    }
    fn events(&self, name: &str) -> Vec<Value> {
        self.events.lock().unwrap().iter().filter(|(e, _)| e == name).map(|(_, v)| v.clone()).collect()
    }
}

/// A git call for the test's own setup (local only).
fn g(dir: &str, args: &[&str]) -> String {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?} in {dir}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

fn commit_all(dir: &str, msg: &str) -> String {
    g(dir, &["add", "-A"]);
    g(dir, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-m", msg]);
    g(dir, &["rev-parse", "HEAD"])
}

fn put(dir: &str, file: &str, text: &str) {
    std::fs::write(Path::new(dir).join(file), text).unwrap();
}

const FAKE_CMD: &str = r#"@echo off
setlocal EnableExtensions EnableDelayedExpansion
set "D=%~dp0.."
>>"%D%\argv.log" echo(%*
set "K=%~1_%~2"
set "K=!K:/=-!"
:scan
if "%~1"=="" goto scanned
if "%~1"=="--body-file" type "%~2" >>"%D%\bodies.log"
if "%~1"=="--body-file" >>"%D%\bodies.log" echo(
if "%~1"=="--body-file" >>"%D%\bodies.log" echo ---BODY-END---
shift
goto scan
:scanned
set N=0
if exist "%D%\resp\%K%.n" set /p N=<"%D%\resp\%K%.n"
set /a N+=1
>"%D%\resp\%K%.n" echo !N!
set "B=%D%\resp\%K%.!N!"
if not exist "!B!.out" if not exist "!B!.err" if not exist "!B!.code" if not exist "!B!.sleep" set "B=%D%\resp\%K%"
if exist "!B!.sleep" (
  set /p S=<"!B!.sleep"
  ping -n !S! 127.0.0.1 >nul
)
if exist "!B!.out" type "!B!.out"
if exist "!B!.err" type "!B!.err" 1>&2
set C=0
if exist "!B!.code" set /p C=<"!B!.code"
exit /b !C!
"#;

const FAKE_SH: &str = r#"#!/bin/sh
D="$(cd "$(dirname "$0")/.." && pwd)"
echo "$*" >> "$D/argv.log"
K=$(printf '%s' "$1_$2" | tr '/' '-')
prev=""
for a in "$@"; do
  if [ "$prev" = "--body-file" ]; then cat "$a" >> "$D/bodies.log"; printf '\n---BODY-END---\n' >> "$D/bodies.log"; fi
  prev="$a"
done
N=0; [ -f "$D/resp/$K.n" ] && N=$(cat "$D/resp/$K.n"); N=$((N+1)); echo $N > "$D/resp/$K.n"
B="$D/resp/$K.$N"
if [ ! -f "$B.out" ] && [ ! -f "$B.err" ] && [ ! -f "$B.code" ] && [ ! -f "$B.sleep" ]; then B="$D/resp/$K"; fi
[ -f "$B.sleep" ] && sleep "$(cat "$B.sleep")"
[ -f "$B.out" ] && cat "$B.out"
[ -f "$B.err" ] && cat "$B.err" >&2
C=0; [ -f "$B.code" ] && C=$(cat "$B.code")
exit $C
"#;

struct Fx {
    dir: PathBuf,
    root: String,
    wt: String,
    bare: String,
    host: TestHost,
    merges: Merges,
    prs: Prs,
}

fn pr_row(n: u64, head: &str, base: &str) -> String {
    format!(r#"{{"number":{n},"url":"https://gh.test/o/r/pull/{n}","state":"OPEN","isDraft":false,"headRefName":"wt","headRefOid":"{head}","baseRefName":"{base}","isCrossRepository":false,"headRepositoryOwner":{{"login":"o"}}}}"#)
}

impl Fx {
    fn new(tag: &str) -> Fx {
        let dir = std::env::temp_dir().join(format!("maestro-prt-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("repo")).unwrap();
        let s = |p: PathBuf| worktree::norm(&p);
        let (root, bare, wt) = (s(dir.join("repo")), s(dir.join("remote.git")), s(dir.join("wt")));
        g(&root, &["init", "-q", "-b", "main"]);
        put(&root, "a.txt", "a\n");
        commit_all(&root, "init");
        Command::new("git").args(["init", "-q", "--bare", "-b", "main", &bare]).output().unwrap();
        g(&root, &["push", "-q", &bare, "main"]);
        g(&root, &["remote", "add", "origin", "https://gh.test/o/r.git"]);
        g(&root, &["config", &format!("url.{bare}.insteadOf"), "https://gh.test/o/r.git"]);
        g(&root, &["fetch", "-q", "origin"]);
        g(&root, &["worktree", "add", "-q", "-b", "wt", &wt, "main"]);
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(dir.join("resp")).unwrap();
        if cfg!(windows) {
            std::fs::write(bin.join("gh.cmd"), FAKE_CMD.replace('\n', "\r\n")).unwrap();
        } else {
            std::fs::write(bin.join("gh"), FAKE_SH).unwrap();
            Command::new("chmod").args(["+x", &bin.join("gh").to_string_lossy()]).output().unwrap();
        }
        let mut paths = vec![bin.clone()];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
        let path_var = std::env::join_paths(paths).unwrap();
        let gh = github::find_gh(&path_var).unwrap();
        // never a real gh, never a real remote
        assert!(gh.starts_with(&dir), "the fake gh must come first: {gh:?}");
        assert!(matches!(github::parse_remote_url(&g(&root, &["config", "--get", "remote.origin.url"])), github::RemoteUrl::Repo { ref host, .. } if host == "gh.test"));
        let t = Timeouts { create: 30.0, auth: 15.0, recover: vec![Duration::ZERO, Duration::from_millis(10), Duration::from_millis(10)], unknown_pause: Duration::from_millis(10), retries: vec![], ..Default::default() };
        let host = TestHost {
            tools: Tools { gh: Some(gh), ssh: Some(bin.join("no-ssh")), path_var: Some(path_var), t },
            home: dir.join("home"),
            now: AtomicU64::new(merge::now_ms()),
            events: Mutex::default(),
            notes: Mutex::default(),
            busy: AtomicBool::new(false),
            pr_busy: AtomicBool::new(false),
        };
        let fx = Fx { merges: Merges::at(dir.join("merge-requests.json")), prs: Prs::at(dir.join("pull-requests.json")), dir, root, wt, bare, host };
        fx.resp("--version_.out", "gh version 2.102.0 (2026-09-30)\n");
        fx.resp("repo_view.out", r#"{"nameWithOwner":"o/r","viewerPermission":"WRITE","mergeCommitAllowed":true,"squashMergeAllowed":true,"isArchived":false}"#);
        fx.resp("pr_list.out", "[]");
        fx.resp("pr_create.out", "https://gh.test/o/r/pull/7\n");
        fx.resp("pr_view.out", r#"{"number":7,"url":"https://gh.test/o/r/pull/7","state":"OPEN","isDraft":false,"headRefOid":"x","headRefName":"wt","baseRefName":"main"}"#);
        fx
    }

    fn resp(&self, name: &str, text: &str) {
        std::fs::write(self.dir.join("resp").join(name), text).unwrap();
    }

    /// The number the next call of `key` (e.g. "auth_status") will read its answer under.
    fn next(&self, key: &str) -> u32 {
        std::fs::read_to_string(self.dir.join("resp").join(format!("{key}.n"))).ok().and_then(|n| n.trim().parse().ok()).unwrap_or(0) + 1
    }

    fn argv(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.join("argv.log")).unwrap_or_default().lines().map(String::from).collect()
    }

    /// The bodies gh got through --body-file, one per call, in order.
    fn bodies(&self) -> Vec<String> {
        let all = std::fs::read_to_string(self.dir.join("bodies.log")).unwrap_or_default().replace("\r\n", "\n");
        let mut v: Vec<String> = all.split("---BODY-END---\n").map(String::from).collect();
        v.pop();
        v
    }

    fn calls(&self, what: &str) -> usize {
        self.argv().iter().filter(|l| l.starts_with(what)).count()
    }

    fn bare_tip(&self, branch: &str) -> Option<String> {
        let out = Command::new("git").args(["-C", &self.bare, "rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).output().unwrap();
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    fn work(&self, file: &str) -> String {
        put(&self.wt, file, &format!("{file}\n"));
        commit_all(&self.wt, &format!("add {file}"))
    }

    fn plan(&self, mode: MergeMode) -> Result<worktree::MergePlan, String> {
        let opts = plan_opts(&self.host.tools, &self.prs, &self.root, "main", &self.wt, mode);
        worktree::plan_merge_with(&self.root, "main", &self.wt, &opts)
    }

    /// A PR-mode request as the conductor's `maestro merge-request` makes it; its id.
    fn request(&self, verified: Option<&str>) -> u64 {
        let plan = self.plan(MergeMode::Pr).unwrap();
        let mut req = MergeRequest { id: 0, root: self.root.clone(), maestro_path: self.root.clone(), worker_path: self.wt.clone(), worker_name: "api".into(), verified: verified.and_then(merge::verified), requested_at: 1, plan, pr: None, pr_draft: None };
        req.pr_draft = Some(draft(&self.host.tools, &req, &self.host.home));
        self.merges.add(req).0.id
    }

    fn approve(&self, id: u64, opts: PrOptions) -> Result<MergeResult, String> {
        approve(&self.host, &self.merges, &self.prs, id, opts)
    }

    fn step(&self, id: u64) -> Step {
        self.merges.get(id).and_then(|r| r.pr).map(|p| p.step).unwrap_or_default()
    }
}

impl Drop for Fx {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn opts(title: &str) -> PrOptions {
    PrOptions { title: title.into(), ..Default::default() }
}

#[test]
fn approve_pushes_the_recorded_tip_and_creates_the_pr() {
    let fx = Fx::new("create");
    let tip = fx.work("b.txt");
    let id = fx.request(Some("cargo test 통과, 경로 C:/Users/someone/x.txt"));
    // later work isn't pushed: the recorded tip is
    fx.work("late.txt");
    let draft = fx.merges.get(id).unwrap().pr_draft.unwrap();
    assert_eq!(draft.title, "add b.txt");
    assert!(!draft.draft, "verified: not a draft by default");
    let r = fx.approve(id, PrOptions { title: "Add b".into(), body: draft.body.clone(), ..Default::default() }).unwrap();
    assert_eq!(r.outcome, "pr");
    assert_eq!(fx.bare_tip("wt").as_deref(), Some(tip.as_str()));
    let create: Vec<String> = fx.argv().into_iter().filter(|l| l.starts_with("pr create")).collect();
    assert_eq!(create.len(), 1);
    assert!(create[0].contains("-R gh.test/o/r --base main --head wt --title") && create[0].contains("--body-file"), "{}", create[0]);
    assert!(!create[0].contains("--draft"));
    let rec = fx.prs.get(&fx.wt).unwrap();
    assert_eq!((rec.number, rec.state.as_str(), rec.app_created, rec.head_sha.as_str()), (7, "OPEN", true, "x"));
    assert_eq!(rec.slug(), "gh.test/o/r");
    assert!(fx.merges.get(id).is_none(), "the request is done");
    let notes = fx.host.notes();
    assert!(notes.iter().any(|(k, n)| k.starts_with("merge:") && n.starts_with("[maestro] PR 생성: 워커 'api' (wt → main) PR #7")), "{notes:?}");
    assert_eq!(fx.host.events("maestro://merge-done")[0]["outcome"], "pr");
    let body = std::fs::read_to_string(fx.dir.join("bodies.log")).unwrap();
    assert!(body.contains("## Maestro 요약") && body.contains("cargo test 통과"), "{body}");
    for local in [fx.root.as_str(), fx.wt.as_str(), "C:/Users/someone"] {
        assert!(!body.contains(local), "{local} in {body}");
    }
    assert!(body.contains("<로컬 경로>"));
    // no force, no tags, no upstream: the worker's config is untouched
    assert!(Command::new("git").args(["-C", &fx.wt, "config", "--get", "branch.wt.remote"]).output().unwrap().stdout.is_empty());
}

#[test]
fn dirty_worker_needs_the_users_commit() {
    let fx = Fx::new("dirty");
    let tip = fx.work("b.txt");
    put(&fx.wt, "a.txt", "edited\n");
    put(&fx.wt, "loose.txt", "untracked\n");
    let id = fx.request(None);
    let req = fx.merges.get(id).unwrap();
    assert_eq!(req.plan.worker_dirty, ["a.txt"]);
    assert!(req.pr_draft.as_ref().unwrap().draft, "no verification: draft");
    // no tick, no message: nothing committed or pushed
    let e = fx.approve(id, opts("t")).unwrap_err();
    assert!(e.contains("커밋하지 않은 변경이 1개"), "{e}");
    assert_eq!(g(&fx.wt, &["rev-parse", "HEAD"]), tip);
    assert!(fx.bare_tip("wt").is_none());
    // the first create fails for sure (nothing made): the commit stays recorded
    fx.resp("pr_create.1.code", "1");
    fx.resp("pr_create.1.err", "pull request create failed: HTTP 422: Validation Failed");
    // the card always sends its body (it never has the app's line)
    let card_body = req.pr_draft.as_ref().unwrap().body.clone();
    let o = PrOptions { commit_message: Some("api: 작업 커밋".into()), title: "t".into(), body: card_body, ..Default::default() };
    let e = fx.approve(id, o.clone()).unwrap_err();
    assert!(e.contains("만들지 않았어요"), "{e}");
    assert_eq!(fx.step(id), Step::Pushed);
    let app_commit = g(&fx.wt, &["rev-parse", "HEAD"]);
    assert_eq!(g(&fx.wt, &["rev-parse", "HEAD^"]), tip);
    assert_eq!(g(&fx.wt, &["diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"]), "a.txt");
    assert!(Path::new(&fx.wt).join("loose.txt").exists());
    assert_eq!(g(&fx.wt, &["status", "--porcelain"]), "?? loose.txt");
    assert_eq!(fx.bare_tip("wt").as_deref(), Some(app_commit.as_str()));
    // again: no second commit
    let r = fx.approve(id, o).unwrap();
    assert_eq!(r.outcome, "pr");
    assert_eq!(g(&fx.wt, &["rev-parse", "HEAD"]), app_commit);
    assert_eq!(fx.calls("pr create"), 2);
    let bodies = fx.bodies();
    assert_eq!(bodies.len(), 2, "{bodies:?}");
    for b in &bodies {
        assert_eq!(b.matches("앱이 사용자 확인으로 커밋했어요: api: 작업 커밋").count(), 1, "{b}");
    }
}

#[test]
fn changed_dirty_files_refuse_the_commit() {
    let fx = Fx::new("dirtymove");
    fx.work("b.txt");
    put(&fx.wt, "a.txt", "edited\n");
    let id = fx.request(None);
    put(&fx.wt, "b.txt", "edited too\n");
    let e = fx.approve(id, PrOptions { commit_message: Some("m".into()), title: "t".into(), ..Default::default() }).unwrap_err();
    assert!(e.contains("변경 파일이 달라졌어요"), "{e}");
    // the agent working: refused too
    g(&fx.wt, &["checkout", "--", "b.txt"]);
    fx.host.busy.store(true, Ordering::SeqCst);
    let e = fx.approve(id, PrOptions { commit_message: Some("m".into()), title: "t".into(), ..Default::default() }).unwrap_err();
    assert!(e.contains("워커가 작업 중"), "{e}");
    assert_eq!(fx.calls("pr create"), 0);
}

#[test]
fn an_open_pr_is_reused_not_created_again() {
    let fx = Fx::new("reuse");
    let tip = fx.work("b.txt");
    let id = fx.request(Some("ok"));
    fx.resp("pr_list.out", &format!("[{}]", pr_row(5, &tip, "main")));
    let r = fx.approve(id, PrOptions { title: "t".into(), update_body: true, ..Default::default() }).unwrap();
    assert_eq!((r.outcome, r.pr.as_ref().unwrap().number), ("recovered", 5));
    assert_eq!(fx.calls("pr create"), 0);
    // the card is gone by now: what it would have said stays on the PR
    assert!(fx.prs.get(&fx.wt).unwrap().notice.unwrap().contains("기존 PR #5을 찾아 이어 썼어요"));
    // not made by the app: never edited, even when asked
    assert_eq!(fx.calls("pr edit"), 0);
    assert!(!fx.prs.get(&fx.wt).unwrap().app_created);
    // the conductor asks again after more work: same PR, updated by a push; edited only when it's the app's own
    fx.prs.update(&fx.wt, |r| {
        r.app_created = true;
        r.checks = github::Checks { state: "success".into(), passed: 1, head_sha: tip.clone(), ..Default::default() };
    });
    let tip2 = fx.work("c.txt");
    let id = fx.request(Some("ok"));
    assert_eq!(fx.merges.get(id).unwrap().plan.existing_pr, Some(5));
    fx.resp("pr_list.out", &format!("[{}]", pr_row(5, &tip2, "main")));
    fx.resp("pr_edit.code", "1");
    fx.resp("pr_edit.err", "HTTP 502: Bad Gateway");
    let r = fx.approve(id, PrOptions { title: "t2".into(), update_body: true, ..Default::default() }).unwrap();
    assert_eq!(r.outcome, "pr-updated");
    assert_eq!(fx.bare_tip("wt").as_deref(), Some(tip2.as_str()));
    // the old head's green CI is not the new head's
    let rec = fx.prs.get(&fx.wt).unwrap();
    assert_eq!((rec.head_sha.as_str(), rec.checks.state.as_str(), rec.checks.head_sha.as_str()), (tip2.as_str(), "pending", tip2.as_str()));
    let notice = rec.notice.unwrap();
    assert!(notice.contains("PR #5을 갱신했어요") && notice.contains("본문은 갱신하지 못했어요"), "{notice}");
    let edits: Vec<String> = fx.argv().into_iter().filter(|l| l.starts_with("pr edit")).collect();
    assert_eq!(edits.len(), 1);
    assert!(edits[0].starts_with("pr edit 5 -R gh.test/o/r --title"), "{}", edits[0]);
    assert_eq!(fx.calls("pr create"), 0);
}

#[test]
fn two_open_prs_or_a_failed_lookup_create_nothing() {
    let fx = Fx::new("refuse");
    let tip = fx.work("b.txt");
    let id = fx.request(None);
    fx.resp("pr_list.out", &format!("[{},{}]", pr_row(5, &tip, "main"), pr_row(6, &tip, "main")));
    let e = fx.approve(id, opts("t")).unwrap_err();
    assert!(e.contains("여러 개") && e.contains("#5") && e.contains("#6"), "{e}");
    fx.resp("pr_list.out", &format!("[{}]", pr_row(5, &tip, "dev")));
    assert!(fx.approve(id, opts("t")).unwrap_err().contains("다른 기준(dev)"));
    std::fs::remove_file(fx.dir.join("resp/pr_list.out")).unwrap();
    fx.resp("pr_list.code", "1");
    fx.resp("pr_list.err", "HTTP 502: Bad Gateway");
    let e = fx.approve(id, opts("t")).unwrap_err();
    assert!(e.contains("확인하지 못해 만들지 않았어요"), "{e}");
    assert_eq!(fx.calls("pr create"), 0);
    assert!(fx.merges.get(id).is_some(), "the card stays");
}

#[test]
fn a_create_that_times_out_recovers_the_pr() {
    let mut fx = Fx::new("timeout");
    // a create that never answers in time (the others get 30 s: process starts are slow under load)
    fx.host.tools.t.create = 2.0;
    let tip = fx.work("b.txt");
    let id = fx.request(None);
    fx.resp("pr_create.sleep", "8");
    fx.resp("pr_list.1.out", "[]");
    fx.resp("pr_list.2.out", &format!("[{}]", pr_row(8, &tip, "main")));
    let r = fx.approve(id, opts("t")).unwrap();
    assert_eq!((r.outcome, r.pr.as_ref().unwrap().number), ("recovered", 8));
    assert!(r.message.contains("기존 PR #8을 찾아 이어 썼어요"));
    assert_eq!(fx.calls("pr create"), 1);
    assert!(fx.prs.get(&fx.wt).unwrap().app_created, "made by the app's attempt");
    assert!(fx.host.notes().iter().any(|(_, n)| n.starts_with("[maestro] 기존 PR #8을 찾아 이어 썼어요")));
}

#[test]
fn already_exists_recovers_the_pr() {
    let fx = Fx::new("exists");
    let tip = fx.work("b.txt");
    let id = fx.request(None);
    fx.resp("pr_create.code", "1");
    fx.resp("pr_create.err", "a pull request for branch \"wt\" into branch \"main\" already exists:\nhttps://gh.test/o/r/pull/9\n");
    fx.resp("pr_list.1.out", "[]");
    fx.resp("pr_list.2.out", &format!("[{}]", pr_row(9, &tip, "main")));
    let r = fx.approve(id, opts("t")).unwrap();
    assert_eq!((r.outcome, r.pr.unwrap().number), ("recovered", 9));
    assert_eq!(fx.calls("pr create"), 1);
}

#[test]
fn unconfirmed_create_needs_a_click_and_30_seconds() {
    let fx = Fx::new("unconfirmed");
    fx.work("b.txt");
    let id = fx.request(None);
    fx.resp("pr_create.1.code", "1");
    fx.resp("pr_create.1.err", "HTTP 502: Bad Gateway");
    let e = fx.approve(id, opts("t")).unwrap_err();
    assert!(e.contains("'다시 확인'은 찾기만 하고"), "{e}");
    assert_eq!(fx.step(id), Step::Unconfirmed);
    // "다시 확인": a lookup only
    let lists = fx.calls("pr list");
    let e = fx.approve(id, opts("t")).unwrap_err();
    assert!(e.contains("아직 PR이 보이지 않아요"), "{e}");
    assert_eq!((fx.calls("pr create"), fx.calls("pr list")), (1, lists + 1));
    assert!(fx.merges.get(id).unwrap().pr.unwrap().last_empty_lookup_at.is_some());
    // "다시 만들기" before 30 s: still nothing
    let e = fx.approve(id, PrOptions { recreate: true, ..opts("t") }).unwrap_err();
    assert!(e.contains("30초"), "{e}");
    assert_eq!(fx.calls("pr create"), 1);
    // after 30 s, with a fresh empty lookup: exactly one more create
    fx.host.advance(31_000);
    let r = fx.approve(id, PrOptions { recreate: true, ..opts("t") }).unwrap();
    assert_eq!(r.outcome, "pr");
    assert_eq!(fx.calls("pr create"), 2);
}

#[test]
fn an_unclear_create_outlives_its_request() {
    let fx = Fx::new("carry");
    fx.work("b.txt");
    let id = fx.request(None);
    fx.resp("pr_create.1.code", "1");
    fx.resp("pr_create.1.err", "HTTP 502: Bad Gateway");
    fx.approve(id, opts("t")).unwrap_err();
    assert_eq!(fx.step(id), Step::Unconfirmed);
    // the conductor asks again for the same worker: the new request keeps the attempt
    let id2 = fx.request(Some("다시"));
    assert!(fx.merges.get(id).is_none());
    let p = fx.merges.get(id2).unwrap().pr.unwrap();
    assert!(p.step == Step::Unconfirmed && p.create_attempt_at.is_some(), "{p:?}");
    // approving it only looks (GitHub's list may lag behind the create): no second PR
    let e = fx.approve(id2, opts("t")).unwrap_err();
    assert!(e.contains("아직 PR이 보이지 않아요"), "{e}");
    assert_eq!(fx.calls("pr create"), 1);
    // the merge mode can't change meanwhile (it would drop the request unseen)
    let e = set_merge_mode(&fx.host, &fx.merges, &fx.root, MergeMode::Pr).unwrap_err();
    assert!(e.contains("찾지 못한"), "{e}");
    assert!(fx.merges.get(id2).is_some());
    // a reject looks first: the PR found is recorded and the conductor hears it stays open
    let tip = g(&fx.wt, &["rev-parse", "HEAD"]);
    fx.resp("pr_list.out", &format!("[{}]", pr_row(8, &tip, "main")));
    let r = reject(&fx.host, &fx.merges, &fx.prs, id2, None).unwrap();
    assert!(r.message.contains("열린 PR #8은 남아 있어요"), "{}", r.message);
    assert_eq!(fx.prs.get(&fx.wt).unwrap().number, 8);
    assert!(fx.host.notes().iter().any(|(k, n)| k.starts_with("merge:") && n.contains("열린 PR #8은 남아 있어요")));
    assert_eq!(fx.calls("pr create"), 1);
    // a request replaced while its create runs: the new one is unconfirmed too
    let id3 = fx.request(None);
    let at = fx.host.now();
    fx.merges.update(id3, |r| r.pr = Some(PrProgress { step: Step::Creating, create_attempt_at: Some(at), ..Default::default() }));
    let id4 = fx.request(None);
    assert_eq!(fx.step(id4), Step::Unconfirmed);
    // a create GitHub certainly refused leaves nothing to carry
    fx.merges.update(id4, |r| r.pr = Some(PrProgress { step: Step::Pushed, ..Default::default() }));
    let id5 = fx.request(None);
    assert!(fx.merges.get(id5).unwrap().pr.is_none());
}

#[test]
fn a_diverged_remote_branch_is_never_overwritten() {
    let fx = Fx::new("diverged");
    fx.work("b.txt");
    let id = fx.request(None);
    // someone else's commit on the remote branch
    g(&fx.root, &["switch", "-q", "-c", "other"]);
    put(&fx.root, "o.txt", "other\n");
    let theirs = commit_all(&fx.root, "theirs");
    g(&fx.root, &["switch", "-q", "main"]);
    g(&fx.root, &["push", "-q", &fx.bare, "other:refs/heads/wt"]);
    let e = fx.approve(id, opts("t")).unwrap_err();
    assert!(e.contains("덮어쓰지 않아요"), "{e}");
    assert_eq!(fx.bare_tip("wt").as_deref(), Some(theirs.as_str()));
    assert_eq!(fx.calls("pr create"), 0);
}

#[test]
fn local_only_base_commits_block_before_the_push() {
    let fx = Fx::new("basecheck");
    put(&fx.root, "c.txt", "local only\n");
    commit_all(&fx.root, "not pushed");
    g(&fx.wt, &["merge", "-q", "main"]);
    fx.work("b.txt");
    let id = fx.request(None);
    let e = fx.approve(id, opts("t")).unwrap_err();
    assert!(e.contains("원격에 없는 커밋 1개"), "{e}");
    assert!(fx.bare_tip("wt").is_none());
}

#[test]
fn a_remote_tip_moved_before_create_is_refused() {
    let fx = Fx::new("moved");
    fx.work("b.txt");
    let id = fx.request(None);
    let (wt, bare) = (fx.wt.clone(), fx.bare.clone());
    BEFORE_CREATE.with(|h| {
        *h.borrow_mut() = Some(Box::new(move || {
            g(&wt, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "late"]);
            g(&wt, &["push", "-q", &bare, "HEAD:refs/heads/wt"]);
        }))
    });
    let e = fx.approve(id, opts("t"));
    BEFORE_CREATE.with(|h| *h.borrow_mut() = None);
    let e = e.unwrap_err();
    assert!(e.contains("원격 브랜치 끝이 승인한 커밋과 달라요"), "{e}");
    assert_eq!(fx.calls("pr create"), 0);
}

#[test]
fn restart_reconciles_with_reads_only() {
    let fx = Fx::new("resume");
    let tip = fx.work("b.txt");
    put(&fx.wt, "a.txt", "edited\n");
    let id = fx.request(None);
    // the app went down right after its commit landed
    fx.merges.update(id, |r| r.pr = Some(PrProgress { step: Step::Committing, commit_parent: Some(tip.clone()), commit_message: Some("m".into()), ..Default::default() }));
    g(&fx.wt, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-am", "m"]);
    let app_commit = g(&fx.wt, &["rev-parse", "HEAD"]);
    let merges = Merges::at(fx.dir.join("merge-requests.json"));
    resume(&fx.host, &merges, &fx.prs);
    let p = merges.get(id).unwrap().pr.unwrap();
    assert_eq!((p.step, p.app_commit.as_deref()), (Step::Committed, Some(app_commit.as_str())));
    // ...or before it did
    merges.update(id, |r| r.pr = Some(PrProgress { step: Step::Committing, commit_parent: Some(app_commit.clone()), commit_message: Some("x".into()), ..Default::default() }));
    resume(&fx.host, &merges, &fx.prs);
    let p = merges.get(id).unwrap().pr.unwrap();
    assert_eq!(p.step, Step::None);
    assert!(p.error.unwrap().contains("커밋 도중 꺼졌어요"));
    // mid-push: the remote already has it
    g(&fx.wt, &["push", "-q", &fx.bare, &format!("{app_commit}:refs/heads/wt")]);
    merges.update(id, |r| r.pr = Some(PrProgress { step: Step::Pushing, app_commit: Some(app_commit.clone()), push_sha: Some(app_commit.clone()), ..Default::default() }));
    resume(&fx.host, &merges, &fx.prs);
    assert_eq!(merges.get(id).unwrap().pr.unwrap().step, Step::Pushed);
    // mid-create, nothing found: unconfirmed
    merges.update(id, |r| r.pr = Some(PrProgress { step: Step::Creating, app_commit: Some(app_commit.clone()), push_sha: Some(app_commit.clone()), create_attempt_at: Some(1), ..Default::default() }));
    resume(&fx.host, &merges, &fx.prs);
    let p = merges.get(id).unwrap().pr.unwrap();
    assert_eq!(p.step, Step::Unconfirmed);
    assert!(p.last_empty_lookup_at.is_some());
    // found: carried on with, the request done
    fx.resp("pr_list.out", &format!("[{}]", pr_row(4, &app_commit, "main")));
    resume(&fx.host, &merges, &fx.prs);
    assert!(merges.get(id).is_none());
    assert_eq!(fx.prs.get(&fx.wt).unwrap().number, 4);
    // a merge click the app didn't see the end of: merged on GitHub
    fx.prs.update(&fx.wt, |r| r.merge_attempt = Some(MergeAttempt { method: "squash".into(), seen_head: app_commit.clone(), at: 1 }));
    fx.resp("pr_view.out", r#"{"state":"MERGED","mergedAt":"2026-11-11T00:00:00Z","mergeCommit":{"oid":"abc1234def"},"autoMergeRequest":null,"headRefOid":"x"}"#);
    let prs = Prs::at(fx.dir.join("pull-requests.json"));
    resume(&fx.host, &merges, &prs);
    let rec = prs.get(&fx.wt).unwrap();
    assert_eq!((rec.state.as_str(), rec.merged.as_ref().unwrap().method.as_str()), ("MERGED", "squash"));
    assert_eq!(rec.merged_heads, [app_commit.clone()]);
    for write in ["pr create", "pr edit", "pr merge"] {
        assert_eq!(fx.calls(write), 0, "{write}");
    }
}

/// An open PR #3 for the worker, pushed, its checks green; the head pushed to the remote.
fn open_pr_fixture(tag: &str) -> (Fx, String) {
    let fx = Fx::new(tag);
    let tip = fx.work("b.txt");
    g(&fx.wt, &["push", "-q", &fx.bare, "wt"]);
    let ready = github::check(&fx.host.tools, &fx.root, "main", true);
    assert!(ready.ok, "{}", ready.message);
    let req = MergeRequest { id: 1, root: fx.root.clone(), maestro_path: fx.root.clone(), worker_path: fx.wt.clone(), worker_name: "api".into(), verified: None, requested_at: 0, plan: fx.plan(MergeMode::Pr).unwrap(), pr: None, pr_draft: None };
    let mut rec = record(&req, &ready, 3, "https://gh.test/o/r/pull/3", &tip, true, false);
    rec.checks.state = "success".into();
    rec.checks.head_sha = tip.clone();
    rec.mergeable = "MERGEABLE".into();
    fx.prs.upsert(rec);
    (fx, tip)
}

/// What GitHub would do for a squash: one commit with the branch's change on the remote base.
fn squash_on_remote(fx: &Fx, tip: &str) -> String {
    let clone = worktree::norm(&fx.dir.join("clone"));
    Command::new("git").args(["clone", "-q", &fx.bare, &clone]).output().unwrap();
    g(&clone, &["merge", "-q", "--squash", tip]);
    g(&clone, &["-c", "user.name=gh", "-c", "user.email=gh@gh", "commit", "-q", "-m", "squash (#3)"]);
    g(&clone, &["push", "-q", "origin", "main"]);
    g(&clone, &["rev-parse", "HEAD"])
}

/// `gh pr view` of PR #3, its head's CI green.
fn view(state: &str, head: &str, commit: Option<&str>) -> String {
    let mc = commit.map(|c| format!(r#"{{"oid":"{c}"}}"#)).unwrap_or("null".into());
    format!(r#"{{"number":3,"state":"{state}","isDraft":false,"mergeable":"MERGEABLE","mergeStateStatus":"CLEAN","headRefOid":"{head}","baseRefName":"main","reviewDecision":"APPROVED","autoMergeRequest":null,"mergedAt":null,"mergeCommit":{mc},"statusCheckRollup":{GREEN}}}"#)
}

const GREEN: &str = r#"[{"__typename":"CheckRun","name":"build","status":"COMPLETED","conclusion":"SUCCESS"}]"#;
const RED: &str = r#"[{"__typename":"CheckRun","name":"build","status":"COMPLETED","conclusion":"FAILURE"}]"#;
const NO_ACK: MergeAck = MergeAck { ci: None, changes_requested: false };

/// The status query's answer for PR #3 (the merge click reads it before `gh pr merge`), the head's checks `checks`.
fn status3(state: &str, head: &str, checks: &str) -> String {
    status_at(state, head, head, checks)
}

/// ...with the commit the checks belong to apart from the head (GitHub not caught up yet).
fn status_at(state: &str, head: &str, commit: &str, checks: &str) -> String {
    let nodes: Value = serde_json::from_str(checks).unwrap();
    json!({"data": {"rateLimit": {"limit": 5000, "remaining": 4999, "resetAt": "2030-01-01T00:00:00Z", "cost": 1}, "r0": {"p0": {
        "number": 3, "url": "https://gh.test/o/r/pull/3", "state": state, "isDraft": false, "headRefName": "wt", "headRefOid": head, "baseRefName": "main",
        "mergeable": "MERGEABLE", "mergeStateStatus": "CLEAN", "reviewDecision": "APPROVED", "mergedAt": null, "mergeCommit": null, "autoMergeRequest": null,
        "commits": {"nodes": [{"commit": {"oid": commit, "statusCheckRollup": {"state": null, "contexts": {"nodes": nodes}}}}]}}}}})
    .to_string()
}

#[test]
fn merge_checks_the_head_then_squashes_and_fast_forwards() {
    let (fx, tip) = open_pr_fixture("merge");
    // the PR moved since the card showed it: no merge call
    fx.resp("api_graphql.out", &status3("OPEN", "fffffff000", GREEN));
    let e = merge_pr(&fx.host, &fx.merges, &fx.prs, &fx.wt, "squash", &tip, &NO_ACK).unwrap_err();
    assert!(e.contains("새 커밋이 생겼어요"), "{e}");
    assert_eq!(fx.calls("pr merge"), 0);
    assert_eq!(fx.prs.get(&fx.wt).unwrap().head_sha, "fffffff000");
    // as seen: squash, pinned to the head; GitHub merged it
    let squashed = squash_on_remote(&fx, &tip);
    fx.resp("api_graphql.out", &status3("OPEN", &tip, GREEN));
    fx.resp("pr_view.1.out", &view("MERGED", &tip, Some(&squashed)));
    let rec = merge_pr(&fx.host, &fx.merges, &fx.prs, &fx.wt, "squash", &tip, &NO_ACK).unwrap();
    let m: Vec<String> = fx.argv().into_iter().filter(|l| l.starts_with("pr merge")).collect();
    assert_eq!(m, [format!("pr merge 3 -R gh.test/o/r --squash --match-head-commit {tip}")]);
    assert_eq!((rec.state.as_str(), rec.merged.as_ref().unwrap().method.as_str()), ("MERGED", "squash"));
    assert_eq!(rec.merged_heads, [tip.clone()]);
    // clean and on main: fast-forwarded to the squash commit
    assert_eq!(g(&fx.root, &["rev-parse", "HEAD"]), squashed);
    assert_eq!(rec.after_merge.as_ref().unwrap().base_updated.as_deref(), Some(squashed.as_str()));
    let note = fx.host.notes().into_iter().find(|(k, _)| k.starts_with("pr:")).unwrap().1;
    assert!(note.contains("PR #3 병합 완료 (squash") && note.contains("빨리감기 했어요") && note.contains("새 워커로 시작"), "{note}");
    // discard: the squash leaves nothing `git cherry` sees, the recorded head says merged
    let heads = fx.prs.merged_heads(&fx.wt, "wt");
    let c = worktree::check_discard(&fx.root, &fx.wt, "wt", "main", false, &heads).unwrap();
    assert!(c.clean(), "{c:?}");
    // a later commit still shows
    fx.work("c.txt");
    let c = worktree::check_discard(&fx.root, &fx.wt, "wt", "main", false, &heads).unwrap();
    assert_eq!(c.commits.len(), 1);
    assert!(c.commits[0].ends_with("add c.txt"));
    // continuing on the squashed branch is refused until the worker takes the remote base
    let e = fx.plan(MergeMode::Pr).unwrap_err();
    assert!(e.contains("squash로 합쳐진 브랜치"), "{e}");
    g(&fx.wt, &["-c", "user.name=t", "-c", "user.email=t@t", "merge", "-q", "--no-edit", "origin/main"]);
    let p = fx.plan(MergeMode::Pr).unwrap();
    assert!(p.commits.iter().any(|c| c.ends_with("add c.txt")) && !p.commits.iter().any(|c| c.ends_with("add b.txt")), "{:?}", p.commits);
}

#[test]
fn merged_folder_is_left_alone_unless_clean_and_on_base() {
    let (fx, tip) = open_pr_fixture("ffguard");
    let before = g(&fx.root, &["rev-parse", "HEAD"]);
    let merged = squash_on_remote(&fx, &tip);
    fx.resp("api_graphql.out", &status3("OPEN", &tip, GREEN));
    fx.resp("pr_view.1.out", &view("MERGED", &tip, Some(&merged)));
    put(&fx.root, "notes.txt", "mine\n");
    let rec = merge_pr(&fx.host, &fx.merges, &fx.prs, &fx.wt, "merge", &tip, &NO_ACK).unwrap();
    assert_eq!(g(&fx.root, &["rev-parse", "HEAD"]), before);
    let after = rec.after_merge.unwrap();
    assert!(after.base_updated.is_none() && after.message.contains("커밋하지 않은 변경"), "{}", after.message);
    assert!(fx.argv().iter().any(|l| l.starts_with("pr merge 3 -R gh.test/o/r --merge --match-head-commit")));
    // off the base: untouched too; the button works once it's back and clean
    std::fs::remove_file(Path::new(&fx.root).join("notes.txt")).unwrap();
    g(&fx.root, &["switch", "-q", "-c", "side"]);
    let e = pr_sync_base(&fx.host, &fx.merges, &fx.prs, &fx.wt).unwrap_err();
    assert!(e.contains("side에 있어요"), "{e}");
    assert_eq!(g(&fx.root, &["rev-parse", "main"]), before);
    g(&fx.root, &["switch", "-q", "main"]);
    pr_sync_base(&fx.host, &fx.merges, &fx.prs, &fx.wt).unwrap();
    assert_eq!(g(&fx.root, &["rev-parse", "HEAD"]), merged);
}

#[test]
fn merge_into_a_queue_and_merges_seen_by_the_poller() {
    let (fx, tip) = open_pr_fixture("queue");
    fx.resp("api_graphql.1.out", &status3("OPEN", &tip, GREEN));
    fx.resp("pr_view.out", &view("OPEN", &tip, None).replace(r#""autoMergeRequest":null"#, r#""autoMergeRequest":{"enabledAt":"x","mergeMethod":"MERGE"}"#));
    let rec = merge_pr(&fx.host, &fx.merges, &fx.prs, &fx.wt, "merge", &tip, &NO_ACK).unwrap();
    assert!(rec.queued && rec.merge_attempt.is_some());
    // the poller sees it merged: recorded, a note, no fast-forward
    let before = g(&fx.root, &["rev-parse", "HEAD"]);
    let merged = squash_on_remote(&fx, &tip);
    fx.resp("api_graphql.out", &json!({"data":{"rateLimit":{"limit":5000,"remaining":4999,"resetAt":"2030-01-01T00:00:00Z","cost":1},"r0":{"p0":{"number":3,"url":"https://gh.test/o/r/pull/3","state":"MERGED","isDraft":false,"headRefName":"wt","headRefOid":tip,"baseRefName":"main","mergeable":"UNKNOWN","mergeStateStatus":"UNKNOWN","reviewDecision":null,"mergedAt":"x","mergeCommit":{"oid":merged},"autoMergeRequest":null,"commits":{"nodes":[]}}}}}).to_string());
    let mut s = Sched::default();
    poll_once(&fx.host, &fx.prs, &mut s);
    let rec = fx.prs.get(&fx.wt).unwrap();
    assert_eq!((rec.state.as_str(), rec.merged.as_ref().unwrap().method.as_str(), rec.queued), ("MERGED", "merge", false));
    assert_eq!(g(&fx.root, &["rev-parse", "HEAD"]), before);
    assert!(fx.host.notes().iter().any(|(k, n)| k.starts_with("pr:") && n.contains("그대로예요")));
    // merged: no longer polled
    let n = fx.calls("api graphql");
    poll_once(&fx.host, &fx.prs, &mut s);
    assert_eq!(fx.calls("api graphql"), n);
}

#[test]
fn merge_refusals() {
    let (fx, tip) = open_pr_fixture("refusals");
    let try_merge = |ack: &MergeAck| merge_pr(&fx.host, &fx.merges, &fx.prs, &fx.wt, "merge", &tip, ack).unwrap_err();
    let open = || status3("OPEN", &tip, GREEN);
    fx.resp("api_graphql.out", &open().replace(r#""isDraft":false"#, r#""isDraft":true"#));
    assert!(try_merge(&NO_ACK).contains("초안"));
    fx.resp("api_graphql.out", &open().replace(r#""MERGEABLE""#, r#""CONFLICTING""#));
    assert!(try_merge(&NO_ACK).contains("충돌"));
    fx.resp("api_graphql.out", &open().replace("APPROVED", "REVIEW_REQUIRED"));
    assert!(try_merge(&NO_ACK).contains("리뷰나 보호 규칙"));
    fx.resp("api_graphql.out", &status3("CLOSED", &tip, GREEN));
    assert!(try_merge(&NO_ACK).contains("닫혔어요"));
    fx.resp("api_graphql.out", &open().replace(r#""MERGEABLE""#, r#""UNKNOWN""#));
    assert!(try_merge(&NO_ACK).contains("계산 중"));
    // CI as GitHub has it now, not as the card showed it: refused, and the record shows it
    fx.resp("api_graphql.out", &status3("OPEN", &tip, RED));
    assert!(try_merge(&NO_ACK).contains("CI가 실패했어요"));
    assert_eq!(fx.prs.get(&fx.wt).unwrap().checks.state, "failure");
    // the user confirmed only the change request (the card showed green): a red CI found now is still refused
    let review_only = MergeAck { ci: None, changes_requested: true };
    fx.resp("api_graphql.out", &status3("OPEN", &tip, RED).replace("APPROVED", "CHANGES_REQUESTED"));
    assert!(try_merge(&review_only).contains("CI가 실패했어요"));
    // ...and a red CI confirmed doesn't cover a change request, nor a run now pending
    let red_only = MergeAck { ci: Some("failure".into()), changes_requested: false };
    assert!(try_merge(&red_only).contains("변경 요청이 있어요"));
    let pending = r#"[{"__typename":"CheckRun","name":"build","status":"IN_PROGRESS","conclusion":""}]"#;
    fx.resp("api_graphql.out", &status3("OPEN", &tip, pending));
    assert!(try_merge(&red_only).contains("진행 중"));
    assert_eq!(fx.prs.get(&fx.wt).unwrap().checks.state, "pending");
    assert_eq!(fx.calls("pr merge"), 0);
    assert!(merge_pr(&fx.host, &fx.merges, &fx.prs, &fx.wt, "rebase", &tip, &red_only).is_err());
    // exactly what was confirmed: the merge goes
    let both = MergeAck { ci: Some("failure".into()), changes_requested: true };
    fx.resp("api_graphql.out", &status3("OPEN", &tip, RED).replace("APPROVED", "CHANGES_REQUESTED"));
    fx.resp("pr_view.out", &view("MERGED", &tip, Some(&tip)));
    merge_pr(&fx.host, &fx.merges, &fx.prs, &fx.wt, "merge", &tip, &both).unwrap();
    assert_eq!(fx.calls("pr merge"), 1);
}

#[test]
fn the_merge_check_reads_partial_answers() {
    let (fx, tip) = open_pr_fixture("partial");
    // a token without Actions read: GitHub answers the PR with errors for the workflow runs, gh exits 1
    let partial = status3("OPEN", &tip, GREEN).replacen('{', r#"{"errors":[{"type":"FORBIDDEN","message":"Resource not accessible by personal access token"}],"#, 1);
    fx.resp("api_graphql.out", &partial);
    fx.resp("api_graphql.code", "1");
    fx.resp("api_graphql.err", "gh: Resource not accessible by personal access token");
    fx.resp("pr_view.out", &view("MERGED", &tip, Some(&tip)));
    let rec = merge_pr(&fx.host, &fx.merges, &fx.prs, &fx.wt, "merge", &tip, &NO_ACK).unwrap();
    assert_eq!(rec.state, "MERGED");
    // no data at all: said in Korean, no merge
    let (fx, tip) = open_pr_fixture("noaccess");
    fx.resp("api_graphql.out", r#"{"data":null,"errors":[{"type":"FORBIDDEN","message":"Resource not accessible by integration"}]}"#);
    fx.resp("api_graphql.code", "1");
    fx.resp("api_graphql.err", "gh: Resource not accessible by integration");
    let e = merge_pr(&fx.host, &fx.merges, &fx.prs, &fx.wt, "merge", &tip, &NO_ACK).unwrap_err();
    assert!(e.contains("읽을 권한이 없어요"), "{e}");
    assert_eq!(fx.calls("pr merge"), 0);
}

#[test]
fn plan_in_pr_mode_skips_the_project_folder() {
    let fx = Fx::new("plan");
    fx.work("b.txt");
    g(&fx.root, &["switch", "-q", "-c", "side"]);
    put(&fx.root, "a.txt", "dirty project folder\n");
    put(&fx.wt, "a.txt", "dirty worker\n");
    // local mode as before: refused off the base, refused dirty
    assert!(fx.plan(MergeMode::Local).unwrap_err().contains("기준 브랜치(main)가 아니라 side"));
    let p = fx.plan(MergeMode::Pr).unwrap();
    assert_eq!((p.mode, p.worker_dirty.clone(), p.project_dirty.len()), (MergeMode::Pr, vec!["a.txt".to_string()], 0));
    assert_eq!((p.cmp_ref.as_str(), p.remote.as_deref(), p.commit_count), ("refs/remotes/origin/main", Some("origin"), 1));
    g(&fx.root, &["switch", "-q", "main"]);
    g(&fx.root, &["checkout", "--", "a.txt"]);
    let e = fx.plan(MergeMode::Local).unwrap_err();
    assert!(e.contains("커밋하지 않은 변경이 1개"), "{e}");
    g(&fx.wt, &["checkout", "--", "a.txt"]);
    let l = fx.plan(MergeMode::Local).unwrap();
    assert_eq!((l.mode, l.cmp_ref.as_str(), l.remote.as_deref()), (MergeMode::Local, "refs/heads/main", None));
}

#[test]
fn readiness_explains_what_is_missing() {
    let fx = Fx::new("ready");
    let t = &fx.host.tools;
    let check = |tools: &Tools, base: &str| github::check(tools, &fx.root, base, true);
    let ok = check(t, "main");
    assert!(ok.ok, "{}", ok.message);
    assert_eq!((ok.slug.as_str(), ok.gh.as_str(), ok.gh_version.as_deref()), ("gh.test/o/r", "ready", Some("2.102.0")));
    assert!(check(t, "dev").message.contains("원격에 기준 브랜치(dev)가 없어요"));
    // gh missing
    let none = Tools { gh: None, ..t.clone() };
    let r = check(&none, "main");
    assert!(!r.ok && r.gh == "missing" && r.message.contains("cli.github.com"), "{r:?}");
    assert_eq!(github::find_gh(&std::env::join_paths([fx.dir.join("nothing")]).unwrap()), None);
    // too old
    fx.resp(&format!("--version_.{}.out", fx.next("--version_")), "gh version 2.30.0 (2023-05-01)
");
    assert!(check(t, "main").message.contains("2.30.0"));
    // logged out (not GitHub unless gh is logged in to that host)
    let n = fx.next("auth_status");
    fx.resp(&format!("auth_status.{n}.code"), "1");
    fx.resp(&format!("auth_status.{n}.err"), "You are not logged into any GitHub hosts.");
    let r = check(t, "main");
    assert_eq!(r.gh, "loggedOut");
    assert!(r.message.contains("GitHub 저장소가 아니에요 (gh.test)") && r.message.contains("gh auth login"), "{}", r.message);
    // an old gh without --active: asked again without it
    let n = fx.next("auth_status");
    fx.resp(&format!("auth_status.{n}.code"), "1");
    fx.resp(&format!("auth_status.{n}.err"), "unknown flag: --active");
    assert!(check(t, "main").ok);
    let auth: Vec<String> = fx.argv().into_iter().filter(|l| l.starts_with("auth status")).collect();
    assert!(auth[auth.len() - 2].contains("--active") && !auth[auth.len() - 1].contains("--active"), "{auth:?}");
    // can't tell (timeout): not "logged out"
    fx.resp(&format!("auth_status.{}.sleep", fx.next("auth_status")), "8");
    let slow = Tools { t: Timeouts { auth: 1.0, ..t.t.clone() }, ..t.clone() };
    let r = check(&slow, "main");
    assert_eq!(r.gh, "unknown");
    assert!(r.message.contains("네트워크"), "{}", r.message);
    // offline: gh can't validate the token and says it's invalid; a probe tells it's the network
    let n = fx.next("auth_status");
    fx.resp(&format!("auth_status.{n}.code"), "1");
    fx.resp(&format!("auth_status.{n}.err"), "github.com\n  X Failed to log in to github.com account me (keyring)\n  - Active account: true\n  - The token in keyring is invalid.\n  - To re-authenticate, run: gh auth login -h github.com");
    let p = fx.next("api_rate_limit");
    fx.resp(&format!("api_rate_limit.{p}.code"), "1");
    fx.resp(&format!("api_rate_limit.{p}.err"), "error connecting to api.gh.test: dial tcp: lookup api.gh.test: no such host");
    let r = check(t, "main");
    assert_eq!(r.gh, "unknown");
    assert!(r.message.contains("네트워크") && r.transient, "{}", r.message);
    // online, the token refused: expired, not "not logged in"
    let n = fx.next("auth_status");
    fx.resp(&format!("auth_status.{n}.code"), "1");
    fx.resp(&format!("auth_status.{n}.err"), "X Failed to log in to gh.test account me (keyring)\n  - The token in keyring is invalid.");
    let p = fx.next("api_rate_limit");
    fx.resp(&format!("api_rate_limit.{p}.code"), "1");
    fx.resp(&format!("api_rate_limit.{p}.err"), "HTTP 401: Bad credentials (https://api.gh.test/rate_limit)");
    let r = check(t, "main");
    assert_eq!(r.gh, "loggedOut");
    assert!(r.message.contains("만료") && !r.transient, "{}", r.message);
    assert!(fx.argv().iter().any(|l| l == "api rate_limit --hostname gh.test"), "{:?}", fx.argv());
    // read-only permission
    fx.resp("repo_view.out", r#"{"nameWithOwner":"o/r","viewerPermission":"READ","mergeCommitAllowed":true,"squashMergeAllowed":true,"isArchived":false}"#);
    assert!(check(t, "main").message.contains("푸시 권한이 없어요"));
    // a local-path remote, then none
    g(&fx.root, &["remote", "set-url", "origin", &fx.bare]);
    assert!(check(t, "main").message.contains("로컬 경로"));
    g(&fx.root, &["remote", "remove", "origin"]);
    assert!(check(t, "main").message.contains("원격 저장소가 없어요"));
    // no auth commands but `auth status`, ever
    assert!(fx.argv().iter().all(|l| !l.starts_with("auth") || l.starts_with("auth status")));
}

#[test]
fn switching_the_mode() {
    let fx = Fx::new("mode");
    fx.work("b.txt");
    let home = fx.host.home();
    // not ready: refused, the mode stays local
    let host = TestHost { tools: Tools { gh: None, ..fx.host.tools.clone() }, home: home.clone(), now: AtomicU64::new(0), events: Mutex::default(), notes: Mutex::default(), busy: AtomicBool::new(false), pr_busy: AtomicBool::new(false) };
    assert!(set_merge_mode(&host, &fx.merges, &fx.root, MergeMode::Pr).unwrap_err().contains("gh(GitHub CLI)가 없어요"));
    assert_eq!(project::merge_mode(&home, &fx.root), MergeMode::Local);
    // ready: switched, pending requests withdrawn, the conductor told
    let id = fx.request(None);
    set_merge_mode(&fx.host, &fx.merges, &fx.root, MergeMode::Pr).unwrap();
    assert_eq!(project::merge_mode(&home, &fx.root), MergeMode::Pr);
    assert!(fx.merges.get(id).is_none());
    assert_eq!(fx.host.events("maestro://merge-done")[0]["outcome"], "withdrawn");
    assert!(fx.host.notes().iter().any(|(k, n)| k == "mode" && n.contains("PR로 바뀌었어요")));
    // not while a PR action runs
    let id = fx.request(None);
    fx.merges.update(id, |r| r.pr = Some(PrProgress { step: Step::Pushing, ..Default::default() }));
    assert!(set_merge_mode(&fx.host, &fx.merges, &fx.root, MergeMode::Local).unwrap_err().contains("PR 작업 중"));
    fx.merges.update(id, |r| r.pr = None);
    let lock = fx.merges.lock_pr(&fx.wt).unwrap();
    assert!(set_merge_mode(&fx.host, &fx.merges, &fx.root, MergeMode::Local).is_err());
    drop(lock);
    set_merge_mode(&fx.host, &fx.merges, &fx.root, MergeMode::Local).unwrap();
    assert!(fx.host.notes().iter().any(|(_, n)| n.contains("로컬로 바뀌었어요")));
}

#[test]
fn rejecting_after_a_push_says_the_branch_stays() {
    let fx = Fx::new("reject");
    let tip = fx.work("b.txt");
    let id = fx.request(None);
    fx.merges.update(id, |r| r.pr = Some(PrProgress { step: Step::Unconfirmed, push_sha: Some(tip.clone()), ..Default::default() }));
    let r = reject(&fx.host, &fx.merges, &fx.prs, id, Some("아직".into())).unwrap();
    assert_eq!(r.outcome, "rejected");
    let note = &fx.host.notes()[0].1;
    assert!(note.contains("사용자 사유: 아직") && note.contains("원격 브랜치(wt)는 남아 있어요"), "{note}");
}

#[test]
fn ci_of_another_head_never_lets_a_merge_through() {
    let (fx, tip) = open_pr_fixture("staleci");
    // the card shows green CI for `tip`; GitHub has a new head whose checks aren't in yet
    let new = "b".repeat(40);
    let pending = r#"[{"__typename":"CheckRun","name":"build","status":"IN_PROGRESS","conclusion":""}]"#;
    fx.resp("api_graphql.out", &status3("OPEN", &new, pending));
    let e = merge_pr(&fx.host, &fx.merges, &fx.prs, &fx.wt, "merge", &tip, &NO_ACK).unwrap_err();
    assert!(e.contains("새 커밋이 생겼어요"), "{e}");
    let rec = fx.prs.get(&fx.wt).unwrap();
    assert_eq!((rec.head_sha.as_str(), rec.checks.state.as_str(), rec.checks.head_sha.as_str()), (new.as_str(), "pending", new.as_str()));
    // the second click, on the new head: CI is running there
    assert!(merge_pr(&fx.host, &fx.merges, &fx.prs, &fx.wt, "merge", &new, &NO_ACK).unwrap_err().contains("CI가 아직 진행 중"));
    // a cached green for another head is not this head's: unknown, no merge (GitHub's checks still of the old commit)
    fx.resp("api_graphql.out", &status_at("OPEN", &new, &tip, GREEN));
    fx.prs.update(&fx.wt, |r| r.checks = github::Checks { state: "success".into(), head_sha: tip.clone(), ..Default::default() });
    let e = merge_pr(&fx.host, &fx.merges, &fx.prs, &fx.wt, "merge", &new, &NO_ACK).unwrap_err();
    assert!(e.contains("CI 결과를 아직 몰라요"), "{e}");
    assert_eq!(fx.calls("pr merge"), 0);
}

#[test]
fn the_merge_click_and_the_poller_tell_the_conductor_once() {
    let (fx, tip) = open_pr_fixture("once");
    // a poll round started before the click: its copy says OPEN
    let stale = fx.prs.get(&fx.wt).unwrap();
    let merged = squash_on_remote(&fx, &tip);
    fx.resp("api_graphql.out", &status3("OPEN", &tip, GREEN));
    fx.resp("pr_view.1.out", &view("MERGED", &tip, Some(&merged)));
    merge_pr(&fx.host, &fx.merges, &fx.prs, &fx.wt, "squash", &tip, &NO_ACK).unwrap();
    let live = github::PrLive { number: 3, state: "MERGED".into(), head_ref_oid: tip.clone(), merge_commit: Some(merged.clone()), ..Default::default() };
    apply(&fx.host, &fx.prs, &stale, &live, fx.host.now());
    let rec = fx.prs.get(&fx.wt).unwrap();
    assert_eq!(rec.merged.as_ref().unwrap().method, "squash", "the click's method stays");
    let notes: Vec<_> = fx.host.notes().into_iter().filter(|(k, _)| k.starts_with("pr:")).collect();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].1.contains("빨리감기 했어요") && notes[0].1.contains("빌드·테스트로 확인하세요"));
    assert!(!fx.prs.get(&fx.wt).unwrap().note_owed);
    // a poll asked before the click and answered after it: OPEN never goes back over MERGED, and no second note
    let open = github::PrLive { number: 3, state: "OPEN".into(), head_ref_oid: tip.clone(), ..Default::default() };
    apply(&fx.host, &fx.prs, &stale, &open, fx.host.now());
    let rec = fx.prs.get(&fx.wt).unwrap();
    assert_eq!((rec.state.as_str(), rec.merged.as_ref().unwrap().method.as_str()), ("MERGED", "squash"));
    apply(&fx.host, &fx.prs, &rec, &live, fx.host.now());
    assert_eq!(fx.host.notes().into_iter().filter(|(k, _)| k.starts_with("pr:")).count(), 1);
    // the record moved on with the click: a round started before it drops its answer
    assert!(rec.checked_at >= stale.checked_at);
    // while a PR action runs for the worker, the poller leaves its PR alone
    let (fx, _) = open_pr_fixture("busy");
    fx.resp("api_graphql.out", &graphql_body(1, 4000, "OPEN", Value::Null).replace("\"number\":1", "\"number\":3"));
    fx.host.pr_busy.store(true, Ordering::SeqCst);
    let mut s = Sched::default();
    poll_once(&fx.host, &fx.prs, &mut s);
    assert_eq!(fx.calls("api graphql"), 0);
    fx.host.pr_busy.store(false, Ordering::SeqCst);
    fx.host.advance(11_000);
    poll_once(&fx.host, &fx.prs, &mut s);
    assert_eq!(fx.calls("api graphql"), 1);
}

#[test]
fn a_pr_out_of_the_queue_can_be_merged_again() {
    let (fx, tip) = open_pr_fixture("dequeued");
    let now = fx.host.now();
    fx.prs.update(&fx.wt, |r| {
        r.queued = true;
        r.merge_attempt = Some(MergeAttempt { method: "squash".into(), seen_head: tip.clone(), at: now });
    });
    let body = graphql_body(1, 4000, "OPEN", Value::Null).replace("\"number\":1", "\"number\":3").replace("\"h1\"", &format!("\"{tip}\""));
    fx.resp("api_graphql.out", &body);
    // just clicked: still waiting
    let mut s = Sched::default();
    poll_once(&fx.host, &fx.prs, &mut s);
    let rec = fx.prs.get(&fx.wt).unwrap();
    assert!(!rec.queued && rec.merge_attempt.is_some());
    // a minute on, still open and not set to merge: over, the button works again
    fx.host.advance(61_000);
    poll_once(&fx.host, &fx.prs, &mut s);
    let rec = fx.prs.get(&fx.wt).unwrap();
    assert!(!rec.queued && rec.merge_attempt.is_none());
    assert!(rec.error.unwrap().contains("빠졌어요"));
}

#[test]
fn old_merge_errors_go_once_they_are_no_longer_true() {
    let (fx, tip) = open_pr_fixture("staleerr");
    let open = graphql_body(1, 4000, "OPEN", Value::Null).replace("\"number\":1", "\"number\":3").replace("\"h1\"", &format!("\"{tip}\""));
    let poll = || {
        let mut s = Sched::default();
        poll_once(&fx.host, &fx.prs, &mut s);
        fx.host.advance(1_000);
        fx.prs.get(&fx.wt).unwrap()
    };
    // an open PR read fine with no click under way: a refused merge's words go
    fx.resp("api_graphql.out", &open);
    fx.prs.update(&fx.wt, |r| r.error = Some("GitHub가 병합을 거부했어요: x".into()));
    assert_eq!(poll().error, None);
    // a queued click keeps its words while it waits...
    let now = fx.host.now();
    fx.prs.update(&fx.wt, |r| {
        r.error = Some("병합 대기열/자동 병합에 들어갔어요".into());
        r.merge_attempt = Some(MergeAttempt { method: "merge".into(), seen_head: tip.clone(), at: now });
    });
    assert!(poll().error.is_some());
    // ...and loses them once merged
    fx.resp("api_graphql.out", &open.replace("\"OPEN\"", "\"MERGED\""));
    let rec = poll();
    assert_eq!((rec.state.as_str(), rec.error), ("MERGED", None));
    // closed: the same
    let (fx, tip) = open_pr_fixture("staleerr2");
    fx.resp("api_graphql.out", &graphql_body(1, 4000, "CLOSED", Value::Null).replace("\"number\":1", "\"number\":3").replace("\"h1\"", &format!("\"{tip}\"")));
    fx.prs.update(&fx.wt, |r| r.error = Some("병합이 끝나지 않았어요".into()));
    let mut s = Sched::default();
    poll_once(&fx.host, &fx.prs, &mut s);
    let rec = fx.prs.get(&fx.wt).unwrap();
    assert_eq!((rec.state.as_str(), rec.error), ("CLOSED", None));
}

#[test]
fn a_squash_seen_only_on_github_blocks_the_next_push() {
    let (fx, tip) = open_pr_fixture("ghsquash");
    // squashed on GitHub, seen by the poller: the project folder never fetched the commit
    let squashed = squash_on_remote(&fx, &tip);
    fx.prs.update(&fx.wt, |r| {
        r.state = "MERGED".into();
        r.merged = Some(Merged { method: "github".into(), commit: Some(squashed.clone()), head: tip.clone(), at: 1 });
        r.merged_heads = vec![tip.clone()];
    });
    assert!(!has_object(&fx.host.tools, Path::new(&fx.root), &squashed));
    // the worker carries on from the old branch: the plan can't tell yet
    fx.work("c.txt");
    let id = fx.request(None);
    // the approval fetches the base, then sees it
    let e = fx.approve(id, opts("t")).unwrap_err();
    assert!(e.contains("PR #3이 squash로 합쳐진 브랜치예요"), "{e}");
    assert_eq!(fx.bare_tip("wt").as_deref(), Some(tip.as_str()), "nothing pushed");
    assert_eq!(fx.calls("pr create"), 0);
    // now the plan knows too
    assert!(fx.plan(MergeMode::Pr).unwrap_err().contains("squash로 합쳐진"));
}

// ---------- polling ----------

fn graphql_body(n: usize, remaining: u64, state: &str, checks: Value) -> String {
    let mut prs = serde_json::Map::new();
    for j in 0..n {
        prs.insert(
            format!("p{j}"),
            json!({"number": j + 1, "url": format!("https://gh.test/o/r/pull/{}", j + 1), "state": state, "isDraft": false, "headRefName": "wt", "headRefOid": "h1", "baseRefName": "main",
                "mergeable": "MERGEABLE", "mergeStateStatus": "CLEAN", "reviewDecision": null, "mergedAt": null, "mergeCommit": null, "autoMergeRequest": null,
                "commits": {"nodes": [{"commit": {"oid": "h1", "statusCheckRollup": checks.clone()}}]}}),
        );
    }
    json!({"data": {"rateLimit": {"limit": 5000, "remaining": remaining, "resetAt": "2030-01-01T00:00:00Z", "cost": 1}, "r0": prs}}).to_string()
}

fn seed(prs: &Prs, n: usize, root: &str) {
    for i in 0..n {
        prs.upsert(PrRecord { root: root.into(), maestro_path: root.into(), worker_path: format!("C:/w/{i}"), worker_name: format!("w{i}"), branch: "wt".into(), base: "main".into(), remote: "origin".into(), host: "gh.test".into(), owner: "o".into(), repo: "r".into(), number: i as u64 + 1, state: "OPEN".into(), ..Default::default() });
    }
}

#[test]
fn polls_in_batches_and_notes_ci_failures_once() {
    let fx = Fx::new("poll");
    seed(&fx.prs, 30, "C:/p/batch");
    let failing = json!({"state": "FAILURE", "contexts": {"nodes": [{"__typename": "CheckRun", "name": "build", "status": "COMPLETED", "conclusion": "FAILURE", "databaseId": 11, "checkSuite": {"workflowRun": {"databaseId": 22}}}]}});
    fx.resp("api_graphql.out", &graphql_body(25, 4000, "OPEN", failing));
    let mut s = Sched::default();
    let next = poll_once(&fx.host, &fx.prs, &mut s);
    // 30 PRs on one host: two calls (25 + 5)
    assert_eq!(fx.calls("api graphql"), 2);
    let r = fx.prs.get("C:/w/0").unwrap();
    assert_eq!((r.checks.state.as_str(), r.checks.failed_jobs[0].job_id), ("failure", Some(11)));
    let ci: Vec<_> = fx.host.notes().into_iter().filter(|(_, n)| n.contains("CI 실패")).collect();
    assert_eq!(ci.len(), 30);
    assert!(ci[0].0.starts_with("pr:") && ci[0].1.contains("작업 build"));
    // not due again yet; checks done, mergeability known: the slow interval
    assert!(next >= 100_000, "{next}");
    poll_once(&fx.host, &fx.prs, &mut s);
    assert_eq!(fx.calls("api graphql"), 2);
    // due again: the same failure on the same head isn't noted twice
    fx.host.advance(125_000);
    poll_once(&fx.host, &fx.prs, &mut s);
    assert_eq!(fx.calls("api graphql"), 4);
    assert_eq!(fx.host.notes().iter().filter(|(_, n)| n.contains("CI 실패")).count(), 30);
}

#[test]
fn rate_limits_pause_the_host() {
    let fx = Fx::new("ratelimit");
    seed(&fx.prs, 2, "C:/p/ratelimit");
    // the budget is nearly spent: polls stop until the reset
    fx.resp("api_graphql.1.out", &graphql_body(2, 100, "OPEN", Value::Null));
    let mut s = Sched::default();
    poll_once(&fx.host, &fx.prs, &mut s);
    let (until, msg) = s.paused.get("gh.test").cloned().unwrap();
    assert_eq!(until, github::parse_iso_ms("2030-01-01T00:00:00Z").unwrap() + 1_000);
    assert!(msg.contains("한도에 가까워"));
    assert_eq!(fx.host.events("maestro://pr-poll")[0]["pausedUntil"], json!(until));
    fx.host.advance(300_000);
    poll_once(&fx.host, &fx.prs, &mut s);
    assert_eq!(fx.calls("api graphql"), 1);
    // "다시 확인" gets through once
    refresh(Some("C:/p/ratelimit"));
    fx.resp("api_graphql.2.code", "1");
    fx.resp("api_graphql.2.err", "GraphQL: API rate limit exceeded for user ID 1.");
    fx.resp("api_rate_limit.out", r#"{"resources":{"graphql":{"limit":5000,"remaining":0,"reset":1900000000}}}"#);
    poll_once(&fx.host, &fx.prs, &mut s);
    assert_eq!(fx.calls("api graphql"), 2);
    assert_eq!(fx.calls("api rate_limit"), 1);
    assert_eq!(s.paused.get("gh.test").unwrap().0, 1_900_000_000_000 + 1_000);
    // over: said so even though the next poll fails (no answer)
    fx.host.advance((1_900_000_000_000u64 + 1_000).saturating_sub(fx.host.now()));
    poll_once(&fx.host, &fx.prs, &mut s);
    assert_eq!(fx.calls("api graphql"), 3);
    assert!(s.paused.is_empty());
    assert_eq!(fx.host.events("maestro://pr-poll").last().unwrap()["pausedUntil"], json!(0));
}

#[test]
fn schedule_math() {
    let mut r = PrRecord { mergeable: "MERGEABLE".into(), ..Default::default() };
    r.checks.state = "success".into();
    assert_eq!(interval(&r), 120_000);
    r.checks.state = "pending".into();
    assert_eq!(interval(&r), 20_000);
    r.checks.state = "success".into();
    r.mergeable = "UNKNOWN".into();
    assert_eq!(interval(&r), 20_000);
    assert_eq!((align(0), align(1), align(5_000), align(5_001)), (0, 5_000, 5_000, 10_000));
    assert_eq!((backoff(20_000, 1), backoff(20_000, 2), backoff(20_000, 3), backoff(20_000, 10)), (20_000, 40_000, 80_000, 300_000));
}

#[test]
fn a_failing_poll_backs_off() {
    let fx = Fx::new("backoff");
    seed(&fx.prs, 1, "C:/p/backoff");
    fx.resp("api_graphql.code", "1");
    fx.resp("api_graphql.err", "HTTP 502: Bad Gateway");
    let mut s = Sched::default();
    let now = fx.host.now();
    poll_once(&fx.host, &fx.prs, &mut s);
    assert_eq!(s.errors["gh.test"], 1);
    assert_eq!(s.due["c:/w/0"], align(now + 20_000));
    fx.host.advance(25_000);
    poll_once(&fx.host, &fx.prs, &mut s);
    assert_eq!(s.errors["gh.test"], 2);
    assert_eq!(s.due["c:/w/0"], align(now + 25_000 + 40_000));
}

/// A job's log as `actions/jobs/{id}/logs` serves it: a BOM, timestamps, `##[group]Run …` steps, post-job cleanup.
fn job_log_sample() -> String {
    let lines = [
        "Current runner version: '2.320.0'",
        "##[group]Run actions/checkout@v4",
        "with:",
        "##[endgroup]",
        "##[group]Run cargo test",
        "cargo test",
        "##[endgroup]",
        "test a ... ok",
        "a\tb\tc",
        "error: test failed",
        "##[error]Process completed with exit code 101.",
        "Post job cleanup.",
        "Cleaning up orphan processes",
    ];
    let body: String = lines.iter().map(|l| format!("2026-11-12T10:00:00.1234567Z {l}\r\n")).collect();
    format!("\u{feff}{body}")
}

fn assert_job_log(t: &str) {
    assert!(t.contains("── Run cargo test ──") && t.contains("error: test failed") && t.contains("##[error]Process completed") && t.contains("a\tb\tc"), "{t}");
    for bad in ["Post job cleanup", "orphan", "2026-11-12T", "##[group]", "##[endgroup]", "\u{feff}"] {
        assert!(!t.contains(bad), "{bad} in {t}");
    }
}

#[test]
fn a_failed_log_reads_the_job_log_first_then_falls_back() {
    let fx = Fx::new("joblog");
    let job = |name: &str, id: u64, run: Option<u64>| github::FailedJob { name: name.into(), job_id: Some(id), run_id: run };
    let rec = |worker: &str, owner: &str, jobs: Vec<github::FailedJob>| PrRecord {
        root: fx.root.clone(),
        maestro_path: fx.root.clone(),
        worker_path: worker.into(),
        worker_name: "api".into(),
        branch: "wt".into(),
        base: "main".into(),
        remote: "origin".into(),
        host: "gh.test".into(),
        owner: owner.into(),
        repo: "r".into(),
        number: 7,
        state: "OPEN".into(),
        head_sha: "h1".into(),
        checks: github::Checks { state: "failure".into(), failed: jobs.len() as u32, head_sha: "h1".into(), failed_jobs: jobs, ..Default::default() },
        ..Default::default()
    };
    fx.prs.upsert(rec("C:/w/joblog", "o", vec![job("build", 11, Some(22)), job("lint", 12, Some(22)), job("ext", 13, None)]));
    // build: the REST log; lint: REST refused, then `run view --job`; ext: a third-party check, never REST
    fx.resp("api_repos-o-r-actions-jobs-11-logs.out", &job_log_sample());
    fx.resp("api_repos-o-r-actions-jobs-12-logs.code", "1");
    fx.resp("api_repos-o-r-actions-jobs-12-logs.err", "gh: Not Found (HTTP 404)");
    fx.resp("run_view.1.out", "lint\tRun lint\t2026-11-12T10:00:00.0000000Z error: lint broke\n");
    let t = failed_log(&fx.host, &fx.prs, "C:/w/joblog", None).unwrap();
    assert_job_log(&t);
    assert!(t.contains("error: lint broke") && t.contains("실패 로그를 받지 못했어요"), "{t}");
    let argv = fx.argv();
    assert!(argv.iter().any(|l| l == "api repos/o/r/actions/jobs/11/logs --hostname gh.test"), "{argv:?}");
    assert!(argv.iter().any(|l| l == "api repos/o/r/actions/jobs/12/logs --hostname gh.test"), "{argv:?}");
    assert!(!argv.iter().any(|l| l.contains("actions/jobs/13")), "{argv:?}");
    assert_eq!(fx.calls("run view --job 11"), 0);
    assert_eq!(fx.calls("run view --job 12 -R gh.test/o/r --log-failed"), 1);
    // an owner a REST path can't carry: straight to `run view`; nothing at all: an error
    fx.resp("run_view.code", "1");
    fx.prs.upsert(rec("C:/w/odd", "o?x", vec![job("build", 11, Some(22))]));
    let e = failed_log(&fx.host, &fx.prs, "C:/w/odd", Some(11)).unwrap_err();
    assert!(e.starts_with("실패 로그를 받지 못했어요"), "{e}");
    assert_eq!(fx.calls("api repos/"), 2);
    assert_eq!(fx.calls("run view --job 11"), 1);
    let e = failed_log(&fx.host, &fx.prs, "C:/w/joblog", Some(13)).unwrap_err();
    assert!(e.starts_with("실패 로그를 받지 못했어요"), "{e}");
}

// ---------- pure parts ----------

#[test]
fn store_keeps_merged_heads_across_prs_and_restarts() {
    let dir = std::env::temp_dir().join(format!("maestro-prs-store-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let prs = Prs::at(dir.join("pull-requests.json"));
    prs.upsert(PrRecord { worker_path: "C:/w/a".into(), branch: "wt".into(), number: 1, merged_heads: vec!["h1".into()], merged: Some(Merged { method: "squash".into(), ..Default::default() }), ..Default::default() });
    prs.upsert(PrRecord { worker_path: "c:\\W\\A".into(), branch: "wt".into(), number: 2, state: "OPEN".into(), ..Default::default() });
    let again = Prs::at(dir.join("pull-requests.json"));
    let r = again.get("C:/w/a").unwrap();
    assert_eq!((r.number, r.merged_heads.clone()), (2, vec!["h1".to_string()]));
    assert_eq!(again.merged_heads("C:/w/a", "wt"), ["h1"]);
    assert!(again.merged_heads("C:/w/a", "other").is_empty());
    assert!(again.remove_worker("C:/W/A").is_some() && again.list().is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn sanitizer_removes_local_paths_and_pings() {
    let locals = vec!["C:/Users/me/proj".to_string(), "C:\\Users\\me\\.maestro".to_string()];
    let body = "경로 C:/Users/me/proj/src/a.rs 와 c:\\users\\me\\.maestro\\x.json, D:\\other\\y.txt, /home/me/z, \\\\srv\\share\\f and file:///Users/me/q\n\
@alice 확인 부탁, email a@b.c\n```text\n@bob in fence\n```\n![shot](C:/tmp/s.png) \u{7}bell";
    let out = sanitize_body(body, &locals);
    for bad in ["C:/Users/me", "users\\me", "D:\\other", "/home/me", "\\\\srv", "file://", "\u{7}", "![", "s.png"] {
        assert!(!out.to_lowercase().contains(&bad.to_lowercase()), "{bad} in {out}");
    }
    assert!(out.contains("`@alice` 확인") && out.contains("a@b.c") && out.contains("@bob in fence") && !out.contains("`@bob`"), "{out}");
    assert!(out.contains("shot"));
    assert!(sanitize_body(&"가".repeat(70_000), &[]).chars().count() <= 60_002);
    assert_eq!(sanitize_title("  첫 줄 C:/Users/me/x\n둘째 줄", &locals), "첫 줄 <로컬 경로>");
    assert_eq!(sanitize_title(&"t".repeat(300), &[]).len(), 200);
    // URLs stay
    assert!(sanitize_body("see https://github.com/home/x", &[]).contains("https://github.com/home/x"));
    // Git Bash, WSL and Cygwin spellings, known or not, also inside the conductor's text fence
    let known = local_paths("C:/Users/me/proj", "C:/Users/me/wt", Path::new("C:/Users/me"));
    for p in ["/c/Users/me/proj/x", "/mnt/c/Users/me/x", "/cygdrive/c/Users/me/wt/y"] {
        assert!(known.iter().any(|k| p.to_lowercase().starts_with(&k.to_lowercase())), "{p} not in {known:?}");
        let out = sanitize_body(&format!("```text\ncd {p} && cargo test\n```"), &[]);
        assert!(!out.contains("Users/me") && out.contains("cd <로컬 경로> && cargo test"), "{out}");
        assert!(!sanitize_body(&format!("x {p}"), &known).contains("me"), "{p}");
    }
    assert_eq!(sanitize_title("cd /c/Users/me/proj", &known), "cd <로컬 경로>");
    assert!(sanitize_body("a/b/c and s/x/y", &[]).contains("a/b/c and s/x/y"));
}

#[test]
fn draft_uses_the_template_and_lists_what_was_not_checked() {
    let fx = Fx::new("draft");
    std::fs::create_dir_all(Path::new(&fx.wt).join(".github")).unwrap();
    put(&fx.wt, ".github/PULL_REQUEST_TEMPLATE.md", "## 변경 이유\n\n");
    commit_all(&fx.wt, "template");
    fx.work("b.txt");
    put(&fx.wt, "loose.txt", "x");
    let id = fx.request(None);
    let d = fx.merges.get(id).unwrap().pr_draft.unwrap();
    assert_eq!(d.title, "api: template 외 1개");
    assert!(d.body.starts_with("## 변경 이유") && d.body.contains("## Maestro 요약"), "{}", d.body);
    assert!(d.body.contains("지휘자가 보고한 확인 내용이 없어요") && d.body.contains("CI가 아직 돌지 않았어요") && d.body.contains("추적하지 않는 파일 1개"), "{}", d.body);
    assert!(d.draft);
    assert!(!d.body.contains(&fx.wt));
}

#[test]
fn log_trim_keeps_errors_and_the_tail() {
    let mut raw = String::new();
    for i in 0..300 {
        let line = if i == 40 { "\u{1b}[31merror: the thing broke\u{1b}[0m".to_string() } else { format!("line {i}\u{200b}") };
        raw.push_str(&format!("build\tRun tests\t2026-11-11T10:00:00.1234567Z {line}\r\n"));
    }
    let out = trim_log(&raw);
    assert!(out.starts_with("── Run tests ──") || out.contains("── Run tests ──"), "{out}");
    assert!(out.contains("error: the thing broke") && !out.contains("\u{1b}") && !out.contains("\u{200b}") && !out.contains("2026-11-11T"));
    assert!(out.contains("line 38") && out.contains("line 42") && !out.contains("line 100\n"));
    assert!(out.contains("… 앞쪽 오류 …") && out.contains("line 299"));
    assert!(!out.contains('\r') && !out.contains("build\t"));
    let big: String = (0..2000).map(|i| format!("x\ty\t{}\n", "z".repeat(50) + &i.to_string())).collect();
    assert!(trim_log(&big).len() <= 16 << 10);
}

#[test]
fn job_log_trim_cuts_after_the_failed_step() {
    let t = trim_job_log(&job_log_sample());
    assert_job_log(&t);
    assert!(t.contains("── Run actions/checkout@v4 ──") && t.contains("Current runner version"), "{t}");
    // no `##[error]`: nothing cut
    let ok = trim_job_log("2026-11-12T10:00:00.1234567Z ##[group]Run x\n2026-11-12T10:00:00.1234567Z done\n2026-11-12T10:00:00.1234567Z Post job cleanup.\n");
    assert!(ok.ends_with("Post job cleanup.\n"), "{ok}");
    // a group inside a step is a line of that step, not a step
    let nested = trim_job_log("##[group]Run actions/checkout@v4\n##[group]Operating System\nUbuntu\n##[endgroup]\n##[error]boom\n");
    assert!(nested.contains("── Run actions/checkout@v4 ──\nOperating System\nUbuntu\n##[error]boom") && !nested.contains("── Operating System"), "{nested}");
    // a later step that also failed stays; the steps after it go
    let two = trim_job_log("##[group]Run a\n##[error]first\n##[group]Run b\n##[error]second\n##[group]Run c\nlater\n");
    assert!(two.contains("first") && two.contains("second") && !two.contains("later"), "{two}");
    let big: String = (0..2000).map(|i| format!("2026-11-12T10:00:00.1234567Z {}\n", "z".repeat(50) + &i.to_string())).collect();
    assert!(trim_job_log(&big).len() <= 16 << 10);
}

#[test]
fn notes_and_urls() {
    let mut r = PrRecord { worker_name: "api".into(), branch: "wt".into(), base: "main".into(), number: 7, url: "https://github.com/o/r/pull/7".into(), host: "github.com".into(), owner: "o".into(), repo: "r".into(), ..Default::default() };
    assert!(note_opened(&r, 2, "pr").starts_with("[maestro] PR 생성: 워커 'api' (wt → main) PR #7 https://github.com/o/r/pull/7. 커밋 2개."));
    r.is_draft = true;
    assert!(note_opened(&r, 1, "recovered").starts_with("[maestro] 기존 PR #7을 찾아 이어 썼어요:") && note_opened(&r, 1, "pr").contains(", 초안."));
    assert!(note_merged(&r, "merge", Some("abcdef1234"), "빨리감기 했어요", true).contains("(merge, 커밋 abcdef1)") && !note_merged(&r, "merge", None, "x", true).contains("squash라"));
    // only a fast-forwarded folder has the merge to build and test
    assert!(note_merged(&r, "merge", None, "빨리감기 했어요 (abc)", true).contains("프로젝트 폴더에서 빌드·테스트로 확인하세요"));
    let left = note_merged(&r, "merge", None, "그대로예요(커밋하지 않은 변경이 있어요)", false);
    assert!(!left.contains("프로젝트 폴더에서 빌드·테스트로 확인하세요") && left.contains("아직 이전 기준이에요") && left.contains("기준 브랜치 받기"), "{left}");
    assert!(note_closed(&r).contains("병합 없이 닫혔어요") && note_github_merged(&r, Some("1234567890")).contains("기준 브랜치 받기"));
    assert!(valid_pr_url(&r));
    // a pushed branch is never rewritten: the conductor hears it with the PR and with a CI failure
    assert!(note_opened(&r, 1, "pr").contains("rebase") && note_ci_failed(&r).contains("rebase") && note_ci_failed(&r).contains("강제 푸시"));
    // the conductor hears CI failures, merges and closes; reviews are not noted, and the note says so
    let o = note_opened(&r, 1, "pr");
    assert!(o.contains("CI 실패와 병합·닫힘은 [maestro] 메모로 알려요 (리뷰는 알리지 않아요)") && !o.contains("CI·리뷰 결과"), "{o}");
    for bad in ["https://evil.com/o/r/pull/7", "https://github.com/o/r/pull/7?x=1", "javascript:alert(1)", "https://github.com/o/r/pull/"] {
        r.url = bad.into();
        assert!(!valid_pr_url(&r), "{bad}");
    }
    let push = push_args("origin", "abc", "wt");
    assert_eq!(push.last().unwrap(), "abc:refs/heads/wt");
    for bad in ["-f", "--force", "--force-with-lease", "--mirror", "--delete", "--tags", "--no-verify", "-u", "--set-upstream"] {
        assert!(!push.iter().any(|a| a == bad), "{bad}");
    }
    assert!(!push.iter().any(|a| a.starts_with('+')));
}

#[test]
fn a_merge_note_owed_when_the_app_went_down_is_sent_on_start() {
    let (fx, tip) = open_pr_fixture("owed");
    let rec = fx.prs.get(&fx.wt).unwrap();
    // recorded merged by the click, then the app went down during the folder's fast-forward
    let (r, first) = mark_merged(&fx.prs, &rec, "merge", Some("abcdef1234".into()), &tip, fx.host.now()).unwrap();
    assert!(first && r.note_owed && r.merge_attempt.is_none());
    let prs = Prs::at(fx.dir.join("pull-requests.json"));
    resume(&fx.host, &fx.merges, &prs);
    let notes: Vec<_> = fx.host.notes().into_iter().filter(|(k, _)| k.starts_with("pr:")).collect();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].1.contains("PR #3 병합 완료 (merge") && notes[0].1.contains("아직 이전 기준"), "{}", notes[0].1);
    assert!(!prs.get(&fx.wt).unwrap().note_owed);
    resume(&fx.host, &fx.merges, &prs);
    assert_eq!(fx.host.notes().into_iter().filter(|(k, _)| k.starts_with("pr:")).count(), 1);
    // merged outside the app: no note owed (the poller's own note goes out with it)
    let (fx, tip) = open_pr_fixture("owed2");
    let rec = fx.prs.get(&fx.wt).unwrap();
    assert!(!mark_merged(&fx.prs, &rec, "github", None, &tip, 1).unwrap().0.note_owed);
}

#[test]
fn a_merge_refusal_names_a_moved_head_only_for_http_409() {
    let o = |err: &str| exec::Out { code: Some(1), stderr: err.into(), ..Default::default() };
    let moved = "새 커밋이 생겼어요. 다시 확인해 주세요";
    assert_eq!(merge_refusal(&o("GraphQL: Head branch was modified. Review and try the merge again. (mergePullRequest)")), moved);
    assert_eq!(merge_refusal(&o("HTTP 409: Head branch was modified")), moved);
    assert_eq!(merge_refusal(&o("failed: HTTP 409 (https://api.github.com/graphql)")), moved);
    // a PR number with 409 in it is not a moved head
    for n in ["409", "1409", "4090"] {
        let e = merge_refusal(&o(&format!("X Pull request o/r#{n} is not mergeable: the base branch policy prohibits the merge.")));
        assert!(e.starts_with("GitHub가 병합을 거부했어요") && e.contains(&format!("#{n}")), "{e}");
    }
}

#[test]
fn push_failures_say_what_git_said() {
    let o = |code: Option<i32>, out: &str, err: &str| exec::Out { code, stdout: out.into(), stderr: err.into(), ..Default::default() };
    let sha = format!("{}403{}", "a".repeat(20), "b".repeat(17));
    let line = |st: &str| format!("To https://gh.test/o/r.git\n!\t{sha}:refs/heads/wt\t{st}\nDone\n");
    assert_eq!(push_failure(&o(Some(1), &line("[rejected] (non-fast-forward)"), "error: failed to push some refs")), DIVERGED);
    assert_eq!(push_failure(&o(Some(1), &line("[rejected] (fetch first)"), "")), DIVERGED);
    // push protection, rulesets, pre-receive hooks: the remote's reason, not "diverged"
    let e = push_failure(&o(Some(1), &line("[remote rejected] (push declined due to repository rule violations)"), "remote: error: GH013: Repository rule violations found\nremote: - Push cannot contain secrets\nerror: failed to push some refs to 'https://gh.test/o/r.git'"));
    assert!(e.contains("원격이 푸시를 거부했어요 (push declined due to repository rule violations)") && e.contains("Push cannot contain secrets") && !e.contains("덮어쓰지"), "{e}");
    // a SHA with 403 in it is not a credential problem
    let e = push_failure(&o(Some(1), &line("[remote rejected] (pre-receive hook declined)"), ""));
    assert!(!e.contains("자격 증명") && e.contains("pre-receive hook declined"), "{e}");
    for err in [
        "fatal: Authentication failed for 'https://gh.test/o/r.git/'",
        "fatal: could not read Username for 'https://gh.test': terminal prompts disabled",
        "remote: Permission to o/r.git denied to me.\nfatal: unable to access 'https://gh.test/o/r.git/': The requested URL returned error: 403",
        "git@gh.test: Permission denied (publickey).",
    ] {
        assert!(push_failure(&o(Some(128), "", err)).contains("자격 증명"), "{err}");
    }
    // a pre-push hook that fails: its own words, every time, not "unsure"
    let e = push_failure(&o(Some(1), "", "husky - pre-push hook exited with code 1 (error)\nerror: failed to push some refs"));
    assert!(e.starts_with("푸시하지 못했어요") && e.contains("husky"), "{e}");
    // only a push that may have gone through is "unsure"
    assert_eq!(push_failure(&exec::Out { timed_out: true, ..Default::default() }), UNSURE_PUSH);
    assert_eq!(push_failure(&o(None, "", "")), UNSURE_PUSH);
    assert_eq!(push_failure(&o(Some(0), "", "")), UNSURE_PUSH);
}

#[test]
fn a_create_github_refused_shows_why_and_creates_nothing_more() {
    let fx = Fx::new("refused");
    fx.work("b.txt");
    let id = fx.request(None);
    assert!(fx.merges.get(id).unwrap().pr_draft.unwrap().draft, "unverified: a draft");
    fx.resp("pr_create.1.code", "1");
    fx.resp("pr_create.1.err", "pull request create failed: GraphQL: Draft pull requests are not supported in this repository. (createPullRequest)\n");
    let lists = fx.calls("pr list");
    let e = fx.approve(id, PrOptions { draft: true, ..opts("t") }).unwrap_err();
    assert!(e.contains("초안 PR을 지원하지 않아요") && e.contains("'초안으로'를 끄고"), "{e}");
    // a definite answer: no lookups to recover, back to "pushed" (not unconfirmed), the reason kept for the card
    assert_eq!(fx.calls("pr list"), lists + 1, "only the check before create");
    assert_eq!(fx.step(id), Step::Pushed);
    assert!(fx.merges.get(id).unwrap().pr.unwrap().error.unwrap().contains("초안"));
    // without the draft: made at once
    let r = fx.approve(id, opts("t")).unwrap();
    assert_eq!(r.outcome, "pr");
    let creates: Vec<String> = fx.argv().into_iter().filter(|l| l.starts_with("pr create")).collect();
    assert_eq!(creates.len(), 2);
    assert!(creates[0].contains("--draft") && !creates[1].contains("--draft"), "{creates:?}");
    // other refusals show GitHub's words
    let fx = Fx::new("refused2");
    fx.work("b.txt");
    let id = fx.request(Some("ok"));
    fx.resp("pr_create.code", "1");
    fx.resp("pr_create.err", "pull request create failed: GraphQL: Resource protected by organization SAML enforcement. You must grant your OAuth token access to this organization. (createPullRequest)\n");
    let e = fx.approve(id, opts("t")).unwrap_err();
    assert!(e.contains("GitHub가 PR을 만들지 않았어요") && e.contains("SAML"), "{e}");
    assert_eq!(fx.step(id), Step::Pushed);
}

#[test]
fn a_pr_merged_or_closed_on_github_is_recorded_before_another_is_made() {
    // merged with a squash on GitHub, the poller hasn't seen it: refused, recorded, the conductor told
    let (fx, tip) = open_pr_fixture("stalemerged");
    let squashed = squash_on_remote(&fx, &tip);
    fx.work("c.txt");
    let id = fx.request(None);
    fx.resp("pr_view.out", &view("MERGED", &tip, Some(&squashed)));
    let e = fx.approve(id, opts("t")).unwrap_err();
    assert!(e.contains("PR #3이 squash로 합쳐진 브랜치예요"), "{e}");
    let rec = fx.prs.get(&fx.wt).unwrap();
    assert_eq!((rec.state.as_str(), rec.merged.as_ref().unwrap().method.as_str()), ("MERGED", "github"));
    assert_eq!(rec.merged_heads, [tip.clone()]);
    assert!(fx.host.notes().iter().any(|(k, n)| k.starts_with("pr:") && n.contains("PR #3이 GitHub에서 합쳐졌어요")));
    assert_eq!(fx.calls("pr create"), 0);
    // closed on GitHub: recorded and told, then a new PR is made
    let (fx, _) = open_pr_fixture("staleclosed");
    let tip2 = fx.work("c.txt");
    let id = fx.request(Some("ok"));
    fx.resp("pr_view.1.out", &view("CLOSED", &tip2, None));
    let r = fx.approve(id, opts("t")).unwrap();
    assert_eq!((r.outcome, r.pr.as_ref().unwrap().number), ("pr", 7));
    assert!(fx.host.notes().iter().any(|(k, n)| k.starts_with("pr:") && n.contains("PR #3이 병합 없이 닫혔어요")));
    // the record can't be read: nothing is decided on the old one
    let (fx, tip) = open_pr_fixture("staleunread");
    fx.work("c.txt");
    let id = fx.request(Some("ok"));
    fx.resp("pr_view.code", "1");
    fx.resp("pr_view.err", "HTTP 502: Bad Gateway");
    let e = fx.approve(id, opts("t")).unwrap_err();
    assert!(e.contains("기존 PR #3을 확인하지 못했어요"), "{e}");
    assert_eq!(fx.bare_tip("wt").as_deref(), Some(tip.as_str()), "nothing pushed");
    assert_eq!(fx.calls("pr create"), 0);
}

/// A request whose create was unclear (pushed `a`), replaced after the worker committed `b`: (fx, a, b, the new id).
fn carried_fixture(tag: &str) -> (Fx, String, String, u64) {
    let fx = Fx::new(tag);
    let a = fx.work("b.txt");
    let id = fx.request(None);
    fx.resp("pr_create.1.code", "1");
    fx.resp("pr_create.1.err", "HTTP 502: Bad Gateway");
    fx.approve(id, opts("t")).unwrap_err();
    assert_eq!(fx.bare_tip("wt").as_deref(), Some(a.as_str()));
    let b = fx.work("c.txt");
    let id2 = fx.request(None);
    let p = fx.merges.get(id2).unwrap().pr.unwrap();
    assert!(p.step == Step::Unconfirmed && p.push_sha.as_deref() == Some(a.as_str()), "{p:?}");
    (fx, a, b, id2)
}

#[test]
fn checking_again_only_looks_and_never_pushes() {
    let (fx, a, b, id) = carried_fixture("checkonly");
    let check = PrOptions { check_only: true, commit_message: Some("x".into()), ..opts("t") };
    // nothing found: a lookup, nothing pushed
    let e = fx.approve(id, check.clone()).unwrap_err();
    assert!(e.contains("아직 PR이 보이지 않아요"), "{e}");
    assert_eq!(fx.bare_tip("wt").as_deref(), Some(a.as_str()));
    assert!(fx.merges.get(id).unwrap().pr.unwrap().last_empty_lookup_at.is_some());
    // found, without this request's commit: recorded, the request waits for an approval that pushes
    fx.resp("pr_list.out", &format!("[{}]", pr_row(9, &a, "main")));
    let r = fx.approve(id, check.clone()).unwrap();
    assert_eq!(r.outcome, "found");
    assert!(r.message.contains("기존 PR #9을 찾았어요"), "{}", r.message);
    assert_eq!(fx.bare_tip("wt").as_deref(), Some(a.as_str()), "a lookup never pushes");
    let req = fx.merges.get(id).unwrap();
    let p = req.pr.clone().unwrap();
    assert!(p.step == Step::None && !p.unresolved(), "{p:?}");
    assert_eq!(req.plan.existing_pr, Some(9));
    let rec = fx.prs.get(&fx.wt).unwrap();
    assert_eq!((rec.number, rec.head_sha.as_str()), (9, a.as_str()));
    assert!(!fx.host.notes().iter().any(|(_, n)| n.contains("PR #9")), "the conductor isn't told yet");
    assert!(fx.host.events("maestro://merge-done").is_empty());
    // the approval pushes the new commit to that PR
    let r = fx.approve(id, opts("t")).unwrap();
    assert_eq!(r.outcome, "pr-updated");
    assert_eq!(fx.bare_tip("wt").as_deref(), Some(b.as_str()));
    assert_eq!(fx.calls("pr create"), 1);
    // with this request's commit already there, a look finishes the request as the approval would
    let (fx, _, b, id) = carried_fixture("checkonly2");
    g(&fx.wt, &["push", "-q", &fx.bare, "wt"]);
    fx.resp("pr_list.out", &format!("[{}]", pr_row(9, &b, "main")));
    let r = fx.approve(id, check.clone()).unwrap();
    assert_eq!(r.outcome, "recovered");
    assert!(fx.merges.get(id).is_none());
    // the PR recorded already (the replaced request's create went through meanwhile): a look still only found it
    let (fx, _, b, id) = carried_fixture("checkonly3");
    g(&fx.wt, &["push", "-q", &fx.bare, "wt"]);
    fx.prs.upsert(PrRecord { root: fx.root.clone(), worker_path: fx.wt.clone(), branch: "wt".into(), base: "main".into(), number: 9, state: "OPEN".into(), app_created: true, ..Default::default() });
    fx.resp("pr_list.out", &format!("[{}]", pr_row(9, &b, "main")));
    let r = fx.approve(id, check).unwrap();
    assert_eq!(r.outcome, "recovered");
    assert!(r.message.contains("기존 PR #9을 찾아 이어 썼어요") && !r.message.contains("푸시"), "{}", r.message);
}

#[test]
fn restart_finishes_only_what_was_pushed() {
    let (fx, a, b, id) = carried_fixture("resumecarry");
    fx.resp("pr_list.out", &format!("[{}]", pr_row(9, &a, "main")));
    let merges = Merges::at(fx.dir.join("merge-requests.json"));
    // an approval running for the worker: the request is left to it
    {
        let _busy = merges.lock_pr(&fx.wt).unwrap();
        resume(&fx.host, &merges, &fx.prs);
        assert_eq!(merges.get(id).unwrap().pr.unwrap().step, Step::Unconfirmed);
        assert!(fx.prs.get(&fx.wt).is_none());
    }
    // the PR found has the old request's push, not this one's commits: recorded, the request stays for a click
    resume(&fx.host, &merges, &fx.prs);
    let req = merges.get(id).expect("not finished without the user's approval");
    let p = req.pr.unwrap();
    // what was found is a notice, not an error
    assert!(p.step == Step::None && !p.unresolved() && p.error.is_none() && p.notice.as_deref().is_some_and(|e| e.contains("기존 PR #9")), "{p:?}");
    assert_eq!(fx.prs.get(&fx.wt).unwrap().head_sha, a);
    assert!(!fx.host.notes().iter().any(|(_, n)| n.contains("이어 썼어요")), "{:?}", fx.host.notes());
    assert_eq!(fx.bare_tip("wt").as_deref(), Some(a.as_str()));
    assert_ne!(a, b);
    // a merge attempt is left to a merge click running now
    fx.prs.update(&fx.wt, |r| r.merge_attempt = Some(MergeAttempt { method: "merge".into(), seen_head: a.clone(), at: 1 }));
    fx.resp("pr_view.out", r#"{"state":"OPEN","mergedAt":null,"mergeCommit":null,"autoMergeRequest":null,"headRefOid":"x"}"#);
    {
        let _busy = merges.lock_pr(&fx.wt).unwrap();
        resume(&fx.host, &merges, &fx.prs);
        assert!(fx.prs.get(&fx.wt).unwrap().merge_attempt.is_some());
    }
}

/// `status3` with the PR in GitHub's merge queue.
fn in_queue(state: &str, head: &str) -> String {
    status3(state, head, GREEN).replace(r#""autoMergeRequest":null"#, r#""autoMergeRequest":null,"isInMergeQueue":true"#)
}

#[test]
fn a_merge_queue_entry_is_not_a_refusal() {
    let (fx, tip) = open_pr_fixture("mqueue");
    // gh exits 0 and the PR stays open with no auto-merge: the base's merge queue took it (only GraphQL says so)
    fx.resp(&format!("api_graphql.{}.out", fx.next("api_graphql")), &status3("OPEN", &tip, GREEN));
    fx.resp("api_graphql.out", &in_queue("OPEN", &tip));
    fx.resp("pr_view.out", &view("OPEN", &tip, None));
    let rec = merge_pr(&fx.host, &fx.merges, &fx.prs, &fx.wt, "squash", &tip, &NO_ACK).unwrap();
    assert!(rec.queued && rec.merge_attempt.is_some(), "{rec:?}");
    assert!(rec.error.is_none() && rec.notice.as_deref().is_some_and(|n| n.contains("대기열")), "{rec:?}");
    // the poller keeps the click past the grace while it is queued, and the merge is the user's
    fx.host.advance(ATTEMPT_GRACE + 10_000);
    let mut s = Sched::default();
    poll_once(&fx.host, &fx.prs, &mut s);
    let rec = fx.prs.get(&fx.wt).unwrap();
    assert!(rec.queued && rec.merge_attempt.is_some(), "{rec:?}");
    // already queued: no second merge call
    let merges = fx.calls("pr merge");
    let e = merge_pr(&fx.host, &fx.merges, &fx.prs, &fx.wt, "squash", &tip, &NO_ACK).unwrap_err();
    assert!(e.contains("병합 대기열에 있어요"), "{e}");
    assert_eq!(fx.calls("pr merge"), merges);
    // gh exits 0, open, no queue seen (yet): not refused, the click stays for the poller
    let (fx, tip) = open_pr_fixture("mqueue2");
    fx.resp("api_graphql.out", &status3("OPEN", &tip, GREEN));
    fx.resp("pr_view.out", &view("OPEN", &tip, None));
    let rec = merge_pr(&fx.host, &fx.merges, &fx.prs, &fx.wt, "merge", &tip, &NO_ACK).unwrap();
    // a status, not an error: the card shows it muted
    assert!(rec.merge_attempt.is_some() && rec.error.is_none() && rec.notice.as_deref().is_some_and(|e| e.contains("아직 열려 있어요")), "{rec:?}");
    // a second click meanwhile never runs `gh pr merge` again (its failure would erase the first click)
    let merges = fx.calls("pr merge");
    let e = merge_pr(&fx.host, &fx.merges, &fx.prs, &fx.wt, "merge", &tip, &NO_ACK).unwrap_err();
    assert!(e.contains("이미 보냈어요"), "{e}");
    assert_eq!(fx.calls("pr merge"), merges);
    assert!(fx.prs.get(&fx.wt).unwrap().merge_attempt.is_some());
}

#[test]
fn an_ssh_host_alias_resolves_to_its_host() {
    let mut fx = Fx::new("sshalias");
    // `Host github-work` → `HostName github.com` in the user's ssh config, as `ssh -G` prints it (a fake ssh)
    let bin = fx.dir.join("bin");
    let ssh = if cfg!(windows) {
        let f = bin.join("ssh.cmd");
        std::fs::write(&f, "@echo off\r\n>>\"%~dp0..\\ssh.log\" echo(%*\r\necho user git\r\necho hostname github.com\r\necho port 22\r\n").unwrap();
        f
    } else {
        let f = bin.join("ssh");
        std::fs::write(&f, "#!/bin/sh\necho \"$*\" >> \"$(dirname \"$0\")/../ssh.log\"\nprintf 'user git\\nhostname github.com\\nport 22\\n'\n").unwrap();
        Command::new("chmod").args(["+x", &f.to_string_lossy()]).output().unwrap();
        f
    };
    fx.host.tools.ssh = Some(ssh);
    g(&fx.root, &["remote", "set-url", "origin", "git@github-work:o/r.git"]);
    g(&fx.root, &["config", "--add", &format!("url.{}.insteadOf", fx.bare), "git@github-work:o/r.git"]);
    let r = github::check(&fx.host.tools, &fx.root, "main", true);
    assert!(r.ok, "{}", r.message);
    assert_eq!((r.host.as_str(), r.slug.as_str()), ("github.com", "o/r"));
    assert!(fx.argv().iter().any(|l| l.starts_with("auth status --hostname github.com")), "{:?}", fx.argv());
    assert!(fx.argv().iter().all(|l| !l.contains("github-work")), "{:?}", fx.argv());
    let log = std::fs::read_to_string(fx.dir.join("ssh.log")).unwrap();
    assert!(log.contains("-G") && log.contains("BatchMode=yes") && log.trim_end().ends_with("github-work"), "{log}");
    // no ssh to ask: the alias stays
    assert_eq!(github::resolve_ssh_host(&Tools { ssh: Some(fx.dir.join("nothing")), ..fx.host.tools.clone() }, "github-work"), "github-work");
}

#[test]
fn a_push_url_to_another_repo_fails_readiness() {
    let fx = Fx::new("pushurl");
    // the same repo spelled otherwise is fine
    g(&fx.root, &["config", "remote.origin.pushurl", "https://gh.test/O/R"]);
    let r = github::check(&fx.host.tools, &fx.root, "main", true);
    assert!(r.ok, "{}", r.message);
    // a fork, or no push at all
    for url in ["https://gh.test/fork/r.git", "no_push"] {
        g(&fx.root, &["config", "remote.origin.pushurl", url]);
        let r = github::check(&fx.host.tools, &fx.root, "main", true);
        assert!(!r.ok && r.message.starts_with("푸시 주소가 다른 저장소예요"), "{url}: {}", r.message);
    }
}

#[test]
fn a_commit_hook_past_the_time_limit_leaves_no_index_lock() {
    let mut fx = Fx::new("hooktime");
    let tip = fx.work("b.txt");
    put(&fx.wt, "a.txt", "edited\n");
    let id = fx.request(None);
    let hooks = fx.dir.join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    std::fs::write(hooks.join("pre-commit"), "#!/bin/sh\nsleep 30\n").unwrap();
    if !cfg!(windows) {
        Command::new("chmod").args(["+x", &hooks.join("pre-commit").to_string_lossy()]).output().unwrap();
    }
    g(&fx.root, &["config", "core.hooksPath", &worktree::norm(&hooks)]);
    fx.host.tools.t.commit = 2.0;
    let o = PrOptions { commit_message: Some("m".into()), ..opts("t") };
    let e = fx.approve(id, o).unwrap_err();
    assert!(e.contains("시간 제한") && !e.contains("index.lock이 남아") && !e.contains("index.lock을 지우지 못했어요"), "{e}");
    // on Windows the killed git leaves index.lock and the app removes it; on Unix git removes it itself when signalled
    if cfg!(windows) {
        assert!(e.contains("index.lock은 지웠어요"), "{e}");
    }
    let lock = g(&fx.wt, &["rev-parse", "--git-path", "index.lock"]);
    let lock = if Path::new(&lock).is_absolute() { PathBuf::from(lock) } else { Path::new(&fx.wt).join(lock) };
    assert!(!lock.exists(), "{lock:?}");
    assert_eq!(g(&fx.wt, &["rev-parse", "HEAD"]), tip);
    assert_eq!(fx.step(id), Step::None);
    // the worker's git works again
    g(&fx.wt, &["add", "a.txt"]);
}

#[test]
fn a_discarded_maestros_prs_keep_their_records_but_get_no_notes() {
    let fx = Fx::new("orphan");
    fx.prs.upsert(PrRecord { root: fx.root.clone(), maestro_path: fx.root.clone(), worker_path: fx.wt.clone(), number: 3, state: "OPEN".into(), ..Default::default() });
    fx.prs.upsert(PrRecord { root: "C:/other".into(), maestro_path: "C:/other".into(), worker_path: "C:/other/w".into(), number: 4, state: "OPEN".into(), ..Default::default() });
    note_pr(&fx.host, &fx.prs.get(&fx.wt).unwrap(), "before");
    assert_eq!(fx.host.notes().len(), 1);
    // only that Maestro's PRs, whatever the spelling
    let gone = fx.prs.orphan_maestro(&format!("{}/", fx.root.to_uppercase()));
    assert_eq!(gone.iter().map(|r| r.number).collect::<Vec<_>>(), [3]);
    let r = fx.prs.get(&fx.wt).unwrap();
    assert!(r.maestro_path.is_empty() && r.number == 3);
    assert_eq!(fx.prs.get("C:/other/w").unwrap().maestro_path, "C:/other");
    // a later Maestro in the folder hears nothing of it
    note_pr(&fx.host, &r, "after");
    assert_eq!(fx.host.notes().len(), 1);
    assert!(fx.prs.orphan_maestro(&fx.root).is_empty());
    // a PR waiting to merge keeps its worker
    let mut p = r.clone();
    assert!(!p.merge_pending());
    p.queued = true;
    assert!(p.merge_pending());
    p.queued = false;
    p.merge_attempt = Some(MergeAttempt::default());
    assert!(p.merge_pending());
    p.state = "MERGED".into();
    assert!(!p.merge_pending());
}

#[test]
fn a_look_never_finishes_a_request_with_changes_left_to_commit() {
    // R1 pushed `a` and its create was unclear; the worker then edited a tracked file and the conductor asked again
    let fx = Fx::new("dirtycarry");
    let a = fx.work("b.txt");
    let id = fx.request(None);
    fx.resp("pr_create.1.code", "1");
    fx.resp("pr_create.1.err", "HTTP 502: Bad Gateway");
    fx.approve(id, opts("t")).unwrap_err();
    put(&fx.wt, "a.txt", "edited\n");
    let id2 = fx.request(None);
    let req = fx.merges.get(id2).unwrap();
    assert!(!req.plan.worker_dirty.is_empty() && req.plan.tip_sha == a && req.pr.unwrap().push_sha.as_deref() == Some(a.as_str()));
    fx.resp("pr_list.out", &format!("[{}]", pr_row(9, &a, "main")));
    // "다시 확인": the PR is found, but the edit is in no commit yet: the request waits for "커밋하고 진행"
    let r = fx.approve(id2, PrOptions { check_only: true, ..opts("t") }).unwrap();
    assert_eq!(r.outcome, "found");
    let p = fx.merges.get(id2).expect("not finished").pr.unwrap();
    assert!(p.step == Step::None && !p.unresolved(), "{p:?}");
    assert!(!fx.host.notes().iter().any(|(_, n)| n.contains("이어 썼어요")), "{:?}", fx.host.notes());
    // the approval commits and pushes it to that PR
    let r = fx.approve(id2, PrOptions { commit_message: Some("edit a".into()), ..opts("t") }).unwrap();
    assert_eq!(r.outcome, "pr-updated");
    let head = g(&fx.wt, &["rev-parse", "HEAD"]);
    assert_ne!(head, a);
    assert_eq!(fx.bare_tip("wt").as_deref(), Some(head.as_str()));
    assert_eq!(fx.calls("pr create"), 1);
    // after a restart the same: found, not finished
    let fx = Fx::new("dirtycarry2");
    let a = fx.work("b.txt");
    let id = fx.request(None);
    fx.resp("pr_create.1.code", "1");
    fx.resp("pr_create.1.err", "HTTP 502: Bad Gateway");
    fx.approve(id, opts("t")).unwrap_err();
    put(&fx.wt, "a.txt", "edited\n");
    let id2 = fx.request(None);
    fx.resp("pr_list.out", &format!("[{}]", pr_row(9, &a, "main")));
    resume(&fx.host, &fx.merges, &fx.prs);
    let p = fx.merges.get(id2).expect("not finished by a restart").pr.unwrap();
    assert!(p.step == Step::None && !p.unresolved(), "{p:?}");
    assert_eq!(fx.prs.get(&fx.wt).unwrap().number, 9);
}

#[test]
fn a_look_finds_the_pr_under_commits_added_on_top() {
    let fx = Fx::new("ontop");
    let a = fx.work("b.txt");
    let id = fx.request(None);
    fx.resp("pr_create.1.code", "1");
    fx.resp("pr_create.1.err", "HTTP 502: Bad Gateway");
    fx.approve(id, opts("t")).unwrap_err();
    // the PR was made after all, and a bot pushed a fix on top of `a` (a commit the worker doesn't have)
    let clone = worktree::norm(&fx.dir.join("bot"));
    Command::new("git").args(["clone", "-q", "-b", "wt", &fx.bare, &clone]).output().unwrap();
    put(&clone, "fix.txt", "bot\n");
    let f = commit_all(&clone, "bot fix");
    g(&clone, &["push", "-q", "origin", "wt"]);
    assert!(Command::new("git").args(["-C", &fx.wt, "cat-file", "-e", &format!("{f}^{{commit}}")]).output().unwrap().status.code() != Some(0));
    fx.resp("pr_list.out", &format!("[{}]", pr_row(9, &f, "main")));
    g(&fx.wt, &["update-ref", "refs/remotes/origin/wt", &a]);
    let r = fx.approve(id, PrOptions { check_only: true, ..opts("t") }).unwrap();
    assert_eq!(r.outcome, "recovered");
    assert!(fx.merges.get(id).is_none());
    let rec = fx.prs.get(&fx.wt).unwrap();
    assert_eq!((rec.number, rec.head_sha.as_str()), (9, f.as_str()));
    // the objects were fetched, no ref moved (the shared repo's remote-tracking branch stays)
    assert_eq!(g(&fx.wt, &["rev-parse", "refs/remotes/origin/wt"]), a);
    assert!(fx.host.notes().iter().any(|(_, n)| n.starts_with("[maestro] 기존 PR #9을 찾아 이어 썼어요")));
    // nothing pushed, nothing created
    assert_eq!(fx.bare_tip("wt").as_deref(), Some(f.as_str()));
    assert_eq!(fx.calls("pr create"), 1);
}

#[test]
fn rejecting_after_a_look_found_the_pr_names_it() {
    let (fx, a, _, id) = carried_fixture("rejectfound");
    fx.resp("pr_list.out", &format!("[{}]", pr_row(9, &a, "main")));
    assert_eq!(fx.approve(id, PrOptions { check_only: true, ..opts("t") }).unwrap().outcome, "found");
    let r = reject(&fx.host, &fx.merges, &fx.prs, id, None).unwrap();
    assert!(r.message.contains("열린 PR #9은 남아 있어요"), "{}", r.message);
    assert!(fx.host.notes().iter().any(|(k, n)| k.starts_with("merge:") && n.contains("열린 PR #9은 남아 있어요")), "{:?}", fx.host.notes());
    assert_eq!(fx.prs.get(&fx.wt).unwrap().number, 9);
}

#[test]
fn a_pr_open_on_another_branch_is_never_replaced() {
    let (fx, tip) = open_pr_fixture("otherbranch");
    fx.prs.update(&fx.wt, |r| r.queued = true);
    // the worker moved to another branch and committed there
    g(&fx.wt, &["switch", "-q", "-c", "b2"]);
    fx.work("c.txt");
    // the plan doesn't take #3 (on wt) for this branch's PR
    assert_eq!(fx.plan(MergeMode::Pr).unwrap().existing_pr, None);
    let id = fx.request(None);
    fx.resp("pr_view.out", &view("OPEN", &tip, None));
    let e = fx.approve(id, opts("t")).unwrap_err();
    assert!(e.contains("다른 브랜치(wt)에 열린 PR #3"), "{e}");
    let rec = fx.prs.get(&fx.wt).unwrap();
    assert!(rec.number == 3 && rec.queued && rec.branch == "wt", "{rec:?}");
    assert_eq!((fx.calls("pr create"), fx.bare_tip("b2")), (0, None));
    // once #3 is closed on GitHub, the new branch gets its PR
    fx.resp("pr_view.out", &view("CLOSED", &tip, None));
    let r = fx.approve(id, opts("t")).unwrap();
    assert_eq!(r.outcome, "pr");
    assert_eq!(fx.prs.get(&fx.wt).unwrap().branch, "b2");
}

#[test]
fn the_next_approval_settles_a_commit_cut_off_by_a_crash() {
    let fx = Fx::new("stalecommit");
    let tip = fx.work("b.txt");
    put(&fx.wt, "a.txt", "edited\n");
    let id = fx.request(None);
    // the app went down after its commit landed; the user clicks before `resume` got to it
    fx.merges.update(id, |r| r.pr = Some(PrProgress { step: Step::Committing, commit_parent: Some(tip.clone()), commit_message: Some("m".into()), ..Default::default() }));
    g(&fx.wt, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-am", "m"]);
    let app_commit = g(&fx.wt, &["rev-parse", "HEAD"]);
    let r = fx.approve(id, PrOptions { commit_message: Some("m".into()), ..opts("t") }).unwrap();
    assert_eq!(r.outcome, "pr");
    assert_eq!(fx.bare_tip("wt").as_deref(), Some(app_commit.as_str()));
    assert_eq!(g(&fx.wt, &["rev-parse", "HEAD^"]), tip, "no second commit");
}

#[test]
fn a_restart_leaves_a_fresh_merge_click_to_the_poller() {
    let (fx, tip) = open_pr_fixture("freshclick");
    let now = fx.host.now();
    fx.prs.update(&fx.wt, |r| r.merge_attempt = Some(MergeAttempt { method: "merge".into(), seen_head: tip.clone(), at: now }));
    fx.resp("pr_view.out", r#"{"state":"OPEN","mergedAt":null,"mergeCommit":null,"autoMergeRequest":null,"headRefOid":"x"}"#);
    fx.resp("api_graphql.out", &status3("OPEN", &tip, GREEN));
    // GitHub shows the queue entry only a moment later: kept
    resume(&fx.host, &fx.merges, &fx.prs);
    let rec = fx.prs.get(&fx.wt).unwrap();
    assert!(rec.merge_attempt.is_some() && rec.error.is_none(), "{rec:?}");
    // past the grace and still not queued: the click is over
    fx.host.advance(ATTEMPT_GRACE + 1_000);
    resume(&fx.host, &fx.merges, &fx.prs);
    let rec = fx.prs.get(&fx.wt).unwrap();
    assert!(rec.merge_attempt.is_none() && rec.error.as_deref() == Some("병합이 끝나지 않았어요"), "{rec:?}");
}

/// A hook in `hooks` for every checkout of the fixture's repo.
fn hook(fx: &Fx, name: &str, body: &str) {
    let hooks = fx.dir.join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    std::fs::write(hooks.join(name), format!("#!/bin/sh\n{body}\n")).unwrap();
    if !cfg!(windows) {
        Command::new("chmod").args(["+x", &hooks.join(name).to_string_lossy()]).output().unwrap();
    }
    g(&fx.root, &["config", "core.hooksPath", &worktree::norm(&hooks)]);
}

#[test]
fn a_commit_whose_post_commit_hook_runs_too_long_is_kept_and_others_locks_stay() {
    let mut fx = Fx::new("posthook");
    let tip = fx.work("b.txt");
    put(&fx.wt, "a.txt", "edited\n");
    let id = fx.request(None);
    // after the commit git released its lock; someone else's git takes it while the hook runs on
    hook(&fx, "post-commit", ": > \"$(git rev-parse --git-path index.lock)\"\nsleep 30");
    fx.host.tools.t.commit = 5.0;
    let e = fx.approve(id, PrOptions { commit_message: Some("edit a".into()), ..opts("t") }).unwrap_err();
    assert!(e.contains("커밋은 됐지만") && e.contains("index.lock이"), "{e}");
    let head = g(&fx.wt, &["rev-parse", "HEAD"]);
    assert_eq!(g(&fx.wt, &["rev-parse", "HEAD^"]), tip);
    let p = fx.merges.get(id).unwrap().pr.unwrap();
    assert_eq!((p.step, p.app_commit.as_deref()), (Step::Committed, Some(head.as_str())));
    // the lock isn't the app's: left alone
    let lock = g(&fx.wt, &["rev-parse", "--git-path", "index.lock"]);
    let lock = if Path::new(&lock).is_absolute() { PathBuf::from(lock) } else { Path::new(&fx.wt).join(lock) };
    assert!(lock.exists(), "{lock:?}");
    std::fs::remove_file(&lock).unwrap();
    // the next click carries on with that commit: pushed, never made twice
    g(&fx.root, &["config", "--unset", "core.hooksPath"]);
    let r = fx.approve(id, PrOptions { commit_message: Some("edit a".into()), ..opts("t") }).unwrap();
    assert_eq!(r.outcome, "pr");
    assert_eq!(fx.bare_tip("wt").as_deref(), Some(head.as_str()));
    let body = fx.bodies().pop().unwrap();
    assert!(body.contains("앱이 사용자 확인으로 커밋했어요: edit a"), "{body}");
}

#[test]
fn a_fast_forward_whose_post_merge_hook_runs_too_long_counts_as_done() {
    let (mut fx, tip) = open_pr_fixture("postmerge");
    let merged = squash_on_remote(&fx, &tip);
    hook(&fx, "post-merge", "sleep 30");
    fx.host.tools.t.commit = 5.0;
    let rec = fx.prs.get(&fx.wt).unwrap();
    let r = sync_base(&fx.host.tools, &fx.merges, &rec);
    assert_eq!(r.as_deref(), Ok(merged.as_str()));
    assert_eq!(g(&fx.root, &["rev-parse", "HEAD"]), merged);
}

#[test]
fn several_remote_urls_must_name_one_repo() {
    let fx = Fx::new("multiurl");
    // the same repo spelled twice is fine
    g(&fx.root, &["config", "--add", "remote.origin.url", "https://gh.test/O/R"]);
    let r = github::check(&fx.host.tools, &fx.root, "main", true);
    assert!(r.ok, "{}", r.message);
    assert_eq!(r.slug, "gh.test/o/r", "the first url's repo, as fetch and ls-remote use it");
    // a second repo: the push would go to both
    g(&fx.root, &["config", "--add", "remote.origin.url", "https://gh.test/o/r-mirror.git"]);
    let r = github::check(&fx.host.tools, &fx.root, "main", true);
    assert!(!r.ok && r.message.contains("주소(url)가 여러 개"), "{}", r.message);
    g(&fx.root, &["config", "--unset-all", "remote.origin.url", "mirror"]);
    // every pushurl counts, not only the first
    g(&fx.root, &["config", "--add", "remote.origin.pushurl", "https://gh.test/o/r.git"]);
    g(&fx.root, &["config", "--add", "remote.origin.pushurl", "https://gh.test/fork/r.git"]);
    let r = github::check(&fx.host.tools, &fx.root, "main", true);
    assert!(!r.ok && r.message.starts_with("푸시 주소가 다른 저장소예요"), "{}", r.message);
}

#[test]
fn an_ssh_alias_that_cant_be_read_says_so() {
    let fx = Fx::new("sshkept");
    // no ssh to resolve `github-work`, and gh has no login for it
    g(&fx.root, &["remote", "set-url", "origin", "git@github-work:o/r.git"]);
    g(&fx.root, &["config", "--add", &format!("url.{}.insteadOf", fx.bare), "git@github-work:o/r.git"]);
    fx.resp("auth_status.code", "1");
    fx.resp("auth_status.err", "You are not logged into any GitHub hosts. To log in, run: gh auth login\n");
    let r = github::check(&fx.host.tools, &fx.root, "main", true);
    assert!(!r.ok && r.message.contains("ssh 호스트 별칭") && r.message.contains("https://github.com/o/r.git"), "{}", r.message);
    // a GitHub Enterprise host over https keeps its own advice
    g(&fx.root, &["remote", "set-url", "origin", "https://ghe.example/o/r.git"]);
    let r = github::check(&fx.host.tools, &fx.root, "main", true);
    assert!(!r.ok && !r.message.contains("ssh 호스트 별칭") && r.message.contains("gh auth login --hostname ghe.example"), "{}", r.message);
}

#[test]
fn a_commit_hook_that_changes_the_files_keeps_the_commit_unpushed() {
    let fx = Fx::new("hookadds");
    let tip = fx.work("b.txt");
    put(&fx.wt, "a.txt", "edited
");
    let id = fx.request(None);
    // with `--only` the hook sees the commit's own index: what it adds goes in
    hook(&fx, "pre-commit", "echo gen > gen.txt
git add gen.txt");
    let with_msg = PrOptions { commit_message: Some("edit a".into()), ..opts("t") };
    let e = fx.approve(id, with_msg.clone()).unwrap_err();
    assert!(e.contains("카드에 없던 파일이 들어갔어요: gen.txt") && e.contains("푸시하지 않아요"), "{e}");
    let head = g(&fx.wt, &["rev-parse", "HEAD"]);
    assert_eq!(g(&fx.wt, &["rev-parse", "HEAD^"]), tip);
    let p = fx.merges.get(id).unwrap().pr.unwrap();
    assert_eq!((p.step, p.app_commit.as_deref()), (Step::Committed, Some(head.as_str())));
    assert!(p.commit_mismatch.is_some());
    // the next click neither pushes that commit nor makes another
    g(&fx.root, &["config", "--unset", "core.hooksPath"]);
    let e = fx.approve(id, with_msg).unwrap_err();
    assert!(e.contains("gen.txt"), "{e}");
    assert_eq!(g(&fx.wt, &["rev-parse", "HEAD"]), head);
    assert_eq!((fx.bare_tip("wt"), fx.calls("pr create")), (None, 0));
}

#[test]
fn a_commit_message_a_hook_added_to_still_counts_as_the_apps() {
    assert!(same_message("edit a

Change-Id: I1
", "edit a"));
    assert!(same_message("[ABC-1] edit a

Signed-off-by: t <t@t>", "edit  a"));
    assert!(!same_message("prefix bug", "fix"));
    assert!(!same_message("edit b", "edit a"));
    // the app went down after its commit landed with a trailer from a commit-msg hook: carried on, not refused
    let fx = Fx::new("trailer");
    let tip = fx.work("b.txt");
    put(&fx.wt, "a.txt", "edited
");
    let id = fx.request(None);
    fx.merges.update(id, |r| r.pr = Some(PrProgress { step: Step::Committing, commit_parent: Some(tip.clone()), commit_message: Some("m".into()), ..Default::default() }));
    g(&fx.wt, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-am", "m", "-m", "Change-Id: I1"]);
    let app_commit = g(&fx.wt, &["rev-parse", "HEAD"]);
    let r = fx.approve(id, PrOptions { commit_message: Some("m".into()), ..opts("t") }).unwrap();
    assert_eq!(r.outcome, "pr");
    assert_eq!(fx.bare_tip("wt").as_deref(), Some(app_commit.as_str()));
}

/// What is written down before an outside call can't be saved (here: the file is a newer Maestro's): the call isn't
/// made, and the file is left as it was.
#[test]
fn nothing_goes_out_unrecorded() {
    let fx = Fx::new("unrecorded");
    fx.work("b.txt");
    let id = fx.request(Some("ok"));
    let newer = r#"{"version":2,"items":[]}"#;
    std::fs::write(fx.dir.join("merge-requests.json"), newer).unwrap();
    let e = fx.approve(id, opts("t")).unwrap_err();
    assert!(e.contains("저장하지 못해"), "{e}");
    assert_eq!(fx.calls("pr create"), 0);
    assert!(fx.bare_tip("wt").is_none());
    assert_eq!(std::fs::read_to_string(fx.dir.join("merge-requests.json")).unwrap(), newer);

    let (fx, tip) = open_pr_fixture("unrecorded-merge");
    fx.resp("api_graphql.out", &status3("OPEN", &tip, GREEN));
    std::fs::write(fx.dir.join("pull-requests.json"), newer).unwrap();
    let e = merge_pr(&fx.host, &fx.merges, &fx.prs, &fx.wt, "merge", &tip, &NO_ACK).unwrap_err();
    assert!(e.contains("병합 시도 기록을 저장하지 못해"), "{e}");
    assert_eq!(fx.calls("pr merge"), 0);
    assert!(fx.prs.get(&fx.wt).unwrap().merge_attempt.is_none());
    assert_eq!(std::fs::read_to_string(fx.dir.join("pull-requests.json")).unwrap(), newer);
}
