import type { ReactNode } from "react";

// Selects get a fixed width (w-44) where they sit under each other: on their
// own they're as wide as their longest option and don't line up.
export const selectClass =
  "rounded-lg border border-border bg-surface-raised px-3 py-2 text-sm text-ink";

/** Settings of one kind within a card, set apart from the group above. */
export function Group({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="grid gap-4 border-t border-border pt-5 first:border-t-0 first:pt-0">
      <h3 className="text-sm font-semibold text-muted">{title}</h3>
      {children}
    </section>
  );
}

export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="flex flex-wrap items-center justify-between gap-3 text-sm">
      <span>{label}</span>
      {children}
    </label>
  );
}
