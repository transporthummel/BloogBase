import { useEffect, useId, useRef, type ReactNode } from "react";

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/**
 * Modal dialog over a dimmed page, or over the dialog rendering it. Escape
 * and the backdrop close it; Tab stays inside, and focus goes back where it
 * was on close. Opening focuses the element marked `data-autofocus`, else
 * the panel, and leaves the dialog scrolled to the top.
 */
export function Dialog({
  title,
  onClose,
  icon,
  wide = false,
  children,
}: {
  title: string;
  onClose: () => void;
  /** Shown before the title, e.g. to mark a warning. */
  icon?: ReactNode;
  /** For pictures. */
  wide?: boolean;
  children: ReactNode;
}) {
  const titleId = useId();
  const panel = useRef<HTMLDivElement>(null);
  // Callers pass inline functions; the effect below must run only once.
  const close = useRef(onClose);
  useEffect(() => {
    close.current = onClose;
  });

  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    // Without scrolling: a long dialog opens at its title, even when the
    // focused element is further down.
    (panel.current?.querySelector<HTMLElement>("[data-autofocus]") ?? panel.current)?.focus({
      preventScroll: true,
    });
    const onKey = (e: KeyboardEvent) => {
      // A dialog opened over this one takes the keys.
      if (panel.current?.querySelector('[role="dialog"]')) return;
      if (e.key === "Escape") close.current();
      if (e.key !== "Tab" || !panel.current) return;
      const items = [...panel.current.querySelectorAll<HTMLElement>(FOCUSABLE)];
      if (items.length === 0) return;
      const first = items[0];
      const last = items[items.length - 1];
      const active = document.activeElement;
      if (e.shiftKey && (active === first || active === panel.current)) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && active === last) {
        e.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("keydown", onKey);
      previous?.focus();
    };
  }, []);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
      onMouseDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <div
        ref={panel}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        className={`grid max-h-full w-full gap-4 overflow-y-auto overscroll-none rounded-xl border border-border bg-surface p-5 shadow-card outline-none ${wide ? "max-w-4xl" : "max-w-lg"}`}
      >
        <h2 id={titleId} className="flex items-center gap-2 text-lg font-semibold">
          {icon}
          {title}
        </h2>
        {children}
      </div>
    </div>
  );
}
