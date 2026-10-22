import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { FolderInput } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Install, type Mod, type ModInfo } from "../bindings";
import { errorMessage } from "../errors";
import { unwrap } from "../ipc";
import { AddModDialog } from "../mods/AddModDialog";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { Notice } from "../ui/Notice";
import { installsKey, profilesKey } from "./queries";

type Outcome = {
  games: Install[];
  mods: Mod[];
  /** File name → what went wrong. */
  failed: [string, string][];
};

const fileName = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path;

/**
 * Folders and ZIP files dropped on the window: games are imported, mods go
 * through the usual confirmation, one after the other.
 */
export function DropImport() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [hovering, setHovering] = useState(false);
  const [busy, setBusy] = useState(false);
  const [mods, setMods] = useState<{ path: string; info: ModInfo }[]>([]);
  const [outcome, setOutcome] = useState<Outcome | null>(null);

  useEffect(() => {
    async function handle(paths: string[]) {
      setBusy(true);
      const result: Outcome = { games: [], mods: [], failed: [] };
      const found: { path: string; info: ModInfo }[] = [];
      for (const path of paths) {
        try {
          const dropped = await unwrap(commands.inspectDropped(path));
          if (dropped.kind === "games") {
            result.games.push(...(await unwrap(commands.importGames(path))));
          } else if (dropped.kind === "mod") {
            found.push({ path, info: dropped.info });
          } else {
            result.failed.push([fileName(path), t("drop.nothing")]);
          }
        } catch (err) {
          result.failed.push([fileName(path), errorMessage(t, err)]);
        }
      }
      if (result.games.length > 0) {
        queryClient.invalidateQueries({ queryKey: profilesKey });
        queryClient.invalidateQueries({ queryKey: installsKey });
      }
      setBusy(false);
      setMods(found);
      setOutcome(result);
    }

    const unlisten = getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (payload.type === "enter" || payload.type === "over") setHovering(true);
      else setHovering(false);
      if (payload.type === "drop" && payload.paths.length > 0) handle(payload.paths);
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, [t, queryClient]);

  const nextMod = (installed?: Mod) => {
    if (installed) {
      queryClient.invalidateQueries({ queryKey: profilesKey });
      setOutcome((o) => o && { ...o, mods: [...o.mods, installed] });
    }
    setMods((m) => m.slice(1));
  };
  const done = outcome && mods.length === 0;
  const anything =
    outcome && (outcome.games.length || outcome.mods.length || outcome.failed.length);
  const single = outcome && outcome.games.length + outcome.mods.length === 1;

  async function openSingle({ games, mods }: Outcome) {
    setOutcome(null);
    let profileId: string | undefined = mods[0]?.profileId;
    if (!profileId) {
      const profiles = await queryClient
        .fetchQuery({
          queryKey: profilesKey,
          queryFn: () => unwrap(commands.listProfiles()),
        })
        .catch(() => []);
      // The game's own play profile, not one of a mod played on it.
      profileId = profiles.find((p) => p.installId === games[0].id && !p.modId)?.id;
    }
    if (profileId) navigate({ to: "/profiles/$profileId", params: { profileId } });
  }

  return (
    <>
      {(hovering || busy) && (
        <div
          role="status"
          className="pointer-events-none fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-6"
        >
          <div className="grid max-w-md justify-items-center gap-3 rounded-xl border-2 border-dashed border-accent bg-surface p-8 text-center shadow-card">
            <FolderInput className="size-10 text-accent" aria-hidden />
            <p className="font-semibold">{busy ? t("drop.working") : t("drop.hint")}</p>
          </div>
        </div>
      )}
      {mods[0] && (
        <AddModDialog
          key={mods[0].path}
          dropped={mods[0]}
          onClose={() => nextMod()}
          onInstalled={nextMod}
        />
      )}
      {done && anything ? (
        <Dialog title={t("drop.title")} onClose={() => setOutcome(null)}>
          <div className="grid gap-2">
            {outcome.games.length > 0 && (
              <Notice>{t("library.imported", { count: outcome.games.length })}</Notice>
            )}
            {outcome.mods.map((mod) => (
              <Notice key={mod.id}>{t("mods.installed", { name: mod.name })}</Notice>
            ))}
            {outcome.failed.map(([name, message]) => (
              <Notice key={name} tone="error">
                <span className="font-semibold">{name}:</span> {message}
              </Notice>
            ))}
          </div>
          <div className="flex justify-end gap-2">
            <Button variant={single ? "secondary" : "primary"} onClick={() => setOutcome(null)}>
              {t("common.close")}
            </Button>
            {single && (
              <Button data-autofocus onClick={() => openSingle(outcome)}>
                {t("discover.openDetails")}
              </Button>
            )}
          </div>
        </Dialog>
      ) : null}
    </>
  );
}
