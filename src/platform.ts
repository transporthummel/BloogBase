/** Whether BloogBase runs on macOS, where keys are named as on Mac keyboards. */
export function isMac() {
  return /Mac/.test(navigator.userAgent);
}
