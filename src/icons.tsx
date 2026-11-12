import type { LucideIcon, LucideProps } from "lucide-react";
import {
  ArrowDownToLine as LArrowDownToLine,
  ArrowLeft as LArrowLeft,
  ArrowRight as LArrowRight,
  Bell as LBell,
  BookOpen as LBookOpen,
  Bot as LBot,
  ChevronDown as LChevronDown,
  ChevronUp as LChevronUp,
  ChevronLeft as LChevronLeft,
  ChevronRight as LChevronRight,
  ChevronsDownUp as LChevronsDownUp,
  ChevronsUpDown as LChevronsUpDown,
  CircleAlert as LCircleAlert,
  CircleHelp as LCircleHelp,
  Columns2 as LColumns2,
  Copy as LCopy,
  Crown,
  Ellipsis as LEllipsis,
  ExternalLink as LExternalLink,
  File as LFile,
  Files as LFiles,
  FileText as LFileText,
  Folder as LFolder,
  FolderOpen as LFolderOpen,
  FolderGit2 as LFolderGit2,
  FolderPlus as LFolderPlus,
  GitBranch as LGitBranch,
  GitCompare as LGitCompare,
  GitMerge as LGitMerge,
  GitPullRequest as LGitPullRequest,
  Info as LInfo,
  ListCollapse as LListCollapse,
  Maximize2 as LMaximize2,
  Minimize2 as LMinimize2,
  Minus as LMinus,
  MessageCircleQuestion as LMessageCircleQuestion,
  MessageSquare as LMessageSquare,
  Palette as LPalette,
  PanelLeft as LPanelLeft,
  PanelRight as LPanelRight,
  Plus as LPlus,
  RotateCw,
  Scan as LScan,
  Search as LSearch,
  Settings as LSettings,
  Settings2 as LSettings2,
  Square as LSquare,
  SquareTerminal as LSquareTerminal,
  Star,
  Trash2 as LTrash2,
  UserRound,
  Workflow as LWorkflow,
  X as LX,
} from "lucide-react";

// The app's one icon set: Lucide (ISC), one stroke (1.5), sized by where it sits: 16 in bars (the top bar, floating
// toolbars, panel heads), 14 in rows, tabs and menus, 12 for the meta inside a row or a badge. Icons take the colour
// of the text around them. Every icon the app draws comes from here; `size` overrides the default.
const icon = (C: LucideIcon, size = 16) => {
  const I = (p: LucideProps) => <C size={size} strokeWidth={1.5} aria-hidden {...p} />;
  I.displayName = C.displayName;
  return I;
};

/** The project's Maestro, the conductor. */
export const MaestroIcon = icon(Crown, 14);
/** A persona attached to an agent. */
export const PersonaIcon = icon(UserRound, 12);
/** A referenced document. */
export const DocIcon = icon(LFileText, 12);
/** The default choice. */
export const DefaultIcon = icon(Star, 12);
/** Fold/unfold: points right, turned down when open (`.caret[data-open]`). */
export const CaretIcon = icon(LChevronRight, 14);
export const RefreshIcon = icon(RotateCw, 14);
export const CloseIcon = icon(LX, 16);

// By their Lucide names (docs/prd/ui-structure.md "공통 규칙 → 아이콘"), 16 unless given a size.
export const Plus = icon(LPlus);
export const X = icon(LX);
export const FolderPlus = icon(LFolderPlus);
export const Ellipsis = icon(LEllipsis);
export const Settings = icon(LSettings);
export const Settings2 = icon(LSettings2);
export const PanelLeft = icon(LPanelLeft);
export const PanelRight = icon(LPanelRight);
export const ArrowLeft = icon(LArrowLeft);
export const ArrowRight = icon(LArrowRight);
export const Bell = icon(LBell);
/** 승인 대기 (the status glyph, status.tsx) */
export const MessageCircleQuestion = icon(LMessageCircleQuestion);
export const SquareTerminal = icon(LSquareTerminal);
export const MessageSquare = icon(LMessageSquare);
export const Maximize2 = icon(LMaximize2);
export const Minimize2 = icon(LMinimize2);
export const Minus = icon(LMinus);
/** 전체 보기 (the zoom bar) */
export const Scan = icon(LScan);
export const Workflow = icon(LWorkflow);
export const Columns2 = icon(LColumns2);
export const ExternalLink = icon(LExternalLink);
export const Square = icon(LSquare);
export const Trash2 = icon(LTrash2);
export const GitBranch = icon(LGitBranch);
export const GitPullRequest = icon(LGitPullRequest);
export const GitMerge = icon(LGitMerge);
export const GitCompare = icon(LGitCompare);
export const Files = icon(LFiles);
export const File = icon(LFile);
export const Folder = icon(LFolder);
export const FolderOpen = icon(LFolderOpen);
export const FolderGit2 = icon(LFolderGit2);
export const BookOpen = icon(LBookOpen);
export const FileText = icon(LFileText);
export const ChevronDown = icon(LChevronDown);
export const ChevronUp = icon(LChevronUp);
export const ChevronLeft = icon(LChevronLeft);
export const ChevronRight = icon(LChevronRight);
export const ChevronsDownUp = icon(LChevronsDownUp);
export const ChevronsUpDown = icon(LChevronsUpDown);
export const CircleHelp = icon(LCircleHelp);
export const CircleAlert = icon(LCircleAlert);
export const ArrowDownToLine = icon(LArrowDownToLine);
export const ListCollapse = icon(LListCollapse);
export const Bot = icon(LBot);
export const Palette = icon(LPalette);
export const Info = icon(LInfo);
export const Copy = icon(LCopy);
/** 빠른 이동's input (QuickSwitch.tsx) */
export const Search = icon(LSearch);
