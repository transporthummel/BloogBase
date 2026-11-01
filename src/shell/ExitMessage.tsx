import { Bug } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { events } from "../bindings";
import { useProfiles } from "../library/queries";
import { ModReportDialog } from "../mods/ModReport";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";

type Ended = { profileId: string; message: string | null; startProblem: boolean };

/**
 * Shows what a game printed on the DOS screen when it ended, e.g. a
 * patcher's error: DOSBox closes right away, so nobody could read it. When a
 * mod's first session with its setup ended like that, or after only a few
 * seconds, it offers to report that the mod doesn't start.
 */
export function ExitMessage() {
  const { t } = useTranslation();
  const [ended, setEnded] = useState<Ended[]>([]);
  const [reporting, setReporting] = useState<{ profileId: string; name: string } | null>(null);
  const profiles = useProfiles();

  useEffect(() => {
    const unlisten = events.playSessionEnded.listen(({ payload }) => {
      const { profileId, message, startProblem } = payload;
      if (message || startProblem) setEnded((e) => [...e, { profileId, message, startProblem }]);
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  if (reporting) {
    return (
      <ModReportDialog
        profileId={reporting.profileId}
        name={reporting.name}
        onClose={() => setReporting(null)}
      />
    );
  }
  const first = ended[0];
  if (!first) return null;
  const name = profiles.data?.find((p) => p.id === first.profileId)?.label ?? first.profileId;
  const close = () => setEnded((e) => e.slice(1));
  const report = first.startProblem && (
    <Button
      variant="secondary"
      onClick={() => {
        close();
        setReporting({ profileId: first.profileId, name });
      }}
    >
      <Bug className="size-4" aria-hidden />
      {t("modReport.button")}
    </Button>
  );

  if (!first.message) {
    return (
      <Dialog title={t("exitMessage.shortTitle", { name })} onClose={close}>
        <p className="text-sm">{t("exitMessage.shortText")}</p>
        <div className="flex flex-wrap justify-end gap-2">
          {report}
          <Button data-autofocus onClick={close}>
            {t("exitMessage.fine")}
          </Button>
        </div>
      </Dialog>
    );
  }
  return (
    <Dialog title={t("exitMessage.title", { name })} onClose={close}>
      <p className="text-sm">{t("exitMessage.text")}</p>
      <pre className="max-h-[50vh] overflow-auto rounded-lg bg-black p-3 font-mono text-sm whitespace-pre-wrap text-[#aaa] select-text">
        {first.message}
      </pre>
      <div className="flex flex-wrap justify-end gap-2">
        {report}
        <Button data-autofocus onClick={close}>
          {t("common.close")}
        </Button>
      </div>
    </Dialog>
  );
}
