import { openUrl } from "@tauri-apps/plugin-opener";
import { useTranslation } from "react-i18next";
import { gameTitle, useGames } from "../catalog";
import { STORE_LINKS } from "../storeLinks";
import { Badge } from "../ui/Badge";
import { Notice } from "../ui/Notice";
import { key, type useStoreScan } from "./storeScan";

/** The games found, each with a checkbox; already imported ones say so. */
export function StoreScanList({ store }: { store: ReturnType<typeof useStoreScan> }) {
  const { t } = useTranslation();
  const games = useGames();
  const { scan, selected, toggle } = store;
  return (
    <>
      {scan.isPending && <Notice>{t("setup.owned.scanning")}</Notice>}
      {scan.data?.length === 0 && <p className="text-sm">{t("setup.owned.noneFound")}</p>}
      <ul className="grid gap-3">
        {scan.data?.map((install) => (
          <li key={install.path} className="grid gap-2 rounded-lg border border-border p-3">
            <p className="flex flex-wrap items-center gap-2 text-sm">
              <Badge>{t(`setup.owned.store.${install.store}`)}</Badge>
              <span className="text-xs break-all text-muted select-text">{install.path}</span>
            </p>
            {install.games.map((g) => (
              <label key={g.gameId} className="flex items-center gap-3 text-sm">
                <input
                  type="checkbox"
                  className="size-4 accent-[var(--bb-accent)]"
                  checked={g.imported || selected.has(key(install, g.gameId))}
                  disabled={g.imported}
                  onChange={() => toggle(key(install, g.gameId))}
                />
                <span>{gameTitle(games.data, g.gameId)}</span>
                {g.imported && (
                  <span className="ml-auto text-xs text-muted">{t("setup.owned.imported")}</span>
                )}
              </label>
            ))}
          </li>
        ))}
      </ul>
    </>
  );
}

/** "Where can I buy Commander Keen?" with the store links. */
export function StoreLinks() {
  const { t } = useTranslation();
  return (
    <div className="grid gap-1 text-sm">
      <p className="text-muted">{t("setup.owned.buy")}</p>
      <p className="flex flex-wrap gap-4">
        {(["gog", "steam"] as const).map((store) => (
          <button
            key={store}
            type="button"
            className="cursor-pointer text-ink underline underline-offset-2"
            onClick={() => openUrl(STORE_LINKS[store])}
          >
            {t(`setup.owned.store.${store}`)}
          </button>
        ))}
      </p>
    </div>
  );
}
