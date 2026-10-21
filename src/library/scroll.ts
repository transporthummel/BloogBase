/** The library as it was left for a details page, to come back to. */
let left: { profileId: string; top: number } | null = null;

/** Pages scroll inside the shell's `<main>`, not the window. */
export function scroller(): HTMLElement | null {
  return document.querySelector("main");
}

export function leaveLibrary(profileId: string) {
  left = { profileId, top: scroller()?.scrollTop ?? 0 };
}

/**
 * Back in the library: the old position, then the card that was opened
 * brought into view (sorting may have moved it) and focused.
 */
export function returnToLibrary(list: HTMLElement) {
  if (!left) return;
  const { profileId, top } = left;
  left = null;
  const main = scroller();
  if (main) main.scrollTop = top;
  const card = list.querySelector(`[data-profile-id="${CSS.escape(profileId)}"]`);
  card?.scrollIntoView?.({ block: "nearest" });
  card?.querySelector<HTMLElement>("h2 a, h3 a")?.focus({ preventScroll: true });
}
