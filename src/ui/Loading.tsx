import { useTranslation } from "react-i18next";

/** A small green Bloog, drawn on an 8×8 grid: body, eye, pupil, feet. */
const BODY = "M2 1h4v1h1v4h-1v1h-4v-1h-1v-4h1z";
const EYE = "M3 2h2v2h-2z";
const PUPIL = "M4 3h1v1h-1z";
const FEET = "M2 7h1v1h-1zM5 7h1v1h-1z";

/** Shown while something loads; a hopping Bloog and a label for screen readers. */
export function Loading({ label }: { label?: string }) {
  const { t } = useTranslation();
  return (
    <div role="status" className="flex items-center justify-center gap-3 py-10 text-sm text-muted">
      <svg viewBox="0 0 8 8" className="bb-hop size-8" shapeRendering="crispEdges" aria-hidden>
        <path d={BODY} fill="var(--bb-ega-bright-green)" />
        <path d={FEET} fill="var(--bb-ega-bright-green)" />
        <path d={EYE} fill="#ffffff" />
        <path d={PUPIL} fill="#000000" />
      </svg>
      <span>{label ?? t("common.loading")}</span>
    </div>
  );
}
