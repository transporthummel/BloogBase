import { useLayoutEffect, useState, type RefObject } from "react";

const REM = 16;
/** The picture never gets narrower than this... */
const MIN_PICTURE = 20 * REM;
/** ...and leaves the text at least this much. */
const MIN_TEXT = 24 * REM;
/** Side by side from Tailwind's `md` up; stacked below. */
const SIDE_BY_SIDE = "(min-width: 48rem)";

/**
 * The width of the detail page picture beside the text: as wide as a 4:3
 * picture of the text's height, so it fills the card's height without bars,
 * within the limits above. `undefined` when stacked (and in tests without
 * layout).
 */
export function usePictureWidth(
  card: RefObject<HTMLElement | null>,
  text: RefObject<HTMLElement | null>,
): number | undefined {
  const [width, setWidth] = useState<number>();

  useLayoutEffect(() => {
    const c = card.current;
    const tx = text.current;
    if (!c || !tx || typeof ResizeObserver === "undefined" || !window.matchMedia) return;
    const media = window.matchMedia(SIDE_BY_SIDE);
    const fit = () => {
      if (!media.matches) return setWidth(undefined);
      const wanted = (tx.offsetHeight * 4) / 3;
      const next = Math.max(MIN_PICTURE, Math.min(wanted, c.clientWidth - MIN_TEXT));
      // A wider picture makes the text taller, and so on; small steps end it.
      setWidth((prev) => (prev !== undefined && Math.abs(prev - next) < 2 ? prev : next));
    };
    const observer = new ResizeObserver(fit);
    observer.observe(c);
    observer.observe(tx);
    media.addEventListener("change", fit);
    fit();
    return () => {
      observer.disconnect();
      media.removeEventListener("change", fit);
    };
  }, [card, text]);

  return width;
}
