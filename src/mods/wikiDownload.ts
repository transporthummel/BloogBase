import type { KeenwikiDownload, KeenwikiOrigin, ModInfo, WikiDownload, WikiMod } from "../bindings";

export type Downloaded = { path: string; info: ModInfo; origin: KeenwikiOrigin };

/** A page's download, ready to confirm; the page's details win over the guesses. */
export function downloaded(
  page: WikiMod,
  chosen: WikiDownload,
  { path, info }: KeenwikiDownload,
): Downloaded {
  return {
    path,
    info: { ...info, name: page.name },
    origin: {
      pageUrl: page.pageUrl,
      downloadUrl: chosen.url,
      gameId: page.gameId,
      authors: page.authors,
      authorLinks: page.authorLinks,
      // Without one the archive name is used.
      version: chosen.version,
      description: page.description,
      released: page.released,
      homepage: page.homepage,
    },
  };
}
