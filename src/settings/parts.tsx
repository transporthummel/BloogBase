import {
  cloneElement,
  isValidElement,
  useId,
  type ComponentProps,
  type ReactElement,
  type ReactNode,
} from "react";

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

/**
 * A label beside its control. A hint goes below and is read out with the
 * control (`aria-describedby`).
 */
export function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: ReactNode;
  children: ReactElement<{ "aria-describedby"?: string }>;
}) {
  const hintId = useId();
  const field = (
    <label className="flex flex-wrap items-center justify-between gap-3 text-sm">
      <span>{label}</span>
      {hint && isValidElement(children)
        ? cloneElement(children, { "aria-describedby": hintId })
        : children}
    </label>
  );
  if (!hint) return field;
  return (
    <div className="grid gap-1">
      {field}
      <p id={hintId} className="max-w-prose text-xs text-muted">
        {hint}
      </p>
    </div>
  );
}

/** A checkbox with its label, and a hint below that is read out with it. */
export function Check({
  label,
  hint,
  ...input
}: { label: ReactNode; hint?: ReactNode } & Omit<ComponentProps<"input">, "type">) {
  const hintId = useId();
  return (
    <div className="grid gap-1">
      <label className="flex items-center gap-2 text-sm">
        <input type="checkbox" aria-describedby={hint ? hintId : undefined} {...input} />
        {label}
      </label>
      {hint && (
        <p id={hintId} className="ml-6 max-w-prose text-xs text-muted">
          {hint}
        </p>
      )}
    </div>
  );
}
