import type { ReactNode } from "react";

/** Small label in the pixel font, e.g. a version or a count. */
export function Badge({ children }: { children: ReactNode }) {
  return (
    <span className="inline-block rounded border border-border bg-surface-raised px-1.5 py-1 font-pixel text-[0.55rem] leading-none text-muted uppercase">
      {children}
    </span>
  );
}
