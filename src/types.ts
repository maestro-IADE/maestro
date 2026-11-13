// Subset of PRD 6.3 implemented so far.
import type { Edge, Node } from "@xyflow/react";

export type AgentStatus = "RUNNING" | "IDLE" | "TERMINATED";

export type Repo = { root: string; name: string; defaultBranch: string };
/** What discarding a worktree would lose (worktree.rs `DiscardCheck`). */
export type DiscardCheck = {
  /** the branch the commits were compared with */
  base: string;
  /** uncommitted or untracked files, "XY path" */
  changes: string[];
  /** commits the base lacks (equivalent ones count as merged), "abc1234 subject" */
  commits: string[];
  /** it checked out a branch that existed before: the branch stays */
  keepBranch: boolean;
  /** PR mode: its PR was merged (commits up to the merged head count as merged, a squash too), or is still open */
  mergedByPr: { number: number; method?: string; open?: boolean } | null;
  /** a discard with nothing to lose still leaves a backup ref of the branch (a merged PR's branch the base lacks, as after a squash) */
  backup: boolean;
};
export type DiscardChoice = "clean" | "keep" | "discard";
export type WorktreeInfo = {
  path: string;
  branch: string;
  adapterId: string | null;
  permissionMode: string | null;
  name: string | null;
  description: string | null;
  /** passed as --model on every start; null = the CLI's own default */
  model: string | null;
  state: { kind: AgentStatus };
  /** the project's Maestro (coordinator) on the project folder itself */
  isMaestro: boolean;
  /** child worktree paths: its workers in the project hierarchy */
  workers: string[];
  /** Markdown files it refers to */
  refs: string[];
  /** made outside Maestro (found in the repo); may have no agent yet */
  external: boolean;
  /** titles of the personas linked to it */
  personas: string[];
  /** Maestro only: where workers branch off and merge back (null = the repo's default branch) */
  baseBranch: string | null;
  /** Maestro only: the project's merge mode */
  mergeMode?: MergeMode | null;
  /** titles of the other (non-persona) documents it refers to */
  docs: string[];
  /** what runs its conversation now; null while its agent is off */
  runtime: AgentRuntime | null;
  /** why its chat runs on the terminal (null: it doesn't, or nothing worth saying) */
  runtimeNote: FallbackReason | null;
};

/** "acp": the chat runtime (Claude Agent over ACP) · "pty": the agent's own terminal (TUI) */
export type AgentRuntime = "acp" | "pty";
/**
 * Why the chat runs on the terminal (acp_setup.rs `FallbackReason::view`): `kind` plus its fields, and `message`,
 * the muted note to show (null for `notSupported` and `cliView`).
 */
export type FallbackReason = { kind: string; message: string | null; [field: string]: unknown };
/** `switch_runtime` / `fork_session` (agent.rs `SwitchResult`) */
export type SwitchResult = {
  /** busy: not now (`busy` says why) · stuck: the old side didn't stop (a fork can open it) · superseded: a newer switch came */
  status: "done" | "busy" | "stuck" | "superseded";
  runtime: AgentRuntime;
  note: FallbackReason | null;
  busy: "turn" | "approval" | "shell" | null;
};
/**
 * `update_agent`: what to change on an agent made before. A field left out stays; "" clears it (name → the branch,
 * model → the CLI's default, baseBranch → the repo's default). baseBranch is the Maestro's only, and its name can't
 * change.
 */
export type UpdateAgentReq = {
  projectRoot: string;
  worktreePath: string;
  name?: string;
  description?: string;
  permissionMode?: string;
  model?: string;
  baseBranch?: string;
};
/** saved: nothing more to do · restart: the running agent keeps what it started with until it restarts (`restart_agent`) */
export type UpdateAgentResult = { info: WorktreeInfo; applied: "saved" | "restart" };
/** When a switch may happen: only now if quiet, after the turn ends, or interrupting the turn (and a background shell) */
export type SwitchWhen = "ifIdle" | "afterTurn" | "interrupt";
/** The chat runtime of one agent for Settings (acp_setup.rs `AcpStatus`). */
export type AcpStatus = {
  state: "ready" | "notInstalled" | "broken" | "nodeMissing" | "nodeTooOld" | "cliUnsupported" | "loggedOut" | "blocked";
  /** why not ready, in Korean; "" when ready */
  message: string;
  node: string | null;
  nodeMin: string;
  adapterVersion: string | null;
  pinned: string | null;
  /** the MAESTRO_ACP_CLAUDE_DIR folder is used, not the managed install */
  dev: boolean;
  mode: "auto" | "pty";
};
/** "acp://install": one npm output line, or the end */
export type AcpInstallEvent = { id: string; kind: "line" | "done" | "failed"; text: string };

