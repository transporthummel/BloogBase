import { type TFunction } from "i18next";

/** "2 h 5 min", "12 min" or "< 1 min". */
export function formatPlayTime(t: TFunction, seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  if (minutes < 1) return t("time.lessThanMinute");
  const hours = Math.floor(minutes / 60);
  if (hours === 0) return t("time.minutes", { count: minutes });
  return t("time.hoursMinutes", { hours, minutes: minutes % 60 });
}

export function formatDate(language: string, epochSeconds: number): string {
  return new Intl.DateTimeFormat(language, { dateStyle: "medium" }).format(
    new Date(epochSeconds * 1000),
  );
}

export function formatDateTime(language: string, epochSeconds: number): string {
  return new Intl.DateTimeFormat(language, { dateStyle: "medium", timeStyle: "short" }).format(
    new Date(epochSeconds * 1000),
  );
}

const monthNames = [
  "jan",
  "feb",
  "mar",
  "apr",
  "may",
  "jun",
  "jul",
  "aug",
  "sep",
  "oct",
  "nov",
  "dec",
];

/**
 * A date as KeenWiki and the index write it ("2017-07-04", "2017-07", "2017",
 * "November 22, 1991[1]", "22 November 1991", "November 1991") in the
 * user's language, as precise as the source. Anything else is shown as is.
 */
export function formatLooseDate(language: string, raw: string): string {
  const text = raw.replace(/\[\d+\]/g, "").trim();
  const month = (name: string) => monthNames.indexOf(name.slice(0, 3).toLowerCase()) + 1;
  let parts: [number, number?, number?] | undefined;
  let m;
  if ((m = /^(\d{4})(?:-(\d{1,2})(?:-(\d{1,2}))?)?$/.exec(text))) {
    parts = [+m[1], m[2] ? +m[2] : undefined, m[3] ? +m[3] : undefined];
  } else if ((m = /^([a-z]+)\.? (?:(\d{1,2})(?:st|nd|rd|th)?,? )?(\d{4})$/i.exec(text))) {
    parts = [+m[3], month(m[1]), m[2] ? +m[2] : undefined];
  } else if ((m = /^(\d{1,2})(?:st|nd|rd|th)?\.? ([a-z]+)\.?,? (\d{4})$/i.exec(text))) {
    parts = [+m[3], month(m[2]), +m[1]];
  }
  if (!parts) return raw;
  const [year, mon, day] = parts;
  if (mon !== undefined && (mon < 1 || mon > 12)) return raw;
  const date = new Date(Date.UTC(year, (mon ?? 1) - 1, day ?? 1));
  if (day !== undefined && date.getUTCDate() !== day) return raw;
  return new Intl.DateTimeFormat(language, {
    timeZone: "UTC",
    year: "numeric",
    ...(mon !== undefined && { month: day !== undefined ? "short" : "long" }),
    ...(day !== undefined && { day: "numeric" }),
  }).format(date);
}

/**
 * "just now", "5 minutes ago", "yesterday", "2 weeks ago". Days and longer
 * count calendar boundaries, so last night reads "yesterday" this morning.
 */
export function formatRelative(
  t: TFunction,
  language: string,
  epochSeconds: number,
  now: number = Date.now(),
): string {
  const then = new Date(epochSeconds * 1000);
  const today = new Date(now);
  const minutes = Math.floor((now - then.getTime()) / 60_000);
  if (minutes < 1) return t("time.justNow");
  // "auto" only for days: "last week" would count calendar weeks, which we don't.
  const rtf = new Intl.RelativeTimeFormat(language);
  if (minutes < 60) return rtf.format(-minutes, "minute");
  const midnight = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((midnight(today) - midnight(then)) / 86_400_000);
  // Shortly after midnight, "2 hours ago" says more than "yesterday".
  if (days === 0 || minutes < 6 * 60) return rtf.format(-Math.floor(minutes / 60), "hour");
  if (days < 7)
    return new Intl.RelativeTimeFormat(language, { numeric: "auto" }).format(-days, "day");
  if (days < 30) return rtf.format(-Math.floor(days / 7), "week");
  const months =
    (today.getFullYear() - then.getFullYear()) * 12 +
    today.getMonth() -
    then.getMonth() -
    (today.getDate() < then.getDate() ? 1 : 0);
  if (months < 12) return rtf.format(-Math.max(months, 1), "month");
  return rtf.format(-Math.floor(months / 12), "year");
}

export function formatNumber(language: string, n: number): string {
  return new Intl.NumberFormat(language).format(n);
}

export function formatMegabytes(bytes: number): string {
  return (bytes / 1024 / 1024).toFixed(1);
}

/** "850 kB", "48.3 MB", "1.2 GB". */
export function formatBytes(language: string, bytes: number): string {
  const units = ["byte", "kilobyte", "megabyte", "gigabyte", "terabyte"];
  let value = bytes;
  let unit = 0;
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000;
    unit++;
  }
  return new Intl.NumberFormat(language, {
    style: "unit",
    unit: units[unit],
    unitDisplay: "short",
    maximumFractionDigits: unit < 2 ? 0 : 1,
  }).format(value);
}
