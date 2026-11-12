//! The command guard (D4): agents don't merge into the base branch, push, or run destructive git commands; finished
//! work reaches the base branch only through `maestro merge-request` and the user's approval (merge.rs). Claude
//! Code's Bash, PowerShell and Monitor calls are checked here before they run: the `PreToolUse` hook Maestro puts in
//! its settings (`/pretool/…`, approval.rs) and, for the chat runtime, also before a permission card is shown
//! (agent.rs). Agents without hooks (Antigravity) only have the brief.
//!
//! `check` reads a command line the way a shell would, well enough to find every git (and `gh`) call in it: chained
//! and piped commands, subshells, `$( )` and backticks, heredocs, quotes and escapes (`g''it`, `$'git'`), wrappers
//! (`env`, `xargs`, `bash -c`, `pwsh -Command`, `cmd /c`, `wsl`, `eval`, `Start-Process`, `find -exec`,
//! `watch`, `setsid`, `flock`, and any program it doesn't know that has a git word among its arguments), bash
//! reserved words (`if git merge …; then`), shells reading commands from a heredoc, here-string or `echo … |`, shell aliases, `cd` and `Push-Location` before the call, `git -C`, git aliases, and the commands git
//! runs itself (`rebase -x`, `difftool -x`, `--upload-pack`, `submodule foreach`, `bisect run`, `-c core.editor=…`,
//! `git config core.pager …`, `GIT_EDITOR=…`, `ext::` URLs). Each call
//! is judged by where it runs: the project folder, a checkout of the base branch, or anywhere it can't tell (`git
//! --git-dir`, `GIT_DIR=`, `cd $X`) is "strict". It fails closed: what it can't read, but mentions git, is denied,
//! and so is a command named by a variable (`${G}t push`, `& $g push`) that comes with a git verb. It is not a
//! sandbox: scripts, Makefiles, npm scripts, hooks and MCP tools aren't looked into (docs: merge-request.md).

use crate::worktree;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use tauri::{AppHandle, Manager};

/// Kill switch: false lets every command through.
pub const GUARD: bool = true;
/// Wrappers and re-read strings (`bash -c`, aliases) nested deeper than this are denied.
const MAX_DEPTH: usize = 6;

pub const PUSH: &str = "푸시는 막혀 있어요. 푸시·PR 생성·병합은 사용자 승인 뒤 앱이 해요: 지휘자는 maestro merge-request로 요청하세요.";
/// R5's words for a local-mode project, where approving a merge request merges in the project folder and never pushes
pub const PUSH_LOCAL: &str = "푸시는 막혀 있어요. 푸시·PR은 사람이 해요.";
pub const MERGE: &str = "기준 브랜치/프로젝트 폴더에서 직접 합칠 수 없어요. 지휘자는 maestro merge-request --to <워커>로 요청하고, 워커는 자기 브랜치에서 작업을 끝냈다고 답하세요. 기준 브랜치를 가져오려면 자기 워크트리에서 git merge/rebase <기준>을 쓰세요.";
/// PR mode: the app never force-pushes, so a pushed branch is only ever added to
pub const MERGE_PR: &str = "기준 브랜치/프로젝트 폴더에서 직접 합칠 수 없어요. 지휘자는 maestro merge-request --to <워커>로 요청하고, 워커는 자기 브랜치에서 작업을 끝냈다고 답하세요. 기준 브랜치를 가져오려면 자기 워크트리에서 git merge <기준>을 쓰세요. 이미 푸시한 브랜치는 rebase·amend·reset으로 기록을 바꾸지 마세요: 앱은 강제 푸시를 하지 않아서 그 뒤 푸시가 막혀요.";
pub const BASE: &str = "프로젝트 폴더(기준 브랜치)의 체크아웃이나 기록을 바꾸는 명령(브랜치 전환, reset, amend, stash 등)은 막혀 있어요. 작업은 자기 워크트리에서 하고, 합치기는 maestro merge-request로 요청하세요.";
pub const DESTRUCTIVE: &str = "되돌리기 어려운 명령(reset --hard / clean -f / branch -D / 강제 체크아웃 …)은 막혀 있어요. 필요하면 사용자에게 부탁하세요.";
pub const MERGING: &str = "Maestro가 지금 이 폴더에서 합치는 중이에요. 잠시 뒤 다시 실행하세요";
pub const UNPARSED: &str = "명령을 해석하지 못했어요: 나눠서 다시 실행하세요";
pub const INDIRECT: &str = "간접 실행(python -c, | sh 등) 안의 git push·merge·reset --hard 같은 명령은 막혀 있어요";
pub const ALIAS: &str = "git 별칭을 만들거나 명령줄에서 정의하는 건 막혀 있어요";
pub const UNKNOWN: &str = "어떤 git 명령인지 알 수 없어요(변수나 모르는 별칭): 풀어서 다시 실행하세요";
pub const PR: &str = "푸시·PR 생성·병합은 사용자 승인 뒤 앱이 해요: 지휘자는 maestro merge-request로 요청하세요 (gh는 읽기만 돼요: pr view/list/diff/checks/checkout, run view/list/download, repo view, issue/release/workflow/label 보기, search, GET뿐인 gh api).";
pub const PR_LOCAL: &str = "PR을 만들거나 합치는 건 사람이 해요: gh pr create/new/merge, 쓰는 gh api(GET이 아닌 -X, -X GET 없는 -f·-F, --input, graphql mutation, 변수가 든 gh api), gh 별칭 정의, 변수로 쓴 gh 명령은 막혀 있어요.";
pub const TOKEN: &str = "로그인 토큰을 출력하는 명령(gh auth token, gh auth status -t, gh auth git-credential, gh config get … token, git credential fill, git credential-<도우미> get, credential.helper 지정, GIT_TRACE_REDACT 바꾸기)은 막혀 있어요.";

/// Where a command runs. None = unknown, which counts as the strictest case.
pub struct Ctx<'a> {
    /// the shell's working directory when the command starts
    pub cwd: Option<&'a str>,
    /// the agent's project folder
    pub root: Option<&'a str>,
    /// the project's base branch
    pub base: Option<&'a str>,
    /// the app is merging into `root` right now
    pub merging: bool,
    /// worker folders the app is committing in right now (PR mode): git writes there wait
    pub locked: &'a [String],
    /// the project merges through PRs (R6), or the project is unknown: gh gets reads only
    pub pr_mode: bool,
}

/// What the guard asks git, behind a trait so the rules are tested without repos.
pub trait Repo {
    fn toplevel(&self, dir: &str) -> Option<String>;
    fn head_branch(&self, dir: &str) -> Option<String>;
    fn alias(&self, dir: &str, name: &str) -> Option<String>;
    fn is_rev(&self, dir: &str, arg: &str) -> bool;
}

/// Some(reason) when the command (a Bash line) must not run. The app goes through `check_tool`.
#[cfg_attr(not(test), allow(dead_code))]
pub fn check(cmd: &str, ctx: &Ctx, repo: &dyn Repo) -> Option<String> {
    check_in(cmd, false, ctx, repo)
}

/// A tool call: the command of Bash, PowerShell and Monitor (a command run in Bash's shell); every other tool passes.
pub fn check_tool(tool: &str, input: &Value, ctx: &Ctx, repo: &dyn Repo) -> Option<String> {
    let ps = shell_tool(tool)?;
    check_in(input["command"].as_str()?, ps, ctx, repo)
}

/// The tools that run a command line: Some(true) for PowerShell's.
pub fn shell_tool(tool: &str) -> Option<bool> {
    match tool {
        "Bash" | "Monitor" => Some(false),
        "PowerShell" => Some(true),
        _ => None,
    }
}

fn check_in(cmd: &str, ps: bool, ctx: &Ctx, repo: &dyn Repo) -> Option<String> {
    // nothing to look for: no work at all
    if !GUARD || !worth_checking(cmd, ps) {
        return None;
    }
    let g = Guard { ctx, repo, gitish: Cell::new(false) };
    let cwd = ctx.cwd.and_then(|c| resolve(None, c));
    g.shell(cmd, ps, cwd, false, false, 0).err()
}

/// Whether a command line may run git, so the full (fail-closed) check is worth it: a word names git or gh (quotes
/// and escapes don't hide it: `g''it`, `g\it`), or a command whose name isn't known here (`${G}t`, `& $g`) comes
/// with a git verb or a variable set to something like git. Other lines run unchecked, so a command the reader
/// can't follow but that has nothing to do with git (`$PY highlight.py`) isn't denied.
fn worth_checking(cmd: &str, ps: bool) -> bool {
    let bare: String = cmd.chars().filter(|c| !"'\"\\`".contains(*c)).collect();
    if names_git(cmd) || names_git(&bare) {
        return true;
    }
    let Ok(toks) = lex(cmd, ps) else { return false };
    let words: Vec<&Word> = toks.iter().filter_map(|t| if let Tok::Word(w) = t { Some(w) } else { None }).collect();
    words.iter().any(|w| names_git(&w.text)) || (words.iter().any(|w| w.dynamic || (ps && is_call(w))) && (words.iter().any(|w| has_verb(&w.text)) || gitish(&words)))
}

/// Text that names git or gh as a whole word (`git`, `GIT.EXE`, `/usr/bin/git`, `git-lfs`), not inside another
/// one (`high`, `github`, `.git`, `digit`).
fn names_git(text: &str) -> bool {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || "_.-".contains(c))).any(|t| {
        let p = program(t);
        p == "git" || p == "gh" || p.starts_with("git-")
    })
}

/// git subcommands the guard has rules for: with a command it can't name, they make it deny.
const VERBS: &[&str] = &[
    "push", "merge", "reset", "clean", "branch", "checkout", "switch", "rebase", "pull", "stash", "update-ref", "worktree", "cherry-pick", "revert", "am",
    "restore", "commit", "fetch", "config", "send-pack", "http-push", "symbolic-ref", "filter-branch", "submodule", "bisect", "replace",
];

/// A word holds one of `VERBS` (as a whole word: `{git,push}` does, `merge.txt` doesn't).
fn has_verb(text: &str) -> bool {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-')).any(|t| VERBS.contains(&t.to_ascii_lowercase().as_str()))
}

/// PowerShell's call operators in command position: `& <command>`, `. <command>`.
fn is_call(w: &Word) -> bool {
    !w.dynamic && (w.text == "&" || w.text == ".")
}

/// A variable is set to git or a piece of it (`G=gi`, `$g = 'git'`, `$g = 'g'+'it'`): a command named by a variable
/// may then be git.
fn gitish(words: &[&Word]) -> bool {
    words.iter().enumerate().any(|(k, w)| {
        let Some((_, v)) = w.text.split_once('=') else { return false };
        let v = if v.is_empty() { words.get(k + 1).map(|n| n.text.as_str()).unwrap_or("") } else { v };
        let v: String = v.chars().filter(|c| !"+ ,()".contains(*c)).collect::<String>().to_ascii_lowercase();
        !v.is_empty() && ("git".contains(&v) || "gh".contains(&v) || names_git(&v))
    })
}

/// The check for a running agent (`/pretool` hook, the ACP permission path). `worktree` is the agent's folder,
/// `cwd` the shell's, when the hook says.
pub fn live_check(app: &AppHandle, worktree: &str, cwd: Option<&str>, tool: &str, input: &Value) -> Option<String> {
    let Some(ps) = shell_tool(tool) else { return None };
    if !GUARD || !input["command"].as_str().is_some_and(|c| worth_checking(c, ps)) {
        return None;
    }
    // any lookup that fails leaves its part None: strict
    let root = project_root(app, worktree);
    let base = root.as_deref().map(|r| crate::base_branch(app, r)).filter(|b| b != "HEAD" && !b.is_empty());
    let merges = app.state::<crate::merge::Merges>();
    let merging = root.as_deref().is_some_and(|r| merges.merging(r));
    // only the agent's own project (every project when it is unknown)
    let locked = merges.committing(root.as_deref());
    let cwd = cwd.filter(|c| !c.is_empty()).or(Some(worktree).filter(|w| !w.is_empty()));
    let pr_mode = root.as_deref().is_none_or(|r| crate::home(app).map_or(true, |h| crate::project::merge_mode(&h, r).is_pr()));
    let ctx = Ctx { cwd, root: root.as_deref(), base: base.as_deref(), merging, locked: &locked, pr_mode };
    let why = check_tool(tool, input, &ctx, &GitRepo::default())?;
    eprintln!("[maestro] guard denied in {worktree}: {}: {why}", input["command"].as_str().unwrap_or(""));
    Some(format!("Maestro가 막았어요: {why}"))
}

/// The registered project the worktree belongs to (the parent of its shared `.git`).
fn project_root(app: &AppHandle, worktree: &str) -> Option<String> {
    if worktree.is_empty() {
        return None;
    }
    let main = worktree::main_checkout(worktree)?;
    crate::read_repos(app).ok()?.into_iter().find(|r| r.replace('\\', "/").trim_end_matches('/').eq_ignore_ascii_case(main.trim_end_matches('/')))
}

/// git itself, each answer asked once per check.
#[derive(Default)]
struct GitRepo {
    cache: RefCell<HashMap<String, Option<String>>>,
}

impl GitRepo {
    fn cached(&self, key: String, f: impl FnOnce() -> Option<String>) -> Option<String> {
        if let Some(v) = self.cache.borrow().get(&key) {
            return v.clone();
        }
        let v = f();
        self.cache.borrow_mut().insert(key, v.clone());
        v
    }
}

impl Repo for GitRepo {
    fn toplevel(&self, dir: &str) -> Option<String> {
        self.cached(format!("top\0{dir}"), || worktree::toplevel(dir).ok())
    }
    fn head_branch(&self, dir: &str) -> Option<String> {
        self.cached(format!("head\0{dir}"), || worktree::head_branch(dir))
    }
    fn alias(&self, dir: &str, name: &str) -> Option<String> {
        self.cached(format!("alias\0{dir}\0{name}"), || worktree::alias(dir, name))
    }
    fn is_rev(&self, dir: &str, arg: &str) -> bool {
        self.cached(format!("rev\0{dir}\0{arg}"), || worktree::is_rev(dir, arg).then(String::new)).is_some()
    }
}

// ---------- reading a command line ----------

#[derive(Debug, Clone, PartialEq)]
struct Word {
    text: String,
    /// holds a variable or a substitution: its value isn't known here
    dynamic: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Word(Word),
    /// a heredoc's (or here-string's) text: data, looked at only by the indirect net
    Data(String),
    /// `;` `&&` `||` newline `{` `}`
    Seq,
    /// `|` and a lone `&` (the other side runs apart: a `cd` there doesn't move this shell); in PowerShell a `&` at the
    /// start of a command is its call operator, a word
    Pipe,
    Open,
    Close,
}

#[derive(Default)]
struct Lex {
    toks: Vec<Tok>,
    word: String,
    started: bool,
    dynamic: bool,
    /// the next word is a redirect's target, not an argument
    skip: bool,
    /// that target is a here-string (`<<< word`): kept as data
    here: bool,
    /// `$( )` and backtick bodies met in the current command, run as subshells after it
    subs: Vec<Vec<Tok>>,
    /// an unquoted `{` in the word (bash): `{git,push}` is brace expansion
    brace: bool,
    /// PowerShell: `@args` (splatting) and `@( )` are values not known here
    ps: bool,
}

impl Lex {
    fn end_word(&mut self) {
        if self.started {
            let text = std::mem::take(&mut self.word);
            if self.skip {
                self.skip = false;
                if std::mem::take(&mut self.here) {
                    self.toks.push(Tok::Data(text));
                }
            } else {
                let expands = self.brace && text.contains('}') && (text.contains(',') || text.contains(".."));
                let dynamic = self.dynamic || expands || text.matches('%').count() >= 2 || (self.ps && text.starts_with('@'));
                self.toks.push(Tok::Word(Word { text, dynamic }));
            }
        }
        self.word.clear();
        self.started = false;
        self.dynamic = false;
        self.brace = false;
    }

    /// At the start of a command: nothing of it read yet.
    fn command_position(&self) -> bool {
        !self.started && !matches!(self.toks.last(), Some(Tok::Word(_)))
    }

    fn sep(&mut self, t: Tok) {
        self.end_word();
        self.skip = false;
        self.here = false;
        for sub in std::mem::take(&mut self.subs) {
            self.toks.extend([Tok::Seq, Tok::Open]);
            self.toks.extend(sub);
            self.toks.push(Tok::Close);
        }
        self.toks.push(t);
    }

    fn push(&mut self, ch: char) {
        self.started = true;
        self.word.push(ch);
    }
}

fn lex(s: &str, ps: bool) -> Result<Vec<Tok>, ()> {
    let c: Vec<char> = s.chars().collect();
    lex_at(&c, 0, false, ps, 0).map(|(t, _)| t)
}

