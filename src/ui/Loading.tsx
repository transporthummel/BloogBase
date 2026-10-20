import { useTranslation } from "react-i18next";
import { Bloog } from "./Bloog";

/** Shown while something loads; a hopping Bloog and a label for screen readers. */
export function Loading({ label }: { label?: string }) {
  const { t } = useTranslation();
  return (
    <div role="status" className="flex items-center justify-center gap-3 py-10 text-sm text-muted">
      <Bloog className="bb-hop size-8" />
      <span>{label ?? t("common.loading")}</span>
    </div>
  );
}
