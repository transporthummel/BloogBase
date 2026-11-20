import { useTranslation } from "react-i18next";
import type { IndexEntry } from "../bindings";
import { variantName } from "../profile/variants";
import { Badge } from "../ui/Badge";

/** "3 variants" for an index mod that has them, with their names on hover (§9.7). */
export function VariantsBadge({ entry }: { entry: Pick<IndexEntry, "variants"> }) {
  const { t, i18n } = useTranslation();
  const variants = entry.variants ?? [];
  if (variants.length < 2) return null;
  return (
    <Badge title={variants.map((v) => variantName(v, i18n.language)).join(", ")}>
      {t("variants.badge", { count: variants.length })}
    </Badge>
  );
}
