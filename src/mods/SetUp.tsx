import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { LibraryBig, Puzzle } from "lucide-react";
import { type ReactElement, type ReactNode, useRef, useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import {
  commands,
  type Game,
  type Install,
  type KeenwikiOrigin,
  type Mod,
  type ModInfo,
  type RecipeOption,
} from "../bindings";
import { gameTitle, shortGameTitle, useGames } from "../catalog";
import { errorMessage } from "../errors";
import { formatLooseDate } from "../format";
import { unwrap } from "../ipc";
import { JobRow } from "../jobs/JobList";
import { isFinished, useJob, useStartJob, waitForJob } from "../jobs/queries";
import { coverKey } from "../library/covers";
import { installsKey, modsKey, profilesKey } from "../library/queries";
import { ReadmePanel } from "../profile/ReadmePanel";
import { readmeKey, useReadmesAt } from "../profile/readme";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { Notice } from "../ui/Notice";
import { useEnterAction } from "../ui/useEnterAction";
import { RecipeLabel } from "./RecipeLabel";
import { fits } from "./recipes";
import { otherVersions, pickTarget, setUpWarnings } from "./setUpChecks";

export type SetUpProps = {
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

/** What was detected, the choices left to the player, and Install. */
export function SetUp({
  path,
  info,
  origin = null,
  replacing,
  queued,
  onCancel,
  onInstalled,
}: SetUpProps) {
  const { t } = useTranslation();
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
  const candidates = installs.data?.filter((i) => fits(info, option, i));
  const others = otherVersions(mods.data ?? [], info, name, origin, replacing);
  // Picked to replace it, but for another game.
  const wrongGame = mods.data?.find((m) => m.id === replacing && m.gameId !== info.gameId);
  const replaced =
    replace === null ? undefined : (others.find((m) => m.id === replace) ?? others[0]);
  // A replaced mod stays on its copy of the game when it can.
  const replacedInstall = profiles.data?.find((p) => p.id === replaced?.profileId)?.installId;
  const target = pickTarget(candidates ?? [], installId, replacedInstall, game);
  // Waits for the mods, so the question about another version isn't skipped.
  const ready = !!installs.data && !!mods.data && !wrongGame;
  const warnings = setUpWarnings({ info, option, origin, game, candidates, target });

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
  const choose = (info.recipes.length > 1 && !info.confident) || (others.length > 0 && !replacing);
  const careful =
    choose ||
    warnings.otherGame ||
    warnings.missingGame ||
    !!warnings.assumedInstead ||
    warnings.modifiedGame ||
    warnings.versionUnchecked ||
    warnings.incomplete ||
    warnings.levelsFor ||
    !!warnings.unknownBase;
  const form = useRef<HTMLFormElement>(null);
  const submit = () => {
    if (ready && !install.isPending) install.mutate();
  };
  useEnterAction(form, careful ? null : submit);
  // Warned that it may not run as installed.
  const warned =
    warnings.missingGame ||
    !!warnings.assumedInstead ||
    warnings.modifiedGame ||
    (warnings.incomplete && !overBase) ||
    !!warnings.unknownBase;
  const versionLabel = (i: Install) =>
    game?.versions.find((v) => v.id === i.versionId)?.label ?? t("library.unknownVersion");

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

      <Facts info={info} option={option} origin={origin} game={game} />

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

      {warnings.otherGame && origin?.gameId && (
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
        <ReplaceChoice others={others} replaced={replaced} onChange={setReplace} />
      )}
      {warnings.missingGame && (
        <Notice>
          {t("mods.missingGame", {
            game: game?.title ?? info.gameId,
            versions: option.gameVersions.map((v) => `v${v}`).join(", "),
            context: option.gameVersions.length > 0 ? "version" : undefined,
          })}
        </Notice>
      )}
      {candidates && candidates.length > 1 && (
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
      {warnings.assumedInstead && target && (
        <Notice>
          {t("mods.assumedVersion", {
            version: warnings.assumedInstead.label,
            yours: versionLabel(target),
          })}
        </Notice>
      )}
      {warnings.modifiedGame && <Notice>{t("mods.modifiedGame")}</Notice>}
      {(warnings.incomplete || warnings.levelsFor || !!warnings.unknownBase) && (
        <BaseNotice
          path={path}
          option={option}
          levelsFor={warnings.levelsFor}
          unknownBase={warnings.unknownBase}
          playOver={playOver}
          onPlayOver={setPlayOver}
        />
      )}
      {warnings.versionUnchecked && <Notice>{t("mods.versionUnchecked")}</Notice>}

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

/** Game, versions it's made for, authors, version, release and file count. */
function Facts({
  info,
  option,
  origin,
  game,
}: {
  info: ModInfo;
  option: RecipeOption;
  origin: KeenwikiOrigin | null;
  game?: Game;
}) {
  const { t, i18n } = useTranslation();
  const authors = origin?.authors.length ? origin.authors : info.authors;
  const version = origin?.version ?? info.version;
  const details = [
    authors.length > 0 && t("mods.by", { authors: authors.join(", ") }),
    version && t("mods.version", { version }),
    info.released && formatLooseDate(i18n.language, info.released),
  ].filter(Boolean);
  return (
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
  );
}

/** Replace one of the other versions the player has, or install alongside. */
function ReplaceChoice({
  others,
  replaced,
  onChange,
}: {
  others: Mod[];
  replaced: Mod | undefined;
  /** A mod's id, `null` for alongside. */
  onChange: (id: string | null) => void;
}) {
  const { t } = useTranslation();
  return (
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
            onChange={() => onChange(m.id)}
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
          onChange={() => onChange(null)}
        />
        <span>{t("mods.alongside")}</span>
      </label>
    </fieldset>
  );
}

/** A mod that won't run on its own: what it needs, and whether to play it over that. */
function BaseNotice({
  path,
  option,
  levelsFor,
  unknownBase,
  playOver,
  onPlayOver,
}: {
  path: string;
  option: RecipeOption;
  levelsFor: boolean;
  unknownBase: string | null;
  playOver: boolean;
  onPlayOver: (playOver: boolean) => void;
}) {
  const { t } = useTranslation();
  const readme = useReadmeLink(path);
  const base = option.baseMod;
  return (
    // Set apart: the mod won't run on its own.
    <div className="grid gap-2 text-sm">
      <p role="status" className="flex items-start gap-2">
        <Puzzle className="mt-0.5 size-4 shrink-0 text-accent" aria-hidden />
        <span>
          {levelsFor && base ? (
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
            onChange={(e) => onPlayOver(e.target.checked)}
          />
          {t("mods.playOver", {
            mod: base.name,
            context: base.modId ? undefined : "install",
          })}
        </label>
      )}
      {readme.dialog}
    </div>
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
