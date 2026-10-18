import { openUrl } from "@tauri-apps/plugin-opener";
import { Fragment } from "react";
import { useTranslation } from "react-i18next";

/**
 * "by A, B", each name linking to the author's page if known. Links open in
 * the browser, never in the app's window.
 */
export function Authors({
  authors,
  links = {},
}: {
  authors: string[];
  links?: { [name in string]?: string };
}) {
  const { t } = useTranslation();
  const [before, after] = t("mods.by", { authors: "\u0000" }).split("\u0000");
  return (
    <>
      {before}
      {authors.map((name, i) => {
        const url = links[name];
        return (
          <Fragment key={name}>
            {i > 0 && ", "}
            {url && /^https?:\/\//.test(url) ? (
              <button
                type="button"
                onClick={() => openUrl(url)}
                className="underline underline-offset-2 hover:text-accent"
              >
                {name}
              </button>
            ) : (
              name
            )}
          </Fragment>
        );
      })}
      {after}
    </>
  );
}
