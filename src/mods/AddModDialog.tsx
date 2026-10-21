import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { FileArchive, FolderOpen } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Install, type KeenwikiOrigin, type Mod, type ModInfo } from "../bindings";
import { bestFit, gameTitle, isModified, useGames } from "../catalog";
import { errorMessage } from "../errors";
import { unwrap } from "../ipc";
import { coverKey } from "../library/covers";
import { installsKey, modsKey, profilesKey } from "../library/queries";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { Notice } from "../ui/Notice";
import { KeenwikiImport } from "./KeenwikiImport";
import type { Downloaded } from "./wikiDownload";
import { RecipeLabel } from "./RecipeLabel";
import { fits } from "./recipes";

/** Get a mod (KeenWiki link, folder or ZIP), confirm what was detected, install it. */
export function AddModDialog({
  dropped,
  downloaded,
  replacing,
  onClose,
  onInstalled,
}: {
  /** A folder or ZIP dropped on the window and inspected already. */
  dropped?: { path: string; info: ModInfo };
  /** A KeenWiki download fetched already, e.g. an update. */
  downloaded?: Downloaded;
  /** The installed mod it updates. */
  replacing?: string;
  onClose: () => void;
  onInstalled: (mod: Mod) => void;
}) {
  const { t } = useTranslation();
  const [path, setPath] = useState<string | null>(null);
  const [fromWiki, setFromWiki] = useState<Downloaded | null>(downloaded ?? null);

  const inspect = useMutation({
    mutationFn: (p: string) => unwrap(commands.inspectMod(p)),
  });

  async function pick(kind: "folder" | "zip") {
    const picked = await open(
      kind === "folder" ? { directory: true } : { filters: [{ name: "ZIP", extensions: ["zip"] }] },
    );
    if (!picked) return;
    setPath(picked);
    inspect.mutate(picked);
  }

  return (
    <Dialog title={t(replacing ? "mods.updateTitle" : "mods.addTitle")} onClose={onClose}>
      {dropped ? (
        <Confirm
          path={dropped.path}
          info={dropped.info}
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
      ) : path && inspect.data ? (
        <Confirm path={path} info={inspect.data} onCancel={onClose} onInstalled={onInstalled} />
      ) : (
        <>
          <KeenwikiImport disabled={inspect.isPending} onDownloaded={setFromWiki} />
          <p className="text-sm text-muted">{t("mods.addHint")}</p>
          <div className="flex flex-wrap gap-3">
            <Button onClick={() => pick("zip")} disabled={inspect.isPending}>
              <FileArchive className="size-4" aria-hidden />
              {t("mods.chooseZip")}
            </Button>
            <Button variant="secondary" onClick={() => pick("folder")} disabled={inspect.isPending}>
              <FolderOpen className="size-4" aria-hidden />
              {t("mods.chooseFolder")}
            </Button>
          </div>
          {inspect.isPending && <Notice>{t("mods.inspecting")}</Notice>}
          {inspect.error && <Notice tone="error">{errorMessage(t, inspect.error)}</Notice>}
          <div className="flex justify-end">
            <Button variant="ghost" onClick={onClose}>
              {t("common.cancel")}
            </Button>
          </div>
        </>
      )}
    </Dialog>
  );
}