export type Branch = { name: string; remote: boolean; inUse: boolean };

export type PromptInputBlockData = {
  label: string;
  attachedContextPaths: string[]; // relative to each worktree root
  lastSentPrompt?: string;
};

/** A block selected on an agent canvas, as that agent sees it (`maestro selection`, the chat box's chip). */
export type Picked =
  | { kind: "agent"; name: string; branch: string; path: string }
  | { kind: "document"; title: string; path: string }
  | { kind: "prompt"; name: string }
  // structure drawn on the canvas: a block (name + Markdown notes), an area holding blocks, and where arrows go
  | { kind: "block"; title: string; body: string; area?: string; to: string[] }
  | { kind: "area"; title: string; holds: string[]; to: string[] };

export type TerminalSize = "S" | "M" | "L";

// Agent status lives in the store, keyed by worktreePath.
export type PtyTerminalNodeData = {
  worktreePath: string;
  branchName: string;
  size?: TerminalSize; // only presets, no free resizing
  // chat or terminal is not the block's: the app's 에이전트 보기 or the agent's own for now (store `viewOverride`);
  // a `view` saved by an older build stays in the file, unread
  compact?: boolean; // shown as a small chip; its chat box comes up at the canvas bottom when selected
  full?: { width: number; height: number }; // the window size to go back to from compact
};

export type AgentView = "chat" | "cli";

export type PromptFlowNode = Node<PromptInputBlockData, "prompt_input_primitive">;
export type TerminalFlowNode = Node<PtyTerminalNodeData, "pty_terminal_node">;
/** Project canvas: one summary card per worktree; clicking it opens that worktree's canvas. */
export type WorktreeCardNode = Node<{ worktreePath: string; compact?: boolean }, "worktree_card">;
/** A Markdown file placed on a canvas; shows only its top `#` heading for now. */
export type MarkdownNode = Node<{ path: string; title: string }, "markdown_block">;
/** A structure block (FigJam-like): a name and, opened by a double click, Markdown notes kept with the canvas. */
export type StructNode = Node<{ title: string; body?: string }, "struct_block">;
/** An area that holds blocks and other areas (React Flow parent); dragging a block into it puts it inside. */
export type ZoneNode = Node<{ title: string }, "zone">;
export type CanvasNode = PromptFlowNode | TerminalFlowNode | WorktreeCardNode | MarkdownNode | StructNode | ZoneNode;
/**
 * command_pipe: prompt block → agent block (saved with the canvas)
 * orchestrates: parent → worker worktree, reference: Markdown block → worktree (both drawn from agent settings)
 * arrow: a drawn arrow between structure blocks / areas (saved with the canvas, may carry a label)
 */
/** `child`: a hierarchy line's worker (its folder), so the line can show that worker at work */
export type CanvasEdge = Edge<{ kind: "command_pipe" | "orchestrates" | "reference" | "arrow"; label?: string; editing?: boolean; child?: string }>;

export interface CanvasStateSchema {
  version: "1.0.0";
  rootPath: string;
  activeSkillId: string;
  activeThemeId: string;
  viewport: { x: number; y: number; zoom: number };
  /** 1 = laid out as a tree at least once; a canvas saved before that gets arranged once on load */
  tree?: number;
  nodes: Array<Pick<CanvasNode, "id" | "type" | "position" | "width" | "height" | "data" | "parentId">>;
  edges: Array<Pick<CanvasEdge, "id" | "source" | "target" | "data" | "sourceHandle" | "targetHandle">>;
}

/** How a turn ended (events.rs `TurnOutcome`). */
export type TurnOutcome =
  | { kind: "done"; stopReason: string }
  | { kind: "failed"; error: string; message: string }
  | { kind: "cancelled" }
  | { kind: "command" }
  | { kind: "replaced" };
