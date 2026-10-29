/** A small green Bloog, drawn on an 8×8 grid: body, eye, pupil, feet. */
const BODY = "M2 1h4v1h1v4h-1v1h-4v-1h-1v-4h1z";
const EYE = "M3 2h2v2h-2z";
const PUPIL = "M4 3h1v1h-1z";
const FEET = "M2 7h1v1h-1zM5 7h1v1h-1z";

/** The Bloog of the boot splash; `bb-hop` makes it hop. */
export function Bloog({ className = "" }: { className?: string }) {
  return (
    <svg viewBox="0 0 8 8" className={className} shapeRendering="crispEdges" aria-hidden>
      <path d={BODY} fill="var(--bb-bloog)" />
      <path d={FEET} fill="var(--bb-bloog)" />
      <path d={EYE} fill="#ffffff" />
      <path d={PUPIL} fill="#000000" />
    </svg>
  );
}
