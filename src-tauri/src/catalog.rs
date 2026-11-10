//! Every agent CLI Maestro knows, in one place: how to find, install, version-check and log in to it, which
//! runtimes run it, and what it supports. The frontend gets this list (`agent_catalog`) instead of keeping its own;
//! detection (`cli.rs`) and launching (`agent.rs`, `antigravity.rs`) read it from here.

use serde::Serialize;

/// How an agent can be run. Only `Pty` exists today; `Acp` is what the ACP runtime (PRD R4) will add.
#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Runtime {
    Pty,
    #[allow(dead_code)] // no agent lists it until the ACP runtime exists
    Acp,
}

/// How the settings' setup terminal gets the CLI.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Install {
    Npm { package: &'static str },
    /// not on npm: the user installs it from this page
    Guide { url: &'static str, text: &'static str },
}

/// The ACP adapter that would run the agent in the ACP runtime (findings of docs/architecture/acp-runtime-research.md).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Acp {
    /// npm package of a translator process; None = the CLI is its own ACP server (`args`)
    pub package: Option<&'static str>,
    /// adapter version pinned for the managed install
    pub version: Option<&'static str>,
    pub args: &'static [&'static str],
    /// env var the adapter reads the user's CLI path from
    pub executable_env: Option<&'static str>,
    /// lowest CLI version known to work with the adapter
    pub min_version: &'static str,
}

/// Where the login state is read from, without running anything (`cli::logged_in`).
#[derive(Clone, Copy, Debug)]
pub enum Login {
    /// `~/.claude/.credentials.json`, `~/.claude.json`'s account, or API key / cloud provider env
    Claude,
    /// `~/.codex/auth.json` or `OPENAI_API_KEY`
    Codex,
    /// agy's token: Windows Credential Manager `gemini:antigravity`, elsewhere `antigravity-oauth-token`
    Antigravity,
}

/// Where the chat's `/` menu comes from, beyond `slash_builtins`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Slash {
    /// `.claude/commands/**/*.md` and `.claude/skills/*/SKILL.md`, worktree and home (`commands::list`)
    Claude,
    None,
}

/// Models offered for `--model` and the chat's `/model` list (`agent_models`).
pub enum Models {
    /// (alias, description)
    Fixed(&'static [(&'static str, &'static str)]),
    /// asked from the CLI: `<bin> <args>` prints one `id<TAB>label` per line
    Command(&'static [&'static str]),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Features {
    /// permission prompts come to the chat as cards (a `PermissionRequest` hook, approval.rs)
    pub approval_hook: bool,
    /// pasted images reach the model (not wired in Maestro's chat yet)
    pub image_input: bool,
    /// the same conversation continues after a restart
    pub session_resume: bool,
    /// `/model` in chat opens Maestro's model list instead of the TUI's picker
    pub model_command: bool,
    /// can answer one-off questions for the assistant model (summaries): its default model, "" = the CLI's own
    pub assistant: Option<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDef {
    pub id: &'static str,
    pub name: &'static str,
    /// file name under `src/assets/agents/`
    pub logo: &'static str,
    /// executable name, looked up on PATH (Windows: `.exe`, then `.cmd`/`.bat`)
    pub bin: &'static str,
    /// where it is usually installed, relative to home: searched after PATH, which may be stale or bare
    #[serde(skip)]
    pub dirs: &'static [&'static str],
    pub install: Install,
    /// what the setup terminal runs once it's installed, so login and first-run screens happen there
    pub login_cmd: &'static str,
    /// runtimes Maestro can run it in now; empty = not launchable yet ("준비 중")
    pub runtimes: &'static [Runtime],
    pub acp: Option<Acp>,
    #[serde(skip)]
    pub version_args: &'static [&'static str],
    /// lowest version the PTY runtime works with (flags and hooks it passes); None = not gated
    pub min_version: Option<&'static str>,
    #[serde(skip)]
    pub login: Login,
    /// lowercase text in the TUI's startup output that means it wants a login (or its first-run setup) first
    #[serde(skip)]
    pub login_hints: &'static [&'static str],
    #[serde(skip)]
    pub slash: Slash,
    /// (name, description, note) of its own commands worth offering in chat
    #[serde(skip)]
    pub slash_builtins: &'static [(&'static str, &'static str, Option<&'static str>)],
    #[serde(skip)]
    pub models: Option<Models>,
    pub features: Features,
}

