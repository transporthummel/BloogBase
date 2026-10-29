import { useEffect } from "react";

/** Names the page in the document title, for screen readers and window lists. */
export function usePageTitle(title: string) {
  useEffect(() => {
    // "About BloogBase" names it already.
    document.title = title.includes("BloogBase") ? title : `${title} – BloogBase`;
    return () => {
      document.title = "BloogBase";
    };
  }, [title]);
}
