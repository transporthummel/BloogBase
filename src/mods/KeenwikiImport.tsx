import { useMutation, useQuery } from "@tanstack/react-query";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ArrowLeft, Download, ExternalLink, Search } from "lucide-react";
import { type ReactNode, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type KeenwikiLookup, type WikiMod } from "../bindings";
import { gameTitle, useGames } from "../catalog";
import { ModCard } from "../discover/ModCard";
import { useIndexMods } from "../discover/queries";
import { errorMessage } from "../errors";
import { formatLooseDate } from "../format";
import { unwrap } from "../ipc";
import { installsKey } from "../library/queries";
import { Button } from "../ui/Button";
import { Notice } from "../ui/Notice";
import { useEnterAction } from "../ui/useEnterAction";
import { DownloadError } from "./DownloadError";
import { downloaded, type Downloaded } from "./wikiDownload";

/**
 * Paste a KeenWiki link: shows the index entry for it, or the page's details.
 * Once a mod is found, only that is shown; `children` (other ways to add one) until then.
 */
export function KeenwikiImport({
  disabled,
  onDownloaded,
  onCancel,
  children,
}: {
  disabled: boolean;
  onDownloaded: (d: Downloaded) => void;
  onCancel: () => void;
  children: ReactNode;
}) {
  const { t } = useTranslation();
  const [url, setUrl] = useState("");
  const lookup = useMutation({
    mutationFn: (u: string) => unwrap(commands.lookupKeenwiki(u)),
  });
  const found = lookup.data?.indexId || lookup.data?.page ? lookup.data : null;

  if (found) {
    return (
      <Result
        lookup={found}
        onDownloaded={onDownloaded}
        onBack={() => lookup.reset()}
        onCancel={onCancel}
      />
    );
  }
  return (
    <div className="grid gap-3">
      <form
        className="grid gap-1 text-sm"
        onSubmit={(e) => {
          e.preventDefault();
          if (url.trim()) lookup.mutate(url.trim());
        }}
      >
        <label htmlFor="keenwiki-url" className="font-medium">
          {t("keenwiki.label")}
        </label>
        <div className="flex gap-2">
          <input
            id="keenwiki-url"
            data-autofocus
            type="text"
            inputMode="url"
            autoComplete="off"
            spellCheck={false}
            className="min-w-0 flex-1 rounded-lg border border-border bg-surface-raised px-3 py-2"
            placeholder="https://keenwiki.shikadi.net/wiki/…"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
          />
          <Button type="submit" disabled={disabled || lookup.isPending || !url.trim()}>
            <Search className="size-4" aria-hidden />
            {t("keenwiki.lookUp")}
          </Button>
        </div>
      </form>
      {lookup.isPending && <Notice>{t("keenwiki.looking")}</Notice>}
      {lookup.error && <Notice tone="error">{errorMessage(t, lookup.error)}</Notice>}
      {children}
    </div>
  );
}

type Actions = { onBack: () => void; onCancel: () => void };

function Result({
  lookup,
  onDownloaded,
  ...actions
}: Actions & {
  lookup: KeenwikiLookup;
  onDownloaded: (d: Downloaded) => void;
}) {
  if (lookup.page && !lookup.indexId) {
    return <WikiCard page={lookup.page} onDownloaded={onDownloaded} {...actions} />;
  }
  return (
    <div className="grid gap-4">
      {lookup.indexId && <IndexHit id={lookup.indexId} />}
      <Footer {...actions} />
    </div>
  );
}

/** Back to the link field, cancel, and the step's own action, if any. */
function Footer({
  onBack,
  onCancel,
  busy = false,
  children,
}: Actions & { busy?: boolean; children?: ReactNode }) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-wrap justify-between gap-3">
      <Button variant="ghost" onClick={onBack} disabled={busy}>
        <ArrowLeft className="size-4" aria-hidden />
        {t("common.back")}
      </Button>
      <div className="flex flex-wrap justify-end gap-3">
        <Button variant="ghost" onClick={onCancel}>
          {t("common.cancel")}
        </Button>
        {children}
      </div>
    </div>
  );
}

/** The mod is in the index: install the tested entry instead, from `archive` when given. */
export function IndexHit({
  id,
  archive,
  notice = "keenwiki.inIndex",
}: {
  id: string;
  archive?: string;
  notice?: "keenwiki.inIndex" | "mods.inIndex";
}) {
  const { t } = useTranslation();
  const mods = useIndexMods();
  const games = useGames();
  const installs = useQuery({
    queryKey: installsKey,
    queryFn: () => unwrap(commands.listInstalls()),
  });
  const mod = mods.data?.find((m) => m.entry.id === id);
  if (!mod || !games.data || !installs.data) return null;
  return (
    <div className="grid gap-2">
      <Notice>{t(notice)}</Notice>
      <ModCard
        mod={mod}
        installs={installs.data.filter((i) => i.gameId === mod.entry.game)}
        games={games.data}
        archive={archive}
        enterInstalls
      />
    </div>
  );
}

