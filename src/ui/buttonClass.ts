export type Variant = "primary" | "secondary" | "ghost" | "card";

const variants: Record<Variant, string> = {
  primary: "bg-accent text-accent-ink shadow-card hover:brightness-110",
  secondary: "border border-border bg-surface-raised text-ink hover:border-muted",
  ghost: "text-muted hover:bg-surface-raised hover:text-ink",
  // Actions repeated on every card: quiet, so the grid isn't a wall of green;
  // they turn green on hover and focus.
  card: "border border-border bg-surface-raised text-ink hover:border-accent hover:bg-accent hover:text-accent-ink focus-visible:border-accent focus-visible:bg-accent focus-visible:text-accent-ink",
};

/** Button styling, also for links that act as buttons. */
export function buttonClass(variant: Variant = "primary", className = "") {
  return `inline-flex items-center justify-center gap-2 rounded-lg px-4 py-2 text-sm font-semibold transition disabled:pointer-events-none disabled:opacity-50 ${variants[variant]} ${className}`;
}
