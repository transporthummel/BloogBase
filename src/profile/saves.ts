import { useQuery } from "@tanstack/react-query";
import type { TFunction } from "i18next";
import { commands, type Backup, type Engine, type SaveFile } from "../bindings";
import { formatNumber } from "../format";
import { unwrap } from "../ipc";

export const savesKey = (profileId: string) => ["saves", profileId] as const;
export const backupsKey = (profileId: string) => ["backups", profileId] as const;

export function useSaves(profileId: string) {
  return useQuery({
    queryKey: savesKey(profileId),
    queryFn: () => unwrap(commands.listSaves(profileId)),
  });
}

/** "Score 48,100 · 13 lives · 45 shots · Hard". */
export function saveSummary(t: TFunction, language: string, save: SaveFile): string {
  const d = save.details;
  if (!d) return t("saves.unreadable");
  const parts: string[] = [];
  if (d.score !== null) parts.push(t("saves.score", { score: formatNumber(language, d.score) }));
  if (d.lives !== null) parts.push(t("saves.lives", { count: d.lives }));
  if (d.ammo !== null) parts.push(t("saves.ammo", { count: d.ammo }));
  if (d.shipParts !== null) parts.push(t("saves.shipParts", { count: d.shipParts }));
  if (d.levelsDone !== null) parts.push(t("saves.levelsDone", { count: d.levelsDone }));
  if (d.difficulty !== null) parts.push(t(`saves.difficulty.${d.difficulty}`));
  return parts.join(" · ");
}

export function saveTitle(t: TFunction, save: SaveFile): string {
  const slot = save.slot === null ? save.file : t("saves.slot", { slot: save.slot });
  const name = save.details?.name;
  return name ? `${slot} · ${name}` : slot;
}

export function backupTitle(t: TFunction, backup: Backup): string {
  if (backup.reason) return t(`backups.reason.${backup.reason}`);
  return backup.label ?? t("backups.manual");
}

/** "3 saves + settings and high scores". */
export function backupContents(t: TFunction, backup: Backup): string {
  const parts: string[] = [];
  if (backup.saves > 0) parts.push(t("backups.holdsSaves", { count: backup.saves }));
  if (backup.settings > 0) parts.push(t("backups.holdsSettings"));
  if (backup.other > 0) parts.push(t("backups.holdsOther", { count: backup.other }));
  const text = parts.join(" + ") || t("backups.holdsNothing");
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/**
 * The save slots an episode has, as file names in the style of `file`:
 * Keen 1-3 use SAVED1-9, the others SAVEGAM0-5 (shown as slots 1-6).
 */
export function slotFiles(engine: Engine, file: string): { file: string; slot: number }[] {
  const digits = engine === "vorticons" ? [1, 2, 3, 4, 5, 6, 7, 8, 9] : [0, 1, 2, 3, 4, 5];
  const match = /\d(?=\.[^.]*$)/.exec(file);
  if (!match) return [];
  return digits.map((d) => ({
    file: file.slice(0, match.index) + d + file.slice(match.index + 1),
    slot: engine === "vorticons" ? d : d + 1,
  }));
}
