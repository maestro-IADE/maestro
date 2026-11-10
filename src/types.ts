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
};

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
  | { type: "reset"; reason: "restart" | "source" | "truncated" | "switch" | "fallback" | "replay" };
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
  acp: { package: string | null; version: string | null; args: string[]; executableEnv: string | null; minVersion: string } | null;
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
};
export type CliStatus = { agents: CliAgent[]; npm: boolean };
export type ModelChoice = { id: string; label: string; description: string };
/** Why an agent's TUI didn't come up (agent.rs `LaunchFailure`). */
export type LaunchFailure = { kind: "loginRequired" | "exited" | "timeout"; message: string };
/** turnId / toolCallId: the turn it was asked in and the tool call it is about (best effort; may be null) */
export type Approval = { id: number; worktreePath: string; toolName: string; toolInput: any; canAlways: boolean; turnId?: string | null; toolCallId?: string | null };
export type ApprovalChoice = "allow" | "always" | "deny" | "cli";
/** "pty://data". `seq` = bytes so far; a newer `epoch` = a new process under this key (the terminal starts over). */
export type PtyDataPayload = { worktreePath: string; dataB64: string; seq: number; epoch: number };
