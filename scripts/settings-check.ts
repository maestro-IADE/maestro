// The settings dialog's pure parts (src/settings/agentState.ts). Run: `npm test`.
import { deepStrictEqual, equal } from "node:assert/strict";
import { acpAction, acpShort, agentRowState, chatRuntimeNeeded, sectionOf } from "../src/settings/agentState.ts";
import type { AcpStatus, CliAgent } from "../src/types.ts";

const acp = (state: AcpStatus["state"], mode: AcpStatus["mode"] = "auto", message = ""): AcpStatus => ({
  state, message, node: "24.1.0", nodeMin: "22.0.0", adapterVersion: state === "ready" ? "0.102.0" : null, pinned: "0.102.0", dev: false, mode,
});
const cli = (status: CliAgent["status"], a: AcpStatus | null = null): CliAgent => ({
  id: "claude-code", status, installed: status !== "missing", path: null, version: "2.1.302", minVersion: null, loggedIn: null, installCmd: "", message: "", acp: a,
});

// agentRowState: the CLI first, then the chat runtime, then on / off
equal(agentRowState(undefined, true, true).label, "확인 중", "no CLI status yet");
equal(agentRowState(cli("ready"), false, true).label, "준비 중", "no runtime in this build (Codex)");
deepStrictEqual(agentRowState(cli("missing", acp("notInstalled")), true, false), { label: "설치 필요", tone: "warn" }, "the CLI before its chat runtime");
equal(agentRowState(cli("tooOld", acp("cliUnsupported")), true, false).label, "버전 낮음");
equal(agentRowState(cli("loginRequired", acp("loggedOut")), true, true).label, "로그인 필요");
deepStrictEqual(agentRowState(cli("ready", acp("notInstalled")), true, true), { label: "채팅 런타임 필요", tone: "warn" });
equal(agentRowState(cli("ready", acp("notInstalled")), true, false).label, "채팅 런타임 필요", "before 꺼짐 too (the order in the plan)");
equal(agentRowState(cli("ready", acp("nodeMissing")), true, true).label, "채팅 런타임 필요");
equal(agentRowState(cli("ready", acp("blocked")), true, true).label, "채팅 런타임 필요", "blocked for this run");
deepStrictEqual(agentRowState(cli("ready", acp("notInstalled", "pty")), true, true), { label: "사용 중", tone: "ok" }, "채팅 연결 터미널 경유 doesn't want the runtime");
deepStrictEqual(agentRowState(cli("ready", acp("ready")), true, true), { label: "사용 중", tone: "ok" });
deepStrictEqual(agentRowState(cli("ready"), true, false), { label: "꺼짐", tone: "muted" }, "an agent without a chat runtime");

// chatRuntimeNeeded
equal(chatRuntimeNeeded(null), false);
equal(chatRuntimeNeeded(acp("ready")), false);
equal(chatRuntimeNeeded(acp("broken")), true);
equal(chatRuntimeNeeded(acp("broken", "pty")), false);

// acpAction: one button at most
equal(acpAction(acp("notInstalled")), "install");
equal(acpAction(acp("broken")), "reinstall");
equal(acpAction(acp("blocked")), "recheck");
equal(acpAction(acp("nodeMissing")), "needNode");
equal(acpAction(acp("nodeTooOld")), "needNode");
equal(acpAction(acp("ready")), null);
equal(acpAction(acp("loggedOut")), null, "the login is the CLI's (상태·로그인)");

// acpShort
equal(acpShort(acp("ready")), "준비됨");
equal(acpShort(acp("notInstalled", "auto", "설치되지 않았어요")), "설치되지 않았어요");
equal(acpShort(acp("broken")), "준비되지 않았어요", "no message from the backend");

// sectionOf: five ids, anything else opens 일반 (the old run / files / style ids too)
equal(sectionOf("agents"), "agents");
equal(sectionOf("look"), "look");
equal(sectionOf(undefined), "general");
equal(sectionOf("style"), "general");
equal(sectionOf("run"), "general");

console.log("ok settings");
