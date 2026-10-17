import { useContext, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { PageFooterContext } from "./pageFooterContext";

/**
 * Puts `children` into the shell's footer, which spans the whole content
 * area and sticks to its bottom. Outside the shell (e.g. in tests) they
 * render in place.
 */
export function PageFooter({ children }: { children: ReactNode }) {
  const slot = useContext(PageFooterContext);
  return slot ? createPortal(children, slot) : children;
}
