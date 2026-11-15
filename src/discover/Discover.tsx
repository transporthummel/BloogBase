import { useSearch } from "@tanstack/react-router";
import { RefreshCw, Search } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { type Game, type ModKind } from "../bindings";
import { useGames } from "../catalog";
import { errorMessage } from "../errors";
import { useInstalls } from "../library/queries";
import { Button } from "../ui/Button";
import { EmptyState } from "../ui/EmptyState";
import { Notice } from "../ui/Notice";
import { PageHeader } from "../ui/PageHeader";
import { RelativeTime } from "../ui/RelativeTime";
import { Tabs } from "../ui/Tabs";
import { entryDescription } from "./description";
import { compare, listedUnder, needsNoGame, playable as isPlayable, SORTS, type Sort } from "./fit";
import { Choice, GameChoice, Toggle } from "./Filters";
import { FreeGames } from "./FreeGames";
import { IndexError } from "./IndexError";
import { ModCard } from "./ModCard";
import { TABS, type DiscoverTab } from "./tabs";
import { useFreeGames, useIndexMods, useIndexStatus, useRefreshIndex } from "./queries";

const KINDS: ModKind[] = ["episode", "levelpack", "conversion"];

const field = "rounded-lg border border-border bg-surface-raised px-3 py-2 text-sm";

const NO_FILTERS = {
  query: "",
  game: "",
  kind: "" as ModKind | "",
  playable: false,
  notInstalled: false,
};

/** The mods tab's filters and sort, kept across pages until the app closes. */
let last = { ...NO_FILTERS, sort: "featured" as Sort };
/** The sort the open page was linked with, so going back to the tab doesn't count as the link. */
let lastLink: Sort | undefined;

/** Mods from the curated index and freely available game versions (PLAN.md §8.3). */
export function Discover() {
  const { t } = useTranslation();
  const games = useGames().data ?? [];
  const refresh = useRefreshIndex();
  const search: { tab?: DiscoverTab; sort?: Sort } = useSearch({ strict: false });
  const [tab, setTab] = useState<DiscoverTab>(search.tab ?? "mods");
  // A link to the page while it's open, e.g. from the news dialog, selects too.
  const [linked, setLinked] = useState(search.tab);
  if (linked !== search.tab) {
    setLinked(search.tab);
    if (search.tab) setTab(search.tab);
  }
  useEffect(
    () => () => {
      lastLink = undefined;
    },
    [],
  );

  return (
    <>
      <PageHeader
        title={t("discover.title")}
        actions={
          tab === "mods" && (
            <Button
              variant="secondary"
              onClick={() => refresh.mutate()}
              disabled={refresh.isPending}
            >
              <RefreshCw
                className={`size-4 ${refresh.isPending ? "animate-spin" : ""}`}
                aria-hidden
              />
              {t("discover.refresh")}
            </Button>
          )
        }
      />
      <Tabs
        tabs={TABS.map((id) => ({ id, label: t(`discover.tabs.${id}`) }))}
        selected={tab}
        onSelect={setTab}
        label={t("discover.tabs.label")}
      >
        {tab === "mods" ? (
          <Mods games={games} sort={search.sort} />
        ) : (
          <FreeGames category={tab} games={games} />
        )}
      </Tabs>
    </>
  );
}

