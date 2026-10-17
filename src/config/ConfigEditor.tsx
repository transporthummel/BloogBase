import { HighlightStyle, StreamLanguage, syntaxHighlighting } from "@codemirror/language";
import { properties } from "@codemirror/legacy-modes/mode/properties";
import { EditorView } from "@codemirror/view";
import { tags } from "@lezer/highlight";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import CodeMirror from "@uiw/react-codemirror";
import { RotateCcw, Save } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type ConfigWarning } from "../bindings";
import { errorMessage } from "../errors";
import { unwrap } from "../ipc";
import { Button } from "../ui/Button";
import { ConfirmDialog } from "../ui/ConfirmDialog";
import { Notice } from "../ui/Notice";

const language = StreamLanguage.define(properties);

/** Follows the app's design tokens, so it fits both themes. */
const theme = EditorView.theme({
  "&": {
    backgroundColor: "var(--bb-surface-raised)",
    color: "var(--bb-ink)",
    fontSize: "0.8rem",
    borderRadius: "0.5rem",
  },
  ".cm-gutters": {
    backgroundColor: "var(--bb-surface)",
    color: "var(--bb-muted)",
    border: "none",
  },
  ".cm-activeLine, .cm-activeLineGutter": { backgroundColor: "transparent" },
  "&.cm-focused": { outline: "2px solid var(--bb-focus)" },
  ".cm-cursor": { borderLeftColor: "var(--bb-ink)" },
  ".cm-content": { fontFamily: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace" },
});

/** The properties mode marks keys "def", values "quote", sections "header". */
const highlight = syntaxHighlighting(
  HighlightStyle.define([
    { tag: tags.comment, color: "var(--bb-muted)", fontStyle: "italic" },
    { tag: tags.heading, color: "var(--bb-accent)", fontWeight: "bold" },
    { tag: tags.definition(tags.variableName), color: "var(--bb-ink)" },
    { tag: tags.quote, color: "var(--bb-galaxy)" },
  ]),
);

const extensions = [language, theme, highlight];

/** A read-only view of a config file. */
export function ConfigText({ text, label }: { text: string; label: string }) {
  return (
    <CodeMirror
      value={text}
      editable={false}
      readOnly
      theme="none"
      extensions={extensions}
      basicSetup={{ foldGutter: false, highlightActiveLine: false }}
      aria-label={label}
    />
  );
}

/** Edits one of the user's override layers: global, or a profile's. */
export function ConfigEditor({
  profileId,
  text,
  warnings,
  label,
}: {
  profileId: string | null;
  text: string;
  warnings: ConfigWarning[];
  label: string;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [draft, setDraft] = useState(text);
  const [confirmReset, setConfirmReset] = useState(false);
  // A new `text` from the server (after a save or reset) replaces the draft.
  const [shown, setShown] = useState(text);
  if (text !== shown) {
    setShown(text);
    setDraft(text);
  }

  const invalidate = () => queryClient.invalidateQueries({ queryKey: ["config"] });
  const save = useMutation({
    mutationFn: () => unwrap(commands.saveConfig(profileId, draft)),
    onSuccess: invalidate,
  });
  const reset = useMutation({
    mutationFn: () => unwrap(commands.resetConfig(profileId)),
    onSuccess: () => {
      setConfirmReset(false);
      save.reset();
      invalidate();
    },
  });
  const dirty = draft !== text;
  const shownWarnings = save.data ?? warnings;

  return (
    <div className="grid gap-3">
      <CodeMirror
        value={draft}
        onChange={setDraft}
        theme="none"
        extensions={extensions}
        basicSetup={{ foldGutter: false }}
        minHeight="8rem"
        aria-label={label}
      />
      {shownWarnings.length > 0 && (
        <ul className="grid gap-1 text-sm text-muted" aria-label={t("config.warnings")}>
          {shownWarnings.map((w) => (
            <li key={`${w.line}-${w.kind}`}>
              {t("config.line", { line: w.line })}: {t(`config.warning.${w.kind}`)}
            </li>
          ))}
        </ul>
      )}
      {(save.error ?? reset.error) && (
        <Notice tone="error">{errorMessage(t, save.error ?? reset.error)}</Notice>
      )}
      <div className="flex flex-wrap items-center gap-3">
        <Button onClick={() => save.mutate()} disabled={!dirty || save.isPending}>
          <Save className="size-4" aria-hidden />
          {t("config.save")}
        </Button>
        <Button variant="ghost" onClick={() => setConfirmReset(true)}>
          <RotateCcw className="size-4" aria-hidden />
          {t("config.reset")}
        </Button>
        {save.isSuccess && !dirty && <Notice>{t("config.saved")}</Notice>}
      </div>
      {confirmReset && (
        <ConfirmDialog
          title={t("config.resetTitle")}
          confirm={t("config.reset")}
          busy={reset.isPending}
          onConfirm={() => reset.mutate()}
          onClose={() => setConfirmReset(false)}
        >
          <p className="text-sm text-muted">{t("config.resetHint")}</p>
        </ConfirmDialog>
      )}
    </div>
  );
}
