/** Whether BloogBase runs on macOS, where keys are named as on Mac keyboards. */
export function isMac() {
  return /Mac/.test(navigator.userAgent);
}

/** The i18next context of texts that name keys: their `_mac` variant on macOS. */
export function keysContext() {
  return isMac() ? "mac" : undefined;
}
