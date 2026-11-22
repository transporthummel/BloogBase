import { useMutation, useQueryClient } from "@tanstack/react-query";
import { CircleCheck, Play } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Profile } from "../bindings";
import { errorMessage } from "../errors";
import { unwrap } from "../ipc";
import { coverKey } from "../library/covers";
import { profilesKey } from "../library/queries";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { Notice } from "../ui/Notice";
import { RelativeTime } from "../ui/RelativeTime";
import { backupsKey, savesKey } from "./saves";
import { VariantCover } from "./VariantDialog";
import { useVariants, variantDescription, variantName, variantsKey } from "./variants";

/**
 * The ways to play a mod (§9.7), in its entry's order: each with its
 * cover, description, saves and last start; choose one without playing,
 * or play it.
 */
export function VariantsPanel({
  profile,
  canPlay,
  onPlay,
  seekActive = false,
  onSought,
}: {
  profile: Profile;
  canPlay: boolean;
  onPlay: (variant: string) => void;
  /** Scrolls to the variant played once the list is there, then says so. */
  seekActive?: boolean;
  onSought?: () => void;
}) {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const variants = useVariants(profile);
  const [message, setMessage] = useState<string | null>(null);
  const choose = useMutation({
    mutationFn: (id: string) => unwrap(commands.setVariant(profile.id, id)),
    onSuccess: (_, id) => {
      const v = variants.data?.variants.find((x) => x.variant.id === id);
      if (v) setMessage(t("variants.chosen", { name: variantName(v.variant, i18n.language) }));
      queryClient.invalidateQueries({ queryKey: variantsKey(profile.id) });
      queryClient.invalidateQueries({ queryKey: profilesKey });
      queryClient.invalidateQueries({ queryKey: savesKey(profile.id) });
      queryClient.invalidateQueries({ queryKey: backupsKey(profile.id) });
      queryClient.invalidateQueries({ queryKey: coverKey(profile.id) });
    },
  });
  const data = variants.data;
  const error = variants.error ?? choose.error;
  const locked = profile.running;
  const activeRow = useRef<HTMLLIElement>(null);

  useEffect(() => {
    if (!seekActive || !data) return;
    const still = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    activeRow.current?.scrollIntoView({ behavior: still ? "auto" : "smooth", block: "center" });
    onSought?.();
  }, [seekActive, data, onSought]);

  return (
    <div className="grid gap-4">
      {locked && <Notice>{t("variants.locked")}</Notice>}
      {message && !error && <Notice>{message}</Notice>}
      {error && <Notice tone="error">{errorMessage(t, error)}</Notice>}
      <p className="max-w-prose text-sm text-muted">{t("variants.intro")}</p>
      <ul className="grid gap-2 @6xl:grid-cols-2">
        {data?.variants.map(({ variant: v, active, saves, lastPlayedAt }) => {
          const name = variantName(v, i18n.language);
          const description = variantDescription(v, i18n.language);
          return (
            <li key={v.id} ref={active ? activeRow : undefined}>
              <Card className="flex flex-wrap items-center gap-3 p-3">
                {!data.sameCovers && (
                  // Large enough to see the artwork, yet a row.
                  <VariantCover profile={profile} variant={v.id} className="mr-3 w-40" />
                )}
                <div className="grid min-w-0 flex-1 gap-0.5">
                  {/* Next below the page's title: the tab has no heading of its own. */}
                  <h2 className="flex flex-wrap items-center gap-2 text-sm font-semibold">
                    <span className="select-text">{name}</span>
                    {active && <Badge>{t("variants.current")}</Badge>}
                  </h2>
                  {description && <p className="text-xs text-muted select-text">{description}</p>}
                  <p className="text-xs text-muted">
                    {lastPlayedAt !== null ? (
                      <RelativeTime at={lastPlayedAt}>
                        {(date) => t("variants.lastStart", { date })}
                      </RelativeTime>
                    ) : (
                      t("variants.notPlayed")
                    )}
                    {saves > 0 && ` · ${t("variants.saves", { count: saves })}`}
                  </p>
                </div>
                <div className="flex gap-1">
                  <Button
                    variant="secondary"
                    disabled={!canPlay || locked}
                    onClick={() => onPlay(v.id)}
                    aria-label={t("variants.playNamed", { name })}
                  >
                    <Play className="size-4" aria-hidden />
                    {t("variants.play")}
                  </Button>
                  <Button
                    variant="ghost"
                    className="px-2"
                    disabled={active || locked || choose.isPending}
                    onClick={() => choose.mutate(v.id)}
                    aria-label={t(active ? "variants.isCurrentNamed" : "variants.chooseNamed", {
                      name,
                    })}
                    title={t(active ? "variants.isCurrent" : "variants.choose")}
                  >
                    <CircleCheck className="size-4" aria-hidden />
                  </Button>
                </div>
              </Card>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
