import type { ReactNode } from "react";

/**
 * Nothing to show yet: the starfield panel with a pixel title, friendly copy
 * and the next action. `level` for one inside a section with a heading of its
 * own; `compact` where it shares the page with other sections.
 */
export function EmptyState({
  title,
  level = 2,
  compact = false,
  children,
}: {
  title: string;
  level?: 2 | 3;
  compact?: boolean;
  children?: ReactNode;
}) {
  const Heading = level === 3 ? "h3" : "h2";
  return (
    <div
      className={`bb-starfield flex flex-col items-center gap-4 rounded-xl border border-border px-6 text-center ${
        compact ? "py-10" : "py-16"
      }`}
    >
      <Heading className="font-pixel text-sm leading-relaxed">{title}</Heading>
      {children}
    </div>
  );
}
