import { useTranslation } from "react-i18next";
import type { Install, Mod } from "../bindings";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { Notice } from "../ui/Notice";

export type Outcome = {
  games: Install[];
  mods: Mod[];
  /** File name → what went wrong. */
  failed: [string, string][];
};

/**
 * What was added at once: games counted, each mod named, failures listed.
 * A single game or mod offers to open it with `onOpenSingle`.
 */
export function AddedDialog({
  outcome,
  onClose,
  onOpenSingle,
}: {
  outcome: Outcome;
  onClose: () => void;
  onOpenSingle?: () => void;
}) {
  const { t } = useTranslation();
  const single = !!onOpenSingle && outcome.games.length + outcome.mods.length === 1;
  return (
    <Dialog title={t("drop.title")} onClose={onClose}>
      <div className="grid gap-2">
        {outcome.games.length > 0 && (
          <Notice>{t("library.imported", { count: outcome.games.length })}</Notice>
        )}
        {outcome.mods.map((mod) => (
          <Notice key={mod.id}>{t("mods.installed", { name: mod.name })}</Notice>
        ))}
        {outcome.failed.map(([name, message]) => (
          <Notice key={name} tone="error">
            <span className="font-semibold">{name}:</span> {message}
          </Notice>
        ))}
      </div>
      <div className="flex justify-end gap-2">
        <Button variant={single ? "secondary" : "primary"} onClick={onClose}>
          {t("common.close")}
        </Button>
        {single && (
          <Button data-autofocus onClick={onOpenSingle}>
            {t("discover.openDetails")}
          </Button>
        )}
      </div>
    </Dialog>
  );
}
