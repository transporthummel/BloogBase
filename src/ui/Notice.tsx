import type { ReactNode } from "react";

/** Inline status message; errors are announced to screen readers. */
export function Notice({
  tone = "info",
  children,
}: {
  tone?: "info" | "error";
  children: ReactNode;
}) {
  return tone === "error" ? (
    <p
      role="alert"
      className="rounded-lg border border-danger/40 bg-danger/10 px-3 py-2 text-sm select-text"
    >
      {children}
    </p>
  ) : (
    <p role="status" className="text-sm text-muted">
      {children}
    </p>
  );
}
