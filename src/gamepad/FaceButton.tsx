import type { Controller } from "../bindings";
import type { FaceButton } from "./buttons";

/** Xbox: coloured buttons with letters. */
const FACE_BG: Record<FaceButton, string> = {
  a: "bg-pad-a",
  b: "bg-pad-b",
  x: "bg-pad-x",
  y: "bg-pad-y",
};

/**
 * PlayStation: dark buttons with coloured symbols, at the same places
 * (Cross for A, Circle for B, Square for X, Triangle for Y).
 */
const SYMBOL_STROKE: Record<FaceButton, string> = {
  a: "stroke-pad-x",
  b: "stroke-pad-b",
  x: "stroke-pad-square",
  y: "stroke-pad-a",
};

/** A PlayStation symbol, drawn around (0, 0), `r` from its centre to its edge. */
export function PsSymbol({ button, r }: { button: FaceButton; r: number }) {
  const className = `fill-none ${SYMBOL_STROKE[button]}`;
  const w = Math.max(1.2, r / 3.5);
  switch (button) {
    case "a": {
      // The arms' ends on the circle of `r`.
      const k = r * 0.75;
      return (
        <path
          className={className}
          strokeWidth={w}
          d={`M${-k} ${-k}L${k} ${k}M${k} ${-k}L${-k} ${k}`}
        />
      );
    }
    case "b":
      return <circle className={className} strokeWidth={w} r={r} />;
    case "x":
      return (
        <rect
          className={className}
          strokeWidth={w}
          x={-r * 0.8}
          y={-r * 0.8}
          width={r * 1.6}
          height={r * 1.6}
        />
      );
    case "y":
      return (
        <path
          className={className}
          strokeWidth={w}
          strokeLinejoin="round"
          d={`M0 ${-r * 1.15}L${r * 1.1} ${r * 0.75}L${-r * 1.1} ${r * 0.75}Z`}
        />
      );
  }
}

/** A face button as a small chip for hints beside text. */
export function Face({ button, layout }: { button: FaceButton; layout: Controller }) {
  if (layout === "playstation") {
    return (
      <svg aria-hidden className="size-5 shrink-0" viewBox="-10 -10 20 20">
        <circle className="fill-pad-letter" r={10} />
        <PsSymbol button={button} r={4} />
      </svg>
    );
  }
  return (
    <span
      aria-hidden
      className={`inline-flex size-5 items-center justify-center rounded-full text-[0.7rem] font-bold text-pad-letter ${FACE_BG[button]}`}
    >
      {button.toUpperCase()}
    </span>
  );
}
