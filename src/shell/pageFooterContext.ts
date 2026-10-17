import { createContext } from "react";

/** The shell's full-width footer slot, below the page's width-limited content. */
export const PageFooterContext = createContext<HTMLElement | null>(null);