/**
 * One step of an agent conversation, the same for every runtime (events.rs `RuntimeEvent`). Durable events come
 * with a `seq` and back from `get_transcript`; live-only ones (delta, approvals, exited, reset) have `seq: null`.
 */
export type RuntimeEvent =
  | { type: "turnStarted"; turnId: string; initiator: "user" | "agent" }
  | { type: "userMessage"; turnId: string; itemId: string; kind: "prompt" | "command" | "shell"; text: string; files: string[] }
  | { type: "agentText"; turnId: string; itemId: string; messageId: string | null; text: string }
  | { type: "thinking"; turnId: string; itemId: string; messageId: string | null; text: string }
  | { type: "toolStarted"; turnId: string; toolCallId: string; messageId: string | null; name: string; input: any }
  | { type: "toolUpdated"; turnId: string; toolCallId: string; status: "pending" | "running" | "completed" | "failed" | null; title: string | null; input: any | null; output: string | null }
  | { type: "toolFinished"; turnId: string; toolCallId: string; output: string; isError: boolean }
  | { type: "notice"; turnId: string; itemId: string; kind: "localCommand" | "interrupted" | "failed"; text: string }
  | { type: "error"; turnId: string | null; kind: string; message: string }
  | { type: "turnEnded"; turnId: string; outcome: TurnOutcome }
  | { type: "unknown"; turnId: string | null; source: string; raw: unknown }
  | { type: "delta"; turnId: string; itemId: string; channel: "text" | "thinking" | "toolOutput"; offset: number; text: string }
  | { type: "approvalRequested"; turnId: string | null; toolCallId: string | null; approvalId: number; toolName: string; toolInput: any; canAlways: boolean }
  | { type: "approvalResolved"; turnId: string | null; toolCallId: string | null; approvalId: number; resolution: "allow" | "always" | "deny" | "cli" | "timeout" | "withdrawn" }
  | { type: "exited"; turnId: string | null }
  | { type: "reset"; reason: "restart" | "source" | "truncated" | "switch" | "fallback" };
/** "agent://event". A newer `epoch` means the stream started over: drop what you hold and refetch. */
export type AgentEventPayload = { worktreePath: string; epoch: number; seq: number | null; event: RuntimeEvent };
export type AgentStatePayload = { worktreePath: string; state: { kind: AgentStatus } };
/** One agent of the backend's catalog (catalog.rs `AgentDef`): everything the UI knows about an agent comes from here. */
export type AgentDef = {
  id: string;
  name: string;
  /** file name under src/assets/agents/ */
  logo: string;
  bin: string;
  install: { kind: "npm"; package: string } | { kind: "guide"; url: string; text: string };
  loginCmd: string;
  /** runtimes Maestro can run it in now; empty = not launchable yet */
  runtimes: Array<"pty" | "acp">;
  acp: {
    package: string | null;
    version: string | null;
    args: string[];
    executableEnv: string | null;
    minVersion: string;
    entry: string;
    nodeMin: string;
    devDirEnv: string | null;
  } | null;
  minVersion: string | null;
  /** permission modes it takes, in picker order (catalog.rs `permission_modes`; agents.tsx `PERMISSION_NAMES`) */
  permissionModes: string[];
  features: { approvalHook: boolean; imageInput: boolean; sessionResume: boolean; modelCommand: boolean; assistant: string | null };
};
/** 미설치 / 버전 낮음 / 로그인 필요 / 준비됨 (cli.rs `decide`) */
export type CliAgentStatus = "missing" | "tooOld" | "loginRequired" | "ready";
export type CliAgent = {
  id: string;
  status: CliAgentStatus;
  installed: boolean;
  path: string | null;
  version: string | null;
  minVersion: string | null;
  /** null: couldn't tell from files */
  loggedIn: boolean | null;
  installCmd: string;
  /** why it isn't ready; "" when ready */
  message: string;
  /** its chat runtime (ACP adapter); null for an agent without one */
  acp: AcpStatus | null;
};
/** node: the Node.js version the chat runtime would run on (null: no node.exe found) */
export type CliStatus = { agents: CliAgent[]; npm: boolean; node: string | null };
export type ModelChoice = { id: string; label: string; description: string };
/** Why an agent's TUI didn't come up (agent.rs `LaunchFailure`). */
export type LaunchFailure = { kind: "loginRequired" | "exited" | "timeout"; message: string };
/**
 * turnId / toolCallId: the turn it was asked in and the tool call it is about (best effort; may be null).
 * cliAnswer false: no terminal to hand it to (an ACP chat prompt), so no "터미널에서 답하기".
 */
