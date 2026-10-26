/**
 * Whether the webview's own context menu may open here: in fields and the
 * config editor, and over selected text, for cut, copy and paste. Elsewhere
 * it only offers browser things like Back, Reload and Print.
 */
export function wantsContextMenu(target: EventTarget | null): boolean {
  if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) return true;
  if (target instanceof Element && target.closest('[contenteditable="true"]')) return true;
  return !(window.getSelection()?.isCollapsed ?? true);
}

/** Keeps the browser's context menu away from the rest of the app. */
export function limitContextMenu() {
  window.addEventListener("contextmenu", (e) => {
    if (!wantsContextMenu(e.target)) e.preventDefault();
  });
}
