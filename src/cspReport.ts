import { warn } from "@tauri-apps/plugin-log";

/** Lines per directive; a page that keeps breaking shouldn't fill the log. */
const LIMIT = 5;

type Violation = Pick<
  SecurityPolicyViolationEvent,
  "effectiveDirective" | "blockedURI" | "sourceFile" | "lineNumber"
>;

/** One log line for a violation, with long (data:) URLs cut short. */
export function describeViolation(v: Violation): string {
  const blocked = v.blockedURI.length > 120 ? `${v.blockedURI.slice(0, 120)}…` : v.blockedURI;
  const source = v.sourceFile ? ` (${v.sourceFile}:${v.lineNumber})` : "";
  return `CSP blocked ${v.effectiveDirective}: ${blocked || "(inline)"}${source}`;
}

/**
 * Writes what the Content Security Policy blocks to the app's log, which the
 * diagnostics report includes: a source missing from the policy shows up there
 * on every platform, not only in a webview console.
 */
export function reportCspViolations() {
  const counts = new Map<string, number>();
  document.addEventListener("securitypolicyviolation", (e) => {
    const n = (counts.get(e.effectiveDirective) ?? 0) + 1;
    counts.set(e.effectiveDirective, n);
    if (n > LIMIT) return;
    void warn(describeViolation(e)).catch(() => {});
  });
}
