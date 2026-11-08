import { ChevronRight } from "lucide-react";
import { useId, useState, type ReactNode } from "react";

/**
 * A section that folds away under its heading: the heading holds the button
 * (WAI-ARIA disclosure), so it still counts as a heading for screen readers.
 */
export function Disclosure({
  title,
  defaultOpen = false,
  children,
}: {
  title: string;
  defaultOpen?: boolean;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);
  const panel = useId();
  return (
    <section className="grid gap-2">
      <h2 className="font-semibold">
        <button
          type="button"
          aria-expanded={open}
          aria-controls={panel}
          onClick={() => setOpen(!open)}
          className="-mx-1 flex items-center gap-2 rounded-lg px-1 text-left hover:text-ink"
        >
          <ChevronRight
            className={`size-4 shrink-0 text-muted transition-transform motion-reduce:transition-none ${
              open ? "rotate-90" : ""
            }`}
            aria-hidden
          />
          {title}
        </button>
      </h2>
      {open && (
        <div id={panel} className="grid gap-2">
          {children}
        </div>
      )}
    </section>
  );
}