/// Lexes from `i`; `until_paren`: the body of `$(`, ending at its `)` (returned: the index after it).
fn lex_at(c: &[char], mut i: usize, until_paren: bool, ps: bool, nest: usize) -> Result<(Vec<Tok>, usize), ()> {
    if nest > 8 {
        return Err(());
    }
    let mut l = Lex { ps, ..Lex::default() };
    let mut depth = 0usize;
    // (delimiter, `<<-`, quoted delimiter, where its text goes: in the command that has the `<<`)
    let mut heredocs: Vec<(String, bool, bool, usize)> = vec![];
    while i < c.len() {
        let ch = c[i];
        match ch {
            ' ' | '\t' | '\r' => {
                l.end_word();
                i += 1;
            }
            '\n' => {
                l.end_word();
                i += 1;
                for (shift, (delim, dash, quoted, at)) in std::mem::take(&mut heredocs).into_iter().enumerate() {
                    let (body, next) = heredoc_body(c, i, &delim, dash);
                    // an unquoted delimiter: the shell runs `$( )` and backticks in the text while feeding it
                    if !quoted {
                        heredoc_substitutions(&mut l, &body, ps, nest)?;
                    }
                    l.toks.insert(at + shift, Tok::Data(body));
                    i = next;
                }
                l.sep(Tok::Seq);
            }
            ';' => {
                l.sep(Tok::Seq);
                i += 1;
            }
            '&' if c.get(i + 1) == Some(&'&') => {
                l.sep(Tok::Seq);
                i += 2;
            }
            '&' if c.get(i + 1) == Some(&'>') => {
                l.end_word();
                i += 2;
                if c.get(i) == Some(&'>') {
                    i += 1;
                }
                l.skip = true;
            }
            // PowerShell's call operator: a marker word, and `& (expr)` a command named by an expression
            '&' if ps && l.command_position() => {
                l.toks.push(Tok::Word(Word { text: "&".into(), dynamic: false }));
                i += 1;
                let mut j = i;
                while matches!(c.get(j), Some(' ' | '\t')) {
                    j += 1;
                }
                if c.get(j) == Some(&'(') {
                    l.started = true;
                    i = subst(&mut l, c, j + 1, ps, nest)?;
                }
            }
            '&' => {
                l.sep(Tok::Pipe);
                i += 1;
            }
            '|' if c.get(i + 1) == Some(&'|') => {
                l.sep(Tok::Seq);
                i += 2;
            }
            '|' => {
                l.sep(Tok::Pipe);
                i += if c.get(i + 1) == Some(&'&') { 2 } else { 1 };
            }
            '(' => {
                depth += 1;
                l.sep(Tok::Open);
                i += 1;
            }
            ')' if depth == 0 && until_paren => {
                l.sep(Tok::Seq);
                return Ok((l.toks, i + 1));
            }
            ')' => {
                depth = depth.checked_sub(1).ok_or(())?;
                l.sep(Tok::Close);
                i += 1;
            }
            // a group or script block; in bash `{` opens one only before a blank (`{}` and `{a,b}` are words)
            '{' if !l.started && (ps || matches!(c.get(i + 1), None | Some(' ' | '\t' | '\r' | '\n'))) => {
                l.sep(Tok::Seq);
                i += 1;
            }
            '}' if !l.started => {
                l.sep(Tok::Seq);
                i += 1;
            }
            '<' if c.get(i + 1) == Some(&'<') && c.get(i + 2) != Some(&'<') => {
                // a heredoc: its text is skipped at the end of the line
                drop_fd(&mut l);
                i += 2;
                let dash = c.get(i) == Some(&'-');
                if dash {
                    i += 1;
                }
                while matches!(c.get(i), Some(' ' | '\t')) {
                    i += 1;
                }
                let mut delim = String::new();
                let mut quoted = false;
                while let Some(&x) = c.get(i) {
                    match x {
                        '\'' | '"' => {
                            quoted = true;
                            i += 1;
                            while let Some(&y) = c.get(i) {
                                i += 1;
                                if y == x {
                                    break;
                                }
                                delim.push(y);
                            }
                        }
                        '\\' => {
                            quoted = true;
                            if let Some(&y) = c.get(i + 1) {
                                delim.push(y);
                            }
                            i += 2;
                        }
                        ' ' | '\t' | '\r' | '\n' | ';' | '&' | '|' | '<' | '>' | '(' | ')' => break,
                        _ => {
                            delim.push(x);
                            i += 1;
                        }
                    }
                }
                if delim.is_empty() {
                    return Err(());
                }
                heredocs.push((delim, dash, quoted, l.toks.len()));
            }
            '>' | '<' => {
                drop_fd(&mut l);
                // `<<<`: a here-string, its word fed to the command's stdin
                let here = ch == '<' && c.get(i + 1) == Some(&'<') && c.get(i + 2) == Some(&'<');
                i += 1;
                while matches!(c.get(i), Some('>' | '<' | '|')) {
                    i += 1;
                }
                if c.get(i) == Some(&'&') {
                    i += 1;
                    while c.get(i).is_some_and(|c| c.is_ascii_digit() || *c == '-') {
                        i += 1;
                    }
                } else if c.get(i) != Some(&'(') {
                    l.skip = true;
                    l.here = here;
                }
            }
            '#' if !l.started => {
                while i < c.len() && c[i] != '\n' {
                    i += 1;
                }
            }
            // PowerShell here-string: @' … '@ or @" … "@, the closing mark at the start of a line
            '@' if ps && !l.started && matches!(c.get(i + 1), Some('\'' | '"')) && matches!(c.get(i + 2), Some('\n' | '\r')) => {
                let q = c[i + 1];
                let mut j = i + 2;
                let mut body = String::new();
                loop {
                    let start = j + usize::from(c.get(j) == Some(&'\r')) + 1;
                    let end = (start..c.len()).find(|&k| c[k] == '\n').unwrap_or(c.len());
                    if start > c.len() {
                        return Err(());
                    }
                    let line: String = c[start..end].iter().collect();
                    if line.starts_with(q) && line[1..].starts_with('@') {
                        j = start + 2;
                        break;
                    }
                    body.push_str(&line);
                    body.push('\n');
                    if end >= c.len() {
                        return Err(());
                    }
                    j = end;
                }
                l.toks.push(Tok::Data(body));
                l.started = true;
                l.dynamic = true;
                i = j;
            }
            '\'' => {
                l.started = true;
                let end = (i + 1..c.len()).find(|&j| c[j] == '\'').ok_or(())?;
                l.word.extend(&c[i + 1..end]);
                i = end + 1;
            }
            '"' => {
                l.started = true;
                i += 1;
                loop {
                    match c.get(i) {
                        None => return Err(()),
                        Some('"') => {
                            i += 1;
                            break;
                        }
                        Some('`') if ps => {
                            if let Some(&n) = c.get(i + 1) {
                                l.word.push(n);
                            }
                            i += 2;
                        }
                        Some('\\') if !ps && matches!(c.get(i + 1), Some('"' | '\\' | '$' | '`')) => {
                            l.word.push(c[i + 1]);
                            i += 2;
                        }
                        Some('\\') if !ps && c.get(i + 1) == Some(&'\n') => i += 2,
                        Some('$') if c.get(i + 1) == Some(&'(') => i = subst(&mut l, c, i + 2, ps, nest)?,
                        Some('`') => i = backtick(&mut l, c, i + 1, ps, nest)?,
                        Some('$') => {
                            l.dynamic = true;
                            l.word.push('$');
                            i += 1;
                        }
                        Some(&x) => {
                            l.word.push(x);
                            i += 1;
                        }
                    }
                }
            }
            '$' if !ps && c.get(i + 1) == Some(&'\'') => {
                l.started = true;
                i += 2;
                loop {
                    match c.get(i) {
                        None => return Err(()),
                        Some('\'') => {
                            i += 1;
                            break;
                        }
                        Some('\\') if i + 1 < c.len() => i = ansi_escape(&mut l, c, i + 1),
                        Some(&x) => {
                            l.word.push(x);
                            i += 1;
                        }
                    }
                }
            }
            '$' if c.get(i + 1) == Some(&'(') => {
                l.started = true;
                i = subst(&mut l, c, i + 2, ps, nest)?;
            }
            '`' if ps => {
                // PowerShell's escape (and line continuation)
                match c.get(i + 1) {
                    Some('\n') => {}
                    Some(&n) => l.push(n),
                    None => {}
                }
                i += 2;
            }
            '`' => {
                l.started = true;
                i = backtick(&mut l, c, i + 1, ps, nest)?;
            }
            '$' => {
                l.dynamic = true;
                l.push('$');
                i += 1;
            }
            '\\' if !ps => match c.get(i + 1) {
                Some('\n') => i += 2,
                Some(&n) if " \t\"'\\$`;&|()<>{}#*?!".contains(n) => {
                    l.push(n);
                    i += 2;
                }
                // a Windows path (`C:\p`, `..\x`, `a\b\c`): the backslash stays
                Some(_) if l.word.contains([':', '\\', '/']) || l.word.ends_with('.') => {
                    l.push('\\');
                    i += 1;
                }
                // bash drops it: `g\it` is git, `\git` git without aliases
                Some(&n) => {
                    l.push(n);
                    i += 2;
                }
                None => {
                    l.push('\\');
                    i += 1;
                }
            },
            '{' if !ps => {
                l.push(ch);
                l.brace = true;
                i += 1;
            }
            _ => {
                l.push(ch);
                i += 1;
            }
        }
    }
    if until_paren || depth != 0 {
        return Err(());
    }
    // a heredoc whose text never came: nothing to skip
    l.sep(Tok::Seq);
    Ok((l.toks, i))
}

/// One escape of a bash `$'…'` string, `i` at the character after the backslash: decoded into the word (`\x67` is
/// `g`); one it can't decode makes the word dynamic. Returns the index after it.
fn ansi_escape(l: &mut Lex, c: &[char], mut i: usize) -> usize {
    let n = c[i];
    i += 1;
    let mut num = |radix: u32, max: usize, first: Option<u32>| -> Option<char> {
        let mut v = first.unwrap_or(0);
        let mut got = usize::from(first.is_some());
        while got < max {
            let Some(d) = c.get(i).and_then(|ch| ch.to_digit(radix)) else { break };
            v = v * radix + d;
            got += 1;
            i += 1;
        }
        (got > 0).then(|| char::from_u32(v)).flatten()
    };
    let ch = match n {
        'n' => Some('\n'),
        't' => Some('\t'),
        'r' => Some('\r'),
        'a' => Some('\u{7}'),
        'b' => Some('\u{8}'),
        'e' | 'E' => Some('\u{1b}'),
        'f' => Some('\u{c}'),
        'v' => Some('\u{b}'),
        'x' => num(16, 2, None),
        'u' => num(16, 4, None),
        'U' => num(16, 8, None),
        '0'..='7' => num(8, 3, n.to_digit(8)),
        // `\cX`, a control character: not followed
        'c' => None,
        other => Some(other),
    };
    match ch {
        Some(ch) => l.word.push(ch),
        None => l.dynamic = true,
    }
    i
}

/// Before a redirect: a lone fd number (`2>`) isn't an argument.
fn drop_fd(l: &mut Lex) {
    if l.started && !l.dynamic && l.word.chars().all(|c| c.is_ascii_digit()) {
        l.word.clear();
        l.started = false;
    } else {
        l.end_word();
    }
}

/// A heredoc's lines from `i` up to its delimiter line: (text, index after the delimiter line).
fn heredoc_body(c: &[char], mut i: usize, delim: &str, dash: bool) -> (String, usize) {
    let mut body = String::new();
    while i < c.len() {
        let end = (i..c.len()).find(|&k| c[k] == '\n').unwrap_or(c.len());
        let line: String = c[i..end].iter().collect();
        i = (end + 1).min(c.len());
        let l = line.trim_end_matches('\r');
        let l = if dash { l.trim_start_matches('\t') } else { l };
        if l == delim {
            break;
        }
        body.push_str(&line);
        body.push('\n');
    }
    (body, i)
}

/// The `$( )` and backtick substitutions in an unquoted heredoc's text: run by the shell, so read as subshells. One
/// that can't be read only fails the line when the text names git.
fn heredoc_substitutions(l: &mut Lex, body: &str, ps: bool, nest: usize) -> Result<(), ()> {
    let c: Vec<char> = body.chars().collect();
    let mut k = 0;
    while k < c.len() {
        let read = match c[k] {
            '\\' => {
                k += 2;
                continue;
            }
            '$' if c.get(k + 1) == Some(&'(') => subst(l, &c, k + 2, ps, nest),
            '`' => backtick(l, &c, k + 1, ps, nest),
            _ => {
                k += 1;
                continue;
            }
        };
        match read {
            Ok(end) => k = end,
            Err(()) if names_git(body) => return Err(()),
            Err(()) => return Ok(()),
        }
    }
    Ok(())
}

fn subst(l: &mut Lex, c: &[char], start: usize, ps: bool, nest: usize) -> Result<usize, ()> {
    let (toks, end) = lex_at(c, start, true, ps, nest + 1)?;
    l.subs.push(toks);
    l.dynamic = true;
    Ok(end)
}

fn backtick(l: &mut Lex, c: &[char], start: usize, ps: bool, nest: usize) -> Result<usize, ()> {
    let mut j = start;
    while j < c.len() && c[j] != '`' {
        j += if c[j] == '\\' { 2 } else { 1 };
    }
    if j >= c.len() {
        return Err(());
    }
    let (toks, _) = lex_at(&c[start..j], 0, false, ps, nest + 1)?;
    l.subs.push(toks);
    l.dynamic = true;
    Ok(j + 1)
}

/// The commands of a token list: their words and heredoc texts (the lexer tests read tokens this way).
#[cfg(test)]
fn segments(toks: &[Tok]) -> Vec<(Vec<&Word>, Vec<&str>)> {
    let mut out = vec![];
    let (mut words, mut data) = (vec![], vec![]);
    for t in toks {
        match t {
            Tok::Word(w) => words.push(w),
            Tok::Data(d) => data.push(d.as_str()),
            _ => {
                if !words.is_empty() || !data.is_empty() {
                    out.push((std::mem::take(&mut words), std::mem::take(&mut data)));
                }
            }
        }
    }
    out
}

// ---------- paths ----------

/// `p` from `base` as an absolute path with forward slashes, `.` and `..` resolved (MSYS `/c/x` is `C:/x`).
/// None when it depends on an unknown base.
fn resolve(base: Option<&str>, p: &str) -> Option<String> {
    let p = msys(&p.replace('\\', "/"));
    let abs = p.starts_with('/') || (p.len() >= 2 && p.as_bytes()[1] == b':' && p.as_bytes()[0].is_ascii_alphabetic());
    let full = if abs { p } else { format!("{}/{p}", base?) };
    let (prefix, rest) = if full.len() >= 2 && full.as_bytes()[1] == b':' { (full[..2].to_ascii_uppercase(), &full[2..]) } else { (String::new(), full.as_str()) };
    let mut parts: Vec<&str> = vec![];
    for seg in rest.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    Some(format!("{prefix}/{}", parts.join("/")))
}

fn msys(p: &str) -> String {
    let q = p.strip_prefix("/cygdrive").unwrap_or(p);
    let b = q.as_bytes();
    if b.len() >= 2 && b[0] == b'/' && b[1].is_ascii_alphabetic() && (b.len() == 2 || b[2] == b'/') {
        format!("{}:{}", (b[1] as char).to_ascii_uppercase(), if b.len() == 2 { "/" } else { &q[2..] })
    } else {
        p.to_string()
    }
}

/// `dir` is `top` or inside it (any slash spelling, case ignored).
fn within(dir: &str, top: &str) -> bool {
    match (resolve(None, dir), resolve(None, top)) {
        (Some(d), Some(t)) => {
            let (d, t) = (d.to_lowercase(), t.trim_end_matches('/').to_lowercase());
            d == t || d.starts_with(&format!("{t}/"))
        }
        _ => false,
    }
}

fn eq_path(a: &str, b: &str) -> bool {
    match (resolve(None, a), resolve(None, b)) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(&b),
        _ => false,
    }
}

// ---------- judging the commands ----------

/// The executable a word names: its file name, lowercased, without .exe/.cmd/.bat.
fn program(w: &str) -> String {
    let t = w.replace('\\', "/");
    let name = t.rsplit('/').next().unwrap_or(&t).to_lowercase();
    for ext in [".exe", ".cmd", ".bat", ".com"] {
        if let Some(b) = name.strip_suffix(ext) {
            return b.to_string();
        }
    }
    name
}

/// A word naming git or gh as a program (`git`, `/usr/bin/git.exe`, `git-push`, `gh`).
fn names_program(w: &str) -> bool {
    let p = program(w);
    p == "git" || p == "gh" || p.starts_with("git-")
}

/// Commands that don't run their arguments: a word naming git among them is text (`man git-push`, `ls git-x`).
/// Any other program is assumed to maybe run one (see `segment`).
const NOT_RUNNERS: &[&str] = &[
    "echo", "printf", "print", "grep", "egrep", "fgrep", "rg", "ag", "ack", "cat", "bat", "less", "more", "head", "tail", "wc", "ls", "dir", "tree", "file",
    "stat", "du", "sort", "uniq", "cut", "tr", "diff", "cmp", "man", "help", "info", "tldr", "apropos", "which", "where", "whereis", "type", "test", "[", "[[",
    "touch", "mkdir", "rm", "rmdir", "cp", "mv", "ln", "chmod", "basename", "dirname", "realpath", "readlink", "read", "printenv", "sed", "awk", "jq", "code",
    "notepad", "vim", "vi", "nano", "write-host", "write-output", "write-error", "write-verbose", "write-warning", "get-command", "gcm", "get-help",
    "select-string", "sls", "get-content", "gc", "set-content", "add-content", "out-file", "get-childitem", "gci", "test-path", "remove-item", "copy-item",
    "move-item", "new-item",
];

/// What a command writes into a pipe when it can be read here: its heredoc or here-string text, the words of
/// `echo`/`printf`/`Write-Output`, or (PowerShell) a lone string. None: not known (`cat file`, `curl …`).
fn piped(w: &[&Word], data: &[String], ps: bool) -> Option<String> {
    if !data.is_empty() {
        return Some(data.join("\n"));
    }
    let start = w.iter().position(|x| assignment(&x.text).is_none())?;
    let (first, args) = (w[start], &w[start + 1..]);
    if ps && w.len() == 1 {
        return Some(first.text.clone());
    }
    match program(&first.text).as_str() {
        "echo" | "printf" | "print" | "write-output" | "write" => {
            let args: Vec<&Word> = args.iter().skip_while(|a| matches!(a.text.as_str(), "-n" | "-e" | "-E" | "-ne" | "-en")).copied().collect();
            Some(plain(&args).replace("\\n", "\n").replace("\\t", " "))
        }
        _ => None,
    }
}

/// `NAME=value` → NAME
fn assignment(t: &str) -> Option<&str> {
    let (name, _) = t.split_once('=')?;
    let mut ch = name.chars();
    let first = ch.next()?;
    ((first.is_ascii_alphabetic() || first == '_') && ch.all(|c| c.is_ascii_alphanumeric() || c == '_')).then_some(name)
}

/// git variables that point it at another repo, index or config
fn git_var(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    ["GIT_DIR", "GIT_WORK_TREE", "GIT_COMMON_DIR", "GIT_INDEX_FILE"].contains(&n.as_str()) || config_var(&n)
}

/// ones that can define aliases
fn config_var(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    n.starts_with("GIT_CONFIG") || n.starts_with("GIT_ALIAS")
}

fn alias_key(kv: &str) -> bool {
    kv.trim().to_ascii_lowercase().starts_with("alias.")
}

/// Words joined as the shell they're handed to joins them before reading the line again (`cmd /c`, `eval`,
/// `pwsh -Command`): quotes already gone.
fn plain(words: &[&Word]) -> String {
    words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ")
}

/// Words back into one command line, each kept one word (arguments appended to a shell alias, `env -S`).
fn join(words: &[&Word]) -> String {
    words
        .iter()
        .map(|w| if w.text.is_empty() || w.text.contains(|c: char| c.is_whitespace() || "'\"$`;&|()<>\\".contains(c)) { format!("'{}'", w.text.replace('\'', "'\\''")) } else { w.text.clone() })
        .collect::<Vec<_>>()
        .join(" ")
}

const BUILTINS: &[&str] = &[
    "add", "am", "annotate", "apply", "archive", "bisect", "blame", "branch", "bugreport", "bundle", "cat-file", "check-attr", "check-ignore", "check-mailmap",
    "check-ref-format", "checkout", "checkout-index", "cherry", "cherry-pick", "citool", "clean", "clone", "column", "commit", "commit-graph", "commit-tree",
    "config", "count-objects", "credential", "describe", "diagnose", "diff", "diff-files", "diff-index", "diff-tree", "difftool", "fast-export", "fast-import",
    "fetch", "fetch-pack", "filter-branch", "for-each-ref", "for-each-repo", "format-patch", "fsck", "gc", "get-tar-commit-id", "grep", "gui", "hash-object",
    "help", "hook", "http-push", "index-pack", "init", "instaweb", "interpret-trailers", "log", "ls-files", "ls-remote", "ls-tree", "mailinfo", "mailsplit",
    "maintenance", "merge", "merge-base", "merge-file", "merge-index", "merge-tree", "mergetool", "mktag", "mktree", "multi-pack-index", "mv", "name-rev",
    "notes", "pack-objects", "pack-refs", "patch-id", "prune", "prune-packed", "pull", "push", "range-diff", "read-tree", "rebase", "receive-pack", "reflog",
    "refs", "remote", "repack", "replace", "replay", "request-pull", "rerere", "reset", "restore", "rev-list", "rev-parse", "revert", "rm", "send-email",
    "send-pack", "shortlog", "show", "show-branch", "show-index", "show-ref", "sparse-checkout", "stage", "stash", "status", "stripspace", "submodule",
    "switch", "symbolic-ref", "tag", "unpack-file", "unpack-objects", "update-index", "update-ref", "update-server-info", "upload-archive", "upload-pack",
    "var", "verify-commit", "verify-pack", "verify-tag", "version", "whatchanged", "worktree", "write-tree",
];

/// Read-only subcommands, still allowed in the project folder while the app merges there.
const READ_ONLY: &[&str] = &[
    "status", "log", "diff", "show", "rev-parse", "ls-files", "blame", "cat-file", "rev-list", "merge-base", "ls-tree", "for-each-ref", "describe", "show-ref",
    "shortlog", "grep", "version", "help", "var", "whatchanged",
];

