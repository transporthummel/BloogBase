import { Link } from "@tanstack/react-router";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink, TriangleAlert } from "lucide-react";
import { Fragment, useRef, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { type Game, type IndexEntry, type Mod, type Profile } from "../bindings";
import { useIndexMods } from "../discover/queries";
import { formatDate, formatLooseDate, formatPlayTime } from "../format";
import { Cover } from "../library/Cover";
import { useCover } from "../library/covers";
import { useMods } from "../library/queries";
import { Authors } from "../mods/Authors";
import { RunsOn } from "../mods/RunsOn";
import { Badge } from "../ui/Badge";
import { Card } from "../ui/Card";
import { RelativeTime } from "../ui/RelativeTime";
import { usePageTitle } from "../ui/usePageTitle";
import { screenshotSrc, useScreenshots } from "./captures";
import { usePictureWidth } from "./heroPicture";
import { ProfileActions } from "./ProfileActions";

/** Picture, title, credits, description, links and stats; Play, and the rest in a menu. */
export function Hero({
  profile,
  game,
  canPlay,
  onPlay,
  modCount,
}: {
  profile: Profile;
  game?: Game;
  canPlay: boolean;
  onPlay: () => void;
  /** For a game: the mods played on it. */
  modCount: number;
}) {
  const { t, i18n } = useTranslation();
  usePageTitle(profile.label);
  const mods = useMods({ enabled: profile.modId !== null });
  const index = useIndexMods();
  const shots = useScreenshots(profile.id);
  const cover = useCover(profile.id, profile.installId);
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
    entry?.translations?.[i18n.language]?.description ?? mod?.description ?? entry?.description;

  return (
    <Card ref={cardRef} className="mb-6 flex flex-col md:flex-row">
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
      <div ref={textRef} className="flex min-w-0 flex-1 flex-col justify-between gap-5 p-5">
        <div className="grid content-start gap-4 select-text">
          <Credits profile={profile} game={game} mod={mod} entry={entry} extensions={extensions} />
          {description && (
            <p className="max-w-prose text-sm leading-relaxed whitespace-pre-line">{description}</p>
          )}
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

/** Title, what it runs on, authors, release, the mods played over it and links. */
function Credits({
  profile,
  game,
  mod,
  entry,
  extensions,
}: {
  profile: Profile;
  game?: Game;
  mod?: Mod;
  entry?: IndexEntry;
  extensions: Mod[];
}) {
  const { t, i18n } = useTranslation();
  const version = game?.versions.find((v) => v.id === profile.versionId);
  const links = modLinks(t, mod, entry);
  return (
    <div className="grid gap-2">
      <h1 className="font-pixel text-lg leading-relaxed">{profile.label}</h1>
      <div className="flex flex-wrap items-center gap-2 text-sm text-muted">
        {profile.missingGame !== null && (
          <span>{t("mods.waitsFor", { game: profile.missingGame })}</span>
        )}
        {profile.modId !== null && profile.missingGame === null && game && (
          <span>
            <RunsOn
              profile={profile}
              game={game.title}
              version={version?.label ?? t("library.unknownVersion")}
            />
          </span>
        )}
        {profile.modId !== null && <Badge>{t("mods.badge")}</Badge>}
        {profile.modId === null && <Badge>{version?.label ?? t("library.unknownVersion")}</Badge>}
        {profile.modId === null && version?.shareware && <Badge>{t("profile.shareware")}</Badge>}
        {mod?.version && (
          // "v2.0" like the game versions; versions with words stay as written.
          <Badge>{/^\d/.test(mod.version) ? `v${mod.version}` : mod.version}</Badge>
        )}
      </div>
      {mod && mod.authors.length > 0 && (
        <p className="text-sm">
          <Authors authors={mod.authors} links={{ ...mod.authorLinks, ...entry?.authorLinks }} />
        </p>
      )}
      {mod?.released && (
        <p className="text-sm text-muted">
          {t("profile.released", { date: formatLooseDate(i18n.language, mod.released) })}
        </p>
      )}
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
  // Columns, so the second row lines up with the first.
  return (
    <dl className="grid grid-cols-[repeat(auto-fill,minmax(5.5rem,1fr))] gap-x-4 gap-y-2">
      {stats.map(([label, value]) => (
        <div key={label} className="grid gap-1">
          <dt className="text-xs text-muted">{label}</dt>
          <dd className="text-sm font-semibold">{value}</dd>
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
