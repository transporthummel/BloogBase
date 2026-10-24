import { type RefObject, useEffect } from "react";

/**
 * Makes `action` the default for Enter while the focus is on nothing in
 * particular: the page, or the dialog holding `ref`, as when the step before
 * went away. Buttons and fields keep their own Enter. `null` while the player
 * should read something first, like a warning.
 */
export function useEnterAction(
  ref: RefObject<HTMLElement | null> | null,
  action: (() => void) | null,
) {
  useEffect(() => {
    if (!ref || !action) return;
    const at = ref;
    function onKeyDown(e: KeyboardEvent) {
      const here = at.current;
      if (e.key !== "Enter" || e.repeat || e.defaultPrevented || !here) return;
      if (e.target === document.body || (e.target instanceof Node && e.target.contains(here))) {
        e.preventDefault();
        action?.();
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [ref, action]);
}