/// Options of a subcommand that take the next word as their value.
fn takes(sub: &str) -> &'static [&'static str] {
    match sub {
        "checkout" => &["-b", "-B", "--orphan", "--pathspec-from-file"],
        "switch" => &["-c", "-C", "--orphan", "--create", "--force-create"],
        "restore" => &["-s", "--source", "--pathspec-from-file"],
        "reset" => &["--pathspec-from-file"],
        "branch" => &["-u", "--set-upstream-to", "--contains", "--no-contains", "--merged", "--no-merged", "--points-at", "--format", "--sort"],
        "commit" => &["-m", "-F", "-C", "-c", "-t", "--author", "--date", "--fixup", "--squash", "--message", "--file", "--template", "--reuse-message", "--reedit-message", "--trailer", "--cleanup"],
        "stash" => &["-m", "--message", "--pathspec-from-file"],
        "fetch" | "pull" => &["--depth", "--deepen", "--shallow-since", "--shallow-exclude", "-j", "--jobs", "--upload-pack", "-o", "--server-option", "--refmap", "--negotiation-tip", "-X", "--strategy-option", "-s", "--strategy"],
        "merge" => &["-m", "-F", "-s", "-X", "--strategy", "--strategy-option", "--file", "--message", "--into-name", "--cleanup"],
        "rebase" => &["--onto", "-s", "--strategy", "-X", "--strategy-option", "-x", "--exec"],
        "difftool" => &["-x", "--extcmd", "-t", "--tool"],
        "clone" => &["-u", "--upload-pack", "-o", "--origin", "-b", "--branch", "-c", "--config", "-j", "--jobs", "--depth", "--reference", "--reference-if-able", "--separate-git-dir", "--template", "--shallow-since", "--shallow-exclude", "--filter", "--server-option", "--bundle-uri", "--ref-format"],
        "ls-remote" => &["--upload-pack", "--exec", "--sort", "-o", "--server-option"],
        "fetch-pack" => &["--upload-pack", "--exec", "--depth", "--shallow-since", "--shallow-exclude"],
        "archive" => &["--remote", "--exec", "-o", "--output", "--prefix", "--format", "--add-file", "--add-virtual-file"],
        "config" => &["-f", "--file", "--blob", "--type", "--default", "--comment", "--value"],
        "worktree" => &["-b", "-B", "--reason", "--expire"],
        "symbolic-ref" => &["-m"],
        "clean" => &["-e", "--exclude"],
        _ => &[],
    }
}

/// A git subcommand's arguments: its flags (short clusters spread: `-fdx` is `-f -d -x`), positionals, what comes
/// after `--`, and the values of the options that take one (`takes`).
struct Args<'w> {
    flags: Vec<String>,
    pos: Vec<&'w Word>,
    paths: Vec<&'w Word>,
    dd: bool,
    /// (option as `flags` has it, its value as a word: `-qxV`, `--exe=V` and `--exec V` alike)
    vals: Vec<(String, Word)>,
}

impl Args<'_> {
    fn has(&self, f: &[&str]) -> bool {
        self.flags.iter().any(|x| f.contains(&x.as_str()))
    }
    fn first(&self) -> Option<&str> {
        self.pos.first().map(|w| w.text.as_str())
    }
    fn values<'s>(&'s self, names: &'s [&str]) -> impl Iterator<Item = &'s Word> {
        self.vals.iter().filter(|(n, _)| names.contains(&n.as_str())).map(|(_, v)| v)
    }
}

/// Options whose value git runs as a shell command, per subcommand (`rebase -x`, `difftool --extcmd`, the
/// `--upload-pack` program of a fetch). Each is in `takes`.
fn runs(sub: &str) -> &'static [&'static str] {
    match sub {
        "rebase" => &["-x", "--exec"],
        "difftool" => &["-x", "--extcmd"],
        "fetch" | "pull" => &["--upload-pack"],
        "clone" => &["-u", "--upload-pack"],
        "ls-remote" | "fetch-pack" => &["--upload-pack", "--exec"],
        "archive" => &["--exec"],
        _ => &[],
    }
}

/// Long options the rules look at, per subcommand: git takes any unambiguous prefix of one (`--ha` is `--hard`).
fn longs(sub: &str) -> &'static [&'static str] {
    match sub {
        "reset" => &["--hard", "--soft", "--mixed", "--merge", "--keep"],
        "clean" => &["--force", "--dry-run"],
        "checkout" => &["--force", "--ignore-other-worktrees", "--detach", "--track", "--patch"],
        "switch" => &["--force", "--discard-changes", "--ignore-other-worktrees"],
        "branch" => &["--delete", "--force", "--move", "--copy", "--unset-upstream", "--edit-description"],
        "commit" => &["--amend"],
        "restore" => &["--source", "--staged", "--worktree"],
        "fetch" => &["--update-head-ok"],
        "symbolic-ref" => &["--delete"],
        "replace" => &["--list", "--delete", "--force", "--edit", "--graft", "--convert-graft-file"],
        "worktree" => &["--force"],
        "merge" | "pull" | "rebase" | "cherry-pick" | "revert" | "am" => &["--abort"],
        _ => &[],
    }
}

/// `--name` as git reads it: a prefix of one of `longs` or of a value-taking option is that option (the first in
/// that order when several fit: git refuses an ambiguous one, so the pick only matters for the deny side).
fn expand(name: String, sub: &str) -> String {
    let all = || longs(sub).iter().chain(takes(sub).iter().filter(|t| t.starts_with("--")));
    if name.len() <= 2 || name.starts_with("--no-") || all().any(|o| *o == name) {
        return name;
    }
    all().find(|o| o.starts_with(name.as_str())).map_or(name, |o| o.to_string())
}

fn parse<'w>(rest: &[&'w Word], sub: &str) -> Args<'w> {
    let takes = takes(sub);
    let mut a = Args { flags: vec![], pos: vec![], paths: vec![], dd: false, vals: vec![] };
    let mut i = 0;
    while i < rest.len() {
        let w = rest[i];
        let t = w.text.as_str();
        i += 1;
        if a.dd {
            a.paths.push(w);
        } else if t == "--" {
            a.dd = true;
        } else if let Some(long) = t.strip_prefix("--") {
            let (n, inline) = match long.split_once('=') {
                Some((n, v)) => (n, Some(v)),
                None => (long, None),
            };
            let name = expand(format!("--{n}"), sub);
            if takes.contains(&name.as_str()) {
                let value = match inline {
                    Some(v) => Some(Word { text: v.to_string(), dynamic: w.dynamic }),
                    None => {
                        i += 1;
                        rest.get(i - 1).map(|v| (*v).clone())
                    }
                };
                a.vals.extend(value.map(|v| (name.clone(), v)));
            }
            a.flags.push(name);
        } else if t.len() > 1 && t.starts_with('-') {
            let chars: Vec<char> = t[1..].chars().collect();
            for (k, ch) in chars.iter().enumerate() {
                let f = format!("-{ch}");
                let value = takes.contains(&f.as_str());
                a.flags.push(f.clone());
                if value {
                    // the rest of the cluster is its value, else the next word
                    let v = if k + 1 == chars.len() {
                        i += 1;
                        rest.get(i - 1).map(|v| (*v).clone())
                    } else {
                        Some(Word { text: chars[k + 1..].iter().collect(), dynamic: w.dynamic })
                    };
                    a.vals.extend(v.map(|v| (f, v)));
                    break;
                }
            }
        } else {
            a.pos.push(w);
        }
    }
    a
}

/// A pathspec for the whole tree.
fn whole_tree(w: &Word) -> bool {
    matches!(w.text.as_str(), "." | "./" | ".\\" | ":/" | ":/." | ":/*" | "*" | ":(top)" | ":(top)." | ":(top)*")
}

/// A fetch refspec writing the base branch (`src:main`, `+src:refs/heads/main`).
fn writes_branch(spec: &str, base: &str) -> bool {
    spec.rsplit_once(':').is_some_and(|(_, dst)| {
        let dst = dst.strip_prefix("refs/heads/").or_else(|| dst.strip_prefix("heads/")).unwrap_or(dst);
        dst == base
    })
}

/// The shell's working directory, per subshell level; None = unknown.
struct State {
    cwd: Vec<Option<String>>,
    /// a git variable was exported for what follows (`export GIT_DIR=…`, `$env:GIT_DIR = …`)
    sticky: bool,
    sticky_config: bool,
    /// what the command being judged reads on stdin, when it's known here (its heredoc or here-string, or
    /// `echo …` piped into it): a shell reading commands from stdin runs it
    stdin: Option<String>,
}

impl State {
    fn cwd(&self) -> Option<String> {
        self.cwd.last().cloned().flatten()
    }
    fn set_cwd(&mut self, v: Option<String>) {
        if let Some(t) = self.cwd.last_mut() {
            *t = v;
        }
    }
}

struct Guard<'a> {
    ctx: &'a Ctx<'a>,
    repo: &'a dyn Repo,
    /// a line read so far sets a variable to something like git (`gitish`)
    gitish: Cell<bool>,
}

/// Variables whose value git runs as a shell command (EDITOR and friends when git's own aren't set).
fn command_var(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    ["GIT_EDITOR", "GIT_SEQUENCE_EDITOR", "GIT_PAGER", "GIT_SSH_COMMAND", "GIT_EXTERNAL_DIFF", "EDITOR", "VISUAL", "PAGER"].contains(&n.as_str())
}

/// git config keys whose value is a command git runs.
fn command_key(k: &str) -> bool {
    [
        "core.editor", "sequence.editor", "core.pager", "core.sshcommand", "diff.external", "core.fsmonitor", "core.askpass", "gpg.program", "core.gitproxy",
        "core.alternaterefscommand", "uploadpack.packobjectshook", "interactive.difffilter",
    ]
    .contains(&k)
        || k.starts_with("pager.")
        || (k.starts_with("submodule.") && k.ends_with(".update"))
        || [".helper", ".textconv", ".cmd", ".driver", ".clean", ".smudge", ".process", ".command", ".program", ".uploadpack", ".receivepack"].iter().any(|s| k.ends_with(s))
}

/// git config keys that bring in other config (aliases among it) or run hooks.
fn config_key(k: &str) -> bool {
    k == "include.path" || k.starts_with("includeif.") || k == "core.hookspath"
}

/// `credential.helper` / `credential.<url>.helper`: git hands every helper the full credential (`store` after a
/// login), so one put in by the agent can print the user's token.
fn credential_helper_key(k: &str) -> bool {
    k.starts_with("credential.") && k.ends_with(".helper")
}

/// Config set through the environment: `GIT_CONFIG_KEY_<n>` names a key as `-c` would, judged by its key alone (its
/// value sits in another variable): an alias or a credential helper is refused as on the command line, and a key whose
/// value git runs (or that lets `ext::` through) can't be read. `GIT_CONFIG_PARAMETERS` is git's own quoting of `-c`
/// pairs: nothing an agent writes by hand, refused. A value in a variable can't be read either.
fn env_config(name: &str, w: &Word) -> Result<(), String> {
    let n = name.to_ascii_uppercase();
    let key = n.starts_with("GIT_CONFIG_KEY_");
    let value = n.starts_with("GIT_CONFIG_VALUE_");
    if !(key || value || n == "GIT_CONFIG_PARAMETERS") {
        return Ok(());
    }
    let v = w.text.split_once('=').map_or("", |(_, v)| v).trim().to_ascii_lowercase();
    if n == "GIT_CONFIG_PARAMETERS" {
        return Err(if v.contains("alias.") {
            ALIAS
        } else if v.contains("credential.") {
            TOKEN
        } else {
            UNKNOWN
        }
        .into());
    }
    // a value matters only next to a key that makes it a command, judged by that key
    if value {
        return if v.starts_with("ext::") { Err(UNKNOWN.into()) } else { Ok(()) };
    }
    if w.dynamic {
        return Err(UNKNOWN.into());
    }
    if alias_key(&v) {
        return Err(ALIAS.into());
    }
    if credential_helper_key(&v) {
        return Err(TOKEN.into());
    }
    if command_key(&v) || (v.starts_with("protocol.") && v.ends_with(".allow")) || v.contains("ext::") {
        return Err(UNKNOWN.into());
    }
    Ok(())
}

/// The values of an option (`-x V`, `-xV`, `--exec V`, `--exec=V`) before `--`.
fn opt_values(rest: &[&Word], short: &str, long: &str) -> Vec<String> {
    let mut out = vec![];
    let mut i = 0;
    while let Some(w) = rest.get(i) {
        let t = w.text.as_str();
        i += 1;
        if t == "--" {
            break;
        }
        if t == short || t == long {
            out.extend(rest.get(i).map(|v| v.text.clone()));
            i += 1;
        } else if let Some(v) = t.strip_prefix(&format!("{long}=")) {
            out.push(v.to_string());
        } else if let Some(v) = t.strip_prefix(short).filter(|v| !v.is_empty() && !t.starts_with("--")) {
            out.push(v.to_string());
        }
    }
    out
}