/** What the page says, with a choice of download. */
function WikiCard({
  page,
  onDownloaded,
  ...actions
}: Actions & {
  page: WikiMod;
  onDownloaded: (d: Downloaded) => void;
}) {
  const { t, i18n } = useTranslation();
  const games = useGames();
  const [choice, setChoice] = useState(0);
  const chosen = page.downloads[choice];
  const download = useMutation({
    mutationFn: () => unwrap(commands.downloadKeenwikiMod(chosen.url, page.gameId)),
    onSuccess: (d) => onDownloaded(downloaded(page, chosen, d)),
  });
  const card = useRef<HTMLDivElement>(null);
  useEnterAction(card, chosen && !download.isPending ? () => download.mutate() : null);

  const facts: [string, string | null][] = [
    [t("mods.forGame"), page.gameId ? gameTitle(games.data, page.gameId) : null],
    [t("keenwiki.authors"), page.authors.join(", ") || null],
    [t("keenwiki.status"), page.status],
    [t("keenwiki.released"), page.released && formatLooseDate(i18n.language, page.released)],
    [t("keenwiki.updated"), page.updated && formatLooseDate(i18n.language, page.updated)],
    [t("keenwiki.levels"), page.levels],
  ];
  const links = [
    ["KeenWiki", page.pageUrl],
    [t("keenwiki.discussion"), page.discussion],
    [t("discover.homepage"), page.homepage],
  ].filter((l): l is [string, string] => !!l[1]);
  const downloadLabel = (d: WikiMod["downloads"][number]) =>
    d.note ? `${d.label} (${d.note})` : d.label;

  return (
    <div ref={card} className="grid gap-4">
      <section className="grid gap-3 rounded-lg border border-border p-4" aria-label={page.name}>
        <h3 className="font-semibold">{page.name}</h3>
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
          {facts
            .filter(([, value]) => value)
            .map(([label, value]) => (
              <div key={label} className="contents">
                <dt className="text-muted">{label}</dt>
                <dd>{value}</dd>
              </div>
            ))}
        </dl>
        {page.description && <p className="line-clamp-4 text-sm">{page.description}</p>}
        <p className="flex flex-wrap gap-3 text-xs">
          {links.map(([label, url]) => (
            <button
              key={label}
              type="button"
              className="inline-flex cursor-pointer items-center gap-1 text-ink underline underline-offset-2"
              onClick={() => openUrl(url)}
            >
              {label}
              <ExternalLink className="size-3" aria-hidden />
            </button>
          ))}
        </p>

        {page.downloads.length === 0 ? (
          <Notice tone="error">{t("keenwiki.noDownload")}</Notice>
        ) : (
          <>
            {page.downloads.length > 1 && (
              <fieldset className="grid gap-1 text-sm">
                <legend className="mb-1 font-medium">{t("keenwiki.download")}</legend>
                {page.downloads.map((d, i) => (
                  <label key={d.url} className="flex items-start gap-2">
                    <input
                      type="radio"
                      name="keenwiki-download"
                      className="mt-1"
                      checked={choice === i}
                      onChange={() => setChoice(i)}
                    />
                    <span>{downloadLabel(d)}</span>
                  </label>
                ))}
              </fieldset>
            )}
            <p className="text-xs break-all text-muted">{chosen.url}</p>
          </>
        )}
      </section>
      {download.isPending && <Notice>{t("keenwiki.downloading")}</Notice>}
      {download.error && <DownloadError error={download.error} url={chosen.url} />}
      <Footer {...actions} busy={download.isPending}>
        {chosen && (
          <Button onClick={() => download.mutate()} disabled={download.isPending}>
            <Download className="size-4" aria-hidden />
            {t("keenwiki.fetch")}
          </Button>
        )}
      </Footer>
    </div>
  );
}

/** Opens a prefilled GitHub issue proposing a KeenWiki mod for the index. */
export function SuggestToIndex({ modId }: { modId: string }) {
  const { t } = useTranslation();
  const suggest = useMutation({
    mutationFn: async () => openUrl(await unwrap(commands.indexSuggestionUrl(modId))),
  });
  return (
    <>
      <button
        type="button"
        className="inline-flex items-center gap-1 text-ink underline underline-offset-2"
        onClick={() => suggest.mutate()}
        disabled={suggest.isPending}
      >
        {t("keenwiki.suggest")}
        <ExternalLink className="size-3" aria-hidden />
      </button>
      {suggest.error && <span className="text-danger"> {errorMessage(t, suggest.error)}</span>}
    </>
  );
}