function Confirm({
  path,
  info,
  origin = null,
  replacing,
  onCancel,
  onInstalled,
}: {
  path: string;
  info: ModInfo;
  /** Set for a mod from a KeenWiki page, whose details win over the guesses. */
  origin?: KeenwikiOrigin | null;
  /** The installed mod to preselect for replacing. */
  replacing?: string;
  onCancel: () => void;
  onInstalled: (mod: Mod) => void;
}) {
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
  // Another version of a mod the player has: same KeenWiki page, or same game and name.
  const others =
    mods.data?.filter(
      (m) =>
        m.id === replacing ||
        (m.gameId === info.gameId &&
          ((origin && m.keenwikiUrl === origin.pageUrl) ||
            same(m.name, name) ||
            same(m.name, info.name))),
    ) ?? [];
  const replaced =
    replace === null ? undefined : (others.find((m) => m.id === replace) ?? others[0]);
  // A replaced mod stays on its copy of the game when it can.
  const replacedInstall = profiles.data?.find((p) => p.id === replaced?.profileId)?.installId;
  const target =
    candidates.find((i) => i.id === installId) ??
    candidates.find((i) => i.id === replacedInstall) ??
    bestFit(game, candidates);
  // Waits for the mods, so the question about another version isn't skipped.
  const ready = !!installs.data && !!mods.data;
  const versionLabel = (i: Install) => versionOf(i)?.label ?? t("library.unknownVersion");
  // Unknown versions get no warning: they may well be the right one.
  const likelyWrongVersion =
    target && assumedVersion && versionOf(target) !== undefined && !isAssumed(target);

  const install = useMutation({
    mutationFn: () =>
      unwrap(
        commands.installMod({
          path,
          name,
          recipe: option.recipe,
          // Without a fitting game, the player agreed to install anyway.
          installId: target?.id ?? null,
          keenwiki: origin,
          // Renaming may have made it no longer match.
          replace: replaced?.id ?? null,
        }),
      ),
    onSuccess: (mod) => {
      queryClient.invalidateQueries({ queryKey: modsKey });
      // The new version may have another title screen.
      if (replaced) queryClient.invalidateQueries({ queryKey: coverKey(mod.profileId) });
      onInstalled(mod);
    },
  });

  const authors = origin?.authors.length ? origin.authors : info.authors;
  const version = origin?.version ?? info.version;
  const details = [
    authors.length > 0 && t("mods.by", { authors: authors.join(", ") }),
    version && t("mods.version", { version }),
    info.released,
  ].filter(Boolean);

  return (
    <form
      className="grid gap-4"
      onSubmit={(e) => {
        e.preventDefault();
        if (ready) install.mutate();
      }}
    >
      <label className="grid gap-1 text-sm">
        <span className="font-medium">{t("mods.name")}</span>
        <input
          className="rounded-lg border border-border bg-surface-raised px-3 py-2"
          value={name}
          onChange={(e) => setName(e.target.value)}
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
            <p className="text-muted">{t("mods.helpHint")}</p>
            <RecipeChoice info={info} value={recipeIndex} onChange={setRecipeIndex} />
          </fieldset>
        ))}

      {origin?.gameId && origin.gameId !== info.gameId && (
        <Notice>
          {t("keenwiki.otherGame", {
            page: gameTitle(games.data, origin.gameId),
            files: gameTitle(games.data, info.gameId),
          })}
        </Notice>
      )}
      {others.length > 0 && (
        <fieldset className="grid gap-2 text-sm">
          <legend className="mb-1 font-medium">{t("mods.alreadyHave")}</legend>
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
      {installs.data && candidates.length === 0 && (
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
                {i.label} ({versionLabel(i)})
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
      {target && option.recipe.type === "patch" && isModified(versionOf(target)) && (
        <Notice>{t("mods.modifiedGame")}</Notice>
      )}
      {target && target.versionId === null && option.gameVersions.length > 0 && (
        <Notice>{t("mods.versionUnchecked")}</Notice>
      )}

      {install.error && <Notice tone="error">{errorMessage(t, install.error)}</Notice>}
      <div className="flex justify-end gap-3">
        <Button variant="ghost" onClick={onCancel}>
          {t("common.cancel")}
        </Button>
        <Button type="submit" disabled={!ready || install.isPending}>
          {install.isPending
            ? t(replaced ? "mods.replacing" : "mods.installing")
            : replaced
              ? t("mods.replace")
              : target
                ? t("mods.install")
                : t("mods.installAnyway")}
        </Button>
      </div>
    </form>
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