impl Guard<'_> {
    /// A command line, in a shell starting in `cwd`. `over`/`config`: git variables set by whoever runs it.
    fn shell(&self, cmd: &str, ps: bool, cwd: Option<String>, over: bool, config: bool, depth: usize) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err(UNPARSED.into());
        }
        let toks = lex(cmd, ps).map_err(|_| UNPARSED.to_string())?;
        let words: Vec<&Word> = toks.iter().filter_map(|t| if let Tok::Word(w) = t { Some(w) } else { None }).collect();
        if gitish(&words) {
            self.gitish.set(true);
        }
        // .NET starting a process (`[Diagnostics.Process]::Start('git', 'push')`): its arguments aren't read here
        // git's traces (GIT_TRACE_CURL, GIT_CURL_VERBOSE) hide the Authorization header unless GIT_TRACE_REDACT turns
        // that off: any line naming it, in any spelling a shell reads (`export`, `env`, `$env:`, quotes inside)
        let bare: String = cmd.chars().filter(|c| !"'\"\\`".contains(*c)).collect();
        if bare.to_ascii_uppercase().contains("GIT_TRACE_REDACT") {
            return Err(TOKEN.into());
        }
        let low = cmd.to_ascii_lowercase();
        if ps && (low.contains("process]::start") || low.contains("processstartinfo")) && (names_git(cmd) || words.iter().any(|w| names_git(&w.text))) {
            return Err(UNKNOWN.into());
        }
        let start = cwd.clone();
        let mut st = State { cwd: vec![cwd], sticky: over, sticky_config: config, stdin: None };
        let mut seg: Vec<Word> = vec![];
        let mut data: Vec<String> = vec![];
        // what the previous command of a pipeline writes into it, when known
        let mut feed: Option<String> = None;
        for t in toks {
            match t {
                Tok::Word(w) => seg.push(w),
                Tok::Data(d) => data.push(d),
                other => {
                    let mut words = std::mem::take(&mut seg);
                    let own = std::mem::take(&mut data);
                    // PowerShell: `git ('push')`: a group after a command's words is an argument, its value unknown here
                    if ps && other == Tok::Open && !words.is_empty() {
                        words.push(Word { text: String::new(), dynamic: true });
                    }
                    let refs: Vec<&Word> = words.iter().collect();
                    // `echo x | (sh)`: the pipe feeds the group
                    if refs.is_empty() && own.is_empty() && other == Tok::Open {
                        let top = st.cwd();
                        st.cwd.push(top);
                        continue;
                    }
                    st.stdin = if own.is_empty() { feed.take() } else { Some(own.join("\n")) };
                    let moved = !refs.is_empty() && self.segment(&refs, &mut st, false, false, ps, depth)?;
                    st.stdin = None;
                    feed = if other == Tok::Pipe { piped(&refs, &own, ps) } else { None };
                    match other {
                        Tok::Pipe if moved => st.set_cwd(None),
                        Tok::Open => {
                            let top = st.cwd();
                            st.cwd.push(top);
                        }
                        Tok::Close if st.cwd.len() > 1 => {
                            st.cwd.pop();
                        }
                        _ => {}
                    }
                }
            }
        }
        // every line read, the nested ones too (`git bisect run python -c …`, `-c core.editor='node -e …'`)
        match indirect(cmd, ps) {
            Some((Hit::Deny, _)) => Err(INDIRECT.into()),
            Some((Hit::Merge, moves)) if moves || self.ctx.merging || self.strict(start.as_deref(), over) => Err(INDIRECT.into()),
            _ => Ok(()),
        }
    }

    /// One command. Ok(true): it moved the working directory.
    fn segment(&self, w: &[&Word], st: &mut State, over: bool, config: bool, ps: bool, depth: usize) -> Result<bool, String> {
        if depth > MAX_DEPTH {
            return Err(UNPARSED.into());
        }
        let (mut over, mut config) = (over || st.sticky, config || st.sticky_config);
        let mut w = w;
        // bash reserved words before a command (`if git merge x`, `then git push`, `! git push`): the command follows
        if !ps {
            while w.first().is_some_and(|x| !x.dynamic && matches!(x.text.as_str(), "if" | "then" | "else" | "elif" | "do" | "while" | "until" | "!" | "coproc" | "{")) {
                w = &w[1..];
            }
            match w.first().map(|x| x.text.as_str()) {
                Some("fi" | "done" | "esac" | "}") if w.len() == 1 => return Ok(false),
                // a loop or case header: its words are values (`for b in $(git branch)`: the substitution is
                // checked on its own)
                Some("for" | "select" | "case") if !w[0].dynamic => return Ok(false),
                _ => {}
            }
        }
        while let Some(name) = w.first().and_then(|x| assignment(&x.text)) {
            if git_var(name) {
                over = true;
                config |= config_var(name);
            }
            self.command_value(name, w[0], st.cwd(), depth)?;
            w = &w[1..];
        }
        let Some(mut first) = w.first() else { return Ok(false) };
        // PowerShell: `$x = <command>`, `$env:GIT_DIR = …`
        if let Some((name, rest)) = ps_assignment(w) {
            if let Some(var) = name.to_ascii_lowercase().strip_prefix("env:") {
                if git_var(var) {
                    st.sticky = true;
                    st.sticky_config |= config_var(var);
                }
                let refs: Vec<&Word> = rest.iter().collect();
                let value = Word { text: format!("{var}={}", plain(&refs)), dynamic: rest.iter().any(|x| x.dynamic) };
                self.command_value(var, &value, st.cwd(), depth)?;
            }
            let rest: Vec<&Word> = rest.iter().collect();
            return self.segment(&rest, st, over, config, ps, depth + 1);
        }
        // PowerShell's call operators: what follows is the command
        let call = ps && is_call(first);
        if call {
            w = &w[1..];
            let Some(f) = w.first() else { return Ok(false) };
            first = f;
        }
        if first.dynamic {
            // PowerShell: `$?`, `$_.Trim()`, `$LASTEXITCODE -ne 0` are values; only `& $x` runs one
            if ps && !call && first.text.starts_with('$') {
                return Ok(false);
            }
            // a command named by a variable: denied when it may be a git call the rules would look at
            if self.gitish.get() || w.iter().any(|a| has_verb(&a.text)) {
                return Err(UNKNOWN.into());
            }
            return Ok(false);
        }
        let mut prog = program(&first.text);
        // bash drops a backslash before a letter, kept by the reader in Windows-looking paths
        if !ps && first.text.contains('\\') {
            let alt = program(&first.text.replace('\\', ""));
            if names_git(&alt) {
                prog = alt;
            }
        }
        let args = &w[1..];
        match prog.as_str() {
            // its arguments are messages, not commands
            "maestro" => Ok(false),
            "export" | "set" | "declare" | "typeset" | "local" => {
                for a in args {
                    if let Some(n) = assignment(&a.text) {
                        if git_var(n) {
                            st.sticky = true;
                            st.sticky_config |= config_var(n);
                        }
                        self.command_value(n, a, st.cwd(), depth)?;
                    }
                }
                Ok(false)
            }
            "start-process" | "saps" | "start" if ps => self.start_process(args, st, over, config, depth),
            // a shell alias for git (`alias p='git push'`, `Set-Alias g git`) would run it under another name later on
            "alias" if !ps => {
                if args.iter().any(|a| a.text.split_once('=').is_some_and(|(_, v)| a.dynamic || names_git(v))) {
                    return Err(ALIAS.into());
                }
                Ok(false)
            }
            "set-alias" | "sal" | "new-alias" | "nal" if ps => {
                if args.iter().any(|a| a.dynamic || names_git(&a.text)) {
                    return Err(ALIAS.into());
                }
                Ok(false)
            }
            // cmd's `start` (Git Bash has one too): options, a title, then the program
            "start" => {
                let mut i = 0;
                while let Some(a) = args.get(i) {
                    match a.text.to_ascii_lowercase().as_str() {
                        "/d" => i += 2,
                        t if t.is_empty() || t.starts_with('/') => i += 1,
                        _ => break,
                    }
                }
                if i >= args.len() {
                    return Ok(false);
                }
                self.segment(&args[i..], st, over, config, false, depth + 1).map(|_| false)
            }
            "find" => {
                let mut i = 0;
                while i < args.len() {
                    if matches!(args[i].text.as_str(), "-exec" | "-execdir" | "-ok" | "-okdir") {
                        let end = (i + 1..args.len()).find(|&k| matches!(args[k].text.as_str(), ";" | "+")).unwrap_or(args.len());
                        if end > i + 1 {
                            self.segment(&args[i + 1..end], st, over, config, false, depth + 1)?;
                        }
                        i = end;
                    }
                    i += 1;
                }
                Ok(false)
            }
            // `watch CMD…` hands the words to `sh -c`
            "watch" => {
                let mut i = 0;
                while let Some(a) = args.get(i).filter(|a| a.text.len() > 1 && a.text.starts_with('-')) {
                    i += if matches!(a.text.as_str(), "-n" | "--interval" | "-q" | "--equexit") { 2 } else { 1 };
                }
                if i >= args.len() {
                    return Ok(false);
                }
                self.shell(&plain(&args[i..]), false, st.cwd(), over, config, depth + 1).map(|_| false)
            }
            "script" => {
                for v in opt_values(args, "-c", "--command") {
                    self.shell(&v, false, st.cwd(), over, config, depth + 1)?;
                }
                Ok(false)
            }
            "cd" | "pushd" | "chdir" | "set-location" | "sl" | "push-location" => {
                let target = args.iter().find(|a| !(a.text.len() > 1 && a.text.starts_with('-')) && !a.text.eq_ignore_ascii_case("/d"));
                let new = match target {
                    Some(t) if !t.dynamic && !t.text.starts_with('~') && t.text != "-" => resolve(st.cwd().as_deref(), &t.text),
                    _ => None,
                };
                st.set_cwd(new);
                Ok(true)
            }
            "popd" | "pop-location" => {
                st.set_cwd(None);
                Ok(true)
            }
            "env" => self.env(args, st, over, config, ps, depth),
            "command" if args.iter().any(|a| a.text == "-v" || a.text == "-V") => Ok(false),
            "xargs" => self.xargs(args, st, over, config, ps, depth),
            "wsl" => self.wsl(args, st, over, config, depth),
            "command" | "builtin" | "exec" | "nohup" | "time" | "winpty" | "nice" | "timeout" | "stdbuf" | "sudo" | "doas" | "call" | "setsid" | "ionice" | "taskset" | "chronic"
            | "unbuffer" => {
                let rest = skip_wrapper(&prog, args);
                if rest.is_empty() {
                    return Ok(false);
                }
                self.segment(rest, st, over, config, ps, depth + 1)
            }
            "flock" => self.flock(args, st, over, config, ps, depth),
            "sh" | "bash" | "zsh" | "dash" | "ksh" | "ash" | "mksh" => match shell_c(args) {
                Some(s) => self.shell(&s, false, st.cwd(), over, config, depth + 1).map(|_| false),
                None if shell_stdin(args) => self.fed(st, false, over, config, depth),
                None => Ok(false),
            },
            "pwsh" | "powershell" => match pwsh_c(args) {
                Ok(Some(s)) if s.trim() == "-" => self.fed(st, true, over, config, depth),
                Ok(Some(s)) => self.shell(&s, true, st.cwd(), over, config, depth + 1).map(|_| false),
                Ok(None) => self.fed(st, true, over, config, depth),
                Err(()) => Err(UNPARSED.into()),
            },
            "cmd" => match args.iter().position(|a| matches!(a.text.to_ascii_lowercase().as_str(), "/c" | "/k" | "/r")) {
                Some(k) => self.shell(&plain(&args[k + 1..]), false, st.cwd(), over, config, depth + 1).map(|_| false),
                None => self.fed(st, false, over, config, depth),
            },
            // PowerShell: `'git push' | iex` runs what the pipe brings
            "invoke-expression" | "iex" if ps && args.is_empty() => self.fed(st, true, over, config, depth),
            "eval" | "invoke-expression" | "iex" => self.shell(&plain(args), ps, st.cwd(), over, config, depth + 1).map(|_| false),
            // `gh pr checkout` switches the folder's branch: like `git switch`, not in the project folder
            "gh" => match gh(args, self.ctx.pr_mode)? {
                Gh::Checkout if self.strict(st.cwd().as_deref(), over) => Err(BASE.into()),
                // it fetches and switches branches in the folder, as a git write would
                Gh::Checkout if self.locked(st.cwd().as_deref()) => Err(MERGING.into()),
                _ => Ok(false),
            },
            "git" => self.git(args, st.cwd(), over, config, ps, depth, &mut vec![]).map(|_| false),
            // `git-push` and friends: the dashed form of a subcommand
            p if p.starts_with("git-") => {
                let sub = Word { text: p[4..].to_string(), dynamic: false };
                let mut all = vec![&sub];
                all.extend_from_slice(args);
                self.git(&all, st.cwd(), over, config, ps, depth, &mut vec![]).map(|_| false)
            }
            p if NOT_RUNNERS.contains(&p) => Ok(false),
            // a program this doesn't know may run its arguments (`parallel git push ::: x`, a wrapper not listed):
            // from a word naming git or gh, judged as that command
            _ => match args.iter().position(|a| !a.dynamic && names_program(&a.text)) {
                Some(k) => self.segment(&args[k..], st, over, config, ps, depth + 1).map(|_| false),
                None => Ok(false),
            },
        }
    }

    /// A shell reading its commands from stdin: what feeds it, when known here.
    fn fed(&self, st: &mut State, ps: bool, over: bool, config: bool, depth: usize) -> Result<bool, String> {
        match st.stdin.take() {
            Some(text) => self.shell(&text, ps, st.cwd(), over, config, depth + 1).map(|_| false),
            None => Ok(false),
        }
    }

    /// `flock [options] FILE CMD…` or `flock FILE -c 'CMD'`.
    fn flock(&self, args: &[&Word], st: &mut State, over: bool, config: bool, ps: bool, depth: usize) -> Result<bool, String> {
        let mut file = false;
        let mut i = 0;
        while let Some(a) = args.get(i) {
            let t = a.text.as_str();
            match t {
                "-c" | "--command" => {
                    let Some(v) = args.get(i + 1) else { return Ok(false) };
                    return self.shell(&v.text, false, st.cwd(), over, config, depth + 1).map(|_| false);
                }
                _ if t.starts_with("--command=") => return self.shell(&t["--command=".len()..], false, st.cwd(), over, config, depth + 1).map(|_| false),
                "-w" | "--wait" | "--timeout" | "-E" | "--conflict-exit-code" => i += 2,
                "--" => {
                    i += 1;
                    if file {
                        break;
                    }
                }
                _ if t.len() > 1 && t.starts_with('-') => i += 1,
                _ if !file => {
                    file = true;
                    i += 1;
                }
                _ => break,
            }
        }
        let rest = args.get(i..).unwrap_or(&[]);
        if rest.is_empty() {
            return Ok(false);
        }
        self.segment(rest, st, over, config, ps, depth + 1)
    }

    fn env(&self, args: &[&Word], st: &mut State, over: bool, config: bool, ps: bool, depth: usize) -> Result<bool, String> {
        let (mut over, mut config) = (over, config);
        let mut i = 0;
        while let Some(a) = args.get(i) {
            let t = a.text.as_str();
            if let Some(n) = assignment(t) {
                if git_var(n) {
                    over = true;
                    config |= config_var(n);
                }
                self.command_value(n, a, st.cwd(), depth)?;
                i += 1;
                continue;
            }
            match t {
                "-u" | "--unset" => i += 2,
                // another directory just for this command: not followed
                "-C" | "--chdir" => {
                    over = true;
                    i += 2;
                }
                "-S" | "--split-string" => {
                    let mut line = args.get(i + 1).map(|w| w.text.clone()).unwrap_or_default();
                    line = format!("{line} {}", join(args.get(i + 2..).unwrap_or(&[])));
                    return self.shell(&line, ps, st.cwd(), over, config, depth + 1).map(|_| false);
                }
                _ if t.starts_with("-S") || t.starts_with("--split-string=") => {
                    let v = t.strip_prefix("--split-string=").unwrap_or_else(|| &t[2..]);
                    let line = format!("{v} {}", join(&args[i + 1..]));
                    return self.shell(&line, ps, st.cwd(), over, config, depth + 1).map(|_| false);
                }
                _ if t.starts_with("--chdir=") || (t.starts_with("-C") && t.len() > 2) => {
                    over = true;
                    i += 1;
                }
                _ if t.starts_with('-') => i += 1,
                _ => break,
            }
        }
        if i >= args.len() {
            return Ok(false);
        }
        self.segment(&args[i..], st, over, config, ps, depth + 1)
    }

    /// `xargs CMD…`: CMD runs with words read from stdin (or `-a FILE`), unknown here: put where `-I`/`-i` says, else
    /// added at the end (`echo push | xargs git` is `git <unknown>`).
    fn xargs(&self, args: &[&Word], st: &mut State, over: bool, config: bool, ps: bool, depth: usize) -> Result<bool, String> {
        let rest = skip_wrapper("xargs", args);
        if rest.is_empty() {
            return Ok(false);
        }
        let opts = &args[..args.len() - rest.len()];
        let mut repl: Option<String> = None;
        for (k, a) in opts.iter().enumerate() {
            let t = a.text.as_str();
            repl = match t {
                "-I" => opts.get(k + 1).map(|v| v.text.clone()),
                "-i" | "--replace" => Some("{}".into()),
                _ if t.starts_with("--replace=") => Some(t["--replace=".len()..].to_string()),
                _ if t.len() > 2 && (t.starts_with("-I") || t.starts_with("-i")) => Some(t[2..].to_string()),
                _ => repl,
            };
        }
        let mut words: Vec<Word> = rest.iter().map(|w| (*w).clone()).collect();
        match repl.filter(|r| !r.is_empty()) {
            Some(r) => words.iter_mut().filter(|w| w.text.contains(&r)).for_each(|w| w.dynamic = true),
            None => words.push(Word { text: String::new(), dynamic: true }),
        }
        let refs: Vec<&Word> = words.iter().collect();
        self.segment(&refs, st, over, config, ps, depth + 1)
    }

    /// `wsl [options] CMD…`: the words as a line for the distro's shell (`-e`/`--exec`: run as they are), in the same
    /// folder unless `--cd` moves it.
    fn wsl(&self, args: &[&Word], st: &mut State, over: bool, config: bool, depth: usize) -> Result<bool, String> {
        let mut over = over;
        let mut i = 0;
        while let Some(a) = args.get(i) {
            match a.text.as_str() {
                "-e" | "--exec" => {
                    let rest = &args[i + 1..];
                    return if rest.is_empty() { Ok(false) } else { self.segment(rest, st, over, config, false, depth + 1).map(|_| false) };
                }
                "--" => {
                    i += 1;
                    break;
                }
                "-d" | "--distribution" | "-u" | "--user" | "--distribution-id" | "--shell-type" => i += 2,
                "--cd" => {
                    over = true;
                    i += 2;
                }
                t if t.starts_with('-') => i += 1,
                _ => break,
            }
        }
        if i >= args.len() {
            return Ok(false);
        }
        self.shell(&plain(&args[i..]), false, st.cwd(), over, config, depth + 1).map(|_| false)
    }

    /// `NAME=value` for a variable git runs as a command (`GIT_EDITOR='git push;:'`): the value read as a command line.
    /// Every assignment comes here: config set through the environment is checked too (`env_config`).
    fn command_value(&self, name: &str, w: &Word, cwd: Option<String>, depth: usize) -> Result<(), String> {
        env_config(name, w)?;
        if !command_var(name) {
            return Ok(());
        }
        if w.dynamic {
            // git's own can't be told; EDITOR and PAGER are too common to deny on
            return if name.to_ascii_uppercase().starts_with("GIT_") { Err(UNKNOWN.into()) } else { Ok(()) };
        }
        let v = w.text.split_once('=').map_or("", |(_, v)| v);
        if v.trim().is_empty() {
            return Ok(());
        }
        self.shell(v, false, cwd, false, false, depth + 1)
    }

    /// PowerShell's `Start-Process`: the program (`-FilePath` or the first positional) with its arguments
    /// (`-ArgumentList`, or the next positionals; comma lists and the words of a string split), checked as a command.
    fn start_process(&self, args: &[&Word], st: &mut State, over: bool, config: bool, depth: usize) -> Result<bool, String> {
        const SWITCHES: &[&str] = &["wait", "nonewwindow", "passthru", "loaduserprofile", "usenewenvironment"];
        let mut over = over;
        let mut file: Option<Word> = None;
        let mut list: Vec<Word> = vec![];
        // a PowerShell array from `i`: `a, b` lexes as `a,` `b`
        let take_list = |i: &mut usize, list: &mut Vec<Word>| {
            while let Some(w) = args.get(*i) {
                list.push((*w).clone());
                *i += 1;
                if !(w.text.ends_with(',') || args.get(*i).is_some_and(|n| n.text.starts_with(','))) {
                    break;
                }
            }
        };
        let mut i = 0;
        let mut pos = 0;
        while let Some(a) = args.get(i) {
            let t = a.text.to_ascii_lowercase();
            let Some(opt) = t.strip_prefix('-').filter(|o| !o.is_empty() && !a.dynamic) else {
                if pos == 0 && file.is_none() {
                    file = Some((*a).clone());
                    i += 1;
                } else {
                    take_list(&mut i, &mut list);
                }
                pos += 1;
                continue;
            };
            i += 1;
            let (name, inline) = match opt.split_once(':') {
                Some((n, v)) => (n, Some(Word { text: a.text[a.text.len() - v.len()..].to_string(), dynamic: a.dynamic })),
                None => (opt, None),
            };
            let named = |full: &str, min: usize| name.len() >= min && full.starts_with(name);
            if named("filepath", 1) || named("path", 3) || named("pspath", 2) {
                file = inline.or_else(|| args.get(i).map(|w| (*w).clone()));
                i += usize::from(!opt.contains(':'));
            } else if named("argumentlist", 1) || name == "args" {
                match inline {
                    Some(v) => list.push(v),
                    None => take_list(&mut i, &mut list),
                }
            } else if !SWITCHES.iter().any(|s| named(s, 2)) {
                // `-WorkingDirectory` elsewhere: not followed
                if named("workingdirectory", 2) {
                    over = true;
                }
                if inline.is_none() {
                    i += 1;
                }
            }
        }
        let Some(file) = file else { return Ok(false) };
        let mut words = vec![file];
        for w in list {
            for part in w.text.split(|c: char| c == ',' || c.is_whitespace()).filter(|p| !p.is_empty()) {
                words.push(Word { text: part.to_string(), dynamic: w.dynamic });
            }
        }
        let refs: Vec<&Word> = words.iter().collect();
        self.segment(&refs, st, over, config, false, depth + 1).map(|_| false)
    }

    /// `git …`: its global options, the subcommand (aliases followed), then the rules.
    #[allow(clippy::too_many_arguments)]
    fn git(&self, args: &[&Word], cwd: Option<String>, over: bool, config: bool, ps: bool, depth: usize, seen: &mut Vec<String>) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err(UNPARSED.into());
        }
        let mut dir = cwd.clone();
        let (mut over, mut config) = (over, config);
        let mut i = 0;
        while let Some(a) = args.get(i) {
            let t = a.text.as_str();
            match t {
                "-C" => {
                    dir = args.get(i + 1).filter(|p| !p.dynamic).and_then(|p| resolve(dir.as_deref(), &p.text));
                    i += 2;
                }
                "-c" | "--config-env" => {
                    if args.get(i + 1).is_some_and(|kv| kv.dynamic || alias_key(&kv.text)) {
                        return Err(ALIAS.into());
                    }
                    if let Some(kv) = args.get(i + 1) {
                        config |= self.config_opt(&kv.text, t == "--config-env", dir.clone(), depth)?;
                    }
                    i += 2;
                }
                "--git-dir" | "--work-tree" => {
                    over = true;
                    i += 2;
                }
                "--namespace" | "--super-prefix" | "--attr-source" | "--shallow-file" => i += 2,
                _ if t.starts_with("--config-env=") => {
                    let kv = &t["--config-env=".len()..];
                    if alias_key(kv) {
                        return Err(ALIAS.into());
                    }
                    config |= self.config_opt(kv, true, dir.clone(), depth)?;
                    i += 1;
                }
                _ if t.starts_with("--git-dir=") || t.starts_with("--work-tree=") => {
                    over = true;
                    i += 1;
                }
                _ if t.starts_with('-') => i += 1,
                _ => break,
            }
        }
        let Some(subw) = args.get(i) else { return Ok(()) };
        if subw.dynamic {
            return Err(UNKNOWN.into());
        }
        let sub = subw.text.as_str();
        let rest = &args[i + 1..];
        // a credential helper run by hand (`git credential-manager get`, `git-credential-store get`): `get` prints
        // the stored password, with Git Credential Manager or `gh auth setup-git` the GitHub login's token
        if sub.to_ascii_lowercase().starts_with("credential-") && rest.iter().any(|w| w.dynamic || w.text.eq_ignore_ascii_case("get")) {
            return Err(TOKEN.into());
        }
        let low = sub.to_ascii_lowercase();
        // git's remote helpers (`git remote-https`, `git-remote-https.exe`) read their commands on stdin, where
        // `push <refspec>` pushes with the user's credentials
        if low.starts_with("remote-") {
            return Err(if self.ctx.pr_mode { PUSH } else { PUSH_LOCAL }.into());
        }
        // the receiving side of a push, run by hand: it updates this repo's refs from what stdin (or a CGI request) brings
        if matches!(low.as_str(), "receive-pack" | "http-backend") {
            return Err(DESTRUCTIVE.into());
        }
        if !BUILTINS.contains(&sub) {
            let look = dir.clone().or_else(|| self.ctx.cwd.and_then(|c| resolve(None, c))).or_else(|| self.ctx.root.map(String::from));
            let name = low;
            let Some(al) = look.as_deref().and_then(|d| self.repo.alias(d, &name)) else {
                // not an alias: an external `git-<name>` (git-lfs …), unless the config was changed on the way
                if config || look.is_none() || rest.first().is_some_and(|w| w.dynamic) {
                    return Err(UNKNOWN.into());
                }
                // or the value of a global option this doesn't know (`git --new-opt HEAD push`): a git verb after it
                // is judged as the subcommand
                if let Some(k) = rest.iter().position(|w| VERBS.contains(&w.text.as_str()) && BUILTINS.contains(&w.text.as_str())) {
                    if self.rules(&rest[k].text, &rest[k + 1..], dir, over, depth).is_err() {
                        return Err(UNKNOWN.into());
                    }
                }
                return Ok(());
            };
            // the lookup read the config on disk; the command's own (`-c include.path=…`, GIT_CONFIG_GLOBAL, env
            // config) may define this alias otherwise, and git would run that one
            if config {
                return Err(UNKNOWN.into());
            }
            if seen.len() >= 5 || seen.contains(&name) {
                return Err(UNKNOWN.into());
            }
            seen.push(name);
            if let Some(sh) = al.trim().strip_prefix('!') {
                // a shell alias runs from the repo's top folder
                let line = format!("{sh} {}", join(rest));
                return self.shell(&line, false, dir.or(cwd), over, config, depth + 1);
            }
            let toks = lex(&al, false).map_err(|_| UNKNOWN.to_string())?;
            let words: Vec<Word> = toks.into_iter().filter_map(|t| if let Tok::Word(w) = t { Some(w) } else { None }).collect();
            let mut next: Vec<&Word> = args[..i].to_vec();
            next.extend(words.iter());
            next.extend_from_slice(rest);
            return self.git(&next, cwd, over, config, ps, depth + 1, seen);
        }
        self.rules(sub, rest, dir, over, depth)
    }

    /// `git -c key=value` / `--config-env=key=VAR`. A key that runs a command has its value read as a command line
    /// (an env-named one can't be: denied); one that brings in other config or hooks counts as changed config.
    /// Ok(true): the config changed in a way that may define aliases.
    fn config_opt(&self, kv: &str, from_env: bool, dir: Option<String>, depth: usize) -> Result<bool, String> {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        let k = k.trim().to_ascii_lowercase();
        // `protocol.ext.allow` lets an `ext::` URL (a command) through; an `ext::` value is one (`url.ext::….insteadOf`)
        if (k.starts_with("protocol.") && k.ends_with(".allow")) || k.contains("ext::") || v.trim().to_ascii_lowercase().starts_with("ext::") {
            return Err(UNKNOWN.into());
        }
        // not a command to read: the helper is handed the user's token
        if credential_helper_key(&k) {
            return Err(TOKEN.into());
        }
        if command_key(&k) {
            if from_env {
                return Err(UNKNOWN.into());
            }
            let v = v.trim().trim_start_matches('!');
            if !v.is_empty() {
                self.shell(v, false, dir, false, false, depth + 1)?;
            }
            return Ok(true);
        }
        Ok(config_key(&k))
    }

    /// Strict: the project folder, a checkout of the base branch, or a place it can't tell.
    fn strict(&self, dir: Option<&str>, over: bool) -> bool {
        if over {
            return true;
        }
        let (Some(d), Some(root), Some(base)) = (dir, self.ctx.root, self.ctx.base) else { return true };
        let Some(top) = self.repo.toplevel(d) else { return true };
        eq_path(&top, root) || self.repo.head_branch(d).as_deref() == Some(base)
    }

    fn rules(&self, sub: &str, rest: &[&Word], dir: Option<String>, over: bool, depth: usize) -> Result<(), String> {
        let p = parse(rest, sub);
        let base = self.ctx.base;
        let deny = |r: &str| Err(r.to_string());
        // commands git runs itself
        let after = |word: &str| rest.iter().position(|w| w.text == word).map(|k| &rest[k + 1..]).unwrap_or(&[]);
        for v in p.values(runs(sub)) {
            if v.dynamic {
                return deny(UNKNOWN);
            }
            self.shell(&v.text, false, dir.clone(), over, false, depth + 1)?;
        }
        // the `ext::` transport runs its URL as a command
        if rest.iter().any(|w| {
            let t = w.text.to_ascii_lowercase();
            t.starts_with("ext::") || t.split_once('=').is_some_and(|(_, v)| v.starts_with("ext::"))
        }) {
            return deny(UNKNOWN);
        }
        match sub {
            // `clone -c key=value` sets config the clone's own fetch reads
            "clone" => {
                for v in p.values(&["-c", "--config"]) {
                    self.config_opt(&v.text, v.dynamic, dir.clone(), depth)?;
                }
            }
            // `git config key value` for a key whose value git runs later
            "config" => {
                for k in 0..p.pos.len().saturating_sub(1) {
                    let (key, val) = (p.pos[k], p.pos[k + 1]);
                    self.config_opt(&format!("{}={}", key.text, val.text), val.dynamic, dir.clone(), depth)?;
                }
            }
            // runs in each submodule (a folder not known here)
            "submodule" if p.first() == Some("foreach") => {
                let mut cmd = after("foreach");
                while cmd.first().is_some_and(|w| matches!(w.text.as_str(), "--recursive" | "-q" | "--quiet")) {
                    cmd = &cmd[1..];
                }
                self.shell(&plain(cmd), false, None, over, false, depth + 1)?;
            }
            "bisect" if p.first() == Some("run") => self.shell(&join(after("run")), false, dir.clone(), over, false, depth + 1)?,
            _ => {}
        }
        // anywhere
        match sub {
            "push" | "send-pack" | "http-push" => return deny(if self.ctx.pr_mode { PUSH } else { PUSH_LOCAL }),
            "reset" if p.has(&["--hard"]) => return deny(DESTRUCTIVE),
            "clean" if p.has(&["-f", "--force"]) && !p.has(&["-n", "--dry-run"]) => return deny(DESTRUCTIVE),
            "branch" if p.has(&["-D", "-M", "-C", "-f", "--force"]) => return deny(DESTRUCTIVE),
            "branch" if p.has(&["-m", "--move"]) && base.is_some_and(|b| p.pos.iter().any(|w| w.text == b)) => return deny(DESTRUCTIVE),
            "update-ref" | "filter-branch" => return deny(DESTRUCTIVE),
            // a replace ref swaps a commit for another in every git read of the shared repo (the base's tip in the
            // app's own merge too) while no branch moves: only listing (`git replace`, `-l`, `--format`)
            "replace" if !(p.pos.is_empty() || p.has(&["-l", "--list"])) || p.has(&["-d", "--delete", "-f", "--force", "-e", "--edit", "-g", "--graft", "--convert-graft-file"]) => {
                return deny(DESTRUCTIVE)
            }
            "fetch" | "pull" if p.pos.iter().skip(1).copied().chain(p.values(&["--refmap"])).any(|w| w.text.to_ascii_lowercase().contains("refs/replace")) => return deny(DESTRUCTIVE),
            // the credential helper protocol prints the stored password: with Git Credential Manager or
            // `gh auth setup-git`, the GitHub login's token
            "credential" if p.pos.first().is_none_or(|w| w.dynamic || w.text == "fill") => return deny(TOKEN),
            "symbolic-ref" if p.pos.len() >= 2 || p.has(&["-d", "--delete"]) => return deny(DESTRUCTIVE),
            "worktree" if matches!(p.first(), Some("remove" | "prune" | "move")) => return deny(DESTRUCTIVE),
            "worktree" if p.first() == Some("add") && p.has(&["-f", "--force"]) => return deny(DESTRUCTIVE),
            "checkout" | "switch" if p.has(&["-f", "--force", "--discard-changes", "--ignore-other-worktrees"]) => return deny(DESTRUCTIVE),
            "checkout" if p.pos.iter().chain(&p.paths).any(|w| whole_tree(w)) => return deny(DESTRUCTIVE),
            // `--staged` alone only unstages
            "restore" if p.pos.iter().chain(&p.paths).any(|w| whole_tree(w)) && !(p.has(&["-S", "--staged"]) && !p.has(&["-W", "--worktree"])) => return deny(DESTRUCTIVE),
            "fetch" if p.has(&["-u", "--update-head-ok"]) => return deny(DESTRUCTIVE),
            "fetch" | "pull" if base.is_some_and(|b| p.pos.iter().skip(1).any(|w| writes_branch(&w.text, b))) => return deny(DESTRUCTIVE),
            "config" if p.pos.iter().position(|w| alias_key(&w.text)).is_some_and(|k| p.pos.len() > k + 1) => return deny(ALIAS),
            _ => {}
        }
        let memo = Cell::new(None);
        let strict = || match memo.get() {
            Some(s) => s,
            None => {
                let s = self.strict(dir.as_deref(), over);
                memo.set(Some(s));
                s
            }
        };
        let merging = self.ctx.merging;
        // the project folder, the base branch, or unknown
        match sub {
            "merge" | "pull" | "rebase" | "cherry-pick" | "revert" | "am" if strict() => {
                if merging {
                    return deny(MERGING);
                }
                // only to back out of one under way
                if !p.has(&["--abort"]) {
                    return deny(if self.ctx.pr_mode { MERGE_PR } else { MERGE });
                }
            }
            "commit" if p.has(&["--amend"]) && strict() => return deny(BASE),
            "reset" if (p.has(&["--soft", "--mixed", "--merge", "--keep"]) || self.reset_moves(&p, dir.as_deref())) && strict() => return deny(BASE),
            "checkout" if !self.checkout_paths(&p, dir.as_deref()) && strict() => return deny(BASE),
            "switch" if strict() => return deny(BASE),
            "restore" if p.has(&["-s", "--source"]) && strict() => return deny(BASE),
            "stash" if !matches!(p.first(), Some("list" | "show")) && strict() => return deny(BASE),
            _ => {}
        }
        if merging && !read_only(sub, &p) && strict() {
            return deny(MERGING);
        }
        if !read_only(sub, &p) && self.locked(dir.as_deref()) {
            return deny(MERGING);
        }
        Ok(())
    }

    /// The app commits in this worker folder now (or the folder is unknown while it commits somewhere).
    fn locked(&self, dir: Option<&str>) -> bool {
        !self.ctx.locked.is_empty() && dir.is_none_or(|d| self.ctx.locked.iter().any(|l| within(d, l)))
    }

    /// `reset <commit>` without paths moves the branch.
    fn reset_moves(&self, p: &Args, dir: Option<&str>) -> bool {
        p.pos.len() == 1 && p.paths.is_empty() && (p.pos[0].dynamic || dir.is_none_or(|d| self.repo.is_rev(d, &p.pos[0].text)))
    }

    /// `checkout` that only restores files from the index (`-- <paths>`, or paths that aren't revisions).
    fn checkout_paths(&self, p: &Args, dir: Option<&str>) -> bool {
        if p.has(&["-b", "-B", "--orphan", "--detach", "-t", "--track", "-p", "--patch"]) {
            return false;
        }
        if p.pos.is_empty() && p.paths.is_empty() {
            return true;
        }
        if p.dd {
            return p.pos.is_empty();
        }
        dir.is_some_and(|d| p.pos.iter().all(|w| !w.dynamic && !self.repo.is_rev(d, &w.text)))
    }
}

