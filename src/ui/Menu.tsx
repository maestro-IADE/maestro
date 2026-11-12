import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore, type ReactNode } from "react";
import { createPortal } from "react-dom";

/**
 * The app's one menu (docs/features/ui/menus-and-dialogs.md): `openMenu(at, items)` from a click or a right click,
 * drawn by the window's `<MenuHost />` (mounted once per window, in WindowLayers). One menu at a time; opening
 * another replaces it.
 *
 * Portalled to `document.body` and kept inside the window; closes on Esc, a press outside, scrolling, the window
 * losing focus or resizing. ↑ ↓ Home End move, Enter or Space picks. Its root carries React Flow's `nodrag nopan
 * nowheel`, so a menu opened from a canvas card never drags or pans the canvas.
 */
export type MenuItem =
  | {
      label: string;
      /** a 14px icon (icons.tsx), drawn muted */
      icon?: ReactNode;
      /** the keys, shown on the right ("F2") */
      shortcut?: string;
      /** destructive: red text */
      danger?: boolean;
      disabled?: boolean;
      onSelect: () => void;
    }
  | "sep";

/** Where it opens: at a point (a right click), or under an element (a ⋯ button). */
export type MenuAt = { x: number; y: number } | Element;

type Open = {
  id: number;
  at: MenuAt;
  items: MenuItem[];
  /** marked `data-menu-open` while the menu is up (its hover actions stay visible) */
  owner: Element | null;
  minWidth: number;
};

let current: Open | null = null;
let seq = 0;
/** the anchor whose press just closed its own menu: the click that follows must not open it again (a toggle) */
let shutBy: { el: Element; t: number } | null = null;
const subs = new Set<() => void>();
const emit = () => subs.forEach((f) => f());
const subscribe = (f: () => void) => {
  subs.add(f);
  return () => void subs.delete(f);
};

function setCurrent(next: Open | null) {
  current?.owner?.removeAttribute("data-menu-open");
  current = next;
  current?.owner?.setAttribute("data-menu-open", "");
  emit();
}

/**
 * Opens a menu. `owner` (default: `at` when it's an element) gets `data-menu-open` until the menu closes, so a row's
 * hover actions stay on screen while its menu is open.
 */
export function openMenu(at: MenuAt, items: MenuItem[], opts: { owner?: Element | null; minWidth?: number } = {}) {
  if (!items.some((i) => i !== "sep")) return;
  const shut = shutBy;
  shutBy = null;
  if (shut && shut.el === at && performance.now() - shut.t < 800) return;
  setCurrent({ id: ++seq, at, items, owner: opts.owner ?? (at instanceof Element ? at : null), minWidth: opts.minWidth ?? 200 });
}

export function closeMenu() {
  if (current) setCurrent(null);
}

/** The point of a mouse event, for `openMenu` from `onContextMenu` (call `e.preventDefault()` too). */
export const pointOf = (e: { clientX: number; clientY: number }) => ({ x: e.clientX, y: e.clientY });

/** Draws the open menu of this window. One per window. */
export function MenuHost() {
  const m = useSyncExternalStore(subscribe, () => current);
  return m ? createPortal(<Menu key={m.id} open={m} />, document.body) : null;
}

const EDGE = 8; // kept this far inside the window

/** Where the menu goes so all of it is on screen: under the anchor (flipped above if there's no room), or at the point. */
function place(at: MenuAt, w: number, h: number) {
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  let x: number;
  let y: number;
  if (at instanceof Element) {
    const r = at.getBoundingClientRect();
    x = r.left;
    y = r.bottom + 4 + h > vh - EDGE && r.top - 4 - h >= EDGE ? r.top - 4 - h : r.bottom + 4;
  } else {
    x = at.x;
    // a point near the bottom opens the menu upward from it, like the OS menus
    y = at.y + h > vh - EDGE && at.y - h >= EDGE ? at.y - h : at.y;
  }
  x = Math.max(EDGE, Math.min(x, vw - EDGE - w));
  y = Math.max(EDGE, Math.min(y, vh - EDGE - h));
  return { x, y };
}