export type Approval = { id: number; worktreePath: string; toolName: string; toolInput: any; canAlways: boolean; turnId?: string | null; toolCallId?: string | null; cliAnswer?: boolean };
/** `agent_ready` */
export type Readiness = { trustPrompt: boolean; failure: LaunchFailure | null; runtime: AgentRuntime; runtimeNote: FallbackReason | null };
export type ApprovalChoice = "allow" | "always" | "deny" | "cli";
/**
 * The agent's terminal waits for an answer only the CLI view can give ("agent://tui-wait", `list_tui_waits`): a
 * permission card handed to the CLI ("cli") or out of time ("timeout"), or a picker seen in its output ("screen").
 * Nothing is sent to the agent meanwhile: it would land in that question.
 */
export type TuiWait = { worktreePath: string; source: "cli" | "timeout" | "screen"; toolName?: string | null; turnId?: string | null; toolCallId?: string | null };
/** A file a merge request changes (code: A M D R T); add/del null: binary. */
export type ChangedFile = { code: string; path: string; oldPath: string | null; add: number | null; del: number | null };
/**
 * A conductor's `maestro merge-request` waiting for the user (merge.rs). The app merges exactly `tipSha` into `base`
 * while it is still `baseSha`. "maestro://merge" brings one, "maestro://merge-done" ends it.
 */
export type MergeRequest = {
  id: number;
  root: string;
  /** the Maestro that asked: its blocks show the card */
  maestroPath: string;
  workerPath: string;
  workerName: string;
  verified: string | null;
  requestedAt: number;
  branch: string;
  base: string;
  baseSha: string;
  tipSha: string;
  mergeBase: string;
  /** "abc1234 subject", oldest first (at most 100; commitCount has them all) */
  commits: string[];
  commitCount: number;
  /** at most 500; fileCount, adds and dels count them all */
  files: ChangedFile[];
  fileCount: number;
  adds: number;
  dels: number;
  /** false: git older than 2.38 couldn't predict conflicts */
  conflictsChecked: boolean;
  workerUntracked: number;
  projectDirty: string[];
  /** the project's merge mode when asked: the approval does what was planned (absent on R5 requests: local) */
  mode?: MergeMode;
  /** PR mode: tracked files the worker hasn't committed (committed only if the user ticks "커밋하고 진행") */
  workerDirty?: string[];
  remote?: string | null;
  /** what commits and conflicts were computed against (`refs/remotes/<remote>/<base>` in PR mode when it exists) */
  cmpRef?: string;
  /** PR mode: the worker's open PR, which the approval updates by a push */
  existingPr?: number | null;
  /** PR mode: what the approval has done so far */
  pr?: PrProgress | null;
  /** PR mode: the title and body the card starts from */
  prDraft?: { title: string; body: string; draft: boolean } | null;
};
/** `answer_merge`: what happened (an Err means the card stays: the folder isn't ready, the remote moved, or it was answered already) */
export type MergeResult = { outcome: "merged" | "rejected" | "stale" | "conflict" | "failed" | "pr" | "recovered" | "pr-updated" | "found"; commit: string | null; message: string; pr?: PullRequest };

// ---------- PR mode (R6) ----------

