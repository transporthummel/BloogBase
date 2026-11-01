import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import {
  ArrowLeftRight,
  Bug,
  ChevronDown,
  Download,
  FileArchive,
  FolderOpen,
  Keyboard,
  MonitorCog,
  PackagePlus,
  RefreshCw,
  Trash2,
} from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Game, type Install, type Mod, type Profile } from "../bindings";
import { useFreeGames } from "../discover/queries";
import { errorMessage } from "../errors";
import { unwrap } from "../ipc";
import { FindOwnedDialog } from "../library/FindOwnedDialog";
import { forgetRemovedCovers } from "../library/covers";
import { installsKey, modsKey, profilesKey } from "../library/queries";
import { AddModDialog } from "../mods/AddModDialog";
import { SuggestToIndex } from "../mods/KeenwikiImport";
import { ModReportDialog } from "../mods/ModReport";
import { ModUpdate } from "../mods/ModUpdate";
import { useModUpdateCheck } from "../mods/modUpdateCheck";
import { RemoveModDialog } from "../mods/RemoveModDialog";
import { Button } from "../ui/Button";
import { buttonClass } from "../ui/buttonClass";
import { Dialog } from "../ui/Dialog";
import { Menu, type MenuItem } from "../ui/Menu";
import { Notice } from "../ui/Notice";
import { PlayButton } from "../ui/PlayButton";
import { oneButtonFiringKey, profileFiringKey } from "./firing";
import { RemoveGameDialog } from "./RemoveGameDialog";

type Open = "removing" | "replacing" | "reporting" | "picking" | null;

