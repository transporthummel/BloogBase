import type { ReactNode } from "react";

export function EmptyState({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="bb-starfield flex flex-col items-center gap-4 rounded-xl border border-border px-6 py-16 text-center">
      <h2 className="font-pixel text-sm leading-relaxed">{title}</h2>
      {children}
    </div>
  );
}
