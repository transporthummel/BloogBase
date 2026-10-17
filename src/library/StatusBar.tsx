import { useQuery } from "@tanstack/react-query";
import { HardDrive } from "lucide-react";
import { useTranslation } from "react-i18next";
import { commands, type Profile, type StorageUsage } from "../bindings";
import { formatBytes, formatNumber, formatPlayTime } from "../format";
import { unwrap } from "../ipc";

const areas: (keyof Omit<StorageUsage, "total">)[] = [
  "games",
  "mods",
  "profiles",
  "backups",
  "runtimes",
  "cache",
];

/** Counts and disk space at the foot of the library. */
export function StatusBar({ profiles, updatedAt }: { profiles: Profile[]; updatedAt: number }) {
  const { t, i18n } = useTranslation();
  // Measured again whenever the profile list was fetched again.
  const usage = useQuery({
    queryKey: ["storage", updatedAt],
    queryFn: () => unwrap(commands.storageUsage()),
    placeholderData: (previous) => previous,
  });
  const lng = i18n.language;
  const games = profiles.filter((p) => p.modId === null).length;
  const mods = profiles.length - games;
  const seconds = profiles.reduce((sum, p) => sum + p.playSeconds, 0);
  const saves = profiles.reduce((sum, p) => sum + p.saveCount, 0);
  const u = usage.data;
  // Sizes are floats, which the bindings type as nullable for NaN.
  const parts = u
    ? areas.map((a) => `${t(`library.status.area.${a}`)}: ${formatBytes(lng, u[a] ?? 0)}`)
    : [];

  const items = [
    t("library.status.games", { count: games, n: formatNumber(lng, games) }),
    t("library.status.mods", { count: mods, n: formatNumber(lng, mods) }),
    t("library.status.played", { time: formatPlayTime(t, seconds) }),
    t("library.status.saves", { count: saves, n: formatNumber(lng, saves) }),
  ];

  return (
    <footer className="sticky bottom-0 -mx-6 mt-8 -mb-6 border-t border-border bg-surface/95 px-6 py-2 text-xs text-muted backdrop-blur xl:-mx-8 xl:-mb-8 xl:px-8">
      <ul className="flex flex-wrap items-center gap-x-5 gap-y-1">
        {items.map((item) => (
          <li key={item}>{item}</li>
        ))}
        {u && (
          <li className="ml-auto flex items-center gap-1.5" title={parts.join("\n")}>
            <HardDrive className="size-3.5" aria-hidden />
            {t("library.status.used", { size: formatBytes(lng, u.total ?? 0) })}
            <span className="sr-only">({parts.join(", ")})</span>
          </li>
        )}
      </ul>
    </footer>
  );
}