fn read_only(sub: &str, p: &Args) -> bool {
    READ_ONLY.contains(&sub)
        || (sub == "branch" && p.pos.is_empty() && !p.has(&["-d", "--delete", "-m", "--move", "-c", "--copy", "-u", "--set-upstream-to", "--unset-upstream", "--edit-description"]))
        || (sub == "stash" && matches!(p.first(), Some("list" | "show")))
        || (sub == "worktree" && p.first() == Some("list"))
}

/// gh's command groups: the word after one is its subcommand, so the command path has two words (`pr view`). Any
/// other first word (`api`, `browse`, `status`, `help`, an alias, an extension) is the whole path.
const GH_GROUPS: &[&str] = &[
    "agent-task", "alias", "attestation", "auth", "cache", "codespace", "cs", "config", "extension", "extensions", "ext", "gist", "gpg-key", "issue",
    "label", "org", "pr", "preview", "project", "release", "repo", "ruleset", "rs", "run", "search", "secret", "ssh-key", "variable", "workflow",
];

/// What an allowed gh command does that the caller still judges.
#[derive(PartialEq)]
enum Gh {
    Other,
    /// `gh pr checkout`: switches the current folder's branch
    Checkout,
}

/// gh runs with the user's own login. In every project: no command that prints the login's token (`auth token`,
/// `auth status -t`, `auth git-credential`, `config get … token`) and no command name in a variable. In PR mode (R6) an agent gets reads
/// only, since pushing, opening, editing, reviewing, commenting on and merging PRs and moving branches on GitHub
/// are the user's clicks in the app: `pr view|list|diff|checks|status|checkout`, `run view|list|watch|download`,
/// `repo view`, `issue view|list|status`, `release list|view`, `search …`, `workflow list|view`, `label list`,
/// `gist list|view`, `cache list`, `browse -n`, `auth status`, `--version`/`version`/`help`, `<group> --help`, and
/// `gh api` read as GET (no `-X`/`--method` other than GET, no field or input flags); anything else, an alias or a
/// word not known here, is refused. In local mode R5's rule stands: `pr create|new|merge` are refused, plus the ways
/// around it R5 didn't see (on purpose, R6): a `gh api` that writes (a method other than GET, fields without an
/// explicit `-X GET`, `--input`, any word in a variable; a GraphQL query is fine unless it is or may be a mutation),
/// defining a gh alias, a command name in a variable, and `gh pr checkout` in the project folder (a branch switch,
/// as `git switch` is) or in a worker folder the app is committing in. The command is read as gh (cobra) finds it, so
/// no flag may come between or before the command words except `-R`/`--repo`, `--help`, a bare `gh --version` and a
/// last `-h`; the token and alias rules also look at every word, wherever it stands.
fn gh(args: &[&Word], pr_mode: bool) -> Result<Gh, String> {
    let refuse = if pr_mode { PR } else { PR_LOCAL };
    let word = |w: &str| args.iter().any(|a| !a.dynamic && a.text == w);
    // the login's token, whatever the words around them: any line with `auth` and `token`/`git-credential`, or
    // `--show-token`
    if (word("auth") && (word("token") || word("git-credential"))) || args.iter().any(|a| a.text.starts_with("--show-token")) {
        return Err(TOKEN.into());
    }
    let mut pos: Vec<&Word> = vec![];
    let mut flags: Vec<&str> = vec![];
    let mut dynamic = false;
    let mut i = 0;
    while let Some(a) = args.get(i) {
        let t = a.text.as_str();
        // the command path is complete: one word, two under a group (`pr view`)
        let path_done = pos.first().is_some_and(|w| pos.len() >= 2 || !GH_GROUPS.contains(&w.text.as_str()));
        match t {
            "-R" | "--repo" => i += 2,
            _ if t.starts_with('-') && !a.dynamic => {
                // gh finds its command the way cobra does: a flag it doesn't know as a boolean at that level takes the
                // next word as its value, so a flag among the command words hides one (`gh pr -b view merge 3` runs
                // `pr merge`). Before the path is complete only the flags read the same way here are let through:
                // `-R`/`--repo` (value next or attached), `--help` (a boolean everywhere), `--version` (a boolean of
                // the bare `gh`), `-h`/`-v` as the last word (nothing after it to take)
                let plain = t == "--help" || t.starts_with("-R") || t.starts_with("--repo=") || (t == "--version" && pos.is_empty()) || (matches!(t, "-h" | "-v") && i + 1 == args.len());
                if !path_done && !plain {
                    return Err(refuse.into());
                }
                flags.push(t);
                i += 1;
            }
            _ => {
                // cobra skips an empty word when it looks for the command
                if !path_done && t.is_empty() {
                    return Err(refuse.into());
                }
                if pos.len() < 2 {
                    pos.push(a);
                } else {
                    dynamic |= a.dynamic;
                }
                i += 1;
            }
        }
    }
    // a command name held in a variable is unknown here
    if pos.iter().any(|w| w.dynamic) {
        return Err(refuse.into());
    }
    let names: Vec<&str> = pos.iter().map(|w| w.text.as_str()).collect();
    // the login's token: `-t`, also inside `-ht`
    let shows_token = || dynamic || flags.iter().any(|f| !f.starts_with("--") && f[1..].contains('t'));
    match names.as_slice() {
        ["auth", "status"] if shows_token() => return Err(TOKEN.into()),
        // `-t` under any `auth` command
        ["auth", ..] if flags.iter().any(|f| !f.starts_with("--") && f[1..].contains('t')) => return Err(TOKEN.into()),
        ["config", "get"] if dynamic || args.iter().any(|a| a.text.to_ascii_lowercase().contains("token")) => return Err(TOKEN.into()),
        _ => {}
    }
    // defining a gh alias, however the words are placed (`alias --clobber x set …`)
    if args.iter().position(|a| a.text == "alias").is_some_and(|k| args[k + 1..].iter().any(|a| a.dynamic || a.text == "set" || a.text == "import")) {
        return Err(refuse.into());
    }
    // every argument but the `api` word itself: flags on either side of it are gh api's
    let api_args: Vec<&Word> = match args.iter().position(|a| a.text == "api") {
        Some(k) => args[..k].iter().chain(&args[k + 1..]).copied().collect(),
        None => args.to_vec(),
    };
    let api = || api_get(&api_args);
    if !pr_mode {
        let ok = match names.as_slice() {
            // `new` is gh's own alias of `pr create`
            ["pr", "create" | "new" | "merge"] => false,
            ["alias", "set" | "import"] => false,
            ["api", ..] => api() || graphql_query(&api_args),
            // `co` is gh's own alias of `pr checkout`
            ["pr", "checkout" | "co"] => return Ok(Gh::Checkout),
            _ => true,
        };
        return if ok { Ok(Gh::Other) } else { Err(refuse.into()) };
    }
    // `gh pr create --help` prints help, but `-b --help` would be a flag's value. Under `auth` and `config`, `-h` is
    // `--hostname` (`gh auth logout -h github.com`), and under `repo create|edit` it is `--homepage`
    // (`gh repo edit -h https://x` writes): so `-h` counts as help only as the command's last word, where it can't
    // take a value, and never under `auth` and `config`.
    let short_help = !matches!(names.first(), Some(&"auth" | &"config")) && args.last().is_some_and(|a| a.text == "-h");
    let help = !flags.is_empty() && flags.iter().all(|f| *f == "--help" || (short_help && *f == "-h"));
    let ok = match names.as_slice() {
        [] => flags.iter().all(|f| matches!(*f, "--version" | "-v" | "--help" | "-h")),
        // help and version print only; plain words after them name a topic
        ["version" | "help", ..] => flags.is_empty(),
        _ if help => true,
        // `--force` resets the local branch to the PR's head
        ["pr", "checkout" | "co"] if !flags.iter().any(|f| matches!(*f, "-f" | "--force")) => return Ok(Gh::Checkout),
        ["pr", "view" | "list" | "diff" | "checks" | "status"] => true,
        ["run", "view" | "list" | "watch" | "download"] => true,
        ["repo", "view"] => true,
        ["issue", "view" | "list" | "status"] => true,
        ["release", "view" | "list"] => true,
        ["search", _] => true,
        ["workflow", "view" | "list"] => true,
        ["label", "list"] => true,
        ["gist", "view" | "list"] => true,
        ["cache", "list"] => true,
        ["browse", ..] => flags.iter().any(|f| matches!(*f, "-n" | "--no-browser")),
        ["auth", "status"] => true,
        ["api", ..] => api(),
        _ => false,
    };
    if ok {
        Ok(Gh::Other)
    } else {
        Err(refuse.into())
    }
}

