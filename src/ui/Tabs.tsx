import { useRef, type KeyboardEvent, type ReactNode } from "react";

/**
 * Tab list after the WAI-ARIA pattern: one tab stop, arrow keys, Home and
 * End move between tabs and select them.
 */
export function Tabs<T extends string>({
  tabs,
  selected,
  onSelect,
  label,
  children,
  className = "",
}: {
  tabs: { id: T; label: ReactNode }[];
  selected: T;
  onSelect: (id: T) => void;
  label: string;
  children: ReactNode;
  className?: string;
}) {
  const list = useRef<HTMLDivElement>(null);

  function onKeyDown(e: KeyboardEvent) {
    const at = tabs.findIndex((x) => x.id === selected);
    const next = {
      ArrowRight: at + 1,
      ArrowLeft: at - 1,
      Home: 0,
      End: tabs.length - 1,
    }[e.key];
    if (next === undefined) return;
    e.preventDefault();
    const id = tabs[(next + tabs.length) % tabs.length].id;
    onSelect(id);
    list.current?.querySelector<HTMLElement>(`#tab-${id}`)?.focus();
  }

  return (
    <>
      <div
        ref={list}
        role="tablist"
        aria-label={label}
        onKeyDown={onKeyDown}
        className="mb-4 flex flex-wrap gap-1 border-b border-border"
      >
        {tabs.map((x) => (
          <button
            key={x.id}
            role="tab"
            type="button"
            id={`tab-${x.id}`}
            aria-selected={selected === x.id}
            aria-controls={`panel-${x.id}`}
            tabIndex={selected === x.id ? 0 : -1}
            onClick={() => onSelect(x.id)}
            className={`-mb-px flex shrink-0 items-center gap-2 border-b-2 px-3 py-2 text-sm font-semibold ${
              selected === x.id
                ? "border-accent text-ink"
                : "border-transparent text-muted hover:text-ink"
            }`}
          >
            {x.label}
          </button>
        ))}
      </div>
      <div
        role="tabpanel"
        id={`panel-${selected}`}
        aria-labelledby={`tab-${selected}`}
        className={className}
      >
        {children}
      </div>
    </>
  );
}
