import { useLayoutEffect, useState, type RefObject } from "react";

const REM = 16;
/** The picture never gets narrower than this... */
const MIN_PICTURE = 20 * REM;
/** ...nor wider than this (480px tall at 4:3)... */
const MAX_PICTURE = 40 * REM;
/** ...takes at least this share of a wide card, so spare room goes to it... */
const CARD_SHARE = 0.4;
/** ...and leaves the text at least this much. */
const MIN_TEXT = 24 * REM;
/** Side by side from Tailwind's `md` up; stacked below. */
const SIDE_BY_SIDE = "(min-width: 48rem)";

/**
 * The width of the detail page picture beside the text: as wide as a 4:3
 * picture of the text's height, so it fills the card's height without bars,
 * within the limits above. `undefined` when stacked (and in tests without
 * layout).
 *
 * The text is a flex column the picture may stretch; its height is taken
 * from its children, so the picture can shrink again when the text does.
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
      const wanted = Math.max((naturalHeight(tx) * 4) / 3, c.clientWidth * CARD_SHARE);
      const next = Math.max(MIN_PICTURE, Math.min(wanted, MAX_PICTURE, c.clientWidth - MIN_TEXT));
      // A wider picture makes the text taller, and so on; small steps end it.
      setWidth((prev) => (prev !== undefined && Math.abs(prev - next) < 2 ? prev : next));
    };
    const observer = new ResizeObserver(fit);
    observer.observe(c);
    observer.observe(tx);
    for (const child of tx.children) observer.observe(child);
    media.addEventListener("change", fit);
    fit();
    return () => {
      observer.disconnect();
      media.removeEventListener("change", fit);
    };
  }, [card, text]);

  return width;
}

/** A column's height without stretching: its children, gaps and padding. */
function naturalHeight(column: HTMLElement): number {
  const style = getComputedStyle(column);
  const children = [...column.children] as HTMLElement[];
  const gap = parseFloat(style.rowGap) || 0;
  return (
    children.reduce((sum, child) => sum + child.offsetHeight, 0) +
    gap * Math.max(0, children.length - 1) +
    parseFloat(style.paddingTop) +
    parseFloat(style.paddingBottom)
  );
}