const OPENS_IN_CLI: Option<&str> = Some("CLI 보기에서 열려요");

/// The order is the frontend's (pickers, settings tiles); the first is the fallback default.
pub static AGENTS: [AgentDef; 3] = [
    AgentDef {
        id: "claude-code",
        name: "Claude Code",
        logo: "claude-code.svg",
        bin: "claude",
        // native installer, its old local install, npm's default global prefix on Windows
        dirs: &[".local/bin", ".claude/local", ".claude/local/node_modules/.bin", "AppData/Roaming/npm"],
        install: Install::Npm { package: "@anthropic-ai/claude-code" },
        login_cmd: "claude",
        runtimes: &[Runtime::Pty],
        acp: Some(Acp {
            package: Some("@agentclientprotocol/claude-agent-acp"),
            version: Some("0.102.0"),
            args: &[],
            executable_env: Some("CLAUDE_CODE_EXECUTABLE"),
            min_version: "2.1.226", // Paradev acpCatalogue.ts, checked against adapter 0.85.0
        }),
        version_args: &["--version"],
        // PermissionRequest hook 2.0.45, its "always allow" permission updates 2.0.54 (Claude Code changelog)
        min_version: Some("2.0.54"),
        login: Login::Claude,
        login_hints: &["select login method", "choose the text style", "please run /login", "invalid api key", "not logged in"],
        slash: Slash::Claude,
        // Ones that open a picker or dialog only work in the terminal.
        slash_builtins: &[
            ("add-dir", "작업 폴더 추가", None),
            ("agents", "서브에이전트 관리", OPENS_IN_CLI),
            ("clear", "대화 지우고 새로 시작", Some("새 세션이 시작돼 Chat 보기가 이어지지 않아요")),
            ("compact", "대화를 요약해 컨텍스트 줄이기", None),
            ("config", "설정", OPENS_IN_CLI),
            ("context", "컨텍스트 사용량 보기", None),
            ("cost", "이번 세션 비용·시간", None),
            ("doctor", "설치 상태 점검", OPENS_IN_CLI),
            ("help", "도움말", None),
            ("hooks", "훅 설정", OPENS_IN_CLI),
            ("init", "CLAUDE.md 만들기", None),
            ("mcp", "MCP 서버 관리", OPENS_IN_CLI),
            ("memory", "메모리 파일 편집", OPENS_IN_CLI),
            ("model", "모델 바꾸기", None),
            ("permissions", "권한 규칙", OPENS_IN_CLI),
            ("review", "변경 사항 리뷰", None),
            ("rewind", "이전 시점으로 되돌리기", OPENS_IN_CLI),
            ("security-review", "보안 리뷰", None),
            ("status", "버전·계정·모델 상태", OPENS_IN_CLI),
            ("usage", "사용량 한도", OPENS_IN_CLI),
        ],
        // the aliases Claude Code accepts for `--model` and `/model`
        models: Some(Models::Fixed(&[
            ("default", "기본값 (계정 권장 모델)"),
            ("best", "지금 쓸 수 있는 가장 좋은 모델"),
            ("opus", "Opus · 어렵고 긴 작업"),
            ("sonnet", "Sonnet · 빠르고 균형 잡힘"),
            ("haiku", "Haiku · 가장 빠르고 저렴"),
            ("fable", "Fable"),
            ("opusplan", "계획은 Opus, 실행은 Sonnet"),
            ("opus[1m]", "Opus · 100만 토큰 컨텍스트"),
            ("sonnet[1m]", "Sonnet · 100만 토큰 컨텍스트"),
        ])),
        features: Features { approval_hook: true, image_input: false, session_resume: true, model_command: true, assistant: Some("haiku") },
    },
    AgentDef {
        id: "antigravity",
        name: "Antigravity",
        logo: "antigravity.png",
        // ships with the Antigravity app (antigravity.google), replaced Gemini CLI for personal accounts in June 2026
        bin: "agy",
        dirs: &["AppData/Local/agy/bin", ".local/bin"],
        install: Install::Guide { url: "https://antigravity.google", text: "npm 패키지가 아니에요. 앱(antigravity.google)을 설치하면 함께 깔려요. 설치 후 다시 확인을 눌러 주세요." },
        login_cmd: "agy",
        runtimes: &[Runtime::Pty],
        // agy speaks ACP itself from 1.5.0; 1.5.4 fixed resume dropping the mode and a hang on a second approval
        acp: Some(Acp { package: None, version: None, args: &["acp"], executable_env: None, min_version: "1.5.4" }),
        version_args: &["--version"],
        // `--mode auto` on Windows (agy changelog 1.4.4)
        min_version: Some("1.4.4"),
        login: Login::Antigravity,
        // shortcut: not seen in a real signed-out run yet
        login_hints: &["sign in with google", "please sign in", "not signed in", "login required"],
        // agy has its own commands and skills (`~/.gemini/...`), not read yet; Claude's must not show
        slash: Slash::None,
        slash_builtins: &[],
        models: Some(Models::Command(&["models"])),
        features: Features { approval_hook: false, image_input: false, session_resume: true, model_command: false, assistant: Some("") },
    },
    AgentDef {
        id: "codex",
        name: "Codex",
        logo: "codex.svg",
        bin: "codex",
        dirs: &["AppData/Roaming/npm", ".local/bin"],
        install: Install::Npm { package: "@openai/codex" },
        login_cmd: "codex login",
        runtimes: &[], // no transcript reader or argv yet
        acp: Some(Acp {
            package: Some("@agentclientprotocol/codex-acp"),
            version: Some("2.6.0"),
            args: &[],
            executable_env: Some("CODEX_PATH"),
            min_version: "0.160.0", // Paradev acpCatalogue.ts (conservative); not run here
        }),
        version_args: &["--version"],
        min_version: None,
        login: Login::Codex,
        login_hints: &["sign in with chatgpt", "not logged in"],
        slash: Slash::None,
        slash_builtins: &[],
        models: None,
        features: Features { approval_hook: false, image_input: false, session_resume: false, model_command: false, assistant: None },
    },
];

