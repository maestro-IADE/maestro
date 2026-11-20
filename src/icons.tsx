import type { LucideIcon, LucideProps } from "lucide-react";
import {
  ArrowDownToLine as LArrowDownToLine,
  ArrowLeft as LArrowLeft,
  ArrowUp as LArrowUp,
  ArrowRight as LArrowRight,
  Bell as LBell,
  BookOpen as LBookOpen,
  Bot as LBot,
  Check as LCheck,
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
  CopyPlus as LCopyPlus,
  Ellipsis as LEllipsis,
  ExternalLink as LExternalLink,
  File as LFile,
  Files as LFiles,
  FileText as LFileText,
  Folder as LFolder,
  FolderOpen as LFolderOpen,
  FolderGit2 as LFolderGit2,
  FolderPlus as LFolderPlus,
  Frame as LFrame,
  Layers as LLayers,
  Image as LImage,
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
  Paperclip as LPaperclip,
  Pencil as LPencil,
  PanelLeft as LPanelLeft,
  PencilRuler as LPencilRuler,
  PanelRight as LPanelRight,
  Pause as LPause,
  Play as LPlay,
  Plus as LPlus,
  RotateCcw as LRotateCcw,
  StepForward as LStepForward,
  RotateCw,
  Scan as LScan,
  Focus as LFocus,
  Search as LSearch,
  Settings as LSettings,
  Gauge as LGauge,
  Settings2 as LSettings2,
  Shield as LShield,
  Shrink as LShrink,
  Slash as LSlash,
  Square as LSquare,
  SquareTerminal as LSquareTerminal,
  Star,
  Trash2 as LTrash2,
  UserRound,
  Waypoints as LWaypoints,
  Spline as LSpline,
  MessageSquarePlus as LMessageSquarePlus,
  SquarePlus as LSquarePlus,
  Network as LNetwork,
  SquareSplitVertical as LSquareSplitVertical,
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
export const Frame = icon(LFrame);
export const Layers = icon(LLayers);
export const ImageIcon = icon(LImage);
export const Ellipsis = icon(LEllipsis);
export const Settings = icon(LSettings);
export const Gauge = icon(LGauge);
export const Settings2 = icon(LSettings2);
export const PanelLeft = icon(LPanelLeft);
export const PencilRuler = icon(LPencilRuler);
export const PanelRight = icon(LPanelRight);
/**
 * The panel icons with their side filled in: the top bar shows these while that panel is open (Lucide has no filled
 * ones; same frame, size and stroke as PanelLeft / PanelRight).
 */
const filledPanel = (side: "left" | "right") => {
  const I = ({ size = 16, className }: { size?: number; className?: string }) => (
    <svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.5} strokeLinecap="round" strokeLinejoin="round" aria-hidden className={className}>
      <rect width="18" height="18" x="3" y="3" rx="2" />
      <path d={side === "left" ? "M9 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h4z" : "M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4z"} fill="currentColor" />
    </svg>
  );
  I.displayName = side === "left" ? "PanelLeftFilled" : "PanelRightFilled";
  return I;
};
export const PanelLeftFilled = filledPanel("left");
export const PanelRightFilled = filledPanel("right");
export const ArrowLeft = icon(LArrowLeft);
export const ArrowUp = icon(LArrowUp);
export const ArrowRight = icon(LArrowRight);
export const Bell = icon(LBell);
/** 승인 대기 (the status glyph, status.tsx) */
export const MessageCircleQuestion = icon(LMessageCircleQuestion);
export const SquareTerminal = icon(LSquareTerminal);
export const Shield = icon(LShield);
export const MessageSquare = icon(LMessageSquare);
export const Maximize2 = icon(LMaximize2);
export const Minimize2 = icon(LMinimize2);
/** 내용에 맞추기 (an area drawn tight around what it holds) */
export const Shrink = icon(LShrink);
export const Minus = icon(LMinus);
/** 전체 보기 (the zoom bar) */
export const Scan = icon(LScan);
/** 선택한 것 보기 (the zoom bar) */
export const Focus = icon(LFocus);
export const Workflow = icon(LWorkflow);
/** 연결점 자동 정리 (the zoom bar) */
export const Waypoints = icon(LWaypoints);
export const Spline = icon(LSpline);
export const MessageSquarePlus = icon(LMessageSquarePlus);
export const SquarePlus = icon(LSquarePlus);
export const Network = icon(LNetwork);
export const SquareSplitVertical = icon(LSquareSplitVertical);
export const Columns2 = icon(LColumns2);
export const ExternalLink = icon(LExternalLink);
export const Square = icon(LSquare);
export const Paperclip = icon(LPaperclip);
export const Slash = icon(LSlash);
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
export const Check = icon(LCheck);
export const CircleAlert = icon(LCircleAlert);
export const ArrowDownToLine = icon(LArrowDownToLine);
export const ListCollapse = icon(LListCollapse);
export const Bot = icon(LBot);
export const Palette = icon(LPalette);
export const Info = icon(LInfo);
export const Copy = icon(LCopy);
export const CopyPlus = icon(LCopyPlus);
export const Pencil = icon(LPencil);
/** 빠른 이동's input (QuickSwitch.tsx) */
export const Search = icon(LSearch);
// the developer tests' controls (DevTools.tsx): run to the end, stop, one step, start over
export const Play = icon(LPlay);
export const Pause = icon(LPause);
export const StepForward = icon(LStepForward);
export const RotateCcw = icon(LRotateCcw);
