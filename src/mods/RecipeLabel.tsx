import { useTranslation } from "react-i18next";
import type { RecipeOption } from "../bindings";
import { recipeCommand } from "./recipes";

export function RecipeLabel({ option }: { option: RecipeOption }) {
  const { t } = useTranslation();
  const command = recipeCommand(option.recipe);
  return (
    <span className="grid gap-0.5">
      <span>{t(`mods.recipe.${option.recipe.type}`)}</span>
      {command && <code className="text-xs text-muted">{command}</code>}
      {option.origin && option.recipe.type !== "batch" && (
        <span className="text-xs text-muted">{t("mods.from", { file: option.origin })}</span>
      )}
    </span>
  );
}
