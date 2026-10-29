import type { ReactNode } from "react";

/**
 * Small label in the pixel font, e.g. a version or a count. `status` marks
 * the player's own state (installed, update) apart from descriptive labels.
 */
export function Badge({ children, tone }: { children: ReactNode; tone?: "status" }) {
  return (
    <span
      className={`inline-block rounded border px-1.5 py-1 font-pixel text-[0.55rem] leading-none uppercase ${
        tone === "status"
          ? "border-accent/50 bg-accent/10 text-accent"
          : "border-border bg-surface-raised text-muted shadow-key"
      }`}
    >
      {children}
    </span>
  );
}