/// One `gh api` argument, read the way gh's flag parser (pflag) reads it: a flag by its long name with its value, or
/// a positional word.
enum ApiArg<'w> {
    Flag(&'static str, Option<String>),
    Pos(&'w Word),
}

/// gh api's shorthands: `-i` (`--include`) and `-h` (help) take no value; the others take the rest of the word or
/// the next word.
const API_SHORT: &[(char, &str, bool)] = &[
    ('i', "--include", false),
    ('h', "--help", false),
    ('X', "--method", true),
    ('f', "--raw-field", true),
    ('F', "--field", true),
    ('H', "--header", true),
    ('q', "--jq", true),
    ('t', "--template", true),
    ('p', "--preview", true),
];
const API_LONG: &[(&str, bool)] = &[
    ("--include", false),
    ("--help", false),
    ("--paginate", false),
    ("--silent", false),
    ("--slurp", false),
    ("--verbose", false),
    ("--method", true),
    ("--raw-field", true),
    ("--field", true),
    ("--header", true),
    ("--hostname", true),
    ("--input", true),
    ("--jq", true),
    ("--preview", true),
    ("--template", true),
    ("--cache", true),
];

/// `gh api`'s arguments as gh reads them, short-flag bundles included (`-iXPUT` is `--include --method PUT`,
/// `-iftitle=x` is `--include --raw-field title=x`). None when a word can't be read here, which callers refuse: a
/// word in a variable (it may split into flags, so even an endpoint in a variable is refused), a flag gh doesn't
/// have, a value missing.
fn api_args<'w>(args: &[&'w Word]) -> Option<Vec<ApiArg<'w>>> {
    let mut out = vec![];
    let mut i = 0;
    let mut flags_done = false;
    while let Some(a) = args.get(i) {
        i += 1;
        if a.dynamic {
            return None;
        }
        let t = a.text.as_str();
        if flags_done || !t.starts_with('-') || t == "-" {
            out.push(ApiArg::Pos(a));
            continue;
        }
        if t == "--" {
            flags_done = true;
            continue;
        }
        if t.starts_with("--") {
            let (name, inline) = match t.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (t, None),
            };
            let &(long, takes) = API_LONG.iter().find(|(l, _)| *l == name)?;
            let value = match (takes, inline) {
                (true, Some(v)) => Some(v),
                (true, None) => {
                    let w = args.get(i).filter(|w| !w.dynamic)?;
                    i += 1;
                    Some(w.text.clone())
                }
                (false, v) => v,
            };
            out.push(ApiArg::Flag(long, value));
            continue;
        }
        // a bundle of shorthands: boolean ones go on; one taking a value takes the rest of the word (after an `=`
        // right behind the letter, as pflag strips it), or else the next word
        let letters: Vec<char> = t[1..].chars().collect();
        let mut k = 0;
        while k < letters.len() {
            let &(_, long, takes) = API_SHORT.iter().find(|(c, _, _)| *c == letters[k])?;
            if !takes {
                out.push(ApiArg::Flag(long, None));
                k += 1;
                continue;
            }
            let rest: String = letters[k + 1..].iter().collect();
            let value = if !rest.is_empty() {
                rest.strip_prefix('=').map(String::from).unwrap_or(rest)
            } else {
                let w = args.get(i).filter(|w| !w.dynamic)?;
                i += 1;
                w.text.clone()
            };
            out.push(ApiArg::Flag(long, Some(value)));
            break;
        }
    }
    Some(out)
}

/// `gh api` that only reads: the method GET (the default without fields), and fields only with an explicit `-X GET`
/// (gh then sends them as the query string). Never `--input`, which is a request body.
fn api_get(args: &[&Word]) -> bool {
    let Some(parsed) = api_args(args) else { return false };
    let (mut get, mut fields) = (false, false);
    for a in &parsed {
        match a {
            ApiArg::Flag("--method", m) => {
                if !m.as_deref().is_some_and(|m| m.eq_ignore_ascii_case("GET")) {
                    return false;
                }
                get = true;
            }
            ApiArg::Flag("--input", _) => return false,
            ApiArg::Flag("--raw-field" | "--field", _) => fields = true,
            _ => {}
        }
    }
    !fields || get
}

/// `gh api graphql` with one query known here that isn't a mutation (local mode: reads through GraphQL, as R5 let
/// them run). Refused: a query from a file or stdin, `--input`, a method set, a word in a variable, any mention of
/// `mutation`, and more than one `query` field (gh applies every `-f` before every `-F`, so which one it sends
/// isn't simply the last one written).
fn graphql_query(args: &[&Word]) -> bool {
    let Some(parsed) = api_args(args) else { return false };
    let mut endpoint = None;
    let mut queries = vec![];
    for a in &parsed {
        match a {
            ApiArg::Flag("--method" | "--input", _) => return false,
            ApiArg::Flag(name @ ("--raw-field" | "--field"), Some(v)) => {
                // gh's key is the text before the first `=`
                if let Some(q) = v.strip_prefix("query=") {
                    // `-F query=@file` reads a file
                    if *name == "--field" && q.starts_with('@') {
                        return false;
                    }
                    queries.push(q.to_string());
                }
            }
            ApiArg::Pos(w) => {
                if endpoint.is_none() {
                    endpoint = Some(w.text.as_str());
                }
            }
            _ => {}
        }
    }
    endpoint == Some("graphql") && queries.len() == 1 && !queries[0].to_ascii_lowercase().contains("mutation")
}

/// `$name = …` / `$name=…`: (name, the command on the right).
fn ps_assignment(w: &[&Word]) -> Option<(String, Vec<Word>)> {
    let body = w[0].text.strip_prefix('$')?;
    let valid = |n: &str| !n.is_empty() && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':');
    let (name, mut rest) = if let Some((n, v)) = body.split_once('=') {
        let first = Some(v).filter(|v| !v.is_empty()).map(|v| Word { text: v.into(), dynamic: false });
        (n.trim_end_matches(['+', '-']).to_string(), first.into_iter().collect::<Vec<_>>())
    } else {
        match w.get(1).map(|x| x.text.as_str()) {
            Some("=" | "+=" | "-=") => (body.to_string(), vec![]),
            Some(t) if t.starts_with('=') => (body.to_string(), vec![Word { text: t[1..].into(), dynamic: w[1].dynamic }]),
            _ => return None,
        }
    };
    if !valid(&name) {
        return None;
    }
    let skip = if body.contains('=') { 1 } else { 2 };
    rest.extend(w.iter().skip(skip).map(|x| (*x).clone()));
    Some((name, rest))
}

/// What a wrapper runs, after its own options.
fn skip_wrapper<'a, 'w>(prog: &str, args: &'a [&'w Word]) -> &'a [&'w Word] {
    let takes: &[&str] = match prog {
        "nice" => &["-n", "--adjustment"],
        "timeout" => &["-s", "--signal", "-k", "--kill-after"],
        "sudo" | "doas" => &["-u", "-g", "-C", "-D", "-h", "-p", "-U", "-r", "-t", "-T"],
        "xargs" => &["-I", "-n", "-P", "-d", "-E", "-L", "-s", "-a", "--max-args", "--max-procs", "--delimiter", "--arg-file", "--max-lines", "--max-chars", "--replace"],
        "stdbuf" => &["-i", "-o", "-e"],
        "exec" => &["-a"],
        "ionice" => &["-c", "-n", "-p", "-P", "-u", "--class", "--classdata", "--pid", "--pgid", "--uid"],
        _ => &[],
    };
    let mut i = 0;
    while i < args.len() && args[i].text.len() > 1 && args[i].text.starts_with('-') {
        if args[i].text == "--" {
            i += 1;
            break;
        }
        i += if takes.contains(&args[i].text.as_str()) { 2 } else { 1 };
    }
    if matches!(prog, "timeout" | "taskset") && i < args.len() {
        i += 1; // the duration, the CPU mask
    }
    &args[i.min(args.len())..]
}

/// `bash -c STRING`: the string (None: a script file or stdin, not looked into).
fn shell_c(args: &[&Word]) -> Option<String> {
    let mut has_c = false;
    let mut i = 0;
    while let Some(a) = args.get(i) {
        let t = a.text.as_str();
        if t == "--" {
            i += 1;
            break;
        }
        if t.starts_with("--") {
            i += 1;
        } else if (t.starts_with('-') || t.starts_with('+')) && t.len() > 1 {
            has_c |= t.starts_with('-') && t.contains('c');
            i += if matches!(t, "-o" | "+o" | "-O" | "+O") { 2 } else { 1 };
        } else {
            break;
        }
    }
    has_c.then(|| args.get(i).map(|w| w.text.clone())).flatten()
}

/// `sh` without `-c` reads its commands from stdin when no script file is named, or with `-s`.
fn shell_stdin(args: &[&Word]) -> bool {
    let mut s = false;
    let mut i = 0;
    while let Some(a) = args.get(i) {
        let t = a.text.as_str();
        if t == "--" || t == "-" {
            i += 1;
            break;
        }
        if matches!(t, "--rcfile" | "--init-file") {
            i += 2;
        } else if t.starts_with("--") {
            i += 1;
        } else if (t.starts_with('-') || t.starts_with('+')) && t.len() > 1 {
            s |= t.starts_with('-') && t.contains('s');
            i += if matches!(t, "-o" | "+o" | "-O" | "+O") { 2 } else { 1 };
        } else {
            break;
        }
    }
    s || i >= args.len()
}

/// `pwsh -Command …` / `powershell …`: the command text. Err: an encoded command (can't be read).
fn pwsh_c(args: &[&Word]) -> Result<Option<String>, ()> {
    let mut i = 0;
    while let Some(a) = args.get(i) {
        let t = a.text.to_ascii_lowercase();
        let Some(opt) = t.strip_prefix('-').or_else(|| t.strip_prefix('/')) else {
            // a bare argument: Windows PowerShell runs it as a command
            return Ok(Some(plain(&args[i..])));
        };
        match opt {
            "c" | "cmd" | "command" | "com" | "comm" | "comma" | "comman" | "commandwithargs" | "cwa" => return Ok(Some(plain(&args[i + 1..]))),
            "e" | "ec" | "en" | "enc" | "encodedcommand" | "encodedarguments" | "ea" => return Err(()),
            "f" | "file" | "fi" | "fil" => return Ok(None),
            "executionpolicy" | "ex" | "ep" | "windowstyle" | "w" | "workingdirectory" | "wd" | "configurationname" | "outputformat" | "of" | "o" | "inputformat" | "if" | "in" | "version" | "v" | "psconsolefile" | "settingsfile" => i += 2,
            _ => i += 1,
        }
    }
    Ok(None)
}

/// What the indirect net saw.
#[derive(Debug, PartialEq)]
enum Hit {
    /// a git command denied anywhere (push, reset --hard …)
    Deny,
    /// `git … merge`: denied where the line runs strict
    Merge,
}

/// The narrow net for what the structured pass can't see: an interpreter (python -c, `| sh`, xargs …) with a git
/// command the rules deny in its text (`git_verb_text`). Run on every line `Guard::shell` reads, nested ones too.
/// `maestro` commands (messages) and the git/gh calls themselves (judged exactly by the structured pass: their
/// commit messages and grep patterns aren't commands) don't count. The bool: the line changes folder (`cd`), so
/// where it runs isn't the start.
fn indirect(cmd: &str, ps: bool) -> Option<(Hit, bool)> {
    const INTERPRETERS: &[&str] = &[
        "python", "python3", "py", "node", "deno", "bun", "perl", "ruby", "php", "lua", "awk", "gawk", "sh", "bash", "zsh", "dash", "ksh", "ash", "mksh", "pwsh",
        "powershell", "cmd", "source", "xargs", "parallel", "iex", "invoke-expression",
    ];
    let interpreter = |w: &Word| INTERPRETERS.contains(&program(&w.text).as_str());
    let toks = lex(cmd, ps).ok()?;
    // the commands: words, heredoc texts, piped into the next one, inside parentheses (a subshell or `$( )`)
    let mut segs: Vec<(Vec<&Word>, Vec<&str>, bool, bool)> = vec![];
    let (mut words, mut data, mut nest) = (vec![], vec![], 0usize);
    for t in &toks {
        match t {
            Tok::Word(w) => words.push(w),
            Tok::Data(d) => data.push(d.as_str()),
            sep => {
                if !words.is_empty() || !data.is_empty() {
                    segs.push((std::mem::take(&mut words), std::mem::take(&mut data), *sep == Tok::Pipe, nest > 0));
                }
                match sep {
                    Tok::Open => nest += 1,
                    Tok::Close => nest = nest.saturating_sub(1),
                    _ => {}
                }
            }
        }
    }
    if !words.is_empty() || !data.is_empty() {
        segs.push((words, data, false, nest > 0));
    }
    // a git call's output may be run as code: piped into an interpreter that reads it (`git log --format=… | sh`; not
    // `| xargs sed`, `| awk …`), or from `$( )` into an interpreter with a word not known here (`sh -c "$(git …)"`)
    let reads = |s: &Vec<&Word>| s.iter().any(|w| interpreter(w) && !matches!(program(&w.text).as_str(), "xargs" | "awk" | "gawk"));
    let fed_dynamic = segs.iter().any(|(w, ..)| w.iter().any(|x| interpreter(x)) && w.iter().any(|x| x.dynamic));
    let mut text = String::new();
    let mut interp = false;
    let mut moves = false;
    for (k, (words, data, pipe, inner)) in segs.iter().enumerate() {
        let start = words
            .iter()
            .position(|w| assignment(&w.text).is_none() && !(!ps && !w.dynamic && matches!(w.text.as_str(), "if" | "then" | "else" | "elif" | "do" | "while" | "until" | "!" | "{")))
            .unwrap_or(words.len());
        // (assignments only, or a command named by a variable: kept)
        if let Some(first) = words.get(start).filter(|w| !w.dynamic) {
            if program(&first.text) == "maestro" {
                continue;
            }
            // git and gh are judged exactly by the structured pass: their messages and patterns aren't commands
            if names_program(&first.text) && !((*pipe && segs.get(k + 1).is_some_and(|n| reads(&n.0))) || (*inner && fed_dynamic)) {
                continue;
            }
        }
        moves |= words.get(start).is_some_and(|w| matches!(program(&w.text).as_str(), "cd" | "pushd" | "chdir" | "set-location" | "sl" | "push-location"));
        interp |= words.iter().any(|w| interpreter(w));
        text.push_str(&words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" "));
        text.push('\n');
        for d in data {
            text.push_str(d);
            text.push('\n');
        }
    }
    if !interp {
        return None;
    }
    git_verb_text(&text).map(|h| (h, moves))
}

