import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, events } from "../bindings";
import { unwrap } from "../ipc";
import { profilesKey } from "../library/queries";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";

type Ended = { profileId: string; message: string };

/**
 * Shows what a game printed on the DOS screen when it ended, e.g. a
 * patcher's error: DOSBox closes right away, so nobody could read it.
 */
export function ExitMessage() {
  const { t } = useTranslation();
  const [ended, setEnded] = useState<Ended[]>([]);
  const profiles = useQuery({
    queryKey: profilesKey,
    queryFn: () => unwrap(commands.listProfiles()),
  });

  useEffect(() => {
    const unlisten = events.playSessionEnded.listen(({ payload }) => {
      const { profileId, message } = payload;
      if (message) setEnded((e) => [...e, { profileId, message }]);
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  const first = ended[0];
  if (!first) return null;
  const name = profiles.data?.find((p) => p.id === first.profileId)?.label ?? first.profileId;
  const close = () => setEnded((e) => e.slice(1));
  return (
    <Dialog title={t("exitMessage.title", { name })} onClose={close}>
      <p className="text-sm">{t("exitMessage.text")}</p>
      <pre className="max-h-[50vh] overflow-auto rounded-lg bg-black p-3 font-mono text-sm whitespace-pre-wrap text-[#aaa] select-text">
        {first.message}
      </pre>
      <div className="flex justify-end">
        <Button data-autofocus onClick={close}>
          {t("common.close")}
        </Button>
      </div>
    </Dialog>
  );
}
