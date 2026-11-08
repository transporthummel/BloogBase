import { useTranslation } from "react-i18next";
import type { SoundDevice } from "../bindings";
import { errorMessage } from "../errors";
import { Notice } from "../ui/Notice";
import { Field, Group, selectClass } from "./parts";
import { useGameSettings } from "./queries";

const UNSET = "";

/** On, off, or left to the game. */
function OnOffField({
  label,
  hint,
  value,
  disabled,
  onChange,
}: {
  label: string;
  hint?: string;
  value: boolean | null;
  disabled: boolean;
  onChange: (value: boolean | null) => void;
}) {
  const { t } = useTranslation();
  return (
    <Field label={label} hint={hint}>
      <select
        className={`${selectClass} w-52`}
        value={value === null ? UNSET : value ? "on" : "off"}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value === UNSET ? null : e.target.value === "on")}
      >
        <option value={UNSET}>{t("settings.setByGame")}</option>
        <option value="on">{t("settings.on")}</option>
        <option value="off">{t("settings.off")}</option>
      </select>
    </Field>
  );
}

function Hint({ children }: { children: string }) {
  return <p className="max-w-prose text-xs text-muted">{children}</p>;
}

export function SoundSection() {
  const { t } = useTranslation();
  const { settings: s, update, pending, error } = useGameSettings();

  return (
    <Group title={t("settings.soundMusic")}>
      {s && (
        <>
          <OnOffField
            label={t("settings.sound")}
            value={s.sound}
            disabled={pending}
            onChange={(sound) => update({ sound })}
          />
          <OnOffField
            label={t("settings.music")}
            hint={t("settings.musicHint")}
            value={s.music}
            disabled={pending}
            onChange={(music) => update({ music })}
          />
          <Field label={t("settings.device")} hint={t("settings.deviceHint")}>
            <select
              className={`${selectClass} w-52`}
              value={s.device ?? UNSET}
              disabled={pending || s.sound === false}
              onChange={(e) =>
                update({
                  device: e.target.value === UNSET ? null : (e.target.value as SoundDevice),
                })
              }
            >
              <option value={UNSET}>{t("settings.setByGame")}</option>
              <option value="pcSpeaker">{t("settings.pcSpeaker")}</option>
              <option value="soundBlaster">{t("settings.soundBlaster")}</option>
            </select>
          </Field>
          <Hint>{t("settings.gameSettingsHint")}</Hint>
        </>
      )}
      {error && <Notice tone="error">{errorMessage(t, error)}</Notice>}
    </Group>
  );
}

/** Whether Keen 4–6 show their score box; sits with the picture options. */
export function ScoreBoxField() {
  const { t } = useTranslation();
  const { settings: s, update, pending, error } = useGameSettings();

  return (
    <>
      {s && (
        <OnOffField
          label={t("settings.scoreBox")}
          hint={t("settings.scoreBoxHint")}
          value={s.scoreBox}
          disabled={pending}
          onChange={(scoreBox) => update({ scoreBox })}
        />
      )}
      {error && <Notice tone="error">{errorMessage(t, error)}</Notice>}
    </>
  );
}
