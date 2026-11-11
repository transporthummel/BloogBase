import { useEffect, useRef } from "react";
import { activate, back, move } from "./actions";
import { keyboardAction } from "./keyboard/store";
import { padReader, type PadState } from "./pad";

/** Pixels per second at full deflection of the right stick. */
const SCROLL_SPEED = 1500;

/** The first pad with the standard layout; others we can't read reliably. */
export function standardPad(): PadState | null {
  try {
    return navigator.getGamepads?.().find((p) => p?.connected && p.mapping === "standard") ?? null;
  } catch {
    return null;
  }
}

function canScroll(el: Element) {
  const { overflowY } = getComputedStyle(el);
  return (overflowY === "auto" || overflowY === "scroll") && el.scrollHeight > el.clientHeight;
}

/** The scroll container around the focus, else the page's. */
function scroller(): Element | null {
  for (let el = document.activeElement; el && el !== document.body; el = el.parentElement) {
    if (canScroll(el)) return el;
  }
  return document.querySelector("main");
}

/**
 * Drives the app with a controller (plans/app-controller.md): reads the pad
 * once a frame while one is connected and the window has the focus, so
 * input meant for DOSBox never arrives here. `goBack` is B with nothing
 * open to close.
 */
export function usePad(goBack: () => void) {
  const back$ = useRef(goBack);
  useEffect(() => {
    back$.current = goBack;
  });

  useEffect(() => {
    if (typeof navigator.getGamepads !== "function") return;
    const read = padReader();
    let frame = 0;
    let last = 0;

    const loop = (now: number) => {
      const pad = document.hasFocus() ? standardPad() : null;
      const { actions, scroll } = read(pad, now);
      for (const a of actions) {
        // The on-screen keyboard, while open, takes every button.
        if (keyboardAction(a)) continue;
        if (a === "activate") activate();
        else if (a === "back") back(() => back$.current());
        else if (a === "up" || a === "down" || a === "left" || a === "right") move(a);
        // The other buttons only mean something on the keyboard so far.
      }
      const dt = Math.min(now - last, 50) / 1000;
      last = now;
      if (scroll.x || scroll.y) {
        scroller()?.scrollBy({
          left: scroll.x * SCROLL_SPEED * dt,
          top: scroll.y * SCROLL_SPEED * dt,
          behavior: "instant",
        });
      }
      frame = requestAnimationFrame(loop);
    };
    const start = () => {
      if (frame || !standardPad()) return;
      last = performance.now();
      frame = requestAnimationFrame(loop);
    };
    const stop = () => {
      if (standardPad()) return;
      cancelAnimationFrame(frame);
      frame = 0;
      read(null, 0);
    };
    // The mouse takes over: the focus ring goes back to the browser's rules.
    const mouse = () => delete document.documentElement.dataset.input;

    window.addEventListener("gamepadconnected", start);
    window.addEventListener("gamepaddisconnected", stop);
    document.addEventListener("pointerdown", mouse, true);
    start();
    return () => {
      window.removeEventListener("gamepadconnected", start);
      window.removeEventListener("gamepaddisconnected", stop);
      document.removeEventListener("pointerdown", mouse, true);
      cancelAnimationFrame(frame);
    };
  }, []);
}
