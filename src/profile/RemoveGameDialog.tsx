import { useMutation } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Profile } from "../bindings";
import { errorMessage } from "../errors";
import { unwrap } from "../ipc";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { Notice } from "../ui/Notice";

/** Asks before uninstalling a game, and whether its mods go too. */
export function RemoveGameDialog({
  profile,
  installId,
  modCount,
  onClose,
  onRemoved,
}: {
  profile: Profile;
  installId: string;
  /** Mods played on this install. */
  modCount: number;
  onClose: () => void;
  onRemoved: () => void;
}) {
  const { t } = useTranslation();
  const [keepSaves, setKeepSaves] = useState(true);
  const [withMods, setWithMods] = useState(false);
  const remove = useMutation({
    mutationFn: () => unwrap(commands.removeGame(installId, keepSaves, withMods)),
    onSuccess: onRemoved,
  });

  return (
    <Dialog title={t("profile.removeGameTitle", { name: profile.label })} onClose={onClose}>
      <p className="text-sm text-muted">{t("profile.removeGameHint")}</p>
      {profile.saveCount > 0 && (
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={keepSaves}
            onChange={(e) => setKeepSaves(e.target.checked)}
          />
          {t("mods.keepSaves", { count: profile.saveCount })}
        </label>
      )}
      {modCount > 0 && (
        <div className="grid gap-1">
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={withMods}
              onChange={(e) => setWithMods(e.target.checked)}
            />
            {t("profile.removeGameMods", { count: modCount })}
          </label>
          {!withMods && (
            <p className="pl-6 text-sm text-muted">{t("profile.removeGameModsKept")}</p>
          )}
        </div>
      )}
      {remove.error && <Notice tone="error">{errorMessage(t, remove.error)}</Notice>}
      <div className="flex justify-end gap-3">
        <Button variant="ghost" onClick={onClose}>
          {t("common.cancel")}
        </Button>
        <Button onClick={() => remove.mutate()} disabled={remove.isPending}>
          {t("profile.removeGame")}
        </Button>
      </div>
    </Dialog>
  );
}
