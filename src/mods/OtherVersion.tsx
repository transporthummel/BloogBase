import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useFreeGames } from "../discover/queries";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";

/**
 * "Learn more" with why patch mods need one exact release, for where a mod
 * doesn't run on the player's copy. Says when a fitting one is free.
 */
export function WhyExactVersion({ requires }: { requires: string[] }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const free = useFreeGames();
  const freeFit = (free.data ?? []).some((f) => requires.includes(f.versionId));
  return (
    <>
      <button
        type="button"
        className="cursor-pointer text-ink underline underline-offset-2"
        onClick={() => setOpen(true)}
      >
        {t("mods.exactVersion.learnMore")}
      </button>
      {open && (
        <Dialog title={t("mods.exactVersion.title")} onClose={() => setOpen(false)}>
          <div className="grid gap-3 text-sm select-text">
            <p>{t("mods.exactVersion.patches")}</p>
            <p>{t("mods.exactVersion.releases")}</p>
            <p>{t("mods.exactVersion.sourceMods")}</p>
            {freeFit && <p>{t("mods.exactVersion.free")}</p>}
          </div>
          <div className="flex justify-end">
            <Button data-autofocus onClick={() => setOpen(false)}>
              {t("common.close")}
            </Button>
          </div>
        </Dialog>
      )}
    </>
  );
}
