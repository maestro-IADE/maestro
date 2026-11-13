import type { ReactNode } from "react";
import { Columns2, Copy, ExternalLink, GitBranch, MessageSquare, Plus, Settings2, SquareTerminal, Trash2 } from "./icons";
import { openMenu, type MenuAt, type MenuItem } from "./ui/Menu";
import { useStore, type Worktree } from "./store";
import { useSettings } from "./settings";
import { useDiscard } from "./discard";
import { copyText, openInSplit, openInWindow, showAgentAs } from "./agentActions";
import { effectiveView, viewSwitch } from "./settingsMigrate";
import { keysOf } from "./shortcuts";

/**
 * An agent's menu (docs/features/ui/menus-and-dialogs.md "에이전트 메뉴"): the same items wherever the agent shows, the
 * sidebar row, a worker card and a map card (⋯ or a right click). `dialog` is the discard dialog to render beside
 * the caller (its events bubble through the React tree, so not inside a clickable block); `busy` while discarding.
 */
export function useAgentMenu(w: Worktree | undefined): { open: (at: MenuAt, owner?: Element) => void; dialog: ReactNode; busy: ReturnType<typeof useDiscard>["busy"] } {
  const { discard, busy, dialog } = useDiscard(w);
  const open = (at: MenuAt, owner?: Element) => {
    if (!w) return;
    const s = useStore.getState();
    // 터미널로 보기 / 채팅으로 돌아가기: this agent's view for now, from the one it shows (docs/features/agents/chat-view.md)
    const global = useSettings.getState().agentView;
    const flip = viewSwitch(effectiveView(w.path, s.viewOverride, global), global);
    const items: MenuItem[] = !w.adapterId
      ? [
          { label: "열기", onSelect: () => s.select(w.path) },
          // its tab shows how to attach an agent (AttachAgent)
          { label: "에이전트 연결…", icon: <Plus size={14} />, onSelect: () => s.select(w.path, true) },
          { label: "폴더 경로 복사", icon: <Copy size={14} />, onSelect: () => copyText(w.path) },
        ]
      : [
          { label: "열기", onSelect: () => s.select(w.path) },
          { label: "고정 탭으로 열기", onSelect: () => s.select(w.path, true) },
          { label: "분할로 열기", icon: <Columns2 size={14} />, onSelect: () => openInSplit(w) },
          { label: "새 창으로 열기", icon: <ExternalLink size={14} />, onSelect: () => openInWindow(w) },
          "sep",
          { label: "에이전트 설정…", icon: <Settings2 size={14} />, shortcut: keysOf("rename"), onSelect: () => s.openAgentSettings({ path: w.path }) },
          { label: "워커 추가", icon: <Plus size={14} />, onSelect: () => s.openNewAgent({ root: w.root, mode: "worker", parentPath: w.path }) },
          { label: flip.menu, icon: flip.to === "cli" ? <SquareTerminal size={14} /> : <MessageSquare size={14} />, onSelect: () => showAgentAs(w.path, flip.to) },
          { label: w.unread ? "읽음으로 표시" : "안 읽음으로 표시", onSelect: () => s.markUnread(w.path, !w.unread) },
          { label: "브랜치 이름 복사", icon: <GitBranch size={14} />, onSelect: () => copyText(w.branch) },
          { label: "폴더 경로 복사", icon: <Copy size={14} />, onSelect: () => copyText(w.path) },
          "sep",
          { label: w.isMaestro ? "Lead 없애기…" : "폐기…", icon: <Trash2 size={14} />, danger: true, disabled: !!busy, onSelect: () => void discard() },
        ];
    openMenu(at, items, { owner, minWidth: 220 });
  };
  return { open, dialog, busy };
}
