import { useMutation, useQuery } from "@tanstack/react-query";
import { Puzzle } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Profile } from "../bindings";
import { errorMessage } from "../errors";
import { unwrap } from "../ipc";
import { modsKey } from "../library/queries";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { Notice } from "../ui/Notice";
import { KeepSaves } from "../profile/KeepSaves";

/** Asks before removing a mod; saves and backups are kept unless unticked. */
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
  const extensions = (mods.data ?? []).filter((m) => m.extends === profile.modId);
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
        <p className="flex items-start gap-2 text-sm text-muted">
          <Puzzle className="mt-0.5 size-4 shrink-0 text-accent" aria-hidden />
          <span>
            {t("mods.removeExtensions", { names: extensions.map((m) => m.name).join(", ") })}
          </span>
        </p>
      )}
      <KeepSaves profile={profile} checked={keepSaves} onChange={setKeepSaves} />
      {remove.error && <Notice tone="error">{errorMessage(t, remove.error)}</Notice>}
      <div className="flex justify-end gap-3">
        <Button variant="ghost" onClick={onClose} data-autofocus>
          {t("common.cancel")}
        </Button>
        <Button variant="danger" onClick={() => remove.mutate()} disabled={remove.isPending}>
          {t("mods.remove")}
        </Button>
      </div>
    </Dialog>
  );
}
