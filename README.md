<p align="center">
  <img src="docs/assets/logo.gif" width="140" alt="Maestro logo" />
</p>

<h1 align="center">Maestro</h1>

<p align="center">
  <strong>English</strong> · <a href="README.ko.md">한국어</a>
</p>

<p align="center">
  <strong>Interactive Agentic Development Environment</strong><br/>
  An interactive development environment for conducting coding agents
</p>

<p align="center">
  <a href="https://www.maestro-iade.com/">Website</a> ·
  <a href="https://github.com/maestro-IADE/maestro/releases/latest">Download</a> ·
  <a href="https://github.com/maestro-IADE/maestro/issues">Send feedback</a>
</p>

<p align="center">
  <a href="https://github.com/maestro-IADE/maestro/releases/latest"><img src="https://img.shields.io/github/v/release/maestro-IADE/maestro?style=flat&include_prereleases" alt="Latest release" /></a>
  <img src="https://img.shields.io/badge/status-early%20preview-orange?style=flat" alt="Early preview" />
  <a href="LICENSE.md"><img src="https://img.shields.io/badge/license-FSL--1.1--MIT-555?style=flat" alt="License: FSL-1.1-MIT" /></a>
</p>

<!-- [Slot: hero image] One screenshot of the canvas after the UI rework (960px wide, docs/assets/readme-hero.png). Uncomment when ready.
<p align="center"><img src="docs/assets/readme-hero.png" alt="The Maestro canvas: a conductor agent handing work to worker agents" width="960" /></p>
-->

> [!WARNING]
> **Maestro is not production-ready yet.** This is an early preview that shows where it is heading. Features are missing or changing, and the interface is being rebuilt. Back up any repository you care about before using it, and check the results yourself.

## Why Maestro

Coding agents can now do a lot on their own. But the more agents you run, the more work lands back on you: switching terminal tabs, splitting branches by hand, keeping track of who changed what.

Maestro started from one question:

> If people conducted instead of playing every instrument, what would a development environment look like?

A conductor doesn't play the instruments. They divide the score, hand parts to the players, point out where the music should go, and bring it all together. Maestro helps developers take that place: design the work together with agents, let several agents work side by side, and keep every change under your own control.

## How it thinks

### 1. An agent that conducts
Every project has a **maestro**. It splits a request into tasks, creates worker agents for them, and when the workers finish, takes their results and merges them into the base branch. Instead of writing every line, you set the direction and judge the results.

### 2. Point, then direct
What is hard to explain in words, you draw. Sketch the structure on a canvas with areas, blocks, and arrows, then pick a block and give it instructions directly. Blocks hold specs in Markdown, and the maestro can draw structures too. We believe this is closer to how people think than a single chat box.

### 3. Process written as documents
Build a team from **personas** (planner, implementer, reviewer, and so on) and **playbooks** that describe how work gets divided. To change how the team works, you edit documents, not code.

## Principles

- **Isolation.** Each agent works on its own branch in its own git worktree. Your original folder stays untouched.
- **Parallelism.** Several workers build at the same time, and the maestro is told each time one finishes.
- **Control.** When an agent wants to run a command or change a file, an approval card appears. Agents never merge on their own either: the app merges once you approve.
- **Bring your own agents.** Maestro does not build a new agent. It launches the CLI agents you already use and puts its interface on top. Your subscriptions and settings stay as they are.
- **Honest limits.** A worktree separates files; it is not a sandbox that contains the agent. The guard that blocks risky commands is there to catch mistakes, not to act as a security boundary.
- **On your machine.** Maestro itself sends no usage data anywhere. Its state lives in `~/.maestro` on your computer. Your code and conversations go only where each agent CLI sends them (Anthropic, Google, and so on).

## What works today

| Agent | Status |
|---|---|
| [Claude Code](https://docs.anthropic.com/en/docs/claude-code) | Chat and terminal views |
| [Antigravity](https://antigravity.google/) | Terminal view |
| [Codex](https://github.com/openai/codex) | Coming soon |

See the [website](https://www.maestro-iade.com/) for more on the features.

<!-- [Slot: feature cards] Fill in once the feature work settles, one cell per feature: title + two sentences + image.
<table>
<tr>
<td width="50%" valign="middle">

### Feature name
Two-sentence description.

</td>
<td width="50%"><img src="docs/assets/readme-features/feature.webp" alt="Image description" width="100%" /></td>
</tr>
</table>
-->

## What's next

<!-- [Slot: roadmap] Polish before going public. One line each, [x] when done. -->
- [ ] Rebuild the interface (UI/UX)
- [ ] Multiple languages, with English as the default
- [ ] Codex support
- [ ] Auto-updates and code signing

## Try it

Download the file for your OS from the [latest release](https://github.com/maestro-IADE/maestro/releases/latest) (Windows `.exe`/`.msi`, macOS `.dmg`, Linux `.AppImage`/`.deb`/`.rpm`).

You will need:

- [Git](https://git-scm.com/)
- The agent CLI you want to use, installed and run once to log in
- [Node.js](https://nodejs.org/) 22 or later, to use Claude Code in the chat view
- [GitHub CLI (`gh`)](https://cli.github.com/), only for the pull-request merge mode

The builds are not code-signed yet, so Windows and macOS may warn that the app is unverified.
<!-- TODO: add one line on opening an unsigned app on macOS -->

<!-- [Slot: FAQ] Bring over the website FAQ (Is it free? / Which agents? / Does my code leave my machine? / Which OSes?), one collapsed <details> per question.
## FAQ
<details><summary>Is it free?</summary>

Answer.
</details>
-->

## Build it with us

At this early stage, what you notice while using it helps most. Please leave bugs, rough edges, and ideas for how you would like to conduct your agents in [Issues](https://github.com/maestro-IADE/maestro/issues).

To build from source you need [Rust](https://rustup.rs/), [Node.js](https://nodejs.org/) 22 or later, and the [Tauri prerequisites](https://tauri.app/start/prerequisites/).

```bash
npm install
npm run tauri dev
```

Architecture and implementation notes live in [docs/](docs/README.md) (in Korean).

## License

[Functional Source License 1.1, MIT Future License (FSL-1.1-MIT)](LICENSE.md)

You are free to use and modify Maestro for yourself or inside your company, and for non-commercial education and research. The one thing not allowed is offering it to others as a commercial product or service that replaces Maestro or does substantially the same thing. Each version becomes available under the MIT license two years after it is released.
