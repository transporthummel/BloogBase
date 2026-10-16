import type { ReactNode } from "react";

export function PageHeader({ title, actions }: { title: string; actions?: ReactNode }) {
  return (
    <header className="mb-6 flex flex-wrap items-center gap-3">
      <h1 className="mr-auto font-pixel text-lg leading-relaxed">{title}</h1>
      {actions}
    </header>
  );
}
