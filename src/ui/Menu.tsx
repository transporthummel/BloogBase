import {
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from "react";

export type MenuItem = {
  label: string;
  icon?: ReactNode;
  onSelect: () => void;
  disabled?: boolean;
  /** Removes or deletes something; set apart at the end. */
  danger?: boolean;
  /** Starts a new group with a line above. */
  separated?: boolean;
};

/**
 * A button opening a list of actions, after the WAI-ARIA menu button
 * pattern: arrow keys, Home and End move, Escape and Tab close. The list
 * floats over the page, so a parent's overflow can't cut it off; scrolling
 * closes it.
 */
export function Menu({
  label,
  trigger,
  items,
  className = "",
}: {
  /** The button's accessible name. */
  label: string;
  trigger: ReactNode;
  items: MenuItem[];
  className?: string;
}) {
  const id = useId();
  const button = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [place, setPlace] = useState<{ top?: number; bottom?: number; left: number }>();
  const hide = () => {
    setOpen(false);
    setPlace(undefined);
  };

  // Below the button, or above it where the window ends; its left edge on the button's.
  useLayoutEffect(() => {
    if (!open || !button.current || !list.current) return;
    const b = button.current.getBoundingClientRect();
    const height = list.current.offsetHeight;
    const width = list.current.offsetWidth;
    const left = Math.max(8, Math.min(b.left, window.innerWidth - width - 8));
    setPlace(
      b.bottom + 4 + height > window.innerHeight && b.top - 4 - height > 0
        ? { bottom: window.innerHeight - b.top + 4, left }
        : { top: b.bottom + 4, left },
    );
  }, [open]);

  // Once placed: hidden, it couldn't take focus.
  useEffect(() => {
    if (place) list.current?.querySelector<HTMLElement>("[role=menuitem]:not([disabled])")?.focus();
  }, [place]);

  useEffect(() => {
    if (!open) return;
    const away = (e: Event) => {
      const t = e.target as Node;
      if (!list.current?.contains(t) && !button.current?.contains(t)) hide();
    };
    document.addEventListener("mousedown", away);
    window.addEventListener("scroll", hide, true);
    window.addEventListener("resize", hide);
    return () => {
      document.removeEventListener("mousedown", away);
      window.removeEventListener("scroll", hide, true);
      window.removeEventListener("resize", hide);
    };
  }, [open]);

  function close() {
    hide();
    button.current?.focus();
  }

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      return close();
    }
    if (e.key === "Tab") return hide();
    const all = [
      ...(list.current?.querySelectorAll<HTMLElement>("[role=menuitem]:not([disabled])") ?? []),
    ];
    const at = all.indexOf(document.activeElement as HTMLElement);
    const next = { ArrowDown: at + 1, ArrowUp: at - 1, Home: 0, End: all.length - 1 }[e.key];
    if (next === undefined || all.length === 0) return;
    e.preventDefault();
    all[(next + all.length) % all.length].focus();
  }

  return (
    <>
      <button
        ref={button}
        type="button"
        aria-label={label}
        title={label}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        onClick={() => (open ? close() : setOpen(true))}
        className={`inline-flex items-center justify-center gap-2 rounded-lg border border-border bg-surface-raised px-3 py-2 text-sm font-semibold text-ink shadow-key transition hover:border-muted active:translate-y-px active:shadow-key-down active:duration-0 ${className}`}
      >
        {trigger}
      </button>
      {open && (
        <div
          ref={list}
          id={id}
          role="menu"
          aria-label={label}
          onKeyDown={onKeyDown}
          style={{ ...place, visibility: place ? undefined : "hidden" }}
          className="bb-menu-in fixed z-40 grid min-w-56 rounded-lg border border-border bg-surface py-1 shadow-card"
        >
          {items.map((item) => (
            <div key={item.label} className="grid">
              {item.separated && <hr className="my-1 border-border" />}
              <button
                type="button"
                role="menuitem"
                tabIndex={-1}
                disabled={item.disabled}
                onClick={() => {
                  close();
                  item.onSelect();
                }}
                className={`flex items-center gap-3 px-3 py-2 text-left text-sm outline-none hover:bg-surface-raised focus-visible:bg-surface-raised disabled:pointer-events-none disabled:opacity-50 ${
                  item.danger ? "text-danger" : "text-ink"
                }`}
              >
                <span
                  className={`flex size-4 shrink-0 items-center ${item.danger ? "" : "text-muted"}`}
                  aria-hidden
                >
                  {item.icon}
                </span>
                {item.label}
              </button>
            </div>
          ))}
        </div>
      )}
    </>
  );
}