function Mods({ games, sort: linkedSort }: { games: Game[]; sort?: Sort }) {
  const { t, i18n } = useTranslation();
  const status = useIndexStatus();
  const mods = useIndexMods();
  const installs = useInstalls();
  const freeGames = useFreeGames();

  // A link, e.g. to the news dialog's new mods, shows what it promises:
  // remembered filters could hide them.
  const [start] = useState(() =>
    linkedSort && linkedSort !== lastLink ? { ...NO_FILTERS, sort: linkedSort } : last,
  );
  const [query, setQuery] = useState(start.query);
  const [game, setGame] = useState(start.game);
  const [kind, setKind] = useState(start.kind);
  const [playable, setPlayable] = useState(start.playable);
  const [notInstalled, setNotInstalled] = useState(start.notInstalled);
  // The mods installed when the filter was turned on or the page opened: one
  // installed from its card meanwhile stays, so it doesn't vanish under the pointer.
  const [hidden, setHidden] = useState<Set<string> | null>(null);
  const [sort, setSort] = useState(start.sort);
  const [linked, setLinked] = useState(linkedSort);
  function clearFilters() {
    setQuery(NO_FILTERS.query);
    setGame(NO_FILTERS.game);
    setKind(NO_FILTERS.kind);
    setPlayable(NO_FILTERS.playable);
    setNotInstalled(NO_FILTERS.notInstalled);
  }
  if (linked !== linkedSort) {
    setLinked(linkedSort);
    if (linkedSort) {
      setSort(linkedSort);
      clearFilters();
    }
  }
  useEffect(() => {
    last = { query, game, kind, playable, notInstalled, sort };
    lastLink = linkedSort;
  });

  const owned = installs.data ?? [];
  const all = mods.data ?? [];
  const installedNow = () => new Set(all.filter((m) => m.modId).map((m) => m.entry.id));
  if (notInstalled && !hidden && mods.data) setHidden(installedNow());
  function toggleNotInstalled(on: boolean) {
    setNotInstalled(on);
    setHidden(on ? installedNow() : null);
  }
  const filtered = !!(query.trim() || game || kind || playable || notInstalled);
  const gamesInIndex = games.filter((g) => all.some((m) => listedUnder(m.entry, g.id)));
  const needle = query.trim().toLowerCase();
  const shown = all
    .filter(({ entry: e }) => {
      if (game && !listedUnder(e, game)) return false;
      if (kind && e.type !== kind) return false;
      // A mod made for a free version is playable: it comes along.
      if (playable && !isPlayable(e, owned, games, freeGames.data)) return false;
      if (notInstalled && hidden?.has(e.id)) return false;
      if (!needle) return true;
      const description = entryDescription(e, i18n.language) ?? "";
      return [e.name, description, ...e.authors, ...(e.tags ?? [])].some((s) =>
        s.toLowerCase().includes(needle),
      );
    })
    .sort((a, b) => compare(sort)(a.entry, b.entry));

  const st = status.data;
  return (
    <>
      <div className="mb-4 grid gap-2">
        {st && (
          <div className="flex min-h-7 flex-wrap items-center justify-between gap-x-3">
            <p className="text-xs text-muted">
              {st.checkedAt ? (
                <RelativeTime at={st.checkedAt}>
                  {(date) =>
                    // Filtered: how many of them are shown.
                    shown.length < all.length
                      ? t("discover.status", {
                          context: "filtered",
                          shown: shown.length,
                          count: all.length,
                          date,
                        })
                      : t("discover.status", { count: st.modCount, date })
                  }
                </RelativeTime>
              ) : (
                t("discover.neverChecked")
              )}
            </p>
            {filtered && (
              <button
                type="button"
                className="rounded-md px-2 py-1 text-xs font-semibold text-muted hover:bg-surface-raised hover:text-ink"
                onClick={clearFilters}
              >
                {t("discover.clearFilters")}
              </button>
            )}
          </div>
        )}
        {st && st.hiddenCount > 0 && (
          <Notice>{t("discover.hidden", { count: st.hiddenCount })}</Notice>
        )}
        {st?.error ? (
          <IndexError error={st.error} cached={st.modCount > 0} />
        ) : (
          mods.error && <Notice tone="error">{errorMessage(t, mods.error)}</Notice>
        )}
      </div>

      {all.length > 0 && (
        <div className="mb-6 flex flex-wrap items-center gap-3">
          <label className="relative min-w-48 flex-1">
            <span className="sr-only">{t("discover.search")}</span>
            <Search
              className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted"
              aria-hidden
            />
            <input
              type="search"
              autoComplete="off"
              className={`${field} w-full pl-9`}
              placeholder={t("discover.search")}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </label>
          <label className="flex items-center gap-2 text-sm">
            <span className="text-muted">{t("discover.sort")}</span>
            <select
              className={field}
              value={sort}
              onChange={(e) => setSort(e.target.value as Sort)}
            >
              {SORTS.map((s) => (
                <option key={s} value={s}>
                  {t(`discover.sortBy.${s}`)}
                </option>
              ))}
            </select>
          </label>
          <div className="flex w-full flex-wrap items-center gap-x-6 gap-y-3">
            <GameChoice
              games={games}
              ids={gamesInIndex.map((g) => g.id)}
              noGame={all.some((m) => needsNoGame(m.entry))}
              value={game}
              onChange={setGame}
            />
            <Choice
              label={t("discover.type")}
              options={KINDS.map((k) => ({ value: k, label: t(`discover.kind.${k}`) }))}
              value={kind}
              onChange={setKind}
            />
            <div className="flex flex-wrap gap-1.5">
              <Toggle on={playable} onChange={setPlayable}>
                {t("discover.playable")}
              </Toggle>
              <Toggle on={notInstalled} onChange={toggleNotInstalled}>
                {t("discover.notInstalled")}
              </Toggle>
            </div>
          </div>
        </div>
      )}

      {/* After a failed check the error above says it already. */}
      {mods.isSuccess && all.length === 0 && !st?.error && (
        <EmptyState title={t("discover.emptyTitle")}>
          <p className="max-w-md text-sm text-muted">{t("discover.empty")}</p>
        </EmptyState>
      )}
      {all.length > 0 && shown.length === 0 && (
        // "Clear filters" is next to them.
        <Notice>{t("discover.noMatch")}</Notice>
      )}

      <ul className="grid grid-cols-[repeat(auto-fill,minmax(16rem,1fr))] gap-4">
        {shown.map((mod) => (
          <li key={mod.entry.id}>
            <ModCard mod={mod} installs={owned} games={games} />
          </li>
        ))}
      </ul>
    </>
  );
}
