import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Install, type ModInfo } from "../bindings";
import { useGames } from "../catalog";
import { errorMessage } from "../errors";
import { unwrap } from "../ipc";
import { installsKey, profilesKey, useInstalls } from "../library/queries";
import { Button } from "../ui/Button";
import { Notice } from "../ui/Notice";

export type Pick = "game" | "mod" | "both";

/**
 * A folder or ZIP holding a mod and a release of its game the library lacks:
 * the game alone, the mod alone, or both, the game first. Both is the pick
 * while the library has no copy of the game, else the mod alone.
 */
export function GameAndMod({
  path,
  info,
  queued,
  onCancel,
  onPicked,
}: {
  path: string;
  /** With `bundledGame` set. */
  info: ModInfo;
  queued?: boolean;
  onCancel: () => void;
  /** After the game was added, for "game" and "both". */
  onPicked: (pick: Pick, games: Install[]) => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const games = useGames();
  const installs = useInstalls();
  const game = games.data?.find((g) => g.id === info.gameId);
  const version = game?.versions.find((v) => v.id === info.bundledGame);
  const [picked, setPicked] = useState<Pick | null>(null);
  const pick = picked ?? (installs.data?.some((i) => i.gameId === info.gameId) ? "mod" : "both");

  const add = useMutation({
    mutationFn: async (p: Pick) => {
      if (p === "mod") return [];
      const added = await unwrap(commands.importBundledGame(path, info.bundledGame ?? ""));
      queryClient.invalidateQueries({ queryKey: installsKey });
      queryClient.invalidateQueries({ queryKey: profilesKey });
      return added;
    },
    onSuccess: (added, p) => onPicked(p, added),
  });

  const options: [Pick, string][] = [
    ["game", t("mods.gameAndMod.game")],
    ["mod", t("mods.gameAndMod.mod")],
    ["both", t("mods.gameAndMod.both")],
  ];
  return (
    <form
      className="grid gap-4"
      onSubmit={(e) => {
        e.preventDefault();
        if (installs.data && !add.isPending) add.mutate(pick);
      }}
    >
      <p>{t("mods.gameAndMod.found")}</p>
      <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
        <dt className="text-muted">{t("mods.gameAndMod.gameLabel")}</dt>
        <dd>
          {game?.title ?? info.gameId}
          {version && ` (${version.label})`}
        </dd>
        <dt className="text-muted">{t("mods.gameAndMod.modLabel")}</dt>
        <dd>{info.name}</dd>
      </dl>
      <fieldset className="grid gap-2 text-sm">
        <legend className="mb-1 font-medium">{t("mods.gameAndMod.question")}</legend>
        {options.map(([value, label]) => (
          <label key={value} className="flex items-center gap-2">
            <input
              type="radio"
              name="game-and-mod"
              checked={pick === value}
              onChange={() => setPicked(value)}
            />
            {label}
          </label>
        ))}
      </fieldset>
      {add.error && <Notice tone="error">{errorMessage(t, add.error)}</Notice>}
      <div className="flex justify-end gap-3">
        <Button variant="ghost" onClick={onCancel}>
          {t(queued ? "drop.skip" : "common.cancel")}
        </Button>
        <Button type="submit" data-autofocus disabled={!installs.data || add.isPending}>
          {add.isPending ? t("mods.gameAndMod.adding") : t("mods.gameAndMod.next")}
        </Button>
      </div>
    </form>
  );
}
