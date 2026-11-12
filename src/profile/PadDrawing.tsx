import type { Controller } from "../bindings";
import { buttonName } from "../gamepad/buttons";
import { PsSymbol } from "../gamepad/FaceButton";
import type { PadInput } from "./controlSheet";

/** One input's callout: its label's lines, beside the drawing. */
export interface Callout {
  input: PadInput;
  lines: string[];
}

/**
 * Where each callout sits: its side, the height of its first line, and the
 * leader line's points from the label to the input. Laid out so no line
 * crosses another input or line.
 */
const anchors: Record<PadInput, { side: "left" | "right"; y: number; line: string }> = {
  lt: { side: "left", y: 38, line: "233,38 286,38 303,47" },
  lb: { side: "left", y: 66, line: "233,66 270,90" },
  view: { side: "left", y: 96, line: "233,96 395,96 409,124" },
  ls: { side: "left", y: 140, line: "233,140 294,140" },
  dpad: { side: "left", y: 215, line: "233,215 372,215" },
  rt: { side: "right", y: 38, line: "667,38 614,38 597,47" },
  rb: { side: "right", y: 66, line: "667,66 630,90" },
  menu: { side: "right", y: 96, line: "667,96 505,96 491,124" },
  y: { side: "right", y: 118, line: "667,118 588,113" },
  b: { side: "right", y: 142, line: "667,142 615,140" },
  a: { side: "right", y: 166, line: "667,166 588,167" },
  x: { side: "right", y: 190, line: "667,190 541,190 541,157" },
  rs: { side: "right", y: 215, line: "667,215 538,215" },
};

const face: { input: "a" | "b" | "x" | "y"; x: number; y: number; fill: string }[] = [
  { input: "y", x: 570, y: 113, fill: "fill-pad-y" },
  { input: "x", x: 541, y: 140, fill: "fill-pad-x" },
  { input: "b", x: 599, y: 140, fill: "fill-pad-b" },
  { input: "a", x: 570, y: 167, fill: "fill-pad-a" },
];

/** The body's outline: shoulders, grips and the curve between them. */
const BODY =
  "M450 82 C500 80 545 73 580 78 C615 84 636 106 646 140 C659 185 672 250 676 285 " +
  "C680 320 655 338 628 334 C605 330 590 312 575 292 C560 272 540 263 500 263 L400 263 " +
  "C360 263 340 272 325 292 C310 312 295 330 272 334 C245 338 220 320 224 285 " +
  "C228 250 241 185 254 140 C264 106 285 84 320 78 C355 73 400 80 450 82 Z";

/** The left bumper; the body hides all but its outer edge. Mirrored for RB. */
const BUMPER =
  "M266 150 L257 128 C262 98 284 70 322 62 C352 56 384 58 404 64 C412 67 416 74 418 92 Z";
/** The left trigger's top, behind the bumper. */
const TRIGGER = "M296 70 C298 44 312 32 334 32 C356 32 370 44 372 66 L372 90 L296 90 Z";
const mirror = "translate(900 0) scale(-1 1)";

/**
 * A generic controller in the Xbox layout (an own drawing, no maker's
 * artwork or logo), with callouts to what each input does. Inputs without
 * one are drawn faintly. For a PlayStation controller, the buttons carry
 * its symbols and names at the same places. Decorative for screen readers:
 * the callouts are listed beside it for them.
 */
