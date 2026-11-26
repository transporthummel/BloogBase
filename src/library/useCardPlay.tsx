import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useLayoutEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { commands, events, type Game, type Profile, type WorldStart } from "../bindings";
import { unwrap } from "../ipc";
import { useControlsGate } from "../profile/controlsGate";
import { useVariantGate } from "../profile/variantGate";
import { variantsKey } from "../profile/variants";
import { worldsKey } from "../profile/worlds";
import { profilesKey } from "./queries";

/**
 * Play for cards of play profiles, as in the library: the keys first on a
 * series' first start, the variant asked for until one is chosen. `dialogs`
 * go on the page.
 */
export function useCardPlay(games: Game[] | undefined) {
  const { i18n } = useTranslation();
  const queryClient = useQueryClient();

  useEffect(() => {
    const unlisten = events.playSessionEnded.listen(() =>
      queryClient.invalidateQueries({ queryKey: profilesKey }),
    );
    return () => {
      unlisten.then((f) => f());
    };
  }, [queryClient]);

  const launch = useMutation({
    mutationFn: ({
      profileId,
      world,
      variant,
    }: {
      profileId: string;
      world: WorldStart | null;
      variant: string | null;
    }) => unwrap(commands.launchProfile(profileId, world, variant, i18n.language)),
    onSettled: (_, __, { profileId }) => {
      queryClient.invalidateQueries({ queryKey: profilesKey });
      // The arrow beside Play lists the other worlds and variants.
      queryClient.invalidateQueries({ queryKey: worldsKey(profileId) });
      queryClient.invalidateQueries({ queryKey: variantsKey(profileId) });
    },
  });
  // The world and variant picked, for once the controls have been shown.
  const world = useRef<WorldStart | null>(null);
  const variant = useRef<string | null>(null);
  // The first start of a series shows its keys first.
  const gate = useControlsGate((profileId) =>
    launch.mutate({ profileId, world: world.current, variant: variant.current }),
  );
  // A mod with variants asks which one until the player has chosen (§9.7).
  const variants = useVariantGate((profile, picked) => {
    variant.current = picked;
    gate.play(
      profile,
      games?.find((g) => g.id === profile.gameId),
    );
  });
  // The same function on every render, so typing in the library's search
  // doesn't redraw every card: the cards' memo only holds with it.
  const playLatest = useRef((profile: Profile, start: WorldStart | null, pick?: string) => {
    world.current = start;
    variants.play(profile, pick);
  });
  useLayoutEffect(() => {
    playLatest.current = (profile, start, pick) => {
      world.current = start;
      variants.play(profile, pick);
    };
  });
  const play = useCallback(
    (profile: Profile, start: WorldStart | null, pick?: string) =>
      playLatest.current(profile, start, pick),
    [],
  );

  return {
    play,
    launch,
    dialogs: (
      <>
        {gate.dialog}
        {variants.dialog}
      </>
    ),
  };
}
