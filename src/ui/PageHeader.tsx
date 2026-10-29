import type { ReactNode } from "react";

/** The page title; as tall as an action button, so it stays put with or without one. */
export function PageHeader({ title, actions }: { title: string; actions?: ReactNode }) {
  return (
    <header className="mb-6 flex min-h-9 flex-wrap items-center gap-3">
      <h1 className="mr-auto font-pixel text-lg leading-relaxed">{title}</h1>
      {actions}
    </header>
  );
}
