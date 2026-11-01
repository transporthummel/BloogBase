import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type EarlierBackup, type Profile } from "../bindings";
import { useGames } from "../catalog";
import { errorMessage } from "../errors";
import { formatDate } from "../format";
import { unwrap } from "../ipc";
import { profilesKey, useProfiles } from "../library/queries";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { Notice } from "../ui/Notice";
import { backupsKey, savesKey } from "./saves";

type Offer = { profile: Profile; earlier: EarlierBackup };

/**
 * Right after a mod or game is added, however that happened: offers once
 * the saves backed up when it was removed before (§9.5).
 */
export function EarlierBackupPrompt() {
  const profiles = useProfiles();
  const [offers, setOffers] = useState<Offer[]>([]);
  // Profiles there before; a mod added again may get its old id back.
  const known = useRef<Set<string> | null>(null);

  useEffect(() => {
    if (!profiles.data) return;
    const key = (p: Profile) => `${p.id}:${p.addedAt}`;
    const before = known.current;
    known.current = new Set(profiles.data.map(key));
    if (!before) return;
    for (const profile of profiles.data) {
      if (before.has(key(profile)) || profile.sessionCount > 0) continue;
      unwrap(commands.earlierBackup(profile.id)).then(
        (earlier) => {
          // Only settings or high scores aren't worth asking about.
          if (earlier && earlier.backup.saves > 0) setOffers((o) => [...o, { profile, earlier }]);
        },
        () => {},
      );
    }
  }, [profiles.data]);

  const offer = offers[0];
  return offer ? (
    <OfferDialog key={offer.profile.id} offer={offer} onDone={() => setOffers((o) => o.slice(1))} />
  ) : null;
}

function OfferDialog({ offer, onDone }: { offer: Offer; onDone: () => void }) {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const games = useGames();
  const { profile, earlier } = offer;
  const { backup, versionId } = earlier;
  const answer = useMutation({
    mutationFn: (restore: boolean) =>
      unwrap(commands.answerEarlierBackup(profile.id, backup.id, restore)),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: savesKey(profile.id) });
      queryClient.invalidateQueries({ queryKey: backupsKey(profile.id) });
      queryClient.invalidateQueries({ queryKey: profilesKey });
      onDone();
    },
  });
  const otherVersion =
    versionId !== null && versionId !== profile.versionId
      ? games.data?.find((g) => g.id === profile.gameId)?.versions.find((v) => v.id === versionId)
          ?.label
      : undefined;

  return (
    <Dialog title={t("earlierBackup.title", { name: profile.label })} onClose={onDone}>
      <p className="text-sm">
        {t("earlierBackup.text", {
          count: backup.saves,
          date: formatDate(i18n.language, backup.createdAt),
        })}
        {otherVersion && <> {t("earlierBackup.otherVersion", { version: otherVersion })}</>}
      </p>
      {answer.error && <Notice tone="error">{errorMessage(t, answer.error)}</Notice>}
      <div className="flex justify-end gap-2">
        <Button
          variant="secondary"
          onClick={() => answer.mutate(false)}
          disabled={answer.isPending}
        >
          {t("earlierBackup.decline")}
        </Button>
        <Button
          data-autofocus
          onClick={() => answer.mutate(true)}
          disabled={answer.isPending || profile.running}
        >
          {t("earlierBackup.restore")}
        </Button>
      </div>
    </Dialog>
  );
}
