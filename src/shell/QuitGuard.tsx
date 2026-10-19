import { useMutation } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, events } from "../bindings";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";

/** Asks what to do when BloogBase is closed while games still run. */
export function QuitGuard() {
  const { t } = useTranslation();
  const [games, setGames] = useState<string[] | null>(null);
  const [stopGames, setStopGames] = useState(false);
  const quit = useMutation({ mutationFn: () => commands.quitApp(stopGames) });

  useEffect(() => {
    const unlisten = events.quitRequested.listen((e) => setGames(e.payload.games));
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  if (!games) return null;
  return (
    <Dialog title={t("quit.title", { count: games.length })} onClose={() => setGames(null)}>
      <p className="text-sm">{t("quit.body", { count: games.length, games: games.join(", ") })}</p>
      <div className="grid gap-1">
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={stopGames}
            onChange={(e) => setStopGames(e.target.checked)}
          />
          {t("quit.stopGames", { count: games.length })}
        </label>
        <p className="pl-6 text-sm text-muted">
          {stopGames ? t("quit.stopHint") : t("quit.keepHint", { count: games.length })}
        </p>
      </div>
      <div className="flex justify-end gap-3">
        <Button variant="ghost" onClick={() => setGames(null)}>
          {t("common.cancel")}
        </Button>
        <Button onClick={() => quit.mutate()} disabled={quit.isPending}>
          {t("quit.confirm")}
        </Button>
      </div>
    </Dialog>
  );
}
