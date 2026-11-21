import { useTranslation } from "react-i18next";
import type { ModStatus } from "../bindings";
import { Badge } from "../ui/Badge";

const SHOWN = ["alpha", "beta", "abandoned"];

/** How far along a mod is, unless complete; a warning, not one more label of what it is. */
export function StatusBadge({ status }: { status?: ModStatus | null }) {
  const { t } = useTranslation();
  if (!status || !SHOWN.includes(status)) return null;
  return <Badge tone="warning">{t(`discover.status.${status}`)}</Badge>;
}
