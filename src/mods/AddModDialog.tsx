import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Compass,
  FileArchive,
  FolderOpen,
  LibraryBig,
  PackagePlus,
  Puzzle,
  RefreshCw,
  Replace,
} from "lucide-react";
import { type ReactElement, type ReactNode, useRef, useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import {
  commands,
  type Dropped,
  type Install,
  type KeenwikiOrigin,
  type Mod,
  type ModInfo,
} from "../bindings";
import { bestFit, gameTitle, isModified, shortGameTitle, useGames } from "../catalog";
import { useIndexInstalled, useIndexMods } from "../discover/queries";
import { errorMessage } from "../errors";
import { formatLooseDate } from "../format";
import { unwrap } from "../ipc";
import { coverKey } from "../library/covers";
import { importPaths } from "../library/importPaths";
import { JobRow } from "../jobs/JobList";
import { isFinished, useJob, useStartJob, waitForJob } from "../jobs/queries";
import { installsKey, modsKey, profilesKey } from "../library/queries";
import { ReadmePanel } from "../profile/ReadmePanel";
import { readmeKey, useReadmesAt } from "../profile/readme";
import { Button } from "../ui/Button";
import { buttonClass } from "../ui/buttonClass";
import { Dialog } from "../ui/Dialog";
import { Notice } from "../ui/Notice";
import { useEnterAction } from "../ui/useEnterAction";
import { IndexHit, KeenwikiImport, OkButton } from "./KeenwikiImport";
import type { Downloaded } from "./wikiDownload";
import { RecipeLabel } from "./RecipeLabel";
import { fits } from "./recipes";

/**
 * Get a mod (KeenWiki link, folder or ZIP), confirm what was detected, install it.
 * A folder or ZIP holding games instead is imported as such.
 * With `replacing` and nothing fetched yet, it asks for a local copy to replace that mod with.
 */
export function AddModDialog({
  dropped,
  queue,
  downloaded,
  replacing,
  onClose,
  onInstalled,
  onGamesImported,
}: {
  /** A folder or ZIP dropped on the window and inspected already. */
  dropped?: { path: string; info: ModInfo };
  /** Where `dropped` is among several added at once, counting from 1. */
  queue?: { at: number; of: number };
  /** A KeenWiki download fetched already, e.g. an update. */
  downloaded?: Downloaded;
  /** The installed mod it updates or replaces. */
  replacing?: string;
  onClose: () => void;
  onInstalled: (mod: Mod) => void;
  /** Games were picked instead of a mod and imported. */
  onGamesImported?: (installs: Install[]) => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [path, setPath] = useState<string | null>(null);
  const [fromWiki, setFromWiki] = useState<Downloaded | null>(downloaded ?? null);
  // Everything from the mod list is installed and current: KeenWiki is where
  // new mods come from, so the list steps back and the link field leads.
  const index = useIndexMods();
  const haveAll =
    !!index.data?.length && index.data.every((m) => m.modId !== null && !m.updateAvailable);

  // A pick may also hold games, which are imported right away; only a replacement must be a mod.
  const inspect = useMutation({
    mutationFn: async (p: string): Promise<Dropped> => {
      if (replacing) return { kind: "mod", info: await unwrap(commands.inspectMod(p)) };
      const found = await unwrap(commands.inspectDropped(p));
      if (found.kind === "games") {
        const imported = await unwrap(commands.importGames(p));
        queryClient.invalidateQueries({ queryKey: profilesKey });
        queryClient.invalidateQueries({ queryKey: installsKey });
        onGamesImported?.(imported);
      }
      return found;
    },
  });

  async function pick(kind: "folder" | "zip") {
    // Several at once, unless it replaces one mod.
    const picked = await open({
      multiple: !replacing,
      ...(kind === "folder"
        ? { directory: true }
        : { filters: [{ name: "ZIP", extensions: ["zip"] }] }),
    });
    const paths = picked === null ? [] : Array.isArray(picked) ? picked : [picked];
    if (paths.length > 1) {
      // Imported like a drop on the window, one after the other.
      onClose();
      importPaths(paths);
      return;
    }
    if (!paths[0]) return;
    setPath(paths[0]);
    inspect.mutate(paths[0]);
  }

  const TitleIcon = downloaded ? RefreshCw : replacing ? Replace : PackagePlus;
  const title = t(
    downloaded ? "mods.updateTitle" : replacing ? "mods.replaceTitle" : "mods.addTitle",
  );
  return (
    <Dialog
      title={queue ? t("drop.progress", { title, at: queue.at, of: queue.of }) : title}
      icon={<TitleIcon className="size-6 shrink-0 text-accent" aria-hidden />}
      onClose={onClose}
    >
      {dropped ? (
        <Confirm
          path={dropped.path}
          info={dropped.info}
          replacing={replacing}
          queued={!!queue}
          onCancel={onClose}
          onInstalled={onInstalled}
        />
      ) : fromWiki ? (
        <Confirm
          path={fromWiki.path}
          info={fromWiki.info}
          origin={fromWiki.origin}
          replacing={replacing}
          onCancel={onClose}
          onInstalled={onInstalled}
        />
      ) : path && inspect.data?.kind === "mod" ? (
        <Confirm
          path={path}
          info={inspect.data.info}
          replacing={replacing}
          onCancel={onClose}
          onInstalled={onInstalled}
        />
      ) : (
        <AddFrom
          wiki={!replacing}
          disabled={inspect.isPending}
          onDownloaded={setFromWiki}
          onCancel={onClose}
          focus={haveAll}
          before={
            // The curated list first: the easy way for anyone new to mods;
            // then KeenWiki, where new mods come from.
            <div className="flex flex-wrap items-center gap-x-4 gap-y-3">
              <div className="grid min-w-0 flex-1 basis-60 gap-1">
                <p className="font-medium">{t("mods.fromList")}</p>
                <p className="text-sm text-muted">
                  {t(haveAll ? "mods.fromListAll" : "mods.fromListHint")}
                </p>
              </div>
              <Link
                to="/discover"
                className={buttonClass(haveAll ? "secondary" : "primary")}
                onClick={onClose}
                data-autofocus={!haveAll || undefined}
              >
                <Compass className="size-4" aria-hidden />
                {t("mods.browseList")}
              </Link>
            </div>
          }
          footer={
            <div className="flex justify-end">
              <Button variant="ghost" onClick={onClose}>
                {t("common.cancel")}
              </Button>
            </div>
          }
        >
          <p className="text-sm text-muted">
            {t(replacing ? "mods.replaceIntro" : "mods.addHint")}
          </p>
          <div className="flex flex-wrap gap-3">
            <Button variant="secondary" onClick={() => pick("zip")} disabled={inspect.isPending}>
              <FileArchive className="size-4" aria-hidden />
              {t("mods.chooseZip")}
            </Button>
            <Button variant="secondary" onClick={() => pick("folder")} disabled={inspect.isPending}>
              <FolderOpen className="size-4" aria-hidden />
              {t("mods.chooseFolder")}
            </Button>
          </div>
          {inspect.isPending && <Notice>{t("mods.inspecting")}</Notice>}
          {inspect.data?.kind === "nothing" && <Notice tone="error">{t("drop.nothing")}</Notice>}
          {inspect.error && <Notice tone="error">{errorMessage(t, inspect.error)}</Notice>}
          {!replacing && <p className="text-sm text-muted">{t("mods.addMore")}</p>}
        </AddFrom>
      )}
    </Dialog>
  );
}

/**
 * The ways to add a mod; a KeenWiki find replaces the others. Without
 * KeenWiki (replacing a mod), `before` is left out too.
 */
function AddFrom({
  wiki,
  before,
  focus,
  children,
  footer,
  ...props
}: {
  wiki: boolean;
  disabled: boolean;
  onDownloaded: (d: Downloaded) => void;
  onCancel: () => void;
  before: ReactNode;
  focus: boolean;
  children: ReactNode;
  footer: ReactNode;
}) {
  return wiki ? (
    <KeenwikiImport before={before} focus={focus} footer={footer} {...props}>
      {children}
    </KeenwikiImport>
  ) : (
    <div className="grid gap-3">
      {children}
      {footer}
    </div>
  );
}

type ConfirmProps = {
  path: string;
  info: ModInfo;
  /** Set for a mod from a KeenWiki page, whose details win over the guesses. */
  origin?: KeenwikiOrigin | null;
  /** The installed mod to preselect for replacing. */
  replacing?: string;
  /** One of several added at once: leaving it skips to the next. */
  queued?: boolean;
  onCancel: () => void;
  onInstalled: (mod: Mod) => void;
};

/** A file that is an index download installs as that entry, unless the player sets it up. */
function Confirm(props: ConfirmProps) {
  const { t } = useTranslation();
  const [myself, setMyself] = useState(false);
  const { info, path, replacing, queued, onCancel } = props;
  const installed = useIndexInstalled(info.indexId);
  if (!info.indexId || replacing || myself) return <SetUp {...props} />;
  return (
    <div className="grid gap-4">
      <IndexHit id={info.indexId} archive={path} notice="mods.inIndex" />
      <div className="flex justify-end gap-3">
        <Button variant="ghost" onClick={() => setMyself(true)}>
          {t("mods.setUpMyself")}
        </Button>
        {queued ? (
          <Button variant={installed ? "primary" : "ghost"} onClick={onCancel}>
            {t("drop.continue")}
          </Button>
        ) : (
          <OkButton installed={installed} onClick={onCancel} label={t("common.close")} />
        )}
      </div>
    </div>
  );
}

function SetUp({
  path,
  info,
  origin = null,
  replacing,
  queued,
  onCancel,
  onInstalled,
}: ConfirmProps) {
  const { t, i18n } = useTranslation();
  const games = useGames();
  const installs = useQuery({
    queryKey: installsKey,
    queryFn: () => unwrap(commands.listInstalls()),
  });
  const [name, setName] = useState(info.name);
  const [recipeIndex, setRecipeIndex] = useState(0);
  const [installId, setInstallId] = useState<string | null>(null);
  const queryClient = useQueryClient();
  const mods = useQuery({ queryKey: modsKey, queryFn: () => unwrap(commands.listMods()) });
  const profiles = useQuery({
    queryKey: profilesKey,
    queryFn: () => unwrap(commands.listProfiles()),
  });
  // The mod this one replaces, `null` to install alongside; unset: the first that matches.
  const [replace, setReplace] = useState<string | null | undefined>(replacing);

  const game = games.data?.find((g) => g.id === info.gameId);
  const option = info.recipes[recipeIndex];
  const candidates = installs.data?.filter((i) => fits(info, option, i)) ?? [];
  const versionOf = (i: Install) => game?.versions.find((v) => v.id === i.versionId);
  // A patch that names no version is most likely made for the version most mods use.
  const assumedVersion =
    option.recipe.type === "patch" && option.gameVersions.length === 0
      ? game?.versions.find((v) => v.id === game.moddingVersion)
      : undefined;
  const isAssumed = (i: Install) => {
    const v = versionOf(i);
    return v !== undefined && (v.original ?? v.id) === assumedVersion?.id;
  };
  const same = (a: string, b: string) => a.trim().toLowerCase() === b.trim().toLowerCase();
  // Known on both sides and different: another mod, whatever it's called.
  const differ = (a: string | null | undefined, b: string | null | undefined) =>
    !!a && !!b && a !== b;
  // Another version of a mod the player has: same KeenWiki page, or same game and name.
  const others =
    mods.data?.filter(
      (m) =>
        m.gameId === info.gameId &&
        (m.id === replacing ||
          (origin && m.keenwikiUrl === origin.pageUrl) ||
          ((same(m.name, name) || same(m.name, info.name)) &&
            !differ(m.keenwikiUrl, origin?.pageUrl) &&
            !differ(m.indexId, info.indexId))),
    ) ?? [];
  // Picked to replace it, but for another game.
  const wrongGame = mods.data?.find((m) => m.id === replacing && m.gameId !== info.gameId);
  const replaced =
    replace === null ? undefined : (others.find((m) => m.id === replace) ?? others[0]);
  // A replaced mod stays on its copy of the game when it can.
  const replacedInstall = profiles.data?.find((p) => p.id === replaced?.profileId)?.installId;
  const target =
    candidates.find((i) => i.id === installId) ??
    candidates.find((i) => i.id === replacedInstall) ??
    bestFit(game, candidates);
  // Waits for the mods, so the question about another version isn't skipped.
  const ready = !!installs.data && !!mods.data && !wrongGame;
  const versionLabel = (i: Install) => versionOf(i)?.label ?? t("library.unknownVersion");
  // Unknown versions get no warning: they may well be the right one.
  const likelyWrongVersion =
    target && assumedVersion && versionOf(target) !== undefined && !isAssumed(target);

  // The mod it belongs over, which it's played over unless unticked.
  const base = option.baseMod;
  const [playOver, setPlayOver] = useState(true);
  const overBase = !!base && playOver;
  // The mod below, installed from the index first when it isn't yet.
  const startBase = useStartJob((indexId: string) =>
    commands.installIndexMod(indexId, target?.id ?? null, !target, null),
  );
  const baseJob = useJob("mod", base?.indexId ?? null);
  const install = useMutation({
    mutationFn: async () => {
      let below = overBase ? (base?.modId ?? null) : null;
      if (overBase && base && !below && base.indexId) {
        const indexId = base.indexId;
        const job = await waitForJob(queryClient, (await startBase.mutateAsync(indexId)).id);
        if (job.phase === "failed") throw job.error;
        below = (await unwrap(commands.listMods())).find((m) => m.indexId === indexId)?.id ?? null;
      }
      return unwrap(
        commands.installMod({
          path,
          name,
          recipe: option.recipe,
          // Without a fitting game, the player agreed to install anyway.
          installId: target?.id ?? null,
          keenwiki: origin,
          extends: below,
          // Renaming may have made it no longer match.
          replace: replaced?.id ?? null,
        }),
      );
    },
    onSuccess: (mod) => {
      queryClient.invalidateQueries({ queryKey: modsKey });
      // The new version may have another title screen and README.
      if (replaced) {
        queryClient.invalidateQueries({ queryKey: coverKey(mod.profileId) });
        queryClient.invalidateQueries({ queryKey: readmeKey(mod.id) });
      }
      onInstalled(mod);
    },
  });

  // Enter installs, unless there's a warning to read or a choice to make.
  const otherGame = !!origin?.gameId && origin.gameId !== info.gameId;
  const missingGame = !!installs.data && candidates.length === 0;
  const modifiedGame = !!target && option.recipe.type === "patch" && isModified(versionOf(target));
  const versionUnchecked = !!target && target.versionId === null && option.gameVersions.length > 0;
  // Made to be copied over another mod, which isn't part of it.
  const incomplete = option.missingFiles.length > 0;
  // Levels alone, for the mod their archive names.
  const levelsFor = !!base && !incomplete;
  // Levels alone, for a mod BloogBase doesn't know.
  const unknownBase = !base && !incomplete ? option.madeFor : null;
  const readme = useReadmeLink(path);
  const choose = (info.recipes.length > 1 && !info.confident) || (others.length > 0 && !replacing);
  const careful =
    otherGame ||
    missingGame ||
    !!likelyWrongVersion ||
    modifiedGame ||
    versionUnchecked ||
    choose ||
    incomplete ||
    levelsFor ||
    !!unknownBase;
  const form = useRef<HTMLFormElement>(null);
  const submit = () => {
    if (ready && !install.isPending) install.mutate();
  };
  useEnterAction(form, careful ? null : submit);
  // Warned that it may not run as installed.
  const warned =
    missingGame ||
    !!likelyWrongVersion ||
    modifiedGame ||
    (incomplete && !overBase) ||
    !!unknownBase;

  const authors = origin?.authors.length ? origin.authors : info.authors;
  const version = origin?.version ?? info.version;
  const details = [
    authors.length > 0 && t("mods.by", { authors: authors.join(", ") }),
    version && t("mods.version", { version }),
    info.released && formatLooseDate(i18n.language, info.released),
  ].filter(Boolean);

  return (
    <form
      ref={form}
      className="grid gap-4"
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
    >
      <label className="grid gap-1 text-sm">
        <span className="font-medium">{t("mods.name")}</span>
        <input
          className="rounded-lg border border-border bg-surface-raised px-3 py-2"
          autoComplete="off"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && careful) e.preventDefault();
          }}
          required
        />
      </label>

      <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
        <dt className="text-muted">{t("mods.forGame")}</dt>
        <dd>{game?.title ?? info.gameId}</dd>
        {option.gameVersions.length > 0 && (
          <>
            <dt className="text-muted">{t("mods.madeFor")}</dt>
            <dd>{option.gameVersions.map((v) => `v${v}`).join(", ")}</dd>
          </>
        )}
        {details.length > 0 && (
          <>
            <dt className="text-muted">{t("mods.details")}</dt>
            <dd>{details.join(" · ")}</dd>
          </>
        )}
        <dt className="text-muted">{t("mods.files")}</dt>
        <dd>{info.fileCount}</dd>
      </dl>

      {info.recipes.length > 1 &&
        (info.confident ? (
          <details className="text-sm">
            <summary className="cursor-pointer text-muted">{t("mods.startAdvanced")}</summary>
            <RecipeChoice info={info} value={recipeIndex} onChange={setRecipeIndex} />
          </details>
        ) : (
          <fieldset className="grid gap-2 text-sm">
            <legend className="mb-1 font-medium">{t("mods.helpTitle")}</legend>
            <HelpHint path={path} />
            <RecipeChoice info={info} value={recipeIndex} onChange={setRecipeIndex} />
          </fieldset>
        ))}

      {otherGame && origin?.gameId && (
        <Notice>
          {t("keenwiki.otherGame", {
            page: gameTitle(games.data, origin.gameId),
            files: gameTitle(games.data, info.gameId),
          })}
        </Notice>
      )}
      {wrongGame && (
        <Notice tone="error">
          {t("mods.replaceOtherGame", {
            name: wrongGame.name,
            game: gameTitle(games.data, wrongGame.gameId),
            files: game?.title ?? info.gameId,
          })}
        </Notice>
      )}
      {others.length > 0 && (
        // A box of its own, so the question and its answers read as one.
        <fieldset className="grid gap-2 rounded-lg border border-border bg-surface-raised/50 p-3 text-sm">
          {/* Floated, so the legend sits inside the box, not on its border. */}
          <legend className="float-left mb-1 flex w-full items-center gap-2 font-semibold">
            <LibraryBig className="size-4 shrink-0 text-accent" aria-hidden />
            {t("mods.alreadyHave")}
          </legend>
          {others.map((m) => (
            <label key={m.id} className="flex items-start gap-2">
              <input
                type="radio"
                name="replace"
                className="mt-1"
                checked={replaced?.id === m.id}
                onChange={() => setReplace(m.id)}
              />
              <span>
                {t("mods.replaceOld", {
                  name: m.name,
                  version: m.version,
                  context: m.version ? "version" : undefined,
                })}
                <span className="block text-muted">{t("mods.replaceHint")}</span>
              </span>
            </label>
          ))}
          <label className="flex items-start gap-2">
            <input
              type="radio"
              name="replace"
              className="mt-1"
              checked={!replaced}
              onChange={() => setReplace(null)}
            />
            <span>{t("mods.alongside")}</span>
          </label>
        </fieldset>
      )}
      {missingGame && (
        <Notice>
          {t("mods.missingGame", {
            game: game?.title ?? info.gameId,
            versions: option.gameVersions.map((v) => `v${v}`).join(", "),
            context: option.gameVersions.length > 0 ? "version" : undefined,
          })}
        </Notice>
      )}
      {candidates.length > 1 && (
        <label className="grid gap-1 text-sm">
          <span className="font-medium">{t("mods.playOn")}</span>
          <select
            className="rounded-lg border border-border bg-surface-raised px-3 py-2"
            value={target?.id}
            onChange={(e) => setInstallId(e.target.value)}
          >
            {candidates.map((i) => (
              <option key={i.id} value={i.id}>
                {shortGameTitle(games.data, i.gameId)} ({versionLabel(i)})
              </option>
            ))}
          </select>
        </label>
      )}
      {likelyWrongVersion && (
        <Notice>
          {t("mods.assumedVersion", {
            version: assumedVersion.label,
            yours: versionLabel(target),
          })}
        </Notice>
      )}
      {modifiedGame && <Notice>{t("mods.modifiedGame")}</Notice>}
      {(incomplete || levelsFor || !!unknownBase) && (
        // Set apart: the mod won't run on its own.
        <div className="grid gap-2 text-sm">
          <p role="status" className="flex items-start gap-2">
            <Puzzle className="mt-0.5 size-4 shrink-0 text-accent" aria-hidden />
            <span>
              {levelsFor ? (
                t("mods.levelsFor", { mod: base.name })
              ) : unknownBase ? (
                t("mods.levelsForUnknown", { mod: unknownBase })
              ) : (
                <Trans
                  i18nKey="mods.missingFiles"
                  context={base ? "mod" : undefined}
                  values={{ mod: base?.name, files: option.missingFiles.join(", ") }}
                  components={{ readme: readme.link }}
                />
              )}
            </span>
          </p>
          {base && (
            <label className="flex items-start gap-2 pl-6">
              <input
                type="checkbox"
                className="mt-1"
                checked={playOver}
                onChange={(e) => setPlayOver(e.target.checked)}
              />
              {t("mods.playOver", {
                mod: base.name,
                context: base.modId ? undefined : "install",
              })}
            </label>
          )}
        </div>
      )}
      {readme.dialog}
      {versionUnchecked && <Notice>{t("mods.versionUnchecked")}</Notice>}

      {install.isPending && base && baseJob && !isFinished(baseJob) && (
        <div className="grid gap-1.5">
          <JobRow job={baseJob} label={base.name} />
        </div>
      )}
      {install.error && <Notice tone="error">{errorMessage(t, install.error)}</Notice>}
      <div className="flex justify-end gap-3">
        <Button variant="ghost" onClick={onCancel}>
          {t(queued ? "drop.skip" : "common.cancel")}
        </Button>
        <Button type="submit" disabled={!ready || install.isPending}>
          {install.isPending
            ? t(replaced ? "mods.replacing" : "mods.installing")
            : replaced
              ? t("mods.replace")
              : warned
                ? t("mods.installAnyway")
                : t("mods.install")}
        </Button>
      </div>
    </form>
  );
}

