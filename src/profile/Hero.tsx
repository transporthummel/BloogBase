import { Link } from "@tanstack/react-router";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink, TriangleAlert } from "lucide-react";
import { Fragment, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { type TFunction } from "i18next";
import { useTranslation } from "react-i18next";
import { type Game, type IndexEntry, type Mod, type Profile, type WorldStart } from "../bindings";
import { useIndexMods } from "../discover/queries";
import { StatusBadge } from "../discover/StatusBadge";
import { formatDate, formatPlayTime, looseDateText } from "../format";
import { Cover } from "../library/Cover";
import { useCover } from "../library/covers";
import { useMods } from "../library/queries";
import { Authors } from "../mods/Authors";
import { RunsOn } from "../mods/RunsOn";
import { Badge } from "../ui/Badge";
import { Card } from "../ui/Card";
import { RelativeTime } from "../ui/RelativeTime";
import { usePageTitle } from "../ui/usePageTitle";
import { SeriesLine } from "../series/SeriesLine";
import {
  episodeTitle,
  profileKey,
  type SeriesPlace,
  seriesName,
  seriesOf,
  useSeriesPlaces,
} from "../series/series";
import { screenshotSrc, useScreenshots } from "./captures";
import { usePictureWidth } from "./heroPicture";
import { ProfileActions } from "./ProfileActions";
import { variantName } from "./variants";

/** Picture, title, credits, description, links and stats; Play, and the rest in a menu. */
export function Hero({
  profile,
  game,
  canPlay,
  onPlay,
  onShowVariant,
  modCount,
}: {
  profile: Profile;
  game?: Game;
  canPlay: boolean;
  /** Goes to the variant played in the Variants tab. */
  onShowVariant: () => void;
  /** With the world to play, for a mod that keeps worlds. */
  onPlay: (world: WorldStart | null, variant?: string) => void;
  /** For a game: the mods played on it. */
  modCount: number;
}) {
  const { t, i18n } = useTranslation();
  const mods = useMods({ enabled: profile.modId !== null });
  const index = useIndexMods();
  const places = useSeriesPlaces();
  const seriesKey = profileKey(profile, index.data);
  const place = seriesKey ? seriesOf(places, seriesKey) : undefined;
  const title = place ? episodeTitle(place.part, profile.label) : profile.label;
  usePageTitle(place ? `${seriesName(place.series, i18n.language)}: ${title}` : profile.label);
  const shots = useScreenshots(profile.id);
  const cover = useCover(profile.id, profile.installId, profile.activeVariant?.id ?? null);
  const cardRef = useRef<HTMLElement>(null);
  const textRef = useRef<HTMLDivElement>(null);
  const pictureWidth = usePictureWidth(cardRef, textRef);

  const mod = mods.data?.find((m) => m.id === profile.modId);
  // Mods played over this one, as level packs for it.
  const extensions = mod ? (mods.data ?? []).filter((m) => m.extends === mod.id) : [];
  const entry = mod?.indexId
    ? index.data?.find((x) => x.entry.id === mod.indexId)?.entry
    : undefined;
  const latest = shots.data?.[0];
  // A translation from the index wins over the English text the mod stored.
  const description =
    profile.modId === null
      ? gameDescription(t, profile)
      : (entry?.translations?.[i18n.language]?.description ??
        mod?.description ??
        entry?.description);

  return (
    <Card ref={cardRef} className="mb-6 flex flex-col md:flex-row short:mb-4">
      {/* As tall as the text, at 4:3. Where the width limits get in the way,
          pictures keep 4:3 centred on the card's own colour, not on black
          bars like a misfit video; only the placeholder fills the box. */}
      <div className="flex shrink-0 items-center" style={{ width: pictureWidth }}>
        {cover.isPending ? (
          // The title screen is read from the files on first use; empty until
          // then, so the placeholder doesn't flash up before it.
          <div className="aspect-[4/3] w-full" />
        ) : cover.data || !latest ? (
          <Cover
            gameId={profile.gameId}
            game={game}
            src={cover.data?.src}
            onError={cover.onError}
            standalone={profile.sourceMod}
            className={`aspect-[4/3] w-full text-7xl ${cover.data ? "" : "h-full"}`}
          />
        ) : (
          <img
            src={screenshotSrc(latest)}
            alt={t("screenshots.alt", { game: profile.label })}
            className="aspect-[4/3] w-full object-cover [image-rendering:pixelated]"
          />
        )}
      </div>
      {/* About it on top, playing it at the bottom. */}
      <div
        ref={textRef}
        className="flex min-w-0 flex-1 flex-col justify-between gap-5 p-5 short:gap-3 short:p-4"
      >
        <div className="grid content-start gap-4 select-text short:gap-3">
          <Credits
            profile={profile}
            title={title}
            place={place}
            game={game}
            mod={mod}
            entry={entry}
            extensions={extensions}
            onShowVariant={onShowVariant}
          />
          {description && <Description key={profile.id} text={description} />}
          {cover.data?.scrambled && (
            <p className="flex max-w-prose items-start gap-2 text-sm">
              <TriangleAlert className="mt-0.5 size-4 shrink-0 text-warning" aria-hidden />
              {t("profile.scrambled")}
            </p>
          )}
          <Stats profile={profile} added={mod?.addedAt} />
        </div>
        <ProfileActions
          profile={profile}
          game={game}
          mod={mod}
          canPlay={canPlay}
          onPlay={onPlay}
          modCount={modCount}
        />
      </div>
    </Card>
  );
}

/** Lines shown of a long description, so Play stays in view... */
const DESCRIPTION_LINES = 3;
/** ...if that hides more than one: "Read more" takes a line of its own. */
const DESCRIPTION_CUT_FROM = DESCRIPTION_LINES + 2;

/** The description, cut after a word in its last line shown when long; "Read more" shows the rest. */
function Description({ text }: { text: string }) {
  const { t } = useTranslation();
  const ruler = useRef<HTMLParagraphElement>(null);
  const [open, setOpen] = useState(false);
  // The text up to where it's cut; null when it's shown whole.
  const [short, setShort] = useState<string | null>(null);

  useLayoutEffect(() => {
    const p = ruler.current;
    if (!p || typeof ResizeObserver === "undefined") return;
    // Words with the spaces and line breaks after them.
    const words = text.match(/\S+\s*/g) ?? [];
    // Measuring changes its height; only a new width calls for it again.
    let width = 0;
    const measure = () => {
      if (!p.clientWidth || p.clientWidth === width) return;
      width = p.clientWidth;
      const line = parseFloat(getComputedStyle(p).lineHeight);
      const height = (s: string) => {
        p.textContent = s;
        return p.scrollHeight;
      };
      if (!line || height(text) < line * (DESCRIPTION_CUT_FROM - 0.5)) return setShort(null);
      // The most words that fit, with the ellipsis, in the lines shown.
      const fits = (n: number) =>
        height(`${words.slice(0, n).join("").trimEnd()}…`) < line * (DESCRIPTION_LINES + 0.5);
      let lo = 1;
      let hi = words.length - 1;
      while (lo < hi) {
        const mid = Math.ceil((lo + hi) / 2);
        if (fits(mid)) lo = mid;
        else hi = mid - 1;
      }
      setShort(`${words.slice(0, lo).join("").trimEnd()}…`);
    };
    const observer = new ResizeObserver(measure);
    observer.observe(p);
    measure();
    return () => observer.disconnect();
  }, [text]);

  const cut = short !== null && !open;
  return (
    <div className="relative grid max-w-prose justify-items-start gap-1">
      {/* The same paragraph out of sight, to measure where to cut. */}
      <p
        ref={ruler}
        aria-hidden
        className="pointer-events-none invisible absolute inset-x-0 top-0 text-sm leading-relaxed whitespace-pre-line"
      />
      <p id="hero-description" className="text-sm leading-relaxed whitespace-pre-line">
        {cut ? short : text}
      </p>
      {short !== null && (
        <button
          type="button"
          aria-expanded={open}
          aria-controls="hero-description"
          onClick={() => setOpen(!open)}
          className="cursor-pointer text-sm text-muted underline underline-offset-2 select-none hover:text-ink"
        >
          {open ? t("profile.readLess") : t("profile.readMore")}
        </button>
      )}
    </div>
  );
}

/** Every episode, demo and beta is id Software's. */
const DEVELOPER = "id Software";
const DEVELOPER_URL = "https://www.idsoftware.com";

/** A demo's or beta's own text, else the episode's; none for unknown games. */
function gameDescription(t: TFunction, profile: Profile): string {
  const keys = [profile.versionId, profile.gameId].flatMap((id) =>
    id ? [`catalog.description.${id}`] : [],
  );
  return t(keys, { defaultValue: "" });
}

/** Title, what it runs on, authors, release, the mods played over it and links. */
function Credits({
  profile,
  title,
  place,
  game,
  mod,
  entry,
  extensions,
  onShowVariant,
}: {
  profile: Profile;
  /** In a series, the episode's own title. */
  title: string;
  place?: SeriesPlace;
  game?: Game;
  mod?: Mod;
  entry?: IndexEntry;
  extensions: Mod[];
  onShowVariant: () => void;
}) {
  const { t, i18n } = useTranslation();
  const version = game?.versions.find((v) => v.id === profile.versionId);
  const links = modLinks(t, mod, entry);
  const released = profile.modId === null ? version?.released : mod?.released;
  const runsOn =
    profile.missingGame !== null ? (
      t("mods.waitsFor", { game: profile.missingGame })
    ) : profile.standalone && (profile.sourceMod || !game) ? (
      t("mods.standalone")
    ) : profile.modId !== null && !profile.sourceMod && game ? (
      <RunsOn
        profile={profile}
        game={game.shortTitle}
        version={version?.label ?? t("library.unknownVersion")}
      />
    ) : null;
  return (
    <div className="grid gap-2 short:gap-1.5">
      <div className="grid gap-1">
        {place && <SeriesLine place={place} />}
        <h1 className="font-pixel text-lg leading-relaxed">{title}</h1>
      </div>
      <div className="flex flex-wrap items-center gap-2 text-sm text-muted">
        {(profile.activeVariant || runsOn) && (
          // The way it's played (§9.7), then what it runs on, in one line.
          <span>
            {profile.activeVariant && (
              <>
                {t("variants.variant")}{" "}
                {/* Leads to it among the others, as the mods played over it lead to theirs. */}
                <button
                  type="button"
                  onClick={onShowVariant}
                  title={t("variants.showAll")}
                  className="font-semibold text-ink underline-offset-2 hover:text-accent hover:underline"
                >
                  {variantName(profile.activeVariant, i18n.language)}
                </button>
                {runsOn && " · "}
              </>
            )}
            {runsOn}
          </span>
        )}
        {profile.modId !== null && !profile.sourceMod && <Badge>{t("mods.badge")}</Badge>}
        {profile.sourceMod && <Badge>{t("mods.sourceMod")}</Badge>}
        {profile.worlds && <Badge>{t("mods.randomWorlds")}</Badge>}
        {profile.variantCount > 0 && (
          <Badge>{t("variants.badge", { count: profile.variantCount })}</Badge>
        )}
        <StatusBadge status={entry?.status ?? mod?.status} />
        {profile.modId === null && <Badge>{version?.label ?? t("library.unknownVersion")}</Badge>}
        {profile.modId === null && version?.shareware && <Badge>{t("profile.shareware")}</Badge>}
        {mod?.version && (
          // "v2.0" like the game versions; versions with words stay as written.
          <Badge>{/^\d/.test(mod.version) ? `v${mod.version}` : mod.version}</Badge>
        )}
      </div>
      {/* Who made it and when, in one line, so the tabs stay in view in a small window. */}
      <ByLine
        authors={
          profile.modId === null
            ? game && <Authors authors={[DEVELOPER]} links={{ [DEVELOPER]: DEVELOPER_URL }} />
            : mod &&
              mod.authors.length > 0 && (
                <Authors
                  authors={mod.authors}
                  links={{ ...mod.authorLinks, ...entry?.authorLinks }}
                />
              )
        }
        released={released && looseDateText(t, i18n.language, "mods.releasedOn", released)}
      />
      {extensions.length > 0 && (
        <p className="text-sm">
          <span className="text-muted">
            {t("profile.playedOverIt", { count: extensions.length })}
          </span>{" "}
          {extensions.map((m, i) => (
            <Fragment key={m.id}>
              {i > 0 && ", "}
              <Link
                to="/profiles/$profileId"
                params={{ profileId: m.profileId }}
                className="underline-offset-2 hover:text-accent hover:underline"
              >
                {m.name}
              </Link>
            </Fragment>
          ))}
        </p>
      )}
      {links.length > 0 && (
        <ul className="flex flex-wrap gap-x-4 gap-y-1 text-sm" aria-label={t("profile.links")}>
          {links.map(([label, url]) => (
            <li key={url}>
              <button
                type="button"
                onClick={() => openUrl(url)}
                className="inline-flex items-center gap-1 text-muted underline-offset-2 hover:text-accent hover:underline"
              >
                <ExternalLink className="size-3.5" aria-hidden />
                {label}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/** Authors, then the release date in muted text; wraps only between the two. */
function ByLine({ authors, released }: { authors: ReactNode; released: ReactNode }) {
  if (!authors && !released) return null;
  return (
    <p className="text-sm">
      {authors}
      {authors && released && <span className="text-muted"> · </span>}
      {released && <span className="whitespace-nowrap text-muted">{released}</span>}
    </p>
  );
}

/** Play time, sessions, last played, saves and, for a mod, when it was added. */
function Stats({ profile, added }: { profile: Profile; added?: number }) {
  const { t, i18n } = useTranslation();
  if (profile.sessionCount === 0) {
    // Nothing to count yet: one line instead of columns of "0" and "–".
    // It wraps only between the parts, never inside a date.
    return (
      <p className="text-sm text-muted">
        {[
          t("library.neverPlayed"),
          t("library.saveCount", { count: profile.saveCount }),
          added !== undefined && `${t("profile.added")} ${formatDate(i18n.language, added)}`,
        ]
          .filter(Boolean)
          .map((part, i) => (
            <Fragment key={i}>
              {i > 0 && " · "}
              <span className="whitespace-nowrap">{part}</span>
            </Fragment>
          ))}
      </p>
    );
  }
  const stats: [string, ReactNode][] = [
    [t("profile.playTime"), formatPlayTime(t, profile.playSeconds)],
    [t("profile.sessions"), String(profile.sessionCount)],
    [
      t("library.lastPlayed"),
      profile.lastPlayedAt === null ? "–" : <RelativeTime at={profile.lastPlayedAt} />,
    ],
    [t("library.saveCountLabel"), String(profile.saveCount)],
  ];
  if (added !== undefined) stats.push([t("profile.added"), formatDate(i18n.language, added)]);
  // Columns, so the second row lines up with the first. auto-fit, not
  // auto-fill: an empty column would narrow the others as the text widens,
  // a label would wrap, and the picture beside it would never settle.
  // In a short window they run on as a line, like the one before the first
  // game, so the tabs stay in view; it breaks only after a " · ".
  return (
    <dl className="grid grid-cols-[repeat(auto-fit,minmax(5.5rem,1fr))] gap-x-4 gap-y-2 short:block short:text-sm short:text-muted">
      {stats.map(([label, value]) => (
        <div key={label} className="grid gap-1 short:inline short:not-last:after:content-['_·_']">
          <dt className="text-xs text-muted short:mr-1 short:inline short:text-sm short:whitespace-nowrap">
            {label}
          </dt>
          <dd className="text-sm font-semibold short:inline short:whitespace-nowrap short:text-ink">
            {value}
          </dd>
        </div>
      ))}
    </dl>
  );
}

function modLinks(
  t: (key: string) => string,
  mod: Mod | undefined,
  entry: IndexEntry | undefined,
): [string, string][] {
  if (!mod) return [];
  const all: [string, string | null | undefined][] = [
    ["KeenWiki", mod.keenwikiUrl ?? entry?.links.keenwiki],
    ["PCKF", mod.pckf ?? entry?.links.pckf],
    ["Keen: Modding", mod.keenmodding ?? entry?.links.keenmodding],
    [t("discover.homepage"), mod.homepage ?? entry?.links.homepage],
  ];
  return all.filter((l): l is [string, string] => !!l[1]);
}
