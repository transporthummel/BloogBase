import { useTranslation } from "react-i18next";
import type { Job } from "../bindings";
import { gameTitle, useGames } from "../catalog";

/** What a job is about, for lists and progress bars. */
export function useJobLabel() {
  const { t } = useTranslation();
  const games = useGames();
  return (job: Job) =>
    job.kind === "runtime"
      ? t("jobs.runtime")
      : t("jobs.shareware", { game: gameTitle(games.data, job.subject ?? "") });
}
