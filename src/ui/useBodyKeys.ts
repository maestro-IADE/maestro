import { useEffect, useRef } from "react";

/**
 * A form dialog's keys (Esc, Ctrl+Enter) while focus has fallen to the page body: the focused field or button was
 * disabled while saving (a disabled element drops focus), or a click landed on dialog text. The form's own
 * `onKeyDown` only hears keys from inside it, so without this the dialog stops answering until a field is clicked.
 * Keys from anywhere else (the form, a terminal, a confirm on top) are left alone.
 */
export function useBodyKeys(handler: (e: KeyboardEvent) => void) {
  const ref = useRef(handler);
  ref.current = handler;
  useEffect(() => {
    const on = (e: KeyboardEvent) => {
      if (e.target === document.body || e.target === document.documentElement) ref.current(e);
    };
    window.addEventListener("keydown", on);
    return () => window.removeEventListener("keydown", on);
  }, []);
}
