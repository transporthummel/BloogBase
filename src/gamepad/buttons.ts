import type { Controller } from "../bindings";
import { padId } from "./usePad";

/** The buttons named by text, not drawn. */
export type NamedButton = "lb" | "rb" | "lt" | "rt" | "view" | "menu" | "l3";

/** What's printed on them (or next to them). */
const NAMES: Record<Controller, Record<NamedButton, string>> = {
  xbox: { lb: "LB", rb: "RB", lt: "LT", rt: "RT", view: "View", menu: "Menu", l3: "L3" },
  playstation: {
    lb: "L1",
    rb: "R1",
    lt: "L2",
    rt: "R2",
    view: "Share",
    menu: "Options",
    l3: "L3",
  },
};

export function buttonName(layout: Controller, button: NamedButton): string {
  return NAMES[layout][button];
}

/** A pad's layout from its Gamepad API id, which names the USB vendor (Sony: 054c). */
export function layoutOf(id: string): Controller {
  return /\b054c\b/i.test(id) ? "playstation" : "xbox";
}

/** The layout of the pad the Gamepad API shows, an Xbox controller's if none. */
export function padLayout(): Controller {
  const id = padId();
  return id === null ? "xbox" : layoutOf(id);
}

/** A face button, by its place on an Xbox controller. */
export type FaceButton = "a" | "b" | "x" | "y";