/** The project's merge mode: "local" (R5's --no-ff in the project folder) or "pr" (push and a pull request) */
export type MergeMode = "local" | "pr";
/** `pr_readiness`: whether PR mode can run for the project, and the first reason it can't */
export type PrReadiness = {
  ok: boolean;
  remote: string | null;
  host: string;
  owner: string;
  repo: string;
  slug: string;
  gh: "missing" | "loggedOut" | "unknown" | "ready" | "";
  ghVersion: string | null;
  mergeAllowed: boolean;
  squashAllowed: boolean;
  message: string;
  checkedAt: number;
};
/** Where a PR-mode approval is (written before each outside call) */
export type PrStep = "none" | "committing" | "committed" | "pushing" | "pushed" | "creating" | "unconfirmed";
export type PrProgress = {
  step: PrStep;
  commitMessage: string | null;
  commitParent: string | null;
  appCommit: string | null;
  /** the app's commit holds other files than the card showed (a commit hook changed them): never pushed, the request has to be made again */
  commitMismatch: string | null;
  pushSha: string | null;
  createAttemptAt: number | null;
  title: string;
  body: string;
  draft: boolean;
  lastEmptyLookupAt: number | null;
  error: string | null;
  /** what a lookup found, not a failure ("기존 PR #N을 찾았어요 …"): shown muted */
  notice: string | null;
};
/** What the card sends with an approval in PR mode */
export type PrOptions = { commitMessage: string | null; title: string; body: string; draft: boolean; updateBody: boolean; recreate: boolean; checkOnly: boolean };
export type FailedJob = { name: string; jobId: number | null; runId: number | null };
export type PrChecks = { state: "none" | "pending" | "success" | "failure"; passed: number; failed: number; pending: number; failedJobs: FailedJob[]; headSha: string };
/** A worker's pull request as the app knows it (pr.rs `PrRecord`); "maestro://pr" brings one, "maestro://pr-done" drops it */
export type PullRequest = {
  root: string;
  maestroPath: string;
  workerPath: string;
  workerName: string;
  branch: string;
  base: string;
  remote: string;
  host: string;
  owner: string;
  repo: string;
  number: number;
  url: string;
  appCreated: boolean;
  state: "OPEN" | "CLOSED" | "MERGED";
  isDraft: boolean;
  headSha: string;
  reviewDecision: string | null;
  mergeable: string;
  mergeStateStatus: string;
  autoMerge: boolean;
  queued: boolean;
  checks: PrChecks;
  ciNotedSha: string | null;
  mergeAttempt: { method: string; seenHead: string; at: number } | null;
  merged: { method: string; commit: string | null; head: string; at: number } | null;
  mergedHeads: string[];
  afterMerge: { baseUpdated: string | null; message: string } | null;
  error: string | null;
  /** a status, not a failure: what the last approval did ("기존 PR #N을 찾아 이어 썼어요", a body not updated: its
   * card is gone by then), or where a merge click stands ("병합 대기열/자동 병합에 들어갔어요") */
  notice: string | null;
  checkedAt: number;
  hidden: boolean;
  /** merged by the user's click, the conductor's note not sent yet (sent on the next start if the app went down) */
  noteOwed: boolean;
};
/** "maestro://pr-poll": polling a host paused (rate limit, login) or resumed (pausedUntil 0) */
export type PrPollPayload = { host: string; pausedUntil: number; message: string };
/** `merge_file`: one file at the recorded merge base (original) and tip (modified); null: not there */
export type MergeFileDiff = { original: string | null; modified: string | null; binary: boolean; tooLarge: boolean };
/** "maestro://merge-done" */
export type MergeDonePayload = { id: number; maestroPath: string; outcome: string };
/** "agent://runtime": what runs the conversation now (started, switched, or fell back to the terminal) */
export type AgentRuntimePayload = { worktreePath: string; runtime: AgentRuntime; note: FallbackReason | null };
/** "agent://switch": a switch is waiting for the turn to end */
/** "agent://switch": a switch waits for the turn to end (`pending`: the view it moves to); null: that wait is over without a swap */
export type AgentSwitchPayload = { worktreePath: string; pending: AgentView | null };
/** `pty_snapshot`. terminal false: the chat runtime owns the conversation, there's no terminal to show */
export type PtySnapshot = { dataB64: string; seq: number; epoch: number; terminal: boolean };
/** "pty://data". `seq` = bytes so far; a newer `epoch` = a new process under this key (the terminal starts over). */
export type PtyDataPayload = { worktreePath: string; dataB64: string; seq: number; epoch: number };
/** "pty://exit": the process under this key ended with this exit code (settings' setup terminals close on 0). */
export type PtyExitPayload = { worktreePath: string; code: number };
