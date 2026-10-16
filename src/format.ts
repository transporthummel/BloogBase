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

export function formatMegabytes(bytes: number): string {
  return (bytes / 1024 / 1024).toFixed(1);
}
