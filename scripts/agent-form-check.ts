// The new agent and agent settings dialogs' pure parts (src/agentForm.ts). Run: `npm test`.
import { deepStrictEqual, equal } from "node:assert/strict";
import { agentPatch, branchTaken, maestroBaseArg, nameFromTask, nextAgentBranch, type AgentFields } from "../src/agentForm.ts";
import type { Branch } from "../src/types.ts";

const br = (name: string, remote = false): Branch => ({ name, remote, inUse: false });

// nextAgentBranch: the first agent-N from `from` up that no branch has, local or remote
equal(nextAgentBranch(null, 3), "agent-3", "branches not read yet: the count's guess");
equal(nextAgentBranch([], 0), "agent-1", "never below 1");
equal(nextAgentBranch([br("main"), br("agent-3"), br("agent-4", true)], 3), "agent-5", "local and remote-only names are both taken");
equal(nextAgentBranch([br("agent-1"), br("agent-3")], 1), "agent-2", "a gap is used");

// branchTaken: a typed new branch name the repo has
equal(branchTaken([br("main"), br("feature", true)], "main"), true);
equal(branchTaken([br("main"), br("feature", true)], " feature "), true, "remote-only, and trimmed");
equal(branchTaken([br("main")], "agent-9"), false);
equal(branchTaken([br("main")], "  "), false, "an empty name is not taken (the button stays off for another reason)");
equal(branchTaken(null, "main"), false, "branches not read yet");

// nameFromTask: the first non-empty line, at most 40 characters
equal(nameFromTask(""), "");
equal(nameFromTask("\n\n  로그인 화면의 오류 문구를 고쳐 줘  \n자세한 건 이슈 12"), "로그인 화면의 오류 문구를 고쳐 줘");
equal(nameFromTask("a".repeat(40)), "a".repeat(40), "40 fits");
const long = nameFromTask("가".repeat(60));
equal(long.length, 40);
equal(long.endsWith("…"), true);

// agentPatch: only what changed; the Maestro's name is fixed and only it has a base branch
const was: AgentFields = { name: "api", description: "테스트", permissionMode: "acceptEdits", model: "", baseBranch: "main" };
const same = agentPatch(was, { ...was, name: " api ", description: "테스트 " }, false);
deepStrictEqual(same, { patch: {}, changed: false, restart: false }, "trimmed values are the same");
deepStrictEqual(agentPatch(was, { ...was, name: "" }, false), { patch: { name: "" }, changed: true, restart: false }, "an emptied name clears it (falls back to the branch)");
deepStrictEqual(agentPatch(was, { ...was, name: "x", description: "y" }, false).restart, false, "name and description need no restart");
deepStrictEqual(agentPatch(was, { ...was, permissionMode: "default" }, false), { patch: { permissionMode: "default" }, changed: true, restart: true });
deepStrictEqual(agentPatch(was, { ...was, model: "opus" }, false), { patch: { model: "opus" }, changed: true, restart: true });
deepStrictEqual(agentPatch({ ...was, model: "opus" }, { ...was, model: "" }, false).patch, { model: "" }, "back to the CLI's default");
deepStrictEqual(agentPatch(was, { ...was, baseBranch: "dev" }, false), { patch: {}, changed: false, restart: false }, "a plain agent has no base branch to change");
deepStrictEqual(agentPatch(was, { ...was, name: "다른 이름", baseBranch: "dev" }, true), { patch: { baseBranch: "dev" }, changed: true, restart: true }, "the Maestro: no name, a base branch that goes into its brief");

// maestroBaseArg: the default the dialog filled in goes as "" (not checked), a picked branch as itself
equal(maestroBaseArg("HEAD", "HEAD"), "", "a detached checkout with no origin: the repo default, not a refused \"HEAD\"");
equal(maestroBaseArg("main", "main"), "", "origin's default, maybe not checked out here");
equal(maestroBaseArg(" dev ", "main"), "dev", "a branch the user picked is checked");
equal(maestroBaseArg("", "main"), "");

console.log("ok agent form");
