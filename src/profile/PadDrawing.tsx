import type { PadInput } from "./controlSheet";

/** One input's callout: its label's lines, beside the drawing. */
export interface Callout {
  input: PadInput;
  lines: string[];
}

/**
 * Where each callout sits: its side, the height of its first line, and the
 * leader line's points from the label to the input. Laid out so no line
 * crosses another input.
 */
const anchors: Record<PadInput, { side: "left" | "right"; y: number; line: string }> = {
  lt: { side: "left", y: 38, line: "233,38 312,39" },
  lb: { side: "left", y: 64, line: "233,64 305,62" },
  view: { side: "left", y: 96, line: "233,96 380,96 409,132" },
  ls: { side: "left", y: 140, line: "233,140 307,140" },
  dpad: { side: "left", y: 215, line: "233,215 371,215" },
  rt: { side: "right", y: 38, line: "667,38 588,39" },
  rb: { side: "right", y: 64, line: "667,64 595,62" },
  menu: { side: "right", y: 90, line: "667,90 520,90 491,132" },
  y: { side: "right", y: 114, line: "667,114 578,112" },
  b: { side: "right", y: 140, line: "667,140 606,140" },
  a: { side: "right", y: 166, line: "667,166 578,168" },
  x: { side: "right", y: 192, line: "667,192 537,192 537,153" },
  rs: { side: "right", y: 218, line: "667,218 533,215" },
};

const face: { input: PadInput; x: number; y: number; letter: string }[] = [
  { input: "y", x: 565, y: 112, letter: "Y" },
  { input: "x", x: 537, y: 140, letter: "X" },
  { input: "b", x: 593, y: 140, letter: "B" },
  { input: "a", x: 565, y: 168, letter: "A" },
];

/**
 * A generic controller in the Xbox layout (an own drawing, no maker's
 * artwork), with callouts to what each input does. Inputs without one are
 * drawn faintly. Decorative for screen readers: the callouts are listed
 * beside it for them.
 */
export function PadDrawing({ callouts }: { callouts: Callout[] }) {
  const used = new Set(callouts.map((c) => c.input));
  const part = (input: PadInput) =>
    `transition-opacity ${used.has(input) ? "" : "opacity-35"} [&_.cap]:fill-surface [&_.cap]:stroke-muted`;
  return (
    <svg
      viewBox="0 0 900 350"
      className="h-auto w-full text-ink"
      aria-hidden
      focusable="false"
      fontFamily="inherit"
    >
      {/* Triggers and bumpers, behind the body. */}
      <g className={part("lt")}>
        <rect className="cap" x={312} y={26} width={56} height={30} rx={10} />
        <PadLabel x={340} y={39} text="LT" />
      </g>
      <g className={part("rt")}>
        <rect className="cap" x={532} y={26} width={56} height={30} rx={10} />
        <PadLabel x={560} y={39} text="RT" />
      </g>
      <g className={part("lb")}>
        <rect className="cap" x={305} y={54} width={82} height={16} rx={7} />
        <PadLabel x={346} y={62} text="LB" />
      </g>
      <g className={part("rb")}>
        <rect className="cap" x={513} y={54} width={82} height={16} rx={7} />
        <PadLabel x={554} y={62} text="RB" />
      </g>

      <path
        className="fill-surface-raised stroke-border"
        strokeWidth={1.5}
        d="M350 72 C390 62 510 62 550 72 C610 84 640 150 650 230 C660 300 640 340 605 340 C575 340 555 310 535 285 C520 268 380 268 365 285 C345 310 325 340 295 340 C260 340 240 300 250 230 C260 150 290 84 350 72 Z"
      />

      <g className={part("ls")}>
        <circle className="cap" cx={335} cy={140} r={28} />
        <circle className="fill-surface-raised stroke-border" cx={335} cy={140} r={17} />
      </g>
      <g className={part("rs")}>
        <circle className="cap" cx={505} cy={215} r={28} />
        <circle className="fill-surface-raised stroke-border" cx={505} cy={215} r={17} />
      </g>
      <g className={part("dpad")}>
        <path
          className="cap"
          d="M387 191 h16 v16 h16 v16 h-16 v16 h-16 v-16 h-16 v-16 h16 Z"
          strokeLinejoin="round"
        />
      </g>
      <g className={part("view")}>
        <circle className="cap" cx={415} cy={140} r={9} />
        <rect className="fill-muted" x={411} y={137} width={8} height={6} rx={1} />
      </g>
      <g className={part("menu")}>
        <circle className="cap" cx={485} cy={140} r={9} />
        <path
          className="stroke-muted"
          strokeWidth={1.5}
          d="M480 137 h10 M480 140 h10 M480 143 h10"
        />
      </g>
      {face.map((f) => (
        <g key={f.input} className={part(f.input)}>
          <circle className="cap" cx={f.x} cy={f.y} r={13} />
          <PadLabel x={f.x} y={f.y} text={f.letter} />
        </g>
      ))}

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

function PadLabel({ x, y, text }: { x: number; y: number; text: string }) {
  return (
    <text
      x={x}
      y={y}
      textAnchor="middle"
      dominantBaseline="central"
      className="fill-ink"
      fontSize={10}
      fontWeight={600}
    >
      {text}
    </text>
  );
}
