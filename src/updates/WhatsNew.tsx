import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { ArrowRight, RefreshCw, Sparkles } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type IndexMod, type IndexNews } from "../bindings";
import { shortGameTitle, useGames } from "../catalog";
import { useIndexMods, useIndexStatus } from "../discover/queries";
import { errorMessage } from "../errors";
import { unwrap } from "../ipc";
import { JobRow } from "../jobs/JobList";
import { isFinished, useJob, useStartJob } from "../jobs/queries";
import { useProfiles } from "../library/queries";
import { getLastVersion, setLastVersion } from "../preferences";
import { Button } from "../ui/Button";
import { buttonClass } from "../ui/buttonClass";
import { Dialog } from "../ui/Dialog";
import { Notice } from "../ui/Notice";
import { changelog, type Release } from "./changelog";
import { modUpdateKey, modUpdates, newIndexMods, releasesSince } from "./news";

type News = { releases: Release[]; mods: IndexMod[]; updates: IndexMod[] };

// The backend checks the mod list as it starts, a moment before this page
// loads; a check from before that was the last run's.
const startedAt = performance.timeOrigin / 1000 - 120;

/** Stores what the library knows about the mod list; not fatal when it fails. */
function save(index: IndexNews): IndexNews {
  commands.setIndexNews(index).catch(() => {});
  return index;
}

function merge(a: IndexMod[], b: IndexMod[]): IndexMod[] {
  return [...a, ...b.filter((m) => !a.some((x) => x.entry.id === m.entry.id))];
}

/**
 * What's new since the last start: the changes of a new BloogBase version,
 * mods new in the mod list, and updates of installed ones. Opens on start, or
 * once the start's check of the mod list finds something; after that check,
 * nothing new opens it until the next start.
 */
