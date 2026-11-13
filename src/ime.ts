// Korean (and any IME) input: is this key event still part of composing a syllable? Every Enter-to-act handler asks
// this first, so a half-composed syllable is never sent (docs/features/agents/chat-view.md, "한글 입력").
//
// `isComposing` alone isn't enough: some WebView builds deliver the Enter that ends a composition with
// `isComposing === false` but `keyCode === 229` (the IME's "process key"; Cline checks the same pair). And when the
// composition ends just before the Enter arrives in the same moment, that Enter would act on the text before the
// last syllable lands, so an Enter within a few milliseconds of `compositionend` counts as composing too.

let endedAt = -Infinity;
window.addEventListener("compositionend", () => void (endedAt = performance.now()), true);

const SAME_TICK_MS = 20;

export function isImeComposing(e: { nativeEvent: KeyboardEvent } | KeyboardEvent): boolean {
  const ev = "nativeEvent" in e ? e.nativeEvent : e;
  return ev.isComposing || ev.keyCode === 229 || (ev.key === "Enter" && performance.now() - endedAt < SAME_TICK_MS);
}