export function PadDrawing({
  callouts,
  layout = "xbox",
}: {
  callouts: Callout[];
  layout?: Controller;
}) {
  const ps = layout === "playstation";
  const used = new Set(callouts.map((c) => c.input));
  const part = (input: PadInput) => (used.has(input) ? "" : "opacity-35");
  const cap = "fill-surface stroke-muted";
  return (
    <svg
      viewBox="0 0 900 350"
      className="h-auto w-full text-ink"
      aria-hidden
      focusable="false"
      fontFamily="inherit"
      strokeWidth={1.5}
    >
      {/* Triggers, then bumpers, behind the body. */}
      <path className={`${cap} ${part("lt")}`} d={TRIGGER} />
      <path className={`${cap} ${part("rt")}`} d={TRIGGER} transform={mirror} />
      <path className={`${cap} ${part("lb")}`} d={BUMPER} />
      <path className={`${cap} ${part("rb")}`} d={BUMPER} transform={mirror} />
      <path className="fill-surface-raised stroke-border" d={BODY} strokeLinejoin="round" />
      <Label x={334} y={50} text={buttonName(layout, "lt")} className={part("lt")} />
      <Label x={566} y={50} text={buttonName(layout, "rt")} className={part("rt")} />
      <Label x={312} y={71} text={buttonName(layout, "lb")} size={9} className={part("lb")} />
      <Label x={588} y={71} text={buttonName(layout, "rb")} size={9} className={part("rb")} />

      {/* Sticks and D-pad sit in wells. */}
      <Stick x={330} y={140} className={part("ls")} />
      <Stick x={505} y={215} className={part("rs")} />
      <g className={part("dpad")}>
        <circle className="fill-surface stroke-border" cx={395} cy={215} r={31} />
        <path
          className="fill-surface-raised stroke-muted"
          strokeLinejoin="round"
          d="M386 192 h18 v14 h14 v18 h-14 v14 h-18 v-14 h-14 v-18 h14 Z"
        />
      </g>

      {/* The guide button between them, plain. */}
      <circle className="fill-surface stroke-border" cx={450} cy={104} r={12} />
      {ps ? (
        <>
          {/* Share and Options: slim upright buttons. */}
          <rect className={`${cap} ${part("view")}`} x={408} y={126} width={8} height={16} rx={4} />
          <rect className={`${cap} ${part("menu")}`} x={484} y={126} width={8} height={16} rx={4} />
        </>
      ) : (
        <>
          <g className={part("view")}>
            <circle className={cap} cx={412} cy={134} r={9} />
            <rect
              className="fill-none stroke-muted"
              strokeWidth={1.2}
              x={408}
              y={131}
              width={8}
              height={6}
              rx={1}
            />
          </g>
          <g className={part("menu")}>
            <circle className={cap} cx={488} cy={134} r={9} />
            <path
              className="stroke-muted"
              strokeWidth={1.2}
              d="M484 131.5 h8 M484 134 h8 M484 136.5 h8"
            />
          </g>
        </>
      )}
      {face.map((f) =>
        ps ? (
          <g key={f.input} className={part(f.input)} transform={`translate(${f.x} ${f.y})`}>
            <circle className="fill-pad-letter stroke-pad-letter/30" r={13} />
            <PsSymbol button={f.input} r={6} />
          </g>
        ) : (
          <g key={f.input} className={part(f.input)}>
            <circle className={`${f.fill} stroke-pad-letter/30`} cx={f.x} cy={f.y} r={13} />
            <text
              x={f.x}
              y={f.y}
              textAnchor="middle"
              dominantBaseline="central"
              className="fill-pad-letter"
              fontSize={12}
              fontWeight={700}
            >
              {f.input.toUpperCase()}
            </text>
          </g>
        ),
      )}

      {callouts.map((c) => {
        const a = anchors[c.input];
        const left = a.side === "left";
        const points = a.line.split(" ");
        const end = points[points.length - 1].split(",").map(Number);
        return (
          <g key={c.input}>
            <polyline
              points={a.line}
              className="fill-none stroke-muted"
              strokeWidth={1}
              strokeLinejoin="round"
            />
            <circle className="fill-muted" cx={end[0]} cy={end[1]} r={2.5} />
            <text
              x={left ? 225 : 675}
              y={a.y}
              textAnchor={left ? "end" : "start"}
              dominantBaseline="middle"
              className="fill-ink"
              fontSize={13}
            >
              {c.lines.map((line, i) => (
                <tspan
                  key={i}
                  x={left ? 225 : 675}
                  dy={i === 0 ? 0 : 16}
                  className={i === 0 ? "" : "fill-muted"}
                  fontSize={i === 0 ? 13 : 12}
                >
                  {line}
                </tspan>
              ))}
            </text>
          </g>
        );
      })}
    </svg>
  );
}

/** A thumbstick in its well: a cap with a dished top. */
function Stick({ x, y, className }: { x: number; y: number; className: string }) {
  return (
    <g className={className}>
      <circle className="fill-surface stroke-border" cx={x} cy={y} r={33} />
      <circle className="fill-surface-raised stroke-muted" cx={x} cy={y} r={23} />
      <circle className="fill-none stroke-border" cx={x} cy={y} r={15} />
    </g>
  );
}

function Label({
  x,
  y,
  text,
  size = 10,
  className,
}: {
  x: number;
  y: number;
  text: string;
  size?: number;
  className: string;
}) {
  return (
    <text
      x={x}
      y={y}
      textAnchor="middle"
      dominantBaseline="central"
      className={`fill-muted ${className}`}
      fontSize={size}
      fontWeight={600}
    >
      {text}
    </text>
  );
}
