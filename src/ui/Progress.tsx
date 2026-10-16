/** Horizontal progress bar; indeterminate while `max` is unknown. */
export function Progress({ value, max, label }: { value?: number; max?: number; label: string }) {
  const known = value !== undefined && max !== undefined && max > 0;
  const percent = known ? Math.min(100, (value / max) * 100) : 100;
  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={known ? max : undefined}
      aria-valuenow={known ? value : undefined}
      className="h-2 w-full overflow-hidden rounded-full bg-surface-raised"
    >
      <div
        className={`h-full rounded-full bg-accent transition-[width] ${known ? "" : "animate-pulse"}`}
        style={{ width: `${percent}%` }}
      />
    </div>
  );
}
