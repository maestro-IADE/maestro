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
};
export type DiscardChoice = "clean" | "keep" | "discard";
export type WorktreeInfo = {
  path: string;
  branch: string;
  adapterId: string | null;
  permissionMode: string | null;
  name: string | null;
  description: string | null;
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
  view?: AgentView; // same conversation, two renderings
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
export type CanvasEdge = Edge<{ kind: "command_pipe" | "orchestrates" | "reference" | "arrow"; label?: string; editing?: boolean }>;

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
  | { type: "notice"; turnId: string; itemId: string; kind: "localCommand" | "interrupted"; text: string }
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
 * cliAnswer false: no terminal to hand it to (an ACP chat prompt), so no "CLI에서 답하기".
 */
export type Approval = { id: number; worktreePath: string; toolName: string; toolInput: any; canAlways: boolean; turnId?: string | null; toolCallId?: string | null; cliAnswer?: boolean };
/** `agent_ready` */
export type Readiness = { trustPrompt: boolean; failure: LaunchFailure | null; runtime: AgentRuntime; runtimeNote: FallbackReason | null };
export type ApprovalChoice = "allow" | "always" | "deny" | "cli";
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
};
/** `answer_merge`: what happened (an Err means the card stays: the folder isn't ready, or it was answered already) */
export type MergeResult = { outcome: "merged" | "rejected" | "stale" | "conflict" | "failed"; commit: string | null; message: string };
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
