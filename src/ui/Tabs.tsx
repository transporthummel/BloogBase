import { ChevronDown } from "lucide-react";
import { useLayoutEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Badge } from "./Badge";
import { fitTabs } from "./fitTabs";
import { Menu } from "./Menu";

export type TabItem<T extends string> = {
  id: T;
  label: string;
  /** How many there are, as a badge after the name; none for 0. */
  count?: number;
};

/**
 * Tab list after the WAI-ARIA pattern: one tab stop, arrow keys, Home and
 * End move between tabs and select them. Tabs that don't fit in one line go
 * into a "More" menu after the others; picking one there brings it into the
 * line.
 */
export function Tabs<T extends string>({
  tabs,
  selected,
  onSelect,
  label,
  children,
  className = "",
}: {
  tabs: TabItem<T>[];
  selected: T;
  onSelect: (id: T) => void;
  label: string;
  children: ReactNode;
  className?: string;
}) {
  const { t } = useTranslation();
  const row = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const ruler = useRef<HTMLDivElement>(null);
  // The tabs in the line; null for all of them.
  const [shown, setShown] = useState<T[] | null>(null);
  // A tab picked in the menu takes the focus once it's in the line.
  const focusNext = useRef<T | null>(null);

  const fit = useRef(() => {});
  // The latest tabs, for the observer below.
  useLayoutEffect(() => {
    fit.current = () => {
      if (!row.current || !ruler.current) return;
      // Not laid out (hidden, or in tests): all of them.
      if (row.current.clientWidth === 0) return setShown(null);
      const items = [...ruler.current.children] as HTMLElement[];
      const more = items.pop()!.offsetWidth;
      const widths = items.map((el) => el.offsetWidth);
      const next = fitTabs(
        widths,
        tabs.findIndex((x) => x.id === selected),
        row.current.clientWidth,
        more,
      );
      const ids = next.map((i) => tabs[i].id);
      setShown((now) =>
        ids.length === tabs.length ? null : now && now.join() === ids.join() ? now : ids,
      );
    };
  });

  // Again whenever a name or count changes, and with the window.
  const key = tabs.map((x) => `${x.id}:${x.label}:${x.count ?? 0}`).join("|");
  useLayoutEffect(() => fit.current(), [key, selected]);
  useLayoutEffect(() => {
    if (!row.current || !ruler.current || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => fit.current());
    observer.observe(row.current);
    // Names get wider once the font has loaded.
    observer.observe(ruler.current);
    return () => observer.disconnect();
  }, []);

  useLayoutEffect(() => {
    const id = focusNext.current;
    if (id === null) return;
    // Once it's in the line, which may take another render.
    const el = list.current?.querySelector<HTMLElement>(`#tab-${id}`);
    if (!el) return;
    focusNext.current = null;
    el.focus();
  });

  const inLine = shown ? tabs.filter((x) => shown.includes(x.id)) : tabs;
  const inMenu = shown ? tabs.filter((x) => !shown.includes(x.id)) : [];

  function onKeyDown(e: KeyboardEvent) {
    const at = inLine.findIndex((x) => x.id === selected);
    const next = {
      ArrowRight: at + 1,
      ArrowLeft: at - 1,
      Home: 0,
      End: inLine.length - 1,
    }[e.key];
    if (next === undefined) return;
    e.preventDefault();
    const id = inLine[(next + inLine.length) % inLine.length].id;
    onSelect(id);
    list.current?.querySelector<HTMLElement>(`#tab-${id}`)?.focus();
  }

  const tabClass =
    "-mb-px flex shrink-0 items-center gap-2 border-b-2 px-3 py-2 text-sm font-semibold";
  const moreLabel = t("common.more");

  return (
    <>
      <div ref={row} className="relative mb-4 flex gap-1 border-b border-border">
        <div
          ref={list}
          role="tablist"
          aria-label={label}
          onKeyDown={onKeyDown}
          className="flex min-w-0 gap-1"
        >
          {inLine.map((x) => (
            <button
              key={x.id}
              role="tab"
              type="button"
              id={`tab-${x.id}`}
              aria-selected={selected === x.id}
              aria-controls={`panel-${x.id}`}
              tabIndex={selected === x.id ? 0 : -1}
              onClick={() => onSelect(x.id)}
              className={`${tabClass} ${
                selected === x.id
                  ? "border-accent text-ink"
                  : "border-transparent text-muted hover:text-ink"
              }`}
            >
              <TabName tab={x} />
            </button>
          ))}
        </div>
        {inMenu.length > 0 && (
          <Menu
            label={moreLabel}
            bare
            className={`${tabClass} border-transparent text-muted hover:text-ink`}
            trigger={
              <>
                {moreLabel}
                <ChevronDown className="size-4" aria-hidden />
              </>
            }
            items={inMenu.map((x) => ({
              label: x.label,
              aside: x.count ? <Badge>{x.count}</Badge> : undefined,
              onSelect: () => {
                focusNext.current = x.id;
                onSelect(x.id);
              },
            }))}
          />
        )}
        {/* Every tab and the "More" button at full width, out of sight, to
            measure what fits; clipped, so the page doesn't scroll sideways. */}
        <div
          aria-hidden
          className="pointer-events-none invisible absolute inset-x-0 top-0 h-0 overflow-hidden"
        >
          <div ref={ruler} className="flex w-max whitespace-nowrap">
            {tabs.map((x) => (
              <span key={x.id} className={`${tabClass} border-transparent`}>
                <TabName tab={x} />
              </span>
            ))}
            <span className={`${tabClass} border-transparent`}>
              {moreLabel}
              <span className="size-4" />
            </span>
          </div>
        </div>
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

function TabName<T extends string>({ tab }: { tab: TabItem<T> }) {
  return (
    <>
      {tab.label}
      {!!tab.count && (
        <>
          {" "}
          <Badge>{tab.count}</Badge>
        </>
      )}
    </>
  );
}
