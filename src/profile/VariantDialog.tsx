import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { Profile, VariantState } from "../bindings";
import { errorMessage } from "../errors";
import { Cover } from "../library/Cover";
import { useCover } from "../library/covers";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { Notice } from "../ui/Notice";
import { useVariants, variantDescription, variantName } from "./variants";

/** "Which variant?": a card per variant to pick, then Play. */
export function VariantDialog({
  profile,
  onPlay,
  onClose,
}: {
  profile: Profile;
  onPlay: (variant: string) => void;
  onClose: () => void;
}) {
  const { t, i18n } = useTranslation();
  const variants = useVariants(profile);
  const data = variants.data;
  const [picked, setPicked] = useState<string | null>(null);
  const selected =
    picked ?? data?.active ?? data?.variants.find((v) => v.variant.default)?.variant.id;
  // A mod that got its variants after being played: the one it played is
  // marked as such.
  const playedBefore = profile.sessionCount > 0 && !!data && !data.chosen ? data.active : null;

  return (
    <Dialog
      title={t("variants.pickTitle")}
      onClose={onClose}
      wide={!!data && !data.sameCovers}
      footer={
        <div className="flex justify-end gap-3">
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button disabled={!selected} onClick={() => selected && onPlay(selected)}>
            {t("variants.play")}
          </Button>
        </div>
      }
    >
      <div className="grid gap-4">
        <p className="text-sm text-muted">{t("variants.pickIntro", { name: profile.label })}</p>
        {variants.error && <Notice tone="error">{errorMessage(t, variants.error)}</Notice>}
        <div role="radiogroup" aria-label={t("variants.pickTitle")} className="grid gap-2">
          {data?.variants.map((v) => (
            <VariantChoice
              key={v.variant.id}
              profile={profile}
              state={v}
              thumbnail={!data.sameCovers}
              checked={v.variant.id === selected}
              playedBefore={v.variant.id === playedBefore}
              language={i18n.language}
              onPick={() => setPicked(v.variant.id)}
            />
          ))}
        </div>
      </div>
    </Dialog>
  );
}

function VariantChoice({
  profile,
  state,
  thumbnail,
  checked,
  playedBefore,
  language,
  onPick,
}: {
  profile: Profile;
  state: VariantState;
  thumbnail: boolean;
  checked: boolean;
  playedBefore: boolean;
  language: string;
  onPick: () => void;
}) {
  const { t } = useTranslation();
  const v = state.variant;
  const description = variantDescription(v, language);
  return (
    <label
      className={`flex cursor-pointer items-center gap-3 rounded-lg border p-3 transition-colors ${
        checked
          ? "border-accent bg-accent/10"
          : "border-border bg-surface-raised hover:border-accent/50"
      }`}
    >
      <input
        type="radio"
        name="variant"
        className="sr-only"
        checked={checked}
        onChange={onPick}
        data-autofocus={checked || undefined}
      />
      {thumbnail && <VariantCover profile={profile} variant={v.id} className="w-28" />}
      <span className="grid min-w-0 flex-1 gap-0.5">
        <span className="flex flex-wrap items-center gap-2 text-sm font-semibold">
          {variantName(v, language)}
          {playedBefore && <Badge tone="status">{t("variants.playedBefore")}</Badge>}
        </span>
        {description && <span className="text-xs text-muted">{description}</span>}
        <span className="text-xs text-muted">{t("variants.saves", { count: state.saves })}</span>
      </span>
    </label>
  );
}

/** The title screen of one variant, as a thumbnail. */
export function VariantCover({
  profile,
  variant,
  className = "",
}: {
  profile: Profile;
  variant: string;
  className?: string;
}) {
  const cover = useCover(profile.id, profile.installId, variant);
  return (
    <span className={`shrink-0 overflow-hidden rounded ${className}`}>
      {cover.isPending ? (
        <span className="block aspect-[4/3] w-full" />
      ) : (
        <Cover
          gameId={profile.gameId}
          src={cover.data?.src}
          onError={cover.onError}
          standalone={profile.sourceMod}
          className="block aspect-[4/3] w-full text-2xl"
        />
      )}
    </span>
  );
}
