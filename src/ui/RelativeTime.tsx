import { useSyncExternalStore, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { formatDateTime, formatRelative } from "../format";

// One shared minute tick, so "just now" moves on while a page stays open.
let now = Date.now();
const listeners = new Set<() => void>();
let timer: ReturnType<typeof setInterval> | undefined;

function subscribe(listener: () => void) {
  listeners.add(listener);
  timer ??= setInterval(() => {
    now = Date.now();
    listeners.forEach((l) => l());
  }, 60_000);
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0) {
      clearInterval(timer);
      timer = undefined;
    }
  };
}

function snapshot() {
  // Without a running tick the clock may be stale; refresh it to the minute
  // so repeated reads within a render stay equal.
  if (timer === undefined) now = Math.floor(Date.now() / 60_000) * 60_000;
  return now;
}

/**
 * "5 minutes ago", with the full date and time on hover. `children` can wrap
 * the phrase into a sentence, e.g. "Last played 5 minutes ago".
 */
export function RelativeTime({
  at,
  children = (relative) => relative,
}: {
  at: number;
  children?: (relative: string) => ReactNode;
}) {
  const { t, i18n } = useTranslation();
  const current = useSyncExternalStore(subscribe, snapshot);
  return (
    <time dateTime={new Date(at * 1000).toISOString()} title={formatDateTime(i18n.language, at)}>
      {children(formatRelative(t, i18n.language, at, current))}
    </time>
  );
}