/**
 * For a `<readme>` in a text: a link opening the mod's documents, plain text
 * when it has none, and the dialog to render next to the text.
 */
function useReadmeLink(path: string): { link: ReactElement; dialog: ReactNode } {
  const { t } = useTranslation();
  const readmes = useReadmesAt(path);
  const [reading, setReading] = useState(false);
  const link = readmes.data?.length ? (
    <button
      type="button"
      className="text-accent underline hover:text-ink"
      onClick={() => setReading(true)}
    />
  ) : (
    <span />
  );
  const dialog = reading && readmes.data && (
    <Dialog title={t("readme.title")} wide onClose={() => setReading(false)}>
      <ReadmePanel path={path} readmes={readmes.data} />
      <div className="flex justify-end">
        <Button variant="ghost" onClick={() => setReading(false)} data-autofocus>
          {t("common.close")}
        </Button>
      </div>
    </Dialog>
  );
  return { link, dialog };
}

/** The hint for picking how to start; "readme" opens the mod's documents, if any. */
function HelpHint({ path }: { path: string }) {
  const readme = useReadmeLink(path);
  return (
    <>
      <p className="text-muted">
        <Trans i18nKey="mods.helpHint" components={{ readme: readme.link }} />
      </p>
      {readme.dialog}
    </>
  );
}

function RecipeChoice({
  info,
  value,
  onChange,
}: {
  info: ModInfo;
  value: number;
  onChange: (index: number) => void;
}) {
  return (
    <div className="mt-2 grid gap-2">
      {info.recipes.map((option, i) => (
        <label key={i} className="flex items-start gap-2">
          <input
            type="radio"
            name="recipe"
            className="mt-1"
            checked={value === i}
            onChange={() => onChange(i)}
          />
          <RecipeLabel option={option} />
        </label>
      ))}
    </div>
  );
}
