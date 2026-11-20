import { useQuery } from "@tanstack/react-query";
import { commands, type Profile, type Variant } from "../bindings";
import { unwrap } from "../ipc";

export const variantsKey = (profileId: string) => ["variants", profileId] as const;

/** The variants of an index mod (§9.7); none for others. */
export function useVariants(profile: Pick<Profile, "id" | "variantCount">) {
  return useQuery({
    queryKey: variantsKey(profile.id),
    queryFn: () => unwrap(commands.listVariants(profile.id)),
    enabled: profile.variantCount > 0,
  });
}

/** A variant's name in the app's language, else in English. */
export function variantName(variant: Pick<Variant, "name" | "translations">, language: string) {
  return variant.translations?.[language.split("-")[0]]?.name ?? variant.name;
}

/** A variant's description in the app's language, else in English. */
export function variantDescription(
  variant: Pick<Variant, "description" | "translations">,
  language: string,
) {
  return variant.translations?.[language.split("-")[0]]?.description ?? variant.description;
}
