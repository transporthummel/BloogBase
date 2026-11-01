import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Compass,
  FileArchive,
  FolderOpen,
  PackagePlus,
  RefreshCw,
  Replace,
  Search,
} from "lucide-react";
import { type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Dropped, type Install, type Mod, type ModInfo } from "../bindings";
import { useIndexInstalled, useIndexMods } from "../discover/queries";
import { errorMessage } from "../errors";
import { unwrap } from "../ipc";
import { importPaths } from "../library/importPaths";
import { installsKey, profilesKey } from "../library/queries";
import { Button } from "../ui/Button";
import { buttonClass } from "../ui/buttonClass";
import { Dialog } from "../ui/Dialog";
import { Notice } from "../ui/Notice";
import { IndexHit, KeenwikiImport, OkButton } from "./KeenwikiImport";
import type { Downloaded } from "./wikiDownload";
import { SetUp, type SetUpProps } from "./SetUp";

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
  onFindOwned,
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
  /** Offers to look for games from Steam and GOG instead. */
  onFindOwned?: () => void;
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
          {/* Bought later: the store search instead of finding the folder by hand. */}
          {!replacing && onFindOwned && (
            <p className="flex flex-wrap items-center gap-x-3 gap-y-2 border-t border-border pt-3 text-sm text-muted">
              {t("mods.ownedHint")}
              <Button variant="secondary" onClick={onFindOwned}>
                <Search className="size-4" aria-hidden />
                {t("library.findOwned")}
              </Button>
            </p>
          )}
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

/** A file that is an index download installs as that entry, unless the player sets it up. */
function Confirm(props: SetUpProps) {
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
