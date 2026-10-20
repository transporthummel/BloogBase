import { useTranslation } from "react-i18next";
import type { Job } from "../bindings";
import { gameTitle, useGames } from "../catalog";
import { useIndexMods } from "../discover/queries";

/** What a job is about, for lists and progress bars. */
export function useJobLabel() {
  const { t } = useTranslation();
  const games = useGames();
  const mods = useIndexMods();
  return (job: Job) => {
    const subject = job.subject ?? "";
    switch (job.kind) {
      case "runtime":
        return t("jobs.runtime");
      case "freeGame": {
        const game = games.data?.find((g) => g.versions.some((v) => v.id === subject));
        const version = game?.versions.find((v) => v.id === subject);
        return t("jobs.freeGame", {
          game: gameTitle(games.data, game?.id ?? subject),
          version: version?.label ?? "",
        });
      }
      case "mod":
        return mods.data?.find((m) => m.entry.id === subject)?.entry.name ?? subject;
    }
  };
}
