import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import type { WorldOption, WorldStart, Worlds } from "../bindings";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { optionDefault, optionLabel, worldOptions } from "./worlds";

const fieldClass = "rounded-lg border border-border bg-surface-raised px-3 py-2 text-sm";

/**
 * "Start new world": a seed (empty for a random one) and the generator's
 * options, as the mod's index entry offers them. Starts with the options
 * of the last new world.
 */
export function NewWorldDialog({
  definition,
  lastOptions,
  onStart,
  onClose,
}: {
  definition: Worlds;
  lastOptions: { [key in string]?: number };
  onStart: (start: WorldStart) => void;
  onClose: () => void;
}) {
  const { t, i18n } = useTranslation();
  const id = useId();
  const [seed, setSeed] = useState("");
  const [options, setOptions] = useState<Record<string, string>>(() =>
    Object.fromEntries(
      worldOptions(definition).map((o) => [o.id, String(lastOptions[o.id] ?? optionDefault(o))]),
    ),
  );
  const max = definition.seed?.max ?? 0;
  const seedValue = seed === "" ? null : Number(seed);
  const seedBad = seedValue !== null && (!/^\d+$/.test(seed) || seedValue > max);
  const bad = (o: WorldOption) => {
    if (o.type === "flag") return false;
    const text = options[o.id];
    const n = Number(text);
    return !/^\d+$/.test(text) || n < (o.min ?? 0) || (o.max != null && n > o.max);
  };
  const invalid = seedBad || worldOptions(definition).some(bad);

  function start() {
    if (invalid) return;
    onStart({
      type: "new",
      seed: seedValue,
      options: Object.fromEntries(
        worldOptions(definition).map((o) => [o.id, Number(options[o.id])]),
      ),
    });
  }

  return (
    <Dialog title={t("worlds.newTitle")} onClose={onClose}>
      <form
        className="grid gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          start();
        }}
      >
        {definition.seed && (
          <div className="grid gap-1">
            <label htmlFor={`${id}-seed`} className="text-sm font-semibold">
              {t("worlds.seed")}
            </label>
            <input
              id={`${id}-seed`}
              className={`${fieldClass} w-48`}
              inputMode="numeric"
              autoComplete="off"
              value={seed}
              placeholder={t("worlds.seedRandom")}
              aria-invalid={seedBad}
              aria-describedby={`${id}-seed-hint`}
              onChange={(e) => setSeed(e.target.value.trim())}
              data-autofocus
            />
            <p
              id={`${id}-seed-hint`}
              className={`text-xs ${seedBad ? "text-danger" : "text-muted"}`}
            >
              {seedBad ? t("worlds.seedBad", { max }) : t("worlds.seedHint")}
            </p>
          </div>
        )}
        {worldOptions(definition).map((o) =>
          o.type === "flag" ? (
            <label key={o.id} className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={options[o.id] === "1"}
                onChange={(e) => setOptions({ ...options, [o.id]: e.target.checked ? "1" : "0" })}
              />
              {optionLabel(o, i18n.language)}
            </label>
          ) : (
            <div key={o.id} className="grid gap-1">
              <label htmlFor={`${id}-${o.id}`} className="text-sm font-semibold">
                {optionLabel(o, i18n.language)}
              </label>
              <input
                id={`${id}-${o.id}`}
                type="number"
                className={`${fieldClass} w-24`}
                min={o.min ?? undefined}
                max={o.max ?? undefined}
                value={options[o.id]}
                aria-invalid={bad(o)}
                onChange={(e) => setOptions({ ...options, [o.id]: e.target.value })}
              />
              {bad(o) && (
                <p className="text-xs text-danger">
                  {t("worlds.optionRange", { min: o.min ?? 0, max: o.max ?? 0 })}
                </p>
              )}
            </div>
          ),
        )}
        <div className="flex justify-end gap-3">
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button type="submit" disabled={invalid}>
            {t("worlds.start")}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
