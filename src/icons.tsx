import type { LucideIcon, LucideProps } from "lucide-react";
import { ChevronRight, Crown, FileText, RotateCw, Star, UserRound, X } from "lucide-react";

// The app's one icon set: Lucide (ISC), one stroke (1.5), 16px by default and 14px where it sits in a dense row
// (tabs, tree, tags). Icons take the colour of the text around them. The hand-drawn SVGs elsewhere are Lucide
// paths too and use the same stroke.
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
export const DocIcon = icon(FileText, 12);
/** The default choice. */
export const DefaultIcon = icon(Star, 12);
/** Fold/unfold: points right, turned down when open (`.caret[data-open]`). */
export const CaretIcon = icon(ChevronRight, 14);
export const RefreshIcon = icon(RotateCw, 14);
export const CloseIcon = icon(X, 16);
