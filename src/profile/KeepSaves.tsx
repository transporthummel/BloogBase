import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { commands, type Backup, type Profile } from "../bindings";
import { unwrap } from "../ipc";
import { backupsKey } from "./saves";

/** The backups "keep saves" decides about: automatic ones go either way. */
function keptBackups(backups: Backup[]): number {
  return backups.filter((b) => !b.auto || b.reason === "modRemoved" || b.reason === "gameRemoved")
    .length;
}

/**
 * The "keep saves" checkbox of the remove dialogs: shown when the profile
 * has saves, or backups that would go without it.
 */
export function KeepSaves({
  profile,
  checked,
  onChange,
}: {
  profile: Profile;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  const { t } = useTranslation();
  const backups = useQuery({
    queryKey: backupsKey(profile.id),
    queryFn: () => unwrap(commands.listBackups(profile.id)),
  });
  const saves = profile.saveCount;
  const kept = keptBackups(backups.data ?? []);
  if (saves === 0 && kept === 0) {
    return null;
  }
  const label =
    saves === 0
      ? t("mods.keepBackups", { count: kept })
      : kept === 0
        ? t("mods.keepSaves", { count: saves })
        : t("mods.keepSavesAndBackups", { count: saves });
  return (
    <label className="flex items-center gap-2 text-sm">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      {label}
    </label>
  );
}
