const event = "bloogbase:import-paths";

/**
 * Imports folders and ZIP files as if dropped on the window: games right
 * away, mods through the usual confirmation, one after the other.
 */
export function importPaths(paths: string[]) {
  window.dispatchEvent(new CustomEvent<string[]>(event, { detail: paths }));
}

/** Calls `handle` for every `importPaths`; returns the unsubscribe. */
export function onImportPaths(handle: (paths: string[]) => void) {
  const listener = (e: Event) => handle((e as CustomEvent<string[]>).detail);
  window.addEventListener(event, listener);
  return () => window.removeEventListener(event, listener);
}
