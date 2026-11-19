import { useQuery } from "@tanstack/react-query";
import type { TFunction } from "i18next";
import { commands, type Backup, type Engine, type SaveDir, type SaveFile } from "../bindings";
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

/**
 * "Slot 1 · pabba". Where another save among `others` has the same slot
 * (a mod saving under two names, like Foray in the Forest's .CKS and .CKC
 * builds), the file name tells them apart: "Slot 1 (SAVEGAM0.CKS)".
 */
export function saveTitle(t: TFunction, save: SaveFile, others: SaveFile[] = []): string {
  // Each folder numbers its own slots.
  const folder = saveFolder(save.file)?.toUpperCase();
  const shared =
    save.slot !== null &&
    others.some(
      (o) =>
        o.slot === save.slot &&
        o.file !== save.file &&
        saveFolder(o.file)?.toUpperCase() === folder,
    );
  const file = save.file.slice(save.file.lastIndexOf("/") + 1);
  const slot =
    save.slot === null
      ? file
      : shared
        ? t("saves.slotFile", { slot: save.slot, file })
        : t("saves.slot", { slot: save.slot });
  const name = save.details?.name;
  return name ? `${slot} · ${name}` : slot;
}

/** The folder a save is in (`KEEN` of `KEEN/SAVEGAM0.CK4`), or null in the root. */
export function saveFolder(file: string): string | null {
  const at = file.lastIndexOf("/");
  return at < 0 ? null : file.slice(0, at);
}

/** What a mod's index entry calls one of its save folders, in `language`. */
export function folderName(dirs: SaveDir[], dir: string, language: string): string {
  const found = dirs.find((d) => d.dir.toUpperCase() === dir.toUpperCase());
  if (!found) return dir;
  return found.translations?.[language.split("-")[0]]?.name ?? found.name;
}

/**
 * "Craig · Slot 1 · pabba": the title of a save of a mod that keeps its
 * saves in folders (a launcher starting each part in its own), with the
 * folder's name; otherwise just its title.
 */
export function placedSaveTitle(
  t: TFunction,
  language: string,
  dirs: SaveDir[],
  save: SaveFile,
  others: SaveFile[] = [],
): string {
  const title = saveTitle(t, save, others);
  const dir = saveFolder(save.file);
  return dir !== null && dirs.length > 0 ? `${folderName(dirs, dir, language)} · ${title}` : title;
}

export type SaveGroup = { dir: string | null; name: string | null; saves: SaveFile[] };

/**
 * Saves by folder: those in the root first, unnamed, then each folder of
 * `dirs` in its order under its name. Folders without saves are left out.
 */
export function groupSaves(saves: SaveFile[], dirs: SaveDir[], language: string): SaveGroup[] {
  if (dirs.length === 0) return [{ dir: null, name: null, saves }];
  const groups: SaveGroup[] = [];
  const root = saves.filter((s) => saveFolder(s.file) === null);
  if (root.length > 0) groups.push({ dir: null, name: null, saves: root });
  for (const d of dirs) {
    const inDir = saves.filter((s) => saveFolder(s.file)?.toUpperCase() === d.dir.toUpperCase());
    if (inDir.length > 0) {
      groups.push({ dir: d.dir, name: folderName(dirs, d.dir, language), saves: inDir });
    }
  }
  return groups;
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