/// A `git` word followed on its line, within 60 characters, by a git command the rules deny: `push`,
/// `update-ref`, `reset … --hard`, `branch … -D`, `clean … -f` (Deny), or `merge` without `--abort` (Merge).
fn git_verb_text(s: &str) -> Option<Hit> {
    let c: Vec<char> = s.chars().collect();
    let l: Vec<char> = c.iter().map(|ch| ch.to_ascii_lowercase()).collect();
    let word = |k: usize| l.get(k).is_some_and(|c| c.is_alphanumeric() || *c == '_');
    let mut merge = false;
    for i in 0..l.len() {
        if !l[i..].starts_with(&['g', 'i', 't']) || (i > 0 && word(i - 1)) || word(i + 3) {
            continue;
        }
        let end = (i + 3..(i + 3 + 60).min(c.len())).find(|&k| c[k] == '\n').unwrap_or((i + 3 + 60).min(c.len()));
        let win: String = c[i + 3..end].iter().collect();
        // words with their dashes (`-D`, `--hard`, `update-ref`)
        let toks: Vec<&str> = win.split(|ch: char| !(ch.is_alphanumeric() || ch == '_' || ch == '-')).filter(|t| !t.is_empty()).collect();
        let has = |w: &str| toks.iter().any(|t| t.eq_ignore_ascii_case(w));
        let short = |f: char| toks.iter().any(|t| t.starts_with('-') && !t.starts_with("--") && t[1..].contains(f));
        let push = win.split(|ch: char| !(ch.is_alphanumeric() || ch == '_')).any(|t| t.eq_ignore_ascii_case("push"));
        if push
            || has("update-ref")
            || (has("reset") && has("--hard"))
            || (has("branch") && (short('D') || (has("--delete") && has("--force"))))
            || (has("clean") && (short('f') || has("--force")) && !(short('n') || has("--dry-run")))
        {
            return Some(Hit::Deny);
        }
        merge |= has("merge") && !has("--abort");
    }
    merge.then_some(Hit::Merge)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A worker at C:/p/wt (branch feat), the project folder C:/p/repo (main), and C:/p/mwt, a worktree on main.
    struct FakeRepo {
        aliases: HashMap<&'static str, &'static str>,
    }

    impl Repo for FakeRepo {
        fn toplevel(&self, dir: &str) -> Option<String> {
            let d = dir.to_lowercase();
            ["C:/p/repo", "C:/p/wt", "C:/p/mwt"].into_iter().find(|t| d == t.to_lowercase() || d.starts_with(&format!("{}/", t.to_lowercase()))).map(String::from)
        }
        fn head_branch(&self, dir: &str) -> Option<String> {
            let t = self.toplevel(dir)?;
            Some(if t.ends_with("wt") && !t.ends_with("mwt") { "feat" } else { "main" }.into())
        }
        fn alias(&self, _: &str, name: &str) -> Option<String> {
            self.aliases.get(name).map(|a| a.to_string())
        }
        fn is_rev(&self, _: &str, arg: &str) -> bool {
            ["main", "feat", "x", "abc", "HEAD"].contains(&arg) || arg.contains('~') || arg.contains('^')
        }
    }

    fn repo() -> FakeRepo {
        FakeRepo { aliases: HashMap::from([("p", "push"), ("pp", "!git push"), ("st", "status"), ("loop1", "loop2"), ("loop2", "loop1"), ("sw", "switch")]) }
    }

    fn at(cwd: &'static str) -> Ctx<'static> {
        Ctx { cwd: Some(cwd), root: Some("C:/p/repo"), base: Some("main"), merging: false, locked: &[], pr_mode: false }
    }

    fn decide(rows: &[&str], ctx: &Ctx, allow: bool) {
        let r = repo();
        for cmd in rows {
            let got = check(cmd, ctx, &r);
            assert_eq!(got.is_none(), allow, "{cmd} (cwd {:?}): {got:?}", ctx.cwd);
        }
    }

    #[test]
    fn decides() {
        let wt = at("C:/p/wt");
        let root = at("C:/p/repo");
        decide(
            &[
                "git status",
                "git log --grep=push",
                "git diff main...HEAD",
                "git add -A && git commit -m wip",
                "git merge main",
                "git rebase main",
                "git pull origin main",
                "git checkout -- a.rs",
                "git branch -d old",
                "git fetch origin",
                "git stash",
                "git clean -n",
                "echo \"git push\"",
                "maestro send --to w --text \"never git push --force\"",
                "maestro merge-request --to api --verified \"tests pass\"",
                "npm test",
                "git st",
                "git commit -m \"$(cat <<'EOF'\nDon't push yet\n\nCo-Authored-By: x\nEOF\n)\"",
                "git -C ../wt status",
                "cd /c/p/repo && git status",
                "git worktree add ../w2 -b w2",
                // a git call's message or pattern on a line with an interpreter isn't a command
                "python -m pytest && git commit -m \"fix push notifications\"",
                "bash scripts/test.sh && git commit -m \"handle push events\"",
                "python manage.py test; git log --grep push",
                "git grep -l push | xargs sed -i s/a/b/",
                "if python -m pytest; then git commit -m \"merge helper\"; fi",
            ],
            &wt,
            true,
        );
        decide(
            &[
                "git status",
                "git add a",
                "git commit -m x",
                "git reset",
                "git reset -- a.txt",
                "git restore --staged a",
                "git checkout -- a.rs",
                "git stash list",
                "git merge --abort",
                "git reset HEAD a.txt",
                "git commit -m \"--amend later\"",
                "python -m pytest && git commit -m \"merge helper\"",
            ],
            &root,
            true,
        );
        let anywhere = [
            "git push",
            "git push -f",
            "git push --force-with-lease",
            "git -C ../x push",
            "GIT_TRACE=1 git push",
            "env git push",
            "command git push",
            "/usr/bin/git push",
            "git.exe push",
            "\"C:/Program Files/Git/bin/git.exe\" push",
            "git --no-pager push",
            "git -c core.x=y push",
            "cd x && git push",
            "true; git push 2>&1",
            "(git push)",
            "echo $(git push)",
            "`git push`",
            "bash -c \"git push\"",
            "pwsh -Command \"git push\"",
            "cmd /c \"git push\"",
            "xargs git push",
            "git p",
            "git pp",
            "git -c alias.p=push p",
            "git config alias.p push",
            "git config --global alias.p push",
            "python -c \"import os; os.system('git push')\"",
            "echo git push | sh",
            "gh pr merge 3",
            "gh pr create",
            "git reset --hard",
            "git reset --hard HEAD~1",
            "git clean -fd",
            "git clean -xdf",
            "git branch -D x",
            "git branch -df x",
            "git branch --delete --force x",
            "git branch -f main HEAD",
            "git update-ref refs/heads/main HEAD",
            "git worktree remove ../w",
            "git checkout -f main",
            "git checkout .",
            "git restore .",
            "git checkout --ignore-other-worktrees main",
            "git fetch . HEAD:main",
            "git push \"",
            "git-push origin",
            "sudo -u me git push",
            "timeout 5 git push",
            "nice -n 5 git push",
            "env -S 'git push'",
            "git loop1",
            "cat <<EOF | sh\ngit push origin\nEOF",
            "eval git push",
            "$g = 'git'; & $g push",
        ];
        decide(&anywhere, &wt, false);
        decide(&anywhere, &root, false);
        decide(
            &["git merge feat", "git merge --no-ff feat", "git pull", "git rebase x", "git cherry-pick abc", "git commit --amend", "git switch feat", "git checkout feat -- a.rs", "git restore --source feat a", "git stash", "git reset --soft HEAD~1", "git reset HEAD~1", "git checkout feat", "git checkout -b new", "git sw feat", "cd sub && git merge feat"],
            &root,
            false,
        );
        decide(
            &[
                "cd C:/p/repo && git merge feat",
                "cd /c/p/repo && git merge feat",
                "cd .. && cd repo && git merge feat",
                "git -C C:\\p\\repo merge feat",
                "git --git-dir=C:/p/repo/.git merge feat",
                "GIT_DIR=x git merge feat",
                "export GIT_DIR=x; git merge feat",
                "cd C:\\p\\repo; git merge feat",
                "bash -c 'cd /c/p/repo && git merge feat'",
                "git -C ../repo merge feat",
            ],
            &wt,
            false,
        );
        // a worktree on the base branch counts as the base
        decide(&["git merge feat", "git reset --soft HEAD~1"], &at("C:/p/mwt"), false);
        // a cd inside a subshell doesn't move the shell
        decide(&["(cd /c/p/repo && git status); git merge main"], &wt, true);
    }

    #[test]
    fn reserved_words_stdin_shells_and_unknown_wrappers() {
        let wt = at("C:/p/wt");
        let root = at("C:/p/repo");
        let anywhere = [
            // bash reserved words before the command
            "if true; then git push; fi",
            "for b in x; do git branch -D $b; done",
            "for b in $(git branch --merged); do git branch -D $b; done",
            "! git push",
            "while true; do git push; done",
            "until false; do :; done; if [ -n x ]; then :; else git push; fi",
            "coproc git push",
            // a shell reading its commands from stdin
            "bash <<< \"git push\"",
            "sh <<< 'git reset --hard'",
            "echo \"git reset --hard\" | sh",
            "printf 'git push\\n' | bash",
            "echo 'git push' | sudo sh -s",
            "cat <<EOF | sh\ngit branch -D x\nEOF",
            "cmd < /dev/null; echo git push | cmd",
            // substitutions in an unquoted heredoc run in the outer shell
            "cat <<EOF\n$(git push)\nEOF",
            "cat <<EOF > f\n`git push`\nEOF",
            // interpreters with other denied commands
            "python -c \"import subprocess; subprocess.run(['git','reset','--hard'])\"",
            "node -e \"require('child_process').execSync('git branch -D x')\"",
            // wrappers not listed before, and unknown ones
            "setsid git push",
            "flock /tmp/l git push",
            "flock -w 5 /tmp/l -c 'git push'",
            "cmd /c call git push",
            "ionice -c 3 git push",
            "taskset -c 0 git push",
            "chronic git push",
            "unbuffer git push",
            "parallel git push ::: x",
            "some-runner --flag git push origin",
        ];
        decide(&anywhere, &wt, false);
        decide(&anywhere, &root, false);
        decide(
            &[
                "if git merge feat; then :; fi",
                "if git merge feat; then echo ok; fi",
                "while ! git merge --no-edit feat; do sleep 1; done",
                "echo \"git merge feat\" | sh",
                "sh <<EOF\ngit merge feat\nEOF",
                "python -c \"import subprocess; subprocess.run(['git','merge','feat'])\"",
                "setsid git merge feat",
            ],
            &root,
            false,
        );
        decide(&["cd ../repo; bash <<EOF\ngit merge feat\nEOF", "cd ../repo && python -c \"import os; os.system('git merge feat')\""], &wt, false);
        decide(
            &[
                "if git merge main; then echo ok; fi",
                "for f in a b; do git add $f; done",
                "if git diff --quiet; then echo clean; else git status; fi",
                "python -c \"import os; os.system('git merge main')\"",
                "echo \"git merge main\" | sh",
                "bash <<< 'git log'",
                "cat <<EOF | sh\ngit status\nEOF",
                "cat <<'EOF'\n$(git push)\nEOF",
                "man git-push",
                "ls git-push",
                "flock /tmp/l cargo build",
                "brew install gh",
                "cat script.sh | sh",
            ],
            &wt,
            true,
        );
        decide(&["if git diff --quiet; then echo clean; fi", "git merge --abort && python x.py"], &root, true);
    }

    #[test]
    fn sees_through_spellings_and_git_run_commands() {
        let wt = at("C:/p/wt");
        decide(
            &[
                "g''it push",
                "g\"i\"t push",
                "g\\it push",
                "\\git push",
                "G=gi; ${G}t push",
                "$'\\x67it' push",
                "$'\\147it' push",
                "$'\\u0067it' push",
                "{git,push}",
                "git rebase -x 'git push origin HEAD' HEAD~1",
                "git rebase --exec='git push' main",
                "git submodule foreach git push",
                "git submodule foreach --recursive 'git push'",
                "git bisect run git push",
                "git -c core.editor='git push;:' commit --allow-empty",
                "git -c credential.helper='!git push' fetch",
                "git --config-env=core.editor=ED commit",
                "git -c include.path=/tmp/a q",
                "GIT_EDITOR='git push;:' git commit --allow-empty",
                "GIT_EDITOR=$X git commit",
                "env GIT_SEQUENCE_EDITOR='git push;:' git rebase -i main",
                "export GIT_EDITOR='git push;:'",
                "find . -exec git push \\;",
                "find . -name x -execdir git push ';'",
                "watch git push",
                "watch -n 5 git push",
                "script -c 'git push' /dev/null",
                "cmd /c start git push",
                // option spellings git accepts: short clusters, an attached value, long prefixes
                "git rebase -qx 'git push origin HEAD' HEAD~1",
                "git rebase -fx 'git push' HEAD~1",
                "git rebase -qx'git push' HEAD~1",
                "git rebase --exe 'git push origin HEAD' HEAD~1",
                "git rebase --exe='git push' HEAD~1",
                "git rebase -x \"$CMD\" main",
                // other options whose value git runs
                "git difftool -x 'git push' HEAD~1",
                "git difftool --extcmd='git push'",
                "git fetch --upload-pack='git push' .",
                "git pull --upload-pack 'git push' .",
                "git clone -u 'git push' . x",
                "git clone --upload-pack='git push' . x",
                "git ls-remote --upload-pack='git push' .",
                "git ls-remote --exec='git push' .",
                "git fetch-pack --upload-pack='git push' .",
                "git archive --remote=. --exec='git push' HEAD",
                "git -c uploadpack.packObjectsHook='git push' fetch",
                "git -c remote.origin.uploadpack='git push' fetch",
                "git -c diff.x.command='git push' diff",
                "git clone -c core.sshCommand='git push' ssh://h/r x",
                "git config core.pager 'git push'",
                "git config --global core.editor 'git push;:'",
                // the ext:: transport runs its URL
                "git -c protocol.ext.allow=always fetch 'ext::sh -c git% push'",
                "git fetch 'ext::sh -c git% push'",
                "git remote add x 'ext::sh -c git% push'",
                "git -c protocol.allow=always ls-remote x",
                // interpreters in commands git runs, and git output run as code
                "git bisect run python -c \"import os; os.system('git push')\"",
                "GIT_EDITOR=\"python -c \\\"import os; os.system('git push')\\\"\" git commit",
                "git log -1 --format='git push origin HEAD' | sh",
                "sh -c \"$(git log -1 --format='git push')\"",
            ],
            &wt,
            false,
        );
        decide(
            &[
                "git rebase -x 'cargo test' main",
                "git rebase -qx 'cargo test' main",
                "git difftool -x 'code --wait --diff' HEAD~1",
                "git fetch --upload-pack=git-upload-pack origin",
                "git clone https://h/r.git x",
                "git -c protocol.version=2 fetch origin",
                "git config core.editor 'code --wait'",
                "git config user.name 'A B'",
                "git archive --format=zip HEAD -o out.zip",
                "GIT_EDITOR=true git commit",
                "git -c core.editor=true commit",
                "git -c color.ui=always log",
                "find . -name '*.rs' -exec grep -l foo {} \\;",
                "git log --format='%h {}'",
                "echo {a,b}",
            ],
            &wt,
            true,
        );
    }

    #[test]
    fn hidden_subcommands_and_option_prefixes() {
        let wt = at("C:/p/wt");
        let root = at("C:/p/repo");
        let anywhere = [
            // global options with a separate value, and one this doesn't know
            "git --attr-source HEAD push origin main",
            "git --shallow-file x push",
            "git --new-option HEAD push origin main",
            // wrappers that add or hide the subcommand
            "echo push origin x1 | xargs git",
            "xargs -I{} git {} origin",
            "xargs -n1 git <<< push",
            "exec -a foo git push",
            "wsl git push",
            "wsl -e git push",
            "wsl --exec git push origin",
            "wsl -d Ubuntu -- git push",
            // an alias defined now, used on the next line
            "alias p='git push'\np",
            "alias g=git; g push",
            // unambiguous prefixes of long options
            "git reset --ha HEAD",
            "git clean --forc",
            "git checkout --forc main",
            "git branch --del --forc x",
            "git switch --discard main",
            "git fetch --update-head origin",
        ];
        decide(&anywhere, &wt, false);
        decide(&anywhere, &root, false);
        decide(&["git --attr-source HEAD merge feat", "git commit --amen", "echo main | xargs git checkout", "git reset --so HEAD~1", "git restore --sour feat a"], &root, false);
        decide(
            &[
                "git --attr-source=HEAD log",
                "git ls-files | xargs git log --",
                "git diff --name-only | xargs -I{} git add {}",
                "find . -name '*.rs' | xargs grep foo",
                "wsl ls",
                "wsl --list",
                "alias ll='ls -la'",
                "git clean --dry -f",
                "git restore --stag a.txt",
                "git lfs pull",
            ],
            &wt,
            true,
        );
        decide(&["git merge --abo", "git clean --dr -f", "git commit --all -m x", "git branch --merg main"], &root, true);
        // an expanded value-taking option still takes its value
        assert_eq!(parse(&[&Word { text: "--sour".into(), dynamic: false }, &Word { text: "feat".into(), dynamic: false }], "restore").pos.len(), 0);
    }

    #[test]
    fn leaves_commands_without_git_alone() {
        let wt = at("C:/p/wt");
        let r = repo();
        for cmd in ["$PY scripts/highlight.py", "\"$JAVA_HOME/bin/java\" -jar ~/github/x.jar", "echo high light right height digit legit", "cat .github/workflows/ci.yml", "$CC -o x x.c"] {
            assert!(!worth_checking(cmd, false), "{cmd}");
            assert_eq!(check(cmd, &wt, &r), None, "{cmd}");
        }
        for cmd in ["Get-ChildItem | Where-Object { $_.Name -like '*light*' }", "$x = 1; $x + 2"] {
            assert_eq!(check_tool("PowerShell", &json!({ "command": cmd }), &wt, &r), None, "{cmd}");
        }
        assert!(names_git("C:/Program Files/Git/cmd/git.exe") && names_git("git-lfs pull") && names_git("(gh pr list)"));
        assert!(!names_git("github .git/config digit high_git git_helper"));
    }

    #[test]
    fn powershell_idioms() {
        let wt = at("C:/p/wt");
        let r = repo();
        let ps = |cmd: &str| check_tool("PowerShell", &json!({ "command": cmd }), &wt, &r);
        for cmd in [
            "git status; if ($?) { Write-Host ok }",
            "if ($LASTEXITCODE -ne 0) { git status }",
            "git log --oneline | ForEach-Object { $_.Split(' ')[0] }",
            "git branch | ForEach-Object { $_.Trim() }",
            "& git status",
            "& \"C:\\Program Files\\Git\\cmd\\git.exe\" log",
            "Start-Process notepad",
            "Start-Process git -ArgumentList 'status'",
        ] {
            assert_eq!(ps(cmd), None, "{cmd}");
        }
        for cmd in [
            "& ('g'+'it') push",
            "$g = 'git'; & $g push",
            "$g = 'git'; & $g p",
            ". git push",
            "& git push",
            "Push-Location C:/p/repo; git merge feat",
            "Push-Location -Path C:\\p\\repo; git merge feat",
            "Start-Process git push",
            "saps git -ArgumentList push",
            "Start-Process -FilePath git -ArgumentList 'push', 'origin'",
            "Start-Process -FilePath:git -ArgumentList:push",
            "Start-Process git -ArgumentList 'merge feat' -WorkingDirectory C:/p/repo",
            "$env:GIT_EDITOR = 'git push;:'; git commit",
            // the subcommand from an expression, an array or splatting
            "git ('push') origin main",
            "git ('pu'+'sh')",
            "git @('push')",
            "$a=@('push','origin'); git @a",
            // another name for git
            "Set-Alias g git; g push",
            "sal -Name g -Value git.exe",
            "New-Alias g 'C:\\Program Files\\Git\\cmd\\git.exe'; g push",
            "nal g git",
            // .NET starting it
            "[Diagnostics.Process]::Start('git','push')",
            "[System.Diagnostics.Process]::Start(\"git\", \"push origin\")",
            // a string piped into something that runs it
            "'git push' | iex",
            "'git push' | Invoke-Expression",
            "'git push' | powershell -Command -",
        ] {
            assert!(ps(cmd).is_some(), "{cmd}");
        }
        for cmd in ["Write-Output (git rev-parse HEAD)", "git commit -m (Get-Content msg.txt)", "if ($x) { git status }", "Set-Alias ll Get-ChildItem", "$files = @(git ls-files)", "foreach ($f in (git ls-files)) { $f }"] {
            assert_eq!(ps(cmd), None, "{cmd}");
        }
    }

    #[test]
    fn unknown_context_is_strict() {
        let none = Ctx { cwd: Some("C:/p/wt"), root: None, base: None, merging: false, locked: &[], pr_mode: true };
        decide(&["git merge x"], &none, false);
        decide(&["git status"], &none, true);
        let wt = at("C:/p/wt");
        decide(&["cd $X && git merge x", "cd ~ && git merge x", "cd - && git merge x", "pushd \"$D\"; git merge x", "cd C:/nowhere && git merge x"], &wt, false);
        decide(&["cd $X && git status", "cd $X && git log"], &wt, true);
        let nocwd = Ctx { cwd: None, root: Some("C:/p/repo"), base: Some("main"), merging: false, locked: &[], pr_mode: false };
        decide(&["git merge x"], &nocwd, false);
    }

    #[test]
    fn merging_blocks_writes_in_the_folder() {
        let root = Ctx { merging: true, ..at("C:/p/repo") };
        decide(&["git commit -m x", "git merge --abort", "git add a", "git stash list && git add b"], &root, false);
        decide(&["git log", "git status", "git diff", "git branch"], &root, true);
        // a worker's own folder isn't affected
        decide(&["git commit -m x"], &Ctx { merging: true, ..at("C:/p/wt") }, true);
    }

    #[test]
    fn app_commit_locks_the_worker_folder() {
        let locked = vec!["c:/p/wt".to_string()];
        let wt = Ctx { locked: &locked, ..at("C:/p/wt") };
        decide(&["git commit -m x", "git add a", "git -C sub commit -m y", "git stash"], &wt, false);
        decide(&["git status", "git log", "git diff"], &wt, true);
        // another worker isn't affected
        decide(&["git commit -m x"], &Ctx { locked: &locked, ..at("C:/p/wt2") }, true);
        // `gh pr checkout` fetches and switches the branch there: it waits too, in both modes
        assert_eq!(check("gh pr checkout 5", &wt, &repo()).as_deref(), Some(MERGING));
        assert_eq!(check("gh pr checkout 5", &Ctx { pr_mode: true, ..Ctx { locked: &locked, ..at("C:/p/wt") } }, &repo()).as_deref(), Some(MERGING));
        let elsewhere = vec!["c:/p/other".to_string()];
        decide(&["gh pr checkout 5"], &Ctx { locked: &elsewhere, ..at("C:/p/wt") }, true);
        // and agents still can't push or open, edit or merge PRs themselves
        decide(&["git push origin wt", "gh pr create --fill", "gh pr edit 3 --body x", "gh pr merge 3 --squash"], &Ctx { pr_mode: true, ..at("C:/p/wt") }, false);
        assert!(PUSH.contains("maestro merge-request") && PR.contains("maestro merge-request"));
        // the words match the mode: only in PR mode does the app push after an approval
        assert_eq!(check("git push origin wt", &Ctx { pr_mode: true, ..at("C:/p/wt") }, &repo()).as_deref(), Some(PUSH));
        assert_eq!(check("git push origin wt", &at("C:/p/wt"), &repo()).as_deref(), Some(PUSH_LOCAL));
        assert!(!PUSH_LOCAL.contains("앱이"));
    }

    #[test]
    fn gh_is_read_only_for_agents() {
        let wt = Ctx { pr_mode: true, ..at("C:/p/wt") };
        // every write runs with the user's login: merging, approving, commenting, moving branches on GitHub
        decide(
            &[
                "gh api -X PUT repos/o/r/pulls/3/merge",
                "gh api --method PUT repos/o/r/pulls/3/merge",
                "gh api --method=PATCH repos/o/r/git/refs/heads/main -F force=true -f sha=abc",
                "gh api -XPUT repos/o/r/pulls/3/merge",
                "gh api repos/o/r/git/refs/heads/main -F force=true",
                "gh api repos/o/r/merges -f base=main -f head=x",
                "gh api repos/o/r/pulls -fbase=main",
                "gh api graphql -f query=x",
                "gh api repos/o/r/pulls --input body.json",
                "gh api -X GET repos/o/r -X POST",
                "gh api $EP",
                "gh pr review 3 --approve",
                "gh pr comment 3 -b hi",
                "gh pr ready 3",
                "gh pr close 3",
                "gh pr reopen 3",
                "gh pr checkout 3 --force",
                "gh pr comment 3 -b --help",
                "gh issue create -t x",
                "gh -R o/r pr merge 3",
                "gh run rerun 9",
                "gh workflow run ci.yml",
                "gh release create v1",
                "gh repo delete o/r --yes",
                "gh auth token",
                "gh auth status --show-token",
                "gh auth status -t",
                "gh auth status -ht",
                "gh auth status -h github.com --show-token",
                "gh config get oauth_token -h github.com",
                "gh browse",
                "gh alias set m 'pr merge'",
                "gh m 3",
                "gh $SUB merge 3",
                "gh secret set X",
                // under `auth` and `config`, `-h` is `--hostname`, not help
                "gh auth logout -h github.com",
                "gh auth switch -h github.com",
                "gh auth login -h github.com",
                "gh auth setup-git -h github.com",
                "gh config set -h github.com git_protocol ssh",
                // a bundle of shorthands behind gh api's boolean `-i` (pflag reads `-iXPUT` as `-i -X PUT`)
                "gh api -iXPUT repos/o/r/pulls/7/merge",
                "gh api -iX PUT repos/o/r/pulls/7/merge",
                "gh api -iX=PUT repos/o/r/pulls/7/merge",
                "gh api -X=PUT repos/o/r/pulls/7/merge",
                "gh api repos/o/r/pulls -iftitle=t -ifhead=wt -ifbase=main",
                "gh api -iXPATCH repos/o/r/git/refs/heads/main -iFsha=abc -iFforce=true",
                "gh api -iXGET repos/o/r/pulls -ifx=1 --input body.json",
                // a flag gh api doesn't have, and a word in a variable, aren't read here
                "gh api --nope repos/o/r",
                "gh api \"repos/$O/r/pulls\"",
                // `-h` is `--homepage` under `repo create|edit`: help only as the last word
                "gh repo edit -h https://x",
                "gh repo edit o/r -h https://x",
                "gh repo edit -h --help",
                "gh repo create n -h https://x --public",
                // the credential helper protocol prints the token
                "gh auth git-credential get",
            ],
            &wt,
            false,
        );
        decide(
            &[
                "gh pr view 3 --json state",
                "gh pr list --head wt",
                "gh pr diff 3",
                "gh pr checks 3",
                "gh pr status",
                "gh -R o/r pr view 3",
                "gh run view 9 --log-failed",
                "gh run list",
                "gh repo view o/r",
                "gh auth status",
                "gh auth status -h github.com",
                "gh auth --help",
                "gh --version",
                "gh api repos/o/r/pulls/3",
                "gh api -X GET repos/o/r/commits -H 'Accept: x' --jq .sha",
                "gh api --method=get repos/o/r",
                "gh api --paginate repos/o/r/issues",
                "gh pr checkout 3",
                "gh release list",
                "gh release view v1",
                "gh search prs --author @me",
                "gh workflow list",
                "gh label list",
                "gh gist view abc",
                "gh run download 9",
                "gh browse -n",
                "gh cache list",
                "gh pr --help",
                "gh pr create --help",
                "gh pr create -h",
                "gh repo edit -h",
                "gh pr co 3",
                "gh api -i repos/o/r/pulls/3",
                "gh api -iXGET search/issues -fq=bug",
                "gh api -- repos/o/r",
            ],
            &wt,
            true,
        );
        // `gh pr checkout` switches the branch, so not in the project folder
        decide(&["gh pr checkout 3"], &Ctx { pr_mode: true, ..at("C:/p/repo") }, false);
        assert!(check("gh auth status -t", &wt, &repo()).is_some_and(|w| w == TOKEN));
    }

    #[test]
    fn gh_in_local_mode_blocks_pr_create_merge_and_ways_around() {
        let wt = at("C:/p/wt");
        // R5: opening and merging PRs; R6 adds the ways around that (a writing gh api, a GraphQL mutation, a gh
        // alias, a command in a variable) and anything that prints the token
        decide(
            &[
                "gh pr create --fill",
                "gh pr merge 3 --squash",
                "gh -R o/r pr merge 3",
                "gh api -X PUT repos/o/r/pulls/3/merge",
                "gh api repos/o/r/pulls/3/merge -f merge_method=squash",
                "gh api graphql -f query='mutation { mergePullRequest(input: {}) { clientMutationId } }'",
                "gh api graphql -F query=@q.graphql",
                "gh api graphql --input q.json",
                "gh api -X GET repos/o/r --input body.json",
                "gh alias set m 'pr merge'",
                "gh $SUB merge 3",
                "gh auth token",
                "gh auth status --show-token",
                "gh auth status -t",
                // `new` is gh's alias of `pr create`
                "gh pr new --fill",
                "gh -R o/r pr new --base main",
                // shorthand bundles behind `-i`
                "gh api -iXPUT repos/o/r/pulls/7/merge",
                "gh api repos/o/r/pulls -iftitle=t -ifhead=wt -ifbase=main",
                "gh api -iXPATCH repos/o/r/git/refs/heads/main -iFsha=abc -iFforce=true",
                // gh applies every -f before every -F: two `query` fields are refused whichever comes last
                "gh api graphql -F query='mutation { mergePullRequest(input: {}) { clientMutationId } }' -f query='{ viewer { login } }'",
                "gh api graphql -f query='{ viewer { login } }' -f query='{ viewer { id } }'",
                "gh api graphql -ifquery='mutation { x }'",
                "gh api graphql -f \"query=$(cat q.graphql)\"",
                // variables in gh api aren't read here, even as the endpoint
                "gh api \"repos/$OWNER/$REPO/pulls/$N\"",
                "printf 'protocol=https\\nhost=github.com\\n\\n' | gh auth git-credential get",
            ],
            &wt,
            false,
        );
        decide(
            &[
                "gh pr checkout 3",
                "gh pr edit 3 --body x",
                "gh pr comment 3 -b hi",
                "gh issue create -t x -b y",
                "gh issue comment 3 -b hi",
                "gh release list",
                "gh search issues bug",
                "gh workflow view ci.yml",
                "gh label list",
                "gh gist list",
                "gh run download 9",
                "gh browse -n",
                "gh pr --help",
                "gh auth status",
                "gh api repos/o/r/pulls/3",
                // reads R5 let through: a GraphQL query, fields sent as a query string with -X GET
                "gh api graphql -f query='{ viewer { login } }'",
                "gh api graphql -f query='query($n: Int!) { viewer { login } }' -F n=3",
                "gh api -X GET search/issues -f q=bug",
                "gh api search/issues --method=GET -f q=bug",
            ],
            &wt,
            true,
        );
        assert!(check("gh pr create", &wt, &repo()).is_some_and(|w| w == PR_LOCAL));
        assert!(check("gh pr new", &wt, &repo()).is_some_and(|w| w == PR_LOCAL));
        assert!(check("gh auth git-credential get", &wt, &repo()).is_some_and(|w| w == TOKEN));
        decide(&["gh api -i repos/o/r/pulls/3", "gh api graphql -if query='{ viewer { login } }'"], &wt, true);
        // PR mode stays reads-as-GET only: a GraphQL query is refused there, -X GET with fields isn't
        let pr = Ctx { pr_mode: true, ..at("C:/p/wt") };
        decide(&["gh api graphql -f query='{ viewer { login } }'"], &pr, false);
        decide(&["gh api -X GET search/issues -f q=bug"], &pr, true);
        // switching the project folder's branch stays refused, as `git switch` is
        decide(&["gh pr checkout 3", "gh pr co 3"], &at("C:/p/repo"), false);
    }

    #[test]
    fn credential_helpers_that_print_the_token_are_refused() {
        for ctx in [at("C:/p/wt"), Ctx { pr_mode: true, ..at("C:/p/wt") }] {
            for cmd in [
                "git credential fill",
                "printf 'protocol=https\\nhost=github.com\\n\\n' | git credential fill",
                "git credential-manager get",
                "git-credential-manager get",
                "git credential-manager-core get",
                "git credential-store get",
                "git credential $OP",
            ] {
                assert_eq!(check(cmd, &ctx, &repo()).as_deref(), Some(TOKEN), "{cmd}");
            }
            decide(&["git credential approve", "git credential-cache exit", "git status"], &ctx, true);
        }
    }

    #[test]
    fn gh_reads_the_command_as_cobra_finds_it() {
        for ctx in [at("C:/p/wt"), Ctx { pr_mode: true, ..at("C:/p/wt") }] {
            // a flag among the command words hides one from cobra (it takes the next word as its value): what runs is
            // not what the words seem to say
            decide(
                &[
                    "gh pr -b view merge 12 --squash",
                    "gh pr --title view create --body x",
                    "gh pr --body view edit 3 --title x",
                    "gh pr -c view close 3",
                    "gh --hostname pr --hostname view auth logout --hostname github.com",
                    "gh --host pr --host view config set git_protocol ssh --host github.com",
                    "gh --title help pr create",
                    "gh --foo help p",
                    "gh --clobber help alias import",
                    "gh \"\" pr merge 3",
                    "gh -h pr merge 3",
                    "gh pr --version view merge 3",
                    // gh api's flags before the `api` word are still gh api's
                    "gh -XPUT api repos/o/r/pulls/1/merge",
                    "gh --method=POST -fhead=wt -fbase=main -ftitle=t api repos/o/r/pulls",
                    "gh --input=body.json api repos/o/r/pulls",
                    "gh -fquery='mutation { mergePullRequest(input: {pullRequestId: \"X\"}) { clientMutationId } }' api graphql",
                    "gh -Fquery=@m.graphql api graphql",
                    "gh -X POST api repos/o/r/pulls -f title=t -f head=wt -f base=main",
                    // a gh alias defined with a flag between the words, then run
                    "gh alias --shell p set 'git push origin HEAD'",
                    "gh alias --clobber m set 'pr merge'",
                    "gh help --clobber alias import",
                ],
                &ctx,
                false,
            );
            for cmd in [
                "gh --hostname pr --hostname view auth token --hostname github.com",
                "gh --hostname search --hostname x auth token --hostname github.com",
                "gh auth --hostname github.com token",
                "gh -R x auth token",
                "gh auth -h github.com git-credential get",
                "gh issue view 3 --show-token",
                "gh auth refresh -t",
            ] {
                assert_eq!(check(cmd, &ctx, &repo()).as_deref(), Some(TOKEN), "{cmd}");
            }
            decide(
                &[
                    "gh -R o/r pr view 3",
                    "gh pr -R o/r view 3",
                    "gh --repo=o/r pr list",
                    "gh -Ro/r pr list",
                    "gh pr --repo o/r checks 3",
                    "gh --help",
                    "gh --version",
                    "gh pr --help",
                    "gh pr -h",
                    "gh help pr create",
                    "gh api -X GET repos/o/r -f q=1",
                    "gh api repos/o/r/pulls/3 --jq .head.sha",
                    "gh browse -n",
                    "gh auth status -h github.com",
                    "gh search prs --author @me",
                ],
                &ctx,
                true,
            );
        }
        // local mode keeps R5's writes that aren't a PR create or merge
        decide(&["gh pr edit 3 --body x", "gh alias list", "gh issue create -t x -b y", "gh auth login -h github.com"], &at("C:/p/wt"), true);
    }

    #[test]
    fn credentials_traces_and_remote_helpers_are_refused() {
        for ctx in [at("C:/p/wt"), Ctx { pr_mode: true, ..at("C:/p/wt") }] {
            // a credential helper is handed the user's token (`store` after a login); a trace without redaction
            // prints the Authorization header
            for cmd in [
                "git -c 'credential.helper=!f(){ cat >&2; }; f' fetch origin",
                "git -c credential.https://github.com.helper=x fetch origin",
                "GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=credential.helper GIT_CONFIG_VALUE_0='!f(){ cat >&2; }; f' git ls-remote origin",
                "git config --global credential.helper store",
                "git config set credential.helper x",
                "git clone -c credential.helper=x https://github.com/o/r",
                "GIT_TRACE_REDACT=0 GIT_TRACE_CURL=1 git fetch origin",
                "export GIT_TRACE_REDACT=0; git fetch origin",
                "env GIT_TRACE_RED''ACT=false git ls-remote origin",
            ] {
                assert_eq!(check(cmd, &ctx, &repo()).as_deref(), Some(TOKEN), "{cmd}");
            }
            for cmd in ["$env:GIT_TRACE_REDACT='0'; $env:GIT_CURL_VERBOSE='1'; git fetch origin", "$env:GIT_CONFIG_KEY_0 = 'credential.helper'; git fetch origin"] {
                assert_eq!(check_tool("PowerShell", &json!({ "command": cmd }), &ctx, &repo()).as_deref(), Some(TOKEN), "{cmd}");
            }
            decide(&["git config credential.helper", "GIT_TRACE_CURL=1 git fetch origin", "git -c user.name=x fetch origin"], &ctx, true);
            // git's remote helpers take `push` on stdin; receive-pack writes this repo's refs
            let push = if ctx.pr_mode { PUSH } else { PUSH_LOCAL };
            for cmd in [
                "printf 'push refs/heads/feat:refs/heads/feat\\n\\n' | git remote-https origin https://github.com/o/r",
                "git remote-https origin https://github.com/o/r < cmds.txt",
                "git-remote-https.exe origin https://github.com/o/r < cmds.txt",
                "git remote-http origin http://x/r <<EOF\npush refs/heads/feat:refs/heads/feat\nEOF",
            ] {
                assert_eq!(check(cmd, &ctx, &repo()).as_deref(), Some(push), "{cmd}");
            }
            decide(&["printf x | git receive-pack ../repo", "git http-backend"], &ctx, false);
            // an alias redefined by the command's own config: git runs that one, not the one on disk
            assert_eq!(check("GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=alias.st GIT_CONFIG_VALUE_0=push git st origin feat", &ctx, &repo()).as_deref(), Some(ALIAS));
            assert_eq!(check("GIT_CONFIG_PARAMETERS=\"'alias.st'='push'\" git st", &ctx, &repo()).as_deref(), Some(ALIAS));
            decide(&["git -c include.path=/tmp/x.cfg st origin feat", "GIT_CONFIG_GLOBAL=/tmp/x.cfg git st", "GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.pager GIT_CONFIG_VALUE_0='sh -c x' git log"], &ctx, false);
            decide(&["git st", "git -c user.name=x st"], &ctx, true);
            // a replace ref swaps the base's tip for every git read of the shared repo
            decide(
                &[
                    "git -C ../repo replace main feat",
                    "git replace -f main abc",
                    "git replace main feat",
                    "git replace -d abc",
                    "git replace --edit HEAD",
                    "git replace --gra HEAD main",
                    "git replace --convert-graft-file",
                    "git fetch /tmp/o 'refs/replace/*:refs/replace/*'",
                ],
                &ctx,
                false,
            );
            decide(&["git replace", "git replace -l", "git replace --list 'a*'", "git replace --format=medium"], &ctx, true);
        }
    }

    #[test]
    fn merge_hint_matches_the_mode() {
        // PR mode: merge the base branch in, never rewrite a pushed branch (the app never force-pushes)
        assert_eq!(check("git merge feat", &Ctx { pr_mode: true, ..at("C:/p/repo") }, &repo()).as_deref(), Some(MERGE_PR));
        assert_eq!(check("git merge feat", &at("C:/p/repo"), &repo()).as_deref(), Some(MERGE));
        assert!(!MERGE_PR.contains("merge/rebase") && MERGE_PR.contains("rebase·amend·reset"));
    }

    #[test]
    fn lexes_quotes_substitutions_and_heredocs() {
        let words = |s: &str| -> Vec<Vec<String>> {
            segments(&lex(s, false).unwrap()).into_iter().map(|(w, _)| w.into_iter().map(|w| w.text.clone()).collect()).collect()
        };
        assert_eq!(words("git commit -m 'a b' \"c \\\"d\\\"\""), [["git", "commit", "-m", "a b", "c \"d\""]]);
        assert_eq!(words("a && b || c; d | e & f"), [["a"], ["b"], ["c"], ["d"], ["e"], ["f"]]);
        assert_eq!(words("echo $(git push) x"), [vec!["echo", "", "x"], vec!["git", "push"]]);
        assert_eq!(words("git log 2>&1 > out.txt | head"), [vec!["git", "log"], vec!["head"]]);
        assert_eq!(words("git log HEAD@{1} # git push"), [["git", "log", "HEAD@{1}"]]);
        assert_eq!(words("cd C:\\p\\repo"), [["cd", "C:\\p\\repo"]]);
        assert_eq!(words("cat <<'EOF'\ngit push\nEOF\ngit status"), [vec!["cat"], vec!["git", "status"]]);
        // a heredoc's text goes with the command that has the `<<`; a here-string is data too
        let data = |s: &str| -> Vec<Vec<String>> { segments(&lex(s, false).unwrap()).into_iter().map(|(_, d)| d.into_iter().map(String::from).collect()).collect() };
        assert_eq!(data("cat <<EOF | sh\ngit push\nEOF"), [vec!["git push\n"], vec![]]);
        assert_eq!(data("bash <<< 'git push'"), [["git push"]]);
        assert_eq!(words("xargs -n1 git <<< push"), [["xargs", "-n1", "git"]]);
        assert!(lex("git push \"", false).is_err());
        assert!(lex("echo 'x", false).is_err());
        assert!(lex("(git status", false).is_err());
        assert!(lex("git status)", false).is_err());
        // PowerShell: backtick escapes, backslashes stay, here-strings are data
        let ps = |s: &str| -> Vec<Vec<String>> { segments(&lex(s, true).unwrap()).into_iter().map(|(w, _)| w.into_iter().map(|w| w.text.clone()).collect()).collect() };
        assert_eq!(ps("git commit -m \"a `\"b`\"\""), [["git", "commit", "-m", "a \"b\""]]);
        assert_eq!(ps("cd \"C:\\p\\\""), [["cd", "C:\\p\\"]]);
        assert_eq!(ps("git commit -m @'\nit's done\n'@"), [["git", "commit", "-m", ""]]);
        assert!(lex("$x = \"a`", true).is_err());
    }

    #[test]
    fn normalizes_paths() {
        assert_eq!(resolve(None, "/c/Users/x/").as_deref(), Some("C:/Users/x"));
        assert_eq!(resolve(None, "C:\\Users\\x").as_deref(), Some("C:/Users/x"));
        assert_eq!(resolve(Some("C:/p/wt"), "../repo/./src/..").as_deref(), Some("C:/p/repo"));
        assert_eq!(resolve(None, "rel"), None);
        assert_eq!(resolve(None, "/cygdrive/d/x").as_deref(), Some("D:/x"));
        assert_eq!(resolve(None, "/usr/bin").as_deref(), Some("/usr/bin"));
        assert!(eq_path("c:\\P\\Repo\\", "C:/p/repo"));
        assert_eq!(program("\"C:/Program Files/Git/cmd/GIT.EXE"), "git");
        assert_eq!(program("C:\\Git\\bin\\git.exe"), "git");
    }

    #[test]
    fn follows_aliases() {
        let wt = at("C:/p/wt");
        let r = FakeRepo { aliases: HashMap::from([("a", "b"), ("b", "c"), ("c", "d"), ("d", "e"), ("e", "f"), ("f", "status"), ("m", "merge"), ("x", "!echo hi")]) };
        assert!(check("git a", &wt, &r).is_some(), "too deep");
        assert!(check("git f", &wt, &r).is_none());
        assert!(check("git m main", &wt, &r).is_none());
        assert!(check("git x", &wt, &r).is_none());
        assert!(check("git m feat", &at("C:/p/repo"), &r).is_some());
        // not an alias: an external git-<name>, allowed unless the config was changed on the way
        assert!(check("git lfs pull", &wt, &r).is_none());
        assert!(check("GIT_CONFIG_GLOBAL=/tmp/c git lfs push", &wt, &r).is_some());
    }

    #[test]
    fn checks_shell_tools_only() {
        let wt = at("C:/p/wt");
        let r = repo();
        assert!(check_tool("PowerShell", &json!({ "command": "Set-Location C:\\p\\repo; git merge feat" }), &wt, &r).is_some());
        assert!(check_tool("PowerShell", &json!({ "command": "$b = git rev-parse HEAD; git log $b" }), &wt, &r).is_none());
        assert!(check_tool("PowerShell", &json!({ "command": "$env:GIT_DIR = 'C:/p/repo/.git'; git merge feat" }), &wt, &r).is_some());
        assert!(check_tool("PowerShell", &json!({ "command": "git `\n  push" }), &wt, &r).is_some());
        assert!(check_tool("Bash", &json!({ "command": "git push" }), &wt, &r).is_some());
        // Monitor runs its command in Bash's shell
        assert!(check_tool("Monitor", &json!({ "command": "git push", "description": "x" }), &wt, &r).is_some());
        assert!(check_tool("Monitor", &json!({ "command": "tail -f log.txt" }), &wt, &r).is_none());
        assert!(check_tool("Write", &json!({ "file_path": "x", "content": "git push" }), &wt, &r).is_none());
        assert!(check_tool("Bash", &json!({}), &wt, &r).is_none());
        assert_eq!(git_verb_text("os.system('git push')"), Some(Hit::Deny));
        assert_eq!(git_verb_text("github pushes"), None);
        assert_eq!(git_verb_text("git\npush"), None);
        assert_eq!(git_verb_text("run(['git','reset','--hard'])"), Some(Hit::Deny));
        assert_eq!(git_verb_text("execSync('git branch -D x')"), Some(Hit::Deny));
        assert_eq!(git_verb_text("execSync('git branch -d x')"), None);
        assert_eq!(git_verb_text("run(['git','merge','feat'])"), Some(Hit::Merge));
        assert_eq!(git_verb_text("git merge --abort; git merge-base a b"), None);
    }
}
