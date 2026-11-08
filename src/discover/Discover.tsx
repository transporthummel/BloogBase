import { useSearch } from "@tanstack/react-router";
import { RefreshCw, Search } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { type Game, type ModKind } from "../bindings";
import { gameTitle, useGames } from "../catalog";
import { errorMessage } from "../errors";
import { useInstalls } from "../library/queries";
import { Button } from "../ui/Button";
import { EmptyState } from "../ui/EmptyState";
import { Notice } from "../ui/Notice";
import { PageHeader } from "../ui/PageHeader";
import { RelativeTime } from "../ui/RelativeTime";
import { Tabs } from "../ui/Tabs";
import { entryDescription } from "./description";
import {
  compare,
  listedUnder,
  needsNoGame,
  NO_GAME,
  playable as isPlayable,
  SORTS,
  type Sort,
} from "./fit";
import { FreeGames } from "./FreeGames";
import { IndexError } from "./IndexError";
import { ModCard } from "./ModCard";
import { TABS, type DiscoverTab } from "./tabs";
import { useIndexMods, useIndexStatus, useRefreshIndex } from "./queries";

const KINDS: ModKind[] = ["episode", "levelpack", "conversion"];

const field = "rounded-lg border border-border bg-surface-raised px-3 py-2 text-sm";

const NO_FILTERS = { query: "", game: "", kind: "" as ModKind | "", playable: false };

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

  // A link, e.g. to the news dialog's new mods, shows what it promises:
  // remembered filters could hide them.
  const [start] = useState(() =>
    linkedSort && linkedSort !== lastLink ? { ...NO_FILTERS, sort: linkedSort } : last,
  );
  const [query, setQuery] = useState(start.query);
  const [game, setGame] = useState(start.game);
  const [kind, setKind] = useState(start.kind);
  const [playable, setPlayable] = useState(start.playable);
  const [sort, setSort] = useState(start.sort);
  const [linked, setLinked] = useState(linkedSort);
  function clearFilters() {
    setQuery(NO_FILTERS.query);
    setGame(NO_FILTERS.game);
    setKind(NO_FILTERS.kind);
    setPlayable(NO_FILTERS.playable);
  }
  if (linked !== linkedSort) {
    setLinked(linkedSort);
    if (linkedSort) {
      setSort(linkedSort);
      clearFilters();
    }
  }
  useEffect(() => {
    last = { query, game, kind, playable, sort };
    lastLink = linkedSort;
  });

  const owned = installs.data ?? [];
  const all = mods.data ?? [];
  const gamesInIndex = games.filter((g) => all.some((m) => listedUnder(m.entry, g.id)));
  const needle = query.trim().toLowerCase();
  const shown = all
    .filter(({ entry: e }) => {
      if (game && !listedUnder(e, game)) return false;
      if (kind && e.type !== kind) return false;
      if (playable && !isPlayable(e, owned, games)) return false;
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
          <p className="text-xs text-muted">
            {st.checkedAt ? (
              <RelativeTime at={st.checkedAt}>
                {(date) => t("discover.status", { count: st.modCount, date })}
              </RelativeTime>
            ) : (
              t("discover.neverChecked")
            )}
          </p>
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
        <div className="mb-6 flex flex-wrap items-end gap-3">
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
          <label className="grid gap-1 text-xs">
            <span className="text-muted">{t("discover.game")}</span>
            <select className={field} value={game} onChange={(e) => setGame(e.target.value)}>
              <option value="">{t("discover.allGames")}</option>
              {gamesInIndex.map((g) => (
                <option key={g.id} value={g.id}>
                  {gameTitle(games, g.id)}
                </option>
              ))}
              {all.some((m) => needsNoGame(m.entry)) && (
                <option value={NO_GAME}>{t("discover.noGame")}</option>
              )}
            </select>
          </label>
          <label className="grid gap-1 text-xs">
            <span className="text-muted">{t("discover.type")}</span>
            <select
              className={field}
              value={kind}
              onChange={(e) => setKind(e.target.value as ModKind | "")}
            >
              <option value="">{t("discover.allKinds")}</option>
              {KINDS.map((k) => (
                <option key={k} value={k}>
                  {t(`discover.kind.${k}`)}
                </option>
              ))}
            </select>
          </label>
          <label className="grid gap-1 text-xs">
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
          <label className="flex items-center gap-2 py-2 text-sm">
            <input
              type="checkbox"
              className="size-4 accent-[var(--bb-accent)]"
              checked={playable}
              onChange={(e) => setPlayable(e.target.checked)}
            />
            {t("discover.playable")}
          </label>
        </div>
      )}

      {/* After a failed check the error above says it already. */}
      {mods.isSuccess && all.length === 0 && !st?.error && (
        <EmptyState title={t("discover.emptyTitle")}>
          <p className="max-w-md text-sm text-muted">{t("discover.empty")}</p>
        </EmptyState>
      )}
      {all.length > 0 && shown.length === 0 && (
        <div className="flex flex-wrap items-center gap-3">
          <Notice>{t("discover.noMatch")}</Notice>
          <Button variant="secondary" onClick={clearFilters}>
            {t("discover.clearFilters")}
          </Button>
        </div>
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