/** Play, the More menu with its dialogs, and what the player should know before playing. */
export function ProfileActions({
  profile,
  game,
  mod,
  canPlay,
  onPlay,
  modCount,
}: {
  profile: Profile;
  game?: Game;
  mod?: Mod;
  canPlay: boolean;
  onPlay: () => void;
  /** For a game: the mods played on it. */
  modCount: number;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [open, setOpen] = useState<Open>(null);
  const close = () => setOpen(null);
  const openFolder = useMutation({
    mutationFn: (of: { modId: string } | { installId: string }) =>
      unwrap(
        "modId" in of ? commands.openModFolder(of.modId) : commands.openGameFolder(of.installId),
      ),
  });
  const installs = useQuery({
    // Under the installs key, so it follows changes to the library.
    queryKey: [...installsKey, "mod", profile.modId],
    queryFn: () => unwrap(commands.modInstalls(profile.modId!)),
    enabled: profile.modId !== null,
  });
  const check = useModUpdateCheck(mod);
  // The index or the KeenWiki page may have a newer version.
  const checkable = mod && (mod.indexId || mod.keenwikiUrl);

  const actions: MenuItem[] = [];
  if (mod && installs.data && installs.data.length > 1)
    actions.push({
      label: t("profile.playOn"),
      icon: <ArrowLeftRight className="size-4" />,
      onSelect: () => setOpen("picking"),
      disabled: profile.running,
    });
  if (mod) {
    actions.push({
      label: t("profile.openModFolder"),
      icon: <FolderOpen className="size-4" />,
      onSelect: () => openFolder.mutate({ modId: mod.id }),
    });
    if (checkable)
      actions.push({
        label: t("modUpdate.check"),
        icon: <RefreshCw className="size-4" />,
        onSelect: () => check.mutate(),
        disabled: check.isPending,
      });
    actions.push({
      label: t("profile.replaceMod"),
      icon: <FileArchive className="size-4" />,
      onSelect: () => setOpen("replacing"),
      disabled: profile.running,
    });
    // Right after a first start that went wrong, the game's end offers it too.
    if (profile.sessionCount > 0)
      actions.push({
        label: t("modReport.button"),
        icon: <Bug className="size-4" />,
        onSelect: () => setOpen("reporting"),
      });
  }
  // A game's own files: the full game, shareware episode or demo.
  if (profile.modId === null && profile.installId !== null) {
    const installId = profile.installId;
    actions.push({
      label: t("profile.openGameFolder"),
      icon: <FolderOpen className="size-4" />,
      onSelect: () => openFolder.mutate({ installId }),
    });
  }
  actions.push({
    label: mod ? t("mods.remove") : t("profile.removeGame"),
    icon: <Trash2 className="size-4" />,
    onSelect: () => setOpen("removing"),
    disabled: profile.running,
    danger: true,
    separated: actions.length > 0,
  });

  return (
    <div className="grid gap-3">
      <div className="flex items-stretch gap-2">
        <PlayButton
          onClick={onPlay}
          disabled={!canPlay || profile.running || profile.missingGame !== null}
          className="min-w-44 px-8 py-3 text-base"
        >
          {profile.missingGame !== null
            ? t("mods.gameMissing")
            : profile.running
              ? t("library.running")
              : t("library.play")}
        </PlayButton>
        <Menu
          label={t("profile.more")}
          trigger={
            <>
              {t("profile.more")}
              <ChevronDown className="size-4" aria-hidden />
            </>
          }
          className="px-4 text-base"
          items={actions}
        />
      </div>
      {profile.missingGame !== null && (
        <MissingGame profile={profile} missingGame={profile.missingGame} game={game} />
      )}
      {checkable && <ModUpdate mod={mod} check={check} disabled={profile.running} />}
      {mod?.source === "keenwiki" && !mod.indexId && (
        <p className="text-sm">
          <SuggestToIndex modId={mod.id} />
        </p>
      )}
      <FiringNote profileId={profile.id} />
      <MachineNote profileId={profile.id} />
      {openFolder.error && <Notice tone="error">{errorMessage(t, openFolder.error)}</Notice>}

      {open === "removing" && profile.modId === null && profile.installId !== null && (
        <RemoveGameDialog
          profile={profile}
          installId={profile.installId}
          modCount={modCount}
          onClose={close}
          onRemoved={() => {
            forgetRemovedCovers(queryClient);
            queryClient.invalidateQueries({ queryKey: modsKey });
            queryClient.invalidateQueries({ queryKey: installsKey });
            navigate({ to: "/" });
          }}
        />
      )}
      {open === "removing" && profile.modId !== null && (
        <RemoveModDialog
          profile={{ ...profile, modId: profile.modId }}
          onClose={close}
          onRemoved={() => {
            // With the mods played over it, removed with it.
            forgetRemovedCovers(queryClient);
            queryClient.invalidateQueries({ queryKey: modsKey });
            navigate({ to: "/" });
          }}
        />
      )}
      {open === "reporting" && (
        <ModReportDialog profileId={profile.id} name={profile.label} onClose={close} />
      )}
      {open === "replacing" && mod && (
        <AddModDialog
          replacing={mod.id}
          onClose={close}
          onInstalled={() => {
            close();
            queryClient.invalidateQueries({ queryKey: profilesKey });
          }}
        />
      )}
      {open === "picking" && mod && profile.installId !== null && installs.data && (
        <GamePicker
          modId={mod.id}
          installId={profile.installId}
          installs={installs.data}
          game={game}
          disabled={profile.running}
          onClose={close}
        />
      )}
    </div>
  );
}

/** What a mod waits for, and where to get it: free shareware from Discover, else the add dialog. */
function MissingGame({
  profile,
  missingGame,
  game,
}: {
  profile: Profile;
  missingGame: string;
  game?: Game;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const free = useFreeGames();
  const [adding, setAdding] = useState(false);
  const [finding, setFinding] = useState(false);
  // A full shareware episode, not a demo or beta.
  const shareware = (free.data ?? []).some(
    (f) =>
      f.gameId === profile.gameId &&
      (game?.versions.find((v) => v.id === f.versionId)?.kind ?? "release") === "release",
  );
  return (
    <div className="grid justify-items-start gap-2">
      <Notice>{t("mods.waitingHint", { game: missingGame })}</Notice>
      {shareware ? (
        <Link to="/discover" search={{ tab: "games" }} className={buttonClass("secondary")}>
          <Download className="size-4" aria-hidden />
          {t("mods.getFreeGame")}
        </Link>
      ) : (
        <Button variant="secondary" onClick={() => setAdding(true)}>
          <PackagePlus className="size-4" aria-hidden />
          {t("mods.addGame")}
        </Button>
      )}
      {adding && (
        <AddModDialog
          onClose={() => setAdding(false)}
          onInstalled={(mod) => {
            setAdding(false);
            queryClient.invalidateQueries({ queryKey: profilesKey });
            navigate({ to: "/profiles/$profileId", params: { profileId: mod.profileId } });
          }}
          // The mod's profile takes the game as soon as the list is fetched again.
          onGamesImported={() => setAdding(false)}
          onFindOwned={() => {
            setAdding(false);
            setFinding(true);
          }}
        />
      )}
      {finding && (
        <FindOwnedDialog onClose={() => setFinding(false)} onImported={() => setFinding(false)} />
      )}
    </div>
  );
}

/** Which copy of the game a mod plays on, when more than one fits. */
function GamePicker({
  modId,
  installId,
  installs,
  game,
  disabled,
  onClose,
}: {
  modId: string;
  installId: string;
  installs: Install[];
  game?: Game;
  disabled: boolean;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const change = useMutation({
    mutationFn: (id: string) => unwrap(commands.setModInstall(modId, id)),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: profilesKey });
      queryClient.invalidateQueries({ queryKey: profileFiringKey(modId) });
    },
  });
  const versionLabel = (versionId: string | null) =>
    game?.versions.find((v) => v.id === versionId)?.label ?? t("library.unknownVersion");

  return (
    <Dialog title={t("mods.playOn")} onClose={onClose}>
      <select
        aria-label={t("mods.playOn")}
        data-autofocus
        className="min-w-0 truncate rounded-lg border border-border bg-surface-raised px-2 py-1.5 text-sm"
        value={installId}
        disabled={disabled || change.isPending}
        onChange={(e) => change.mutate(e.target.value)}
      >
        {installs.map((i) => (
          <option key={i.id} value={i.id}>
            {game?.shortTitle ?? i.label} ({versionLabel(i.versionId)})
          </option>
        ))}
      </select>
      <p className="text-sm text-muted">{t("mods.changeGameHint")}</p>
      {change.error && <Notice tone="error">{errorMessage(t, change.error)}</Notice>}
      <div className="flex justify-end">
        <Button variant="secondary" onClick={onClose}>
          {t("common.close")}
        </Button>
      </div>
    </Dialog>
  );
}

