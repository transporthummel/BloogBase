import { Gamepad2, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { events } from "../bindings";
import { useProfiles } from "../library/queries";

/**
 * DOSBox looks for a controller only at its start. When one is connected
 * while a game runs that found none, this says to start the game again. It
 * goes when the game ends or the player closes it.
 */
export function LateController() {
  const { t } = useTranslation();
  const [profileId, setProfileId] = useState<string | null>(null);
  const profiles = useProfiles();

  useEffect(() => {
    const late = events.controllerTooLate.listen(({ payload }) => setProfileId(payload.profileId));
    const ends = events.playSessionEnded.listen(({ payload }) =>
      setProfileId((id) => (id === payload.profileId ? null : id)),
    );
    return () => {
      late.then((f) => f());
      ends.then((f) => f());
    };
  }, []);

  if (profileId === null) return null;
  const name = profiles.data?.find((p) => p.id === profileId)?.label ?? "";
  return (
    <div
      role="status"
      className="bb-hint-in fixed top-4 left-1/2 z-40 flex w-max max-w-[calc(100%-2rem)] -translate-x-1/2 items-center gap-2.5 rounded-full border border-border bg-surface py-1.5 pr-1.5 pl-3 text-sm shadow-card"
    >
      <Gamepad2 className="size-4 shrink-0 text-muted" aria-hidden />
      {t("pad.tooLate", { name })}
      <button
        type="button"
        aria-label={t("pad.dismiss")}
        title={t("pad.dismiss")}
        className="flex size-7 shrink-0 items-center justify-center rounded-full text-muted transition hover:bg-surface-raised hover:text-ink"
        onClick={() => setProfileId(null)}
      >
        <X className="size-4" aria-hidden />
      </button>
    </div>
  );
}