function Menu({ open }: { open: Open }) {
  const box = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ x: number; y: number } | null>(null);
  const enabled = open.items.map((it, i) => (it !== "sep" && !it.disabled ? i : -1)).filter((i) => i >= 0);
  const [hot, setHot] = useState(-1);
  const hotRef = useRef(hot);
  hotRef.current = hot;

  // measured before paint, so it never shows off screen for a frame
  useLayoutEffect(() => {
    const el = box.current;
    if (el) setPos(place(open.at, el.offsetWidth, el.offsetHeight));
  }, []);

  const pick = (i: number) => {
    const it = open.items[i];
    if (!it || it === "sep" || it.disabled) return;
    closeMenu();
    it.onSelect();
  };

  useEffect(() => {
    // focus back where it was (a row, a terminal) once the menu is gone
    const before = document.activeElement as HTMLElement | null;
    box.current?.focus({ preventScroll: true });
    const inside = (t: EventTarget | null) => t instanceof Node && !!box.current?.contains(t);
    const step = (d: 1 | -1) => {
      if (!enabled.length) return;
      const at = enabled.indexOf(hotRef.current);
      setHot(enabled[at < 0 ? (d > 0 ? 0 : enabled.length - 1) : (at + d + enabled.length) % enabled.length]);
    };
    // keys go to the menu only while it's open (not the terminal, the canvas or the window's shortcuts)
    const onKey = (e: KeyboardEvent) => {
      if (e.isComposing) return;
      e.stopPropagation();
      if (e.key === "Escape" || e.key === "Tab") {
        e.preventDefault();
        closeMenu();
      } else if (e.key === "ArrowDown") {
        e.preventDefault();
        step(1);
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        step(-1);
      } else if (e.key === "Home" || e.key === "End") {
        e.preventDefault();
        setHot(e.key === "Home" ? (enabled[0] ?? -1) : (enabled[enabled.length - 1] ?? -1));
      } else if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        pick(hotRef.current);
      }
    };
    const onDown = (e: PointerEvent) => {
      if (inside(e.target)) return;
      if (open.at instanceof Element && e.target instanceof Node && open.at.contains(e.target)) shutBy = { el: open.at, t: performance.now() };
      closeMenu();
    };
    const onScroll = (e: Event) => !inside(e.target) && closeMenu();
    const onGone = () => closeMenu();
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("pointerdown", onDown, true);
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("wheel", onScroll, true);
    window.addEventListener("blur", onGone);
    window.addEventListener("resize", onGone);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("wheel", onScroll, true);
      window.removeEventListener("blur", onGone);
      window.removeEventListener("resize", onGone);
      if (before?.isConnected && document.activeElement === box.current) before.focus({ preventScroll: true });
    };
  }, []);

  return (
    <div
      ref={box}
      className="menu nodrag nopan nowheel"
      role="menu"
      tabIndex={-1}
      style={{ left: pos?.x ?? 0, top: pos?.y ?? 0, minWidth: open.minWidth, visibility: pos ? undefined : "hidden" }}
      onContextMenu={(e) => e.preventDefault()}
    >
      {open.items.map((it, i) =>
        it === "sep" ? (
          <hr key={`sep-${i}`} className="menu-sep" />
        ) : (
          <button
              key={`${i}-${it.label}`}
              type="button"
              role="menuitem"
              className="menu-item"
              data-danger={it.danger || undefined}
              data-hot={hot === i || undefined}
              disabled={it.disabled}
              tabIndex={-1}
              onPointerEnter={() => !it.disabled && setHot(i)}
              onPointerLeave={() => setHot((h) => (h === i ? -1 : h))}
              onClick={() => pick(i)}
            >
              <span className="menu-icon" aria-hidden>{it.icon}</span>
              <span className="menu-label ellipsis">{it.label}</span>
              {it.shortcut && <kbd className="menu-key">{it.shortcut}</kbd>}
          </button>
        ),
      )}
    </div>
  );
}