export function WhatsNew() {
  const { t, i18n } = useTranslation();
  const info = useQuery({ queryKey: ["appInfo"], queryFn: () => unwrap(commands.appInfo()) });
  const status = useIndexStatus().data;
  const indexMods = useIndexMods();
  const games = useGames().data;
  const profiles = useProfiles();
  // Kept with the library: what it knows about the mod list.
  const known = useQuery({
    queryKey: ["indexNews"],
    queryFn: () => unwrap(commands.indexNews()),
    staleTime: Infinity,
  });
  // The app version is the device's.
  const stored = useRef<{ last: string | null; index: IndexNews | null }>({
    last: getLastVersion(),
    index: null,
  });
  const done = useRef(false);
  const [news, setNews] = useState<News | null>(null);
  const [closed, setClosed] = useState(false);
  const version = info.data?.version;
  const mods = indexMods.data;
  const checked =
    !!status && !indexMods.isFetching && (!!status.error || (status.checkedAt ?? 0) >= startedAt);

  useEffect(() => {
    if (!version || closed || done.current) return;
    const s = stored.current;
    const releases = releasesSince(changelog, s.last, version);
    if (releases.length === 0 && s.last !== version) {
      s.last = version;
      setLastVersion(version);
    }
    s.index ??= known.data ?? null;
    let fresh: IndexMod[] = [];
    let updates: IndexMod[] = [];
    // An empty list hasn't loaded yet.
    if (s.index && mods && mods.length > 0) {
      if (s.index.seen === null) {
        // The first start: nothing is new yet.
        s.index = save({ ...s.index, seen: mods.map((m) => m.entry.id) });
      }
      fresh = newIndexMods(mods, s.index.seen ?? []);
      updates = modUpdates(mods, s.index.offered);
    }
    if (news) {
      const next = {
        ...news,
        mods: merge(news.mods, fresh),
        updates: merge(news.updates, updates),
      };
      if (next.mods.length > news.mods.length || next.updates.length > news.updates.length) {
        setNews(next);
      }
    } else if (releases.length + fresh.length + updates.length > 0) {
      setNews({ releases, mods: fresh, updates });
    } else if (checked && s.index) {
      done.current = true;
      // Entries that left the list are new again when they come back.
      const ids = new Set(mods?.map((m) => m.entry.id));
      const seen = s.index.seen;
      if (ids.size > 0 && seen?.some((id) => !ids.has(id))) {
        s.index = save({ ...s.index, seen: seen.filter((id) => ids.has(id)) });
      }
    }
  }, [version, mods, checked, closed, news, known.data]);

  if (!news || closed) return null;

  const close = () => {
    setClosed(true);
    if (version) setLastVersion(version);
    const s = stored.current;
    // An empty list hasn't loaded yet.
    if (!s.index || !mods || mods.length === 0) return;
    // Only updates still waiting are kept, so the list doesn't grow forever.
    const waiting = new Set(mods.filter((m) => m.modId && m.updateAvailable).map(modUpdateKey));
    const offered = new Set([...s.index.offered, ...news.updates.map(modUpdateKey)]);
    s.index = save({
      seen: mods.map((m) => m.entry.id),
      offered: [...offered].filter((k) => waiting.has(k)),
    });
  };
  const running = (mod: IndexMod) =>
    profiles.data?.some((p) => p.modId === mod.modId && p.running) ?? false;

  return (
    <Dialog
      title={t("news.title")}
      icon={<Sparkles className="size-6 shrink-0 text-accent" aria-hidden />}
      onClose={close}
    >
      {news.releases.map((r) => (
        <section key={r.version} className="grid gap-2">
          <h3 className="font-semibold">{t("news.release", { version: r.version })}</h3>
          <ul className="list-disc pl-6 text-sm select-text">
            {(i18n.language === "de" ? r.de : r.en).map((line) => (
              <li key={line}>{line}</li>
            ))}
          </ul>
        </section>
      ))}
      {news.mods.length > 0 && (
        <section className="grid gap-2">
          <h3 className="font-semibold">{t("news.mods", { count: news.mods.length })}</h3>
          <ul className="grid gap-1 text-sm">
            {news.mods.map(({ entry }) => (
              <li key={entry.id}>
                <span className="font-medium">{entry.name}</span>{" "}
                <span className="text-muted">
                  · {shortGameTitle(games, entry.game)} · {t(`discover.kind.${entry.type}`)}
                </span>
              </li>
            ))}
          </ul>
          <Link
            to="/discover"
            search={{ tab: "mods", sort: "newest" }}
            onClick={close}
            className={buttonClass("secondary", "justify-self-start")}
          >
            {t("news.toDiscover")}
            <ArrowRight className="size-4" aria-hidden />
          </Link>
        </section>
      )}
      {news.updates.length > 0 && (
        <section className="grid gap-2">
          <h3 className="font-semibold">{t("news.updates", { count: news.updates.length })}</h3>
          <ul className="grid gap-3">
            {news.updates.map((m) => (
              <UpdateRow key={m.entry.id} mod={m} disabled={running(m)} />
            ))}
          </ul>
        </section>
      )}
      <div className="flex justify-end">
        <Button onClick={close}>{t("common.close")}</Button>
      </div>
    </Dialog>
  );
}

/** An installed index mod's update, installed over the old one like from Discover. */
function UpdateRow({ mod, disabled }: { mod: IndexMod; disabled: boolean }) {
  const { t } = useTranslation();
  const { entry } = mod;
  const job = useJob("mod", entry.id);
  // The installed mod's own game version gets it.
  const start = useStartJob(() => commands.installIndexMod(entry.id, null, true, null));

  if (job && (!isFinished(job) || start.isSuccess)) {
    return (
      <li>
        <JobRow job={job} label={entry.name} />
      </li>
    );
  }
  return (
    <li className="grid gap-2">
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0 text-sm">
          <p className="font-medium">{entry.name}</p>
          <p className="text-muted">
            {t("modUpdate.inIndex", {
              version: entry.version,
              context: entry.version ? "version" : undefined,
            })}
          </p>
        </div>
        <Button
          variant="secondary"
          onClick={() => start.mutate()}
          disabled={disabled || start.isPending}
        >
          <RefreshCw className="size-4" aria-hidden />
          {t("modUpdate.update")}
        </Button>
      </div>
      {start.error && <Notice tone="error">{errorMessage(t, start.error)}</Notice>}
    </li>
  );
}
