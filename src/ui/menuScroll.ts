/**
 * Whether a `scroll` event closes the open menu (Menu.tsx): only a scroll that moves the menu's anchor, i.e. of
 * the page or of a box the anchor is in. A capture listener on window sees every element's scroll, and many of
 * them come from code, not the user (a chat following its stream, a terminal printing, `scrollIntoView` in the
 * sidebar or the tab bar): those leave the anchor where it is, so the menu stays. The user's own scrolling closes it
 * by its `wheel` event anyway. `target`: the event's target (the document for a page scroll); `anchor`: the element
 * the menu opened under, or the one at the point it opened at; `inMenu`: the scroll is the menu's own.
 */
export function scrollClosesMenu(target: unknown, anchor: unknown, inMenu: boolean): boolean {
  if (inMenu) return false;
  const box = target as { contains?: (n: unknown) => boolean } | null;
  // not an element or the document (the window itself): the page moved
  if (!box || typeof box.contains !== "function") return true;
  return !!anchor && box.contains(anchor);
}
