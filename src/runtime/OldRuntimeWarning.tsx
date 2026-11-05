import { TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";

/** The player's own DOSBox is older than the one BloogBase is tested with. */
export function OldRuntimeWarning({ testedVersion }: { testedVersion: string }) {
  const { t } = useTranslation();
  return (
    <p role="status" className="flex items-start gap-2 text-sm text-muted">
      <TriangleAlert className="mt-0.5 size-4 shrink-0 text-warning" aria-hidden />
      {t("runtime.crtSwitchIgnored", { version: testedVersion })}
    </p>
  );
}
