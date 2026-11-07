import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { Save } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type CustomStatus } from "../bindings";
import { errorMessage } from "../errors";
import { unwrap } from "../ipc";
import { Button } from "../ui/Button";
import { Notice } from "../ui/Notice";

// Every profile's view shares the prefix, so saving a game's patches
// refreshes what its mods' pages say.
const customPatchesKey = (profileId: string) => ["customPatches", profileId] as const;

/** Patch file lines for a game copy, which its game and mods get at launch. */
export function CustomPatchesSection({ profileId }: { profileId: string }) {
  const { t } = useTranslation();
  const view = useQuery({
    queryKey: customPatchesKey(profileId),
    queryFn: () => unwrap(commands.profileCustomPatches(profileId)),
  });
  const v = view.data;
  if (view.error) return <Notice tone="error">{errorMessage(t, view.error)}</Notice>;
  if (!v) return null;

  return (
    <section className="grid gap-2" aria-labelledby="custom-patches">
      <h2 id="custom-patches" className="font-semibold">
        {t("patches.title")}
      </h2>
      {v.kind === "game" && <GamePatches profileId={profileId} text={v.text} status={v.status} />}
      {v.kind === "unpatchable" && <p className="text-sm text-muted">{t("patches.unpatchable")}</p>}
      {v.kind === "mod" && (
        <p className="text-sm text-muted">
          {t(`patches.status.${v.status}`, { game: v.gameLabel })}{" "}
          <Link
            to="/profiles/$profileId"
            params={{ profileId: v.gameProfileId }}
            className="underline"
          >
            {t("patches.edit")}
          </Link>
        </p>
      )}
    </section>
  );
}

function GamePatches({
  profileId,
  text,
  status,
}: {
  profileId: string;
  text: string;
  status: CustomStatus | null;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [draft, setDraft] = useState(text);
  // A new `text` from the server (after a save) replaces the draft.
  const [shown, setShown] = useState(text);
  if (text !== shown) {
    setShown(text);
    setDraft(text);
  }
  const save = useMutation({
    mutationFn: () => unwrap(commands.setCustomPatches(profileId, draft)),
    onSuccess: (bad) => {
      if (!bad) void queryClient.invalidateQueries({ queryKey: ["customPatches"] });
    },
  });
  const dirty = draft !== text;
  const bad = save.data;

  return (
    <>
      <p className="text-sm text-muted">{t("patches.hint")}</p>
      <textarea
        aria-label={t("patches.title")}
        value={draft}
        onChange={(e) => {
          setDraft(e.target.value);
          save.reset();
        }}
        rows={6}
        spellCheck={false}
        className="w-full rounded-lg border border-border bg-surface-raised px-3 py-2 font-mono text-xs select-text"
      />
      {status && status !== "applied" && status !== "ownProgram" && (
        <p className="text-sm text-muted">{t(`patches.gameStatus.${status}`)}</p>
      )}
      {bad && (
        <Notice tone="error">
          {t(`patches.bad.${bad.problem}`, { line: bad.line, word: bad.word })}
        </Notice>
      )}
      {save.error && <Notice tone="error">{errorMessage(t, save.error)}</Notice>}
      <div className="flex flex-wrap items-center gap-3">
        <Button onClick={() => save.mutate()} disabled={!dirty || save.isPending}>
          <Save className="size-4" aria-hidden />
          {t("config.save")}
        </Button>
        {save.isSuccess && !bad && !dirty && <Notice>{t("config.saved")}</Notice>}
      </div>
    </>
  );
}
