import { Link } from "@tanstack/react-router";
import { ArrowRight } from "lucide-react";
import { useTranslation } from "react-i18next";
import { buttonClass } from "../ui/buttonClass";

/** Opens an installed game's or mod's play profile, styled as a button. */
export function DetailsLink({ profileId }: { profileId: string }) {
  const { t } = useTranslation();
  return (
    <Link
      to="/profiles/$profileId"
      params={{ profileId }}
      className={buttonClass("secondary")}
    >
      {t("discover.openDetails")}
      <ArrowRight className="size-4" aria-hidden />
    </Link>
  );
}
