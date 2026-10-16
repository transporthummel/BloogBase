import type { Install, ModInfo, Recipe, RecipeOption } from "../bindings";

/** The DOS command a recipe runs, or null when it starts the episode itself. */
export function recipeCommand(recipe: Recipe): string | null {
  switch (recipe.type) {
    case "patch":
      return [recipe.patcher, recipe.patch, ...recipe.args].join(" ");
    case "program":
      return [recipe.program, ...recipe.args].join(" ");
    case "batch":
      return recipe.file;
    case "episode":
      return null;
  }
}

/**
 * Whether `option` can run on `install`. Unknown versions get the benefit of
 * the doubt; a known version the patch doesn't name doesn't fit.
 */
export function fits(info: ModInfo, option: RecipeOption, install: Install) {
  return (
    install.gameId === info.gameId &&
    (option.gameVersions.length === 0 ||
      install.versionId === null ||
      option.versionIds.includes(install.versionId))
  );
}
