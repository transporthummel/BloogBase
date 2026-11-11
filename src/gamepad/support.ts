/**
 * One log line saying whether the webview offers the Gamepad API, which
 * driving the app with a controller needs. WebKitGTK only has it when built
 * with it, and WKWebView may hold it back from the app's `tauri://` pages;
 * the CSP smoke test (csp-smoke.yml) prints the line for macOS and Linux.
 */
export function describeGamepadSupport(
  nav: { getGamepads?: () => unknown },
  secureContext: boolean,
): string {
  const secure = `secure context: ${secureContext ? "yes" : "no"}`;
  if (typeof nav.getGamepads !== "function") return `Gamepad API missing (${secure})`;
  try {
    nav.getGamepads();
  } catch (e) {
    return `Gamepad API fails: ${e instanceof Error ? e.message : String(e)} (${secure})`;
  }
  return `Gamepad API available (${secure})`;
}
