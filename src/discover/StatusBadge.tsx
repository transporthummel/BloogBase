import { useTranslation } from "react-i18next";
import type { ModStatus } from "../bindings";
import { Badge } from "../ui/Badge";

const SHOWN = ["alpha", "beta", "abandoned"];

/** How far along a mod is, unless complete; abandoned ones as a warning. */
export function StatusBadge({ status }: { status?: ModStatus | null }) {
  const { t } = useTranslation();
  if (!status || !SHOWN.includes(status)) return null;
  return (
    <Badge tone={status === "abandoned" ? "warning" : undefined}>
      {t(`discover.status.${status}`)}
    </Badge>
  );
}
