import type { ReactNode } from "react";

/**
 * Small label in the pixel font, e.g. a version or a count. `status` marks
 * the player's own state (installed, update) apart from descriptive labels;
 * `warning` a label to heed, e.g. an abandoned mod.
 */
export function Badge({ children, tone }: { children: ReactNode; tone?: "status" | "warning" }) {
  return (
    <span
      className={`inline-block rounded border px-1.5 py-1 font-pixel text-[0.55rem] leading-none uppercase ${
        tone === "status"
          ? "border-accent/50 bg-accent/10 text-accent"
          : tone === "warning"
            ? "border-warning/50 bg-warning/10 text-warning"
            : "border-border bg-surface-raised text-muted shadow-key"
      }`}
    >
      {children}
    </span>
  );
}
