import { invoke } from "@tauri-apps/api/core";
import type { WS } from "./store";
import type { Drawing } from "./drawing";
import type { RelFile, Relation } from "./relations";
import type { AgentDef, AgentView, Approval, ApprovalChoice, Branch, CliStatus, DiscardCheck, DiscardChoice, MergeFileDiff, MergeMode, MergeRequest, MergeResult, ModelChoice, Picked, PrOptions, PrReadiness, PtySnapshot, PullRequest, Readiness, Repo, RuntimeEvent, SwitchResult, SwitchWhen, TuiWait, UpdateAgentReq, UpdateAgentResult, WorktreeInfo } from "./types";
/** A Markdown file in Maestro's documents or playbooks; `summary` is its first line of body text. */
export type DocFile = { path: string; title: string; summary: string };

export const b64ToBytes = (b64: string) => Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));

export const ipc = {
  listRepos: () => invoke<Repo[]>("list_repos"),
  addRepo: (path: string) => invoke<Repo>("add_repo", { path }),
  repoState: (path: string) => invoke<"ready" | "empty" | "none">("repo_state", { path }),
  initRepo: (path: string) => invoke<void>("init_repo", { path }),
  removeRepo: (root: string) => invoke<void>("remove_repo", { root }),
  listWorktrees: (projectRoot: string) => invoke<WorktreeInfo[]>("list_worktrees", { projectRoot }),
  /** existingBranch: check out branchName as it is instead of making it new from baseBranch */
  createAgent: (req: { projectRoot: string; adapterId: string; permissionMode: string; branchName: string; baseBranch: string; existingBranch: boolean; name: string; description: string; model: string }) =>
    invoke<WorktreeInfo>("create_agent", { req }),
  listBranches: (projectRoot: string) => invoke<Branch[]>("list_branches", { projectRoot }),
  /** Maestro's documents: the personas, then each category (the same for every project) */
  libraryList: () => invoke<Array<{ folder: string; persona: boolean; files: DocFile[] }>>("library_list"),
  /** the playbooks (how the Maestro coordinates workers), as one folder */
  playbookList: () => invoke<Array<{ folder: string; persona: boolean; files: DocFile[] }>>("playbook_list"),
  docsCreate: (folder: string, name: string) => invoke<string>("docs_create", { folder, name }),
  docsImport: (folder: string, files: string[]) => invoke<void>("docs_import", { folder, files }),
  docsDelete: (path: string) => invoke<void>("docs_delete", { path }),
  docsCategory: (name: string) => invoke<void>("docs_category", { name }),
  personaCreate: (name: string) => invoke<string>("persona_create", { name }),
  mdTitle: (path: string) => invoke<string>("md_title", { path }),
  createMaestro: (req: { projectRoot: string; adapterId: string; permissionMode: string; model: string; baseBranch: string }) => invoke<WorktreeInfo>("create_maestro", { req }),
  attachAgent: (req: { projectRoot: string; worktreePath: string; adapterId: string; permissionMode: string; model: string }) => invoke<void>("attach_agent", { req }),
  /** an agent's name, description, permission mode, model or (Maestro) base branch, saved; applied says whether a restart applies it */
  updateAgent: (req: UpdateAgentReq) => invoke<UpdateAgentResult>("update_agent", { req }),
  /** an idle agent stopped and started again on the same conversation and view; rejects with why not (off, working, waiting) */
  restartAgent: (projectRoot: string, worktreePath: string) => invoke<void>("restart_agent", { projectRoot, worktreePath }),
  linkWorktrees: (projectRoot: string, parent: string, child: string) => invoke<void>("link_worktrees", { projectRoot, parent, child }),
  unlinkWorktrees: (projectRoot: string, parent: string, child: string) => invoke<void>("unlink_worktrees", { projectRoot, parent, child }),
  /** worktree "" with on=false: drop the reference everywhere */
  setRef: (projectRoot: string, worktree: string, md: string, on: boolean) => invoke<void>("set_ref", { projectRoot, worktree, md, on }),
  send: (projectRoot: string, worktreePath: string, text: string, filesAttached: string[] = []) =>
    invoke<void>("send_message", { projectRoot, worktreePath, text, filesAttached }),
  stop: (worktreePath: string) => invoke<void>("stop_agent", { worktreePath }),
  // the shared workspace (workspace.rs): every window's layout and tab groups, committed against a version
  wsGet: () => invoke<{ state: WS; version: number }>("ws_get"),
  wsCommit: (state: WS, base: number) => invoke<{ ok: boolean; version: number; state: WS | null }>("ws_commit", { state, base }),
  /** a file's unsaved editor text (null: none), kept where every window sees it */
  wsDraftSet: (key: string, text: string | null) => invoke<void>("ws_draft_set", { key, text }),
  wsDraftGet: (key: string) => invoke<string | null>("ws_draft_get", { key }),
  /** a hidden pop-out window that finished loading reports in */
  wsWindowReady: () => invoke<void>("ws_window_ready"),
  /** a window for a pop-out (a warm one from the pool), then shown (at screen x, y if given) */
  wsTakeWindow: () => invoke<string>("ws_take_window"),
  wsShowWindow: (label: string, x?: number, y?: number) => invoke<void>("ws_show_window", { label, x, y }),
  readTextFile:(base: string, rel: string) => invoke<{ text: string; mtime: number }>("read_text_file", { base, rel }),
  /** rejects with "CHANGED" when the file moved on since `expected` (unless force) */
  writeTextFile: (base: string, rel: string, text: string, expected: number, force = false) => invoke<number>("write_text_file", { base, rel, text, expected, force }),
  fileMtime: (base: string, rel: string) => invoke<number>("file_mtime", { base, rel }),
  fileAtHead: (base: string, rel: string) => invoke<string>("file_at_head", { base, rel }),
  /** the projects on screen in this window, and after how many minutes away the others' idle agents stop (0 = never) */
  /** the blocks selected on agent `owner`'s canvas, for `maestro selection` */
  /** what the agent `owner` drew with `maestro draw` since its canvas last took it */
  takeCanvasDrawings: (owner: string) => invoke<Drawing[]>("take_canvas_drawings", { owner }),
  setCanvasSelection: (owner: string, items: Picked[]) => invoke<void>("set_canvas_selection", { owner, items }),
  setAway: (activeRoots: string[], minutes: number) => invoke<void>("set_away", { activeRoots, minutes }),
  /** the `/` menu for this worktree's agent: its built-ins, then its custom commands and skills */
  slashCommands: (worktreePath: string, adapterId: string) =>
    invoke<Array<{ name: string; description: string; source: string; note?: string }>>("list_slash_commands", { worktreePath, adapterId }),
  agentCatalog: () => invoke<AgentDef[]>("agent_catalog"),
  /** `recheck` ("다시 확인"): also lifts a chat runtime blocked after failed starts or a crash loop */
  cliStatus: (recheck = false) => invoke<CliStatus>("cli_status", { recheck }),
  agentModels: (id: string) => invoke<ModelChoice[]>("agent_models", { id }),
  launchPreview: (id: string, permissionMode: string, model: string) => invoke<string>("launch_preview", { id, permissionMode, model }),
  setupStart: (id: string) => invoke<string>("setup_start", { id }),
  setupStop: (id: string) => invoke<void>("setup_stop", { id }),
  listApprovals: () => invoke<Approval[]>("list_approvals"),
  listTuiWaits: () => invoke<TuiWait[]>("list_tui_waits"),
  answerApproval: (id: number, choice: ApprovalChoice) => invoke<void>("answer_approval", { id, choice }),
  /** merge requests waiting for the user (also after a restart) */
  listMerges: () => invoke<MergeRequest[]>("list_merges"),
  /**
   * approve: the app merges with --no-ff (PR mode: commits if asked, pushes and opens the PR with `pr`) · reject (with an
   * optional reason). Rejects when it can't go ahead now: the card stays
   */
  answerMerge: (id: number, approve: boolean, reason?: string, pr?: PrOptions) => invoke<MergeResult>("answer_merge", { id, approve, reason: reason ?? null, pr: pr ?? null }),
  // PR mode (R6)
  projectMergeMode: (root: string) => invoke<MergeMode>("project_merge_mode", { root }),
  /** rejects with the reason PR mode can't be used (the mode stays local) or while a PR action runs */
  setMergeMode: (root: string, mode: MergeMode) => invoke<void>("set_merge_mode", { root, mode }),
  /** cached 5 minutes; recheck ("다시 확인") asks again */
  prReadiness: (root: string, recheck = false) => invoke<PrReadiness>("pr_readiness", { root, recheck }),
  listPrs: () => invoke<PullRequest[]>("list_prs"),
  /** poll the project's PRs (every project's without a root) now */
  refreshPrs: (root?: string) => invoke<void>("refresh_prs", { root: root ?? null }),
  /**
   * method "merge" | "squash", pinned to the head the card showed. ack: what the user confirmed with "그래도 병합" (the
   * red CI state shown, a change request); GitHub's state then must be green or exactly that, else it's refused.
   */
  mergePr: (workerPath: string, method: "merge" | "squash", headSha: string, ack: { ci: "failure" | "none" | null; changesRequested: boolean } | null) =>
    invoke<PullRequest>("merge_pr", { workerPath, method, headSha, ack }),
  /** "기준 브랜치 받기": fast-forward the project folder when it's clean and on the base */
  prSyncBase: (workerPath: string) => invoke<string>("pr_sync_base", { workerPath }),
  /** the failed CI jobs' logs, trimmed, as text for the dialog (nothing is sent by itself) */
  prFailedLog: (workerPath: string, jobId?: number) => invoke<string>("pr_failed_log", { workerPath, jobId: jobId ?? null }),
  openPr: (workerPath: string) => invoke<void>("open_pr", { workerPath }),
  dismissPr: (workerPath: string) => invoke<void>("dismiss_pr", { workerPath }),
  /** one file of a merge request, at the recorded merge base and tip */
  mergeFile: (id: number, path: string) => invoke<MergeFileDiff>("merge_file", { id, path }),
  summarize: (projectRoot: string, worktreePath: string, assistantAgent: string, assistantModel: string) =>
    invoke<string | null>("summarize_agent", { projectRoot, worktreePath, assistantAgent, assistantModel }),
  /** waits until the agent takes prompts (or shows the trust dialog, or failed); also says what runs it */
  agentReady: (projectRoot: string, worktreePath: string) => invoke<Readiness>("agent_ready", { projectRoot, worktreePath }),
  /**
   * The chat ↔ CLI switch: the conversation moves to the runtime of `view` (chat: ACP when it can, CLI: the
   * terminal), the same session either way. Starts the agent when none runs.
   */
  switchRuntime: (projectRoot: string, worktreePath: string, view: AgentView, when: SwitchWhen) =>
    invoke<SwitchResult>("switch_runtime", { projectRoot, worktreePath, view, when }),
  /**
   * A view mounts: starts the agent on the view's runtime when none runs; a running one stays where it is (never a
   * takeover on mount). Answers what runs it now.
   */
  startView: (projectRoot: string, worktreePath: string, view: AgentView) =>
    invoke<SwitchResult>("start_view", { projectRoot, worktreePath, view }),
  /**
   * The app's 에이전트 보기 (settings `agentView`): starts no view asked for (a delegation, a resume after away, a
   * quick prompt) run on it. The main window sends it at launch and on every change; memory only in the backend.
   */
  setViewDefault: (view: AgentView) => invoke<void>("set_view_default", { view }),
  /** after a "stuck" switch, with the user's yes: the conversation copied and opened in `view` */
  forkSession: (projectRoot: string, worktreePath: string, view: AgentView) =>
    invoke<SwitchResult>("fork_session", { projectRoot, worktreePath, view }),
  /** installs the chat runtime's adapter; progress comes as "acp://install" */
  acpInstall: (id: string) => invoke<void>("acp_install", { id }),
  acpInstallCancel: (id: string) => invoke<void>("acp_install_cancel", { id }),
  /** "auto": ACP when it can · "pty": always the terminal. Takes effect at the agent's next start */
  setChatRuntime: (id: string, mode: "auto" | "pty") => invoke<void>("set_chat_runtime", { id, mode }),
  acceptTrust: (worktreePath: string) => invoke<void>("accept_trust", { worktreePath }),
  ensureAgent: (projectRoot: string, worktreePath: string) => invoke<void>("ensure_agent", { projectRoot, worktreePath }),
  ptySnapshot: (worktreePath: string) => invoke<PtySnapshot>("pty_snapshot", { worktreePath }),
  ptyWrite: (worktreePath: string, data: string) => invoke<void>("pty_write", { worktreePath, data }),
  ptyResize: (worktreePath: string, cols: number, rows: number) => invoke<void>("pty_resize", { worktreePath, cols, rows }),
  /** the conversation as common events, and the stream (epoch, seq) it was read at */
  transcript: (projectRoot: string, worktreePath: string) => invoke<{ epoch: number; seq: number; events: RuntimeEvent[] }>("get_transcript", { projectRoot, worktreePath }),
  /** what discarding would lose: uncommitted files and commits the base branch lacks (empty for the Maestro and outside worktrees) */
  checkDiscard: (projectRoot: string, worktreePath: string, branch: string) => invoke<DiscardCheck>("check_discard", { projectRoot, worktreePath, branch }),
  /** "clean": refused if anything would be lost · "keep": the work stays on the branch · "discard": a backup ref stays. Returns the kept branch or backup ref */
  discardWorktree: (projectRoot: string, worktreePath: string, branch: string, choice: DiscardChoice) =>
    invoke<string | null>("discard_worktree", { projectRoot, worktreePath, branch, choice }),
  listDir: (path: string) => invoke<Array<{ name: string; isDir: boolean }>>("list_dir", { path }),
  gitStatus: (worktreePath: string) => invoke<Array<{ code: string; path: string }>>("git_status", { worktreePath }),
  loadCanvas: (projectRoot: string, key: string) => invoke<string | null>("load_canvas_state", { projectRoot, key }),
  saveCanvas: (projectRoot: string, key: string, stateJson: string) => invoke<void>("save_canvas_state", { projectRoot, key, stateJson }),
  /** the project's relations (relations.ts): its own, and the colour picked for a built-in one */
  loadRelations: (projectRoot: string) => invoke<RelFile>("load_relations", { projectRoot }),
  /** one change on the file as it is now (never the whole file: an agent may have declared since): returns the file after */
  changeRelations: (projectRoot: string, op: "upsert" | "remove" | "color", args: { relation?: Relation; id?: string; color?: string | null }) =>
    invoke<RelFile>("change_relations", { projectRoot, op, relation: args.relation ?? null, id: args.id ?? null, color: args.color ?? null }),
};
