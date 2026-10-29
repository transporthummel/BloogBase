import { RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { CommandError } from "../bindings";
import { errorMessage } from "../errors";
import { Button } from "../ui/Button";
import { Notice } from "../ui/Notice";
import { useRefreshIndex } from "./queries";

/** Why the background check of the mod list failed, with a way to try again.
 * The player didn't start that download, so a bare network error wouldn't say
 * what failed. `wizard` speaks of the suggestions instead of the list. */
export function IndexError({
  error,
  cached,
  wizard = false,
}: {
  error: CommandError;
  /** An older list is still shown. */
  cached: boolean;
  wizard?: boolean;
}) {
  const { t } = useTranslation();
  const refresh = useRefreshIndex();
  const network = error.kind === "network";
  const message = wizard
    ? t("setup.mods.unavailable")
    : cached
      ? network
        ? t("discover.offline")
        : t("discover.stale", { message: errorMessage(t, error) })
      : network
        ? t("discover.listFailed")
        : errorMessage(t, error);
  return (
    <Notice tone="error">
      <span className="block">{message}</span>
      <Button
        variant="secondary"
        className="mt-2"
        onClick={() => refresh.mutate()}
        disabled={refresh.isPending}
      >
        <RefreshCw className={`size-4 ${refresh.isPending ? "animate-spin" : ""}`} aria-hidden />
        {t("jobs.retry")}
      </Button>
    </Notice>
  );
}
