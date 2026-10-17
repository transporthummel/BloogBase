import type { ComponentProps } from "react";

export function Card({ className = "", ...props }: ComponentProps<"article">) {
  return (
    <article
      className={`overflow-hidden rounded-xl border border-border bg-surface shadow-card ${className}`}
      {...props}
    />
  );
}
