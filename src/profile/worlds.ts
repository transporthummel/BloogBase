import { useQuery } from "@tanstack/react-query";
import type { TFunction } from "i18next";
import { commands, type Profile, type World, type WorldOption, type Worlds } from "../bindings";
import { unwrap } from "../ipc";

export const worldsKey = (profileId: string) => ["worlds", profileId] as const;

/** The worlds of a mod that generates its levels (§9.6); none for others. */
export function useWorlds(profile: Pick<Profile, "id" | "worlds">) {
  return useQuery({
    queryKey: worldsKey(profile.id),
    queryFn: () => unwrap(commands.profileWorlds(profile.id)),
    enabled: profile.worlds,
  });
}

/** The player's name for a world, else its seed, else its number. */
export function worldName(t: TFunction, world: Pick<World, "id" | "name" | "seed">): string {
  if (world.name) return world.name;
  if (world.seed !== null) return t("worlds.seedName", { seed: world.seed });
  return t("worlds.numberName", { number: world.id });
}

/** An option's label in the app's language, else in English. */
export function optionLabel(option: WorldOption, language: string): string {
  return option.label[language.split("-")[0]] ?? option.label.en ?? option.id;
}

/** The value an option has when the player leaves it alone. */
export function optionDefault(option: WorldOption): number {
  return option.type === "flag" ? 0 : (option.default ?? option.min ?? 0);
}

/** The options a world was made with that aren't the defaults, e.g. "Starting ammo: 15". */
export function optionsSummary(definition: Worlds, world: World, language: string): string[] {
  return worldOptions(definition).flatMap((o) => {
    const value = world.options[o.id];
    if (value === undefined || value === optionDefault(o)) return [];
    return [o.type === "flag" ? optionLabel(o, language) : `${optionLabel(o, language)}: ${value}`];
  });
}

/** The generator's options; the index leaves out an empty list. */
export function worldOptions(definition: Worlds): WorldOption[] {
  return definition.options ?? [];
}

/** Whether starting a new world asks for a seed or options first. */
export function asksFirst(definition: Worlds): boolean {
  return !!definition.seed || worldOptions(definition).length > 0;
}