pub fn get(id: &str) -> Option<&'static AgentDef> {
    AGENTS.iter().find(|a| a.id == id)
}

/// Like `get`, for code paths that start an agent.
pub fn require(id: &str) -> Result<&'static AgentDef, String> {
    get(id).ok_or(format!("알 수 없는 에이전트: {id}"))
}

/// The agent a worktree gets when nothing else says which.
pub fn default_id() -> &'static str {
    AGENTS[0].id
}

impl AgentDef {
    pub fn launchable(&self) -> bool {
        self.runtimes.contains(&Runtime::Pty)
    }

    pub fn npm_package(&self) -> Option<&'static str> {
        match self.install {
            Install::Npm { package } => Some(package),
            Install::Guide { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_unique_and_launchable_ones_have_a_floor() {
        let mut ids: Vec<_> = AGENTS.iter().map(|a| a.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), AGENTS.len());
        assert_eq!(default_id(), "claude-code");
        for a in AGENTS.iter().filter(|a| a.launchable()) {
            assert!(a.min_version.is_some(), "{}", a.id);
            assert!(a.login_hints.iter().all(|h| *h == h.to_lowercase()), "{}", a.id);
        }
        assert!(!get("codex").unwrap().launchable());
        assert!(get(crate::antigravity::ID).is_some_and(|a| a.launchable())); // the PTY runtime's own branch

        assert!(require("nope").is_err());
    }

    #[test]
    fn slash_lists_are_per_agent() {
        assert!(get("claude-code").unwrap().slash_builtins.iter().any(|c| c.0 == "compact"));
        // Antigravity must not borrow Claude's commands
        let agy = get("antigravity").unwrap();
        assert!(agy.slash_builtins.is_empty() && agy.slash == Slash::None);
    }
}
