import type { Update } from "@tauri-apps/plugin-updater";
import { Download } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { errorMessage } from "../errors";
import { useProfiles } from "../library/queries";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { Notice } from "../ui/Notice";
import { Progress } from "../ui/Progress";
import { useInstallUpdate } from "./queries";

/**
 * Offers a new release to install and restart into. The release notes aren't
 * shown: they'd only be in one language, and the changes appear in the news
 * dialog after updating (`WhatsNew`).
 */
export function UpdateCard({ update, onLater }: { update: Update; onLater?: () => void }) {
  const { t } = useTranslation();
  const install = useInstallUpdate();
  const [progress, setProgress] = useState<number | null>(null);
  const profiles = useProfiles();
  // Installing restarts BloogBase, which would lose track of a running game.
  const playing = profiles.data?.some((p) => p.running) ?? false;

  return (
    <Card className="grid gap-3 p-4">
      <h2 className="font-semibold">{t("updates.available", { version: update.version })}</h2>
      {playing && <Notice>{t("updates.whilePlaying")}</Notice>}
      {install.isPending && (
        <Progress
          label={t("updates.downloading")}
          value={progress === null ? undefined : progress * 100}
          max={progress === null ? undefined : 100}
        />
      )}
      {install.error && <Notice tone="error">{errorMessage(t, install.error)}</Notice>}
      <div className="flex flex-wrap gap-2">
        <Button
          onClick={() => install.mutate({ update, onProgress: setProgress })}
          disabled={playing || install.isPending}
        >
          <Download className="size-4" aria-hidden />
          {t("updates.install")}
        </Button>
        {onLater && (
          <Button variant="ghost" onClick={onLater} disabled={install.isPending}>
            {t("updates.later")}
          </Button>
        )}
      </div>
    </Card>
  );
}
