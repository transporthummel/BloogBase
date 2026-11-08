import { TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";

/** The player's own DOSBox is older than the one BloogBase is tested with. */
export function OldRuntimeWarning({ testedVersion }: { testedVersion: string }) {
  const { t } = useTranslation();
  return (
    // Boxed in the warning colour, like an error but softer: it matters, but
    // games still start.
    <p
      role="status"
      className="flex items-start gap-2 rounded-lg border border-warning/50 bg-warning/10 px-3 py-2 text-sm"
    >
      <TriangleAlert className="mt-0.5 size-4 shrink-0 text-warning" aria-hidden />
      {t("runtime.crtSwitchIgnored", { version: testedVersion })}
    </p>
  );
}
