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
