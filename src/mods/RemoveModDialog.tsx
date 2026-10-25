import { useMutation, useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Profile } from "../bindings";
import { errorMessage } from "../errors";
import { unwrap } from "../ipc";
import { modsKey } from "../library/queries";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { Notice } from "../ui/Notice";

/** Asks before removing a mod; saves are kept as a backup unless unticked. */
export function RemoveModDialog({
  profile,
  onClose,
  onRemoved,
}: {
  profile: Profile & { modId: string };
  onClose: () => void;
  onRemoved: () => void;
}) {
  const { t } = useTranslation();
  const [keepSaves, setKeepSaves] = useState(true);
  const mods = useQuery({ queryKey: modsKey, queryFn: () => unwrap(commands.listMods()) });
  // Mods played over this one go with it.
  const indexId = mods.data?.find((m) => m.id === profile.modId)?.indexId;
  const extensions = indexId ? (mods.data ?? []).filter((m) => m.extends === indexId) : [];
  const remove = useMutation({
    mutationFn: () => unwrap(commands.deleteMod(profile.modId, keepSaves)),
    onSuccess: onRemoved,
  });

  return (
    <Dialog title={t("mods.removeTitle", { name: profile.label })} onClose={onClose}>
      <p className="text-sm text-muted">
        {t("mods.removeHint", {
          // Without its game, there is no game to stay.
          context: profile.missingGame === null ? undefined : "noGame",
        })}
      </p>
      {extensions.length > 0 && (
        <p className="text-sm text-muted">
          {t("mods.removeExtensions", { names: extensions.map((m) => m.name).join(", ") })}
        </p>
      )}
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
      {remove.error && <Notice tone="error">{errorMessage(t, remove.error)}</Notice>}
      <div className="flex justify-end gap-3">
        <Button variant="ghost" onClick={onClose}>
          {t("common.cancel")}
        </Button>
        <Button onClick={() => remove.mutate()} disabled={remove.isPending}>
          {t("mods.remove")}
        </Button>
      </div>
    </Dialog>
  );
}
