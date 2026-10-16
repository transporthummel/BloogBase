import type { HTMLAttributes } from "react";

export function Card({ className = "", ...props }: HTMLAttributes<HTMLElement>) {
  return (
    <article
      className={`overflow-hidden rounded-xl border border-border bg-surface shadow-card ${className}`}
      {...props}
    />
  );
}