/** How Keen 1-3 shoot here, where it isn't the usual Ctrl and Alt or can't be changed. */
function FiringNote({ profileId }: { profileId: string }) {
  const { t } = useTranslation();
  const enabled = useQuery({
    queryKey: oneButtonFiringKey,
    queryFn: () => unwrap(commands.oneButtonFiring()),
  });
  const status = useQuery({
    queryKey: profileFiringKey(profileId),
    queryFn: () => unwrap(commands.profileFiring(profileId)),
  });
  const s = status.data;
  // A mod's own one-button firing is worth knowing even with the setting off.
  if (!s || (s !== "builtIn" && !enabled.data)) return null;
  return (
    <p className="flex items-start gap-1.5 text-sm text-muted">
      <Keyboard className="mt-0.5 size-4 shrink-0" aria-hidden />
      {t(`profile.firing.${s === "patchable" ? "on" : s}`)}
    </p>
  );
}

/** The machine DOSBox runs as for a mod that needs another than the usual one. */
function MachineNote({ profileId }: { profileId: string }) {
  const { t } = useTranslation();
  const machine = useQuery({
    queryKey: ["machine", profileId],
    queryFn: () => unwrap(commands.profileMachine(profileId)),
  });
  if (!machine.data) return null;
  return (
    <p className="flex items-start gap-1.5 text-sm text-muted">
      <MonitorCog className="mt-0.5 size-4 shrink-0" aria-hidden />
      {t(`profile.machine.${machine.data}`)}
    </p>
  );
}
