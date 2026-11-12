import { X } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Controller } from "../bindings";
import { connectedController } from "../profile/controller";
import { PsSymbol } from "./FaceButton";
import { standardPad } from "./usePad";

/** How often to ask whether a controller is there that the webview hasn't shown yet. */
const EVERY = 2000;

/** Closed by the player: not again until the app starts anew. */
let dismissed = false;

/**
 * The webview shows a controller only after a button was pressed in the
 * window. Windows (XInput) knows earlier: until then, this asks for a press.
 * Elsewhere `connectedController` says none, and nothing shows.
 */
export function PadHint() {
  const { t } = useTranslation();
  const [seen, setSeen] = useState(() => standardPad() !== null);
  const [connected, setConnected] = useState<Controller | null>(null);
  const [closed, setClosed] = useState(dismissed);

  useEffect(() => {
    const update = () => setSeen(standardPad() !== null);
    window.addEventListener("gamepadconnected", update);
    window.addEventListener("gamepaddisconnected", update);
    return () => {
      window.removeEventListener("gamepadconnected", update);
      window.removeEventListener("gamepaddisconnected", update);
    };
  }, []);

  useEffect(() => {
    if (seen || typeof navigator.getGamepads !== "function") return;
    let gone = false;
    const ask = async () => {
      // While DOSBox is in front, the controller is for the game.
      if (!document.hasFocus()) return;
      const pad = await connectedController();
      if (!gone) setConnected(pad);
    };
    void ask();
    const timer = setInterval(() => void ask(), EVERY);
    return () => {
      gone = true;
      clearInterval(timer);
    };
  }, [seen]);

  const show = !seen && connected !== null && !closed;
  // Kept while leaving, when `connected` may be gone already.
  const [layout, setLayout] = useState<Controller>("xbox");
  if (connected !== null && connected !== layout) setLayout(connected);
  // Kept a moment after it should go, to leave as softly as it came.
  const [shown, setShown] = useState(false);
  if (show && !shown) setShown(true);
  if (!shown) return null;
  const leaving = !show;

  // A green A (a blue Cross) on the controller that keeps getting pressed,
  // with a ring going out from it: it says "press" before the text is read.
  const ps = layout === "playstation";
  return (
    <div
      role={leaving ? undefined : "status"}
      aria-hidden={leaving || undefined}
      onAnimationEnd={(e) => {
        if (leaving && e.target === e.currentTarget) setShown(false);
      }}
      className={`${leaving ? "bb-hint-out" : "bb-hint-in"} fixed bottom-16 left-1/2 z-40 flex w-max max-w-[calc(100%-2rem)] -translate-x-1/2 items-center gap-2.5 rounded-full border border-border bg-surface py-1.5 pr-1.5 pl-2 text-sm shadow-card`}
    >
      <span aria-hidden className="relative flex size-5 shrink-0">
        <span
          className={`bb-pad-ping absolute inset-0 rounded-full ${ps ? "bg-pad-x" : "bg-pad-a"}`}
        />
        {ps ? (
          <svg className="bb-pad-press relative size-5" viewBox="-10 -10 20 20">
            <circle className="fill-pad-letter" r={10} />
            <PsSymbol button="a" r={4} />
          </svg>
        ) : (
          <span className="bb-pad-press relative flex size-5 items-center justify-center rounded-full bg-pad-a text-[0.7rem] font-bold text-pad-letter">
            A
          </span>
        )}
      </span>
      {t("pad.pressButton")}
      <button
        type="button"
        aria-label={t("pad.dismiss")}
        title={t("pad.dismiss")}
        className="flex size-7 shrink-0 items-center justify-center rounded-full text-muted transition hover:bg-surface-raised hover:text-ink"
        onClick={() => {
          dismissed = true;
          setClosed(true);
        }}
      >
        <X className="size-4" aria-hidden />
      </button>
    </div>
  );
}
