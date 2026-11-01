import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { ChevronRight, Info } from "lucide-react";
import { Fragment, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Engine, type Game, type Profile } from "../bindings";
import { unwrap } from "../ipc";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { PlayButton } from "../ui/PlayButton";
import { type Binding, type Chord, sheetFor, spaceFires } from "./controlSheet";
import { oneButtonFiringKey, profileFiringKey } from "./firing";

const mac = () => /Mac/.test(navigator.userAgent);

const arrows: Record<string, string> = { left: "←", right: "→", up: "↑", down: "↓" };
// Keys whose name is a word; the rest (letters, digits, F-keys) show as they are printed.
const named = new Set(["ctrl", "alt", "space", "enter", "esc", "tab", "shiftLeft", "cmd"]);

/** One key, drawn as a keycap. */
function Keycap({
  id,
  changed = false,
  unused = false,
}: {
  id: string;
  changed?: boolean;
  /** Drawn faintly: a key of the cluster the game doesn't use. */
  unused?: boolean;
}) {
  const { t } = useTranslation();
  const arrow = arrows[id];
  const label = arrow ?? (named.has(id) ? t(`controls.key.${id}`) : id.toUpperCase());
  return (
    <kbd
      aria-label={arrow ? t(`controls.key.${id}`) : undefined}
      className={`inline-flex h-8 shrink-0 items-center justify-center rounded-md border border-b-[3px] bg-surface-raised px-2 font-sans text-xs font-semibold shadow-key transition-[translate,border-width] duration-150 hover:translate-y-px hover:border-b-2 ${
        id === "space" ? "min-w-24" : "min-w-8"
      } ${changed ? "border-accent/70 text-accent" : "border-border text-ink"} ${unused ? "opacity-40" : ""}`}
    >
      {label}
    </kbd>
  );
}

/** Keys pressed together. */
function ChordKeys({ keys, changed }: { keys: Chord; changed?: boolean }) {
  return (
    <span className="flex shrink-0 items-center gap-1">
      {keys.map((k, i) => (
        <Fragment key={k}>
          {i > 0 && (
            <span className="text-xs text-muted" aria-hidden>
              +
            </span>
          )}
          <Keycap id={k} changed={changed} />
        </Fragment>
      ))}
    </span>
  );
}

function Bindings({ items, prefix }: { items: Binding[]; prefix: "action" | "cheat" }) {
  const { t } = useTranslation();
  return (
    // Keys in one column, so the labels line up beside them.
    <ul className="grid grid-cols-[max-content_1fr] items-center gap-x-3 gap-y-2.5">
      {items.map((b) => (
        <li key={b.action + b.keys.join()} className="contents">
          <ChordKeys keys={b.keys} changed={b.changed} />
          <span className="flex flex-wrap items-center gap-2 text-sm">
            {t(`controls.${prefix}.${b.action}`)}
            {b.changed && (
              <span title={t("controls.changedHint")}>
                <Badge tone="status">{t("controls.changed")}</Badge>
              </span>
            )}
          </span>
        </li>
      ))}
    </ul>
  );
}

function Group({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="grid content-start gap-3 rounded-xl border border-border bg-surface p-4">
      <h3 className="text-xs font-semibold tracking-wide text-muted uppercase">{title}</h3>
      {children}
    </section>
  );
}

/** The arrow keys in their inverted T. */
function ArrowCluster({ upDown }: { upDown: boolean }) {
  return (
    <div className="grid w-max grid-cols-3 gap-1">
      <span />
      <Keycap id="up" unused={!upDown} />
      <span />
      <Keycap id="left" />
      <Keycap id="down" unused={!upDown} />
      <Keycap id="right" />
    </div>
  );
}

const engineDot: Record<Engine, string> = {
  vorticons: "bg-vorticons",
  galaxy: "bg-galaxy",
  dreams: "bg-dreams",
};

/**
 * The keys of a game or mod and, folded away, its cheats. With one-button
 * firing on (or a mod that has it), Keen 1-3 shoot with Space here too.
 */
export function ControlsSheet({ profile, game }: { profile: Profile; game: Game }) {
  const { t } = useTranslation();
  const enabled = useQuery({
    queryKey: oneButtonFiringKey,
    queryFn: () => unwrap(commands.oneButtonFiring()),
    enabled: game.engine === "vorticons",
  });
  const firing = useQuery({
    queryKey: profileFiringKey(profile.id),
    queryFn: () => unwrap(commands.profileFiring(profile.id)),
    enabled: game.engine === "vorticons",
  });
  const space = spaceFires(enabled.data, firing.data);
  const sheet = sheetFor(game, space, mac());
  // A copy the patch fits, with the setting off: worth a pointer.
  const offerSpace = firing.data === "patchable" && enabled.data === false;

  return (
    <div className="grid gap-4">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <span className="flex items-center gap-2 font-pixel text-xs leading-relaxed">
          <span className={`size-2.5 rounded-sm ${engineDot[sheet.engine]}`} aria-hidden />
          {t(`controls.series.${sheet.engine}`)}
        </span>
        <span className="text-sm text-muted">{t(`controls.remap.${sheet.engine}`)}</span>
      </div>
      {profile.modId !== null && (
        <p className="flex items-start gap-2 text-sm text-muted">
          <Info className="mt-0.5 size-4 shrink-0" aria-hidden />
          {t("controls.mod")}
        </p>
      )}

      <div className="grid gap-4 sm:grid-cols-2">
        <Group title={t("controls.moving")}>
          <div className="flex flex-wrap items-center gap-4">
            <ArrowCluster upDown={sheet.engine !== "vorticons"} />
            <p className="max-w-56 text-sm">{t(`controls.move.${sheet.move}`)}</p>
          </div>
        </Group>
        <Group title={t("controls.actions")}>
          <Bindings items={sheet.actions} prefix="action" />
          {offerSpace && (
            <p className="text-xs text-muted">
              {t("controls.spaceOffer")}{" "}
              <Link to="/settings" className="underline">
                {t("controls.spaceOfferLink")}
              </Link>
            </p>
          )}
        </Group>
        <Group title={t("controls.inGame")}>
          <Bindings items={sheet.game} prefix="action" />
        </Group>
        <Group title={t("controls.window")}>
          <Bindings items={sheet.window} prefix="action" />
        </Group>
      </div>

      <Cheats sheet={sheet} />
    </div>
  );
}

/** Folded away, so nobody reads them by accident; closed on every visit. */
function Cheats({ sheet }: { sheet: ReturnType<typeof sheetFor> }) {
  const { t } = useTranslation();
  return (
    <details className="group rounded-xl border border-dashed border-border bg-surface/60">
      <summary className="flex cursor-pointer list-none items-center gap-2 rounded-xl px-4 py-3 text-sm font-semibold [&::-webkit-details-marker]:hidden">
        <ChevronRight
          className="size-4 text-muted transition-transform group-open:rotate-90"
          aria-hidden
        />
        {t("controls.cheats")}
        <Badge>{t("controls.spoiler")}</Badge>
      </summary>
      <div className="grid gap-4 px-4 pb-4">
        <p className="text-sm text-muted">{t("controls.cheatsHint")}</p>
        {sheet.cheats.length > 0 && <Bindings items={sheet.cheats} prefix="cheat" />}
        {sheet.debug && (
          <div className="grid gap-3">
            {sheet.debug.enable ? (
              <p className="flex flex-wrap items-center gap-2 text-sm">
                {t("controls.debugFirst")}
                <ChordKeys keys={sheet.debug.enable} />
                {t("controls.debugThen")}
              </p>
            ) : (
              <p className="text-sm">{t("controls.debugAlways")}</p>
            )}
            <Bindings items={sheet.debug.cheats} prefix="cheat" />
          </div>
        )}
      </div>
    </details>
  );
}

/** Shown before a series' first start: the keys, then Play. */
export function ControlsDialog({
  profile,
  game,
  onStart,
  onClose,
}: {
  profile: Profile;
  game: Game;
  /** `remember`: don't show it again for this series. */
  onStart: (remember: boolean) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [remember, setRemember] = useState(true);
  return (
    <Dialog
      title={t("controls.dialogTitle", { series: t(`controls.series.${game.engine}`) })}
      onClose={onClose}
      wide
    >
      <p className="text-sm text-muted">{t("controls.dialogIntro")}</p>
      <ControlsSheet profile={profile} game={game} />
      <div className="flex flex-wrap items-center justify-end gap-x-4 gap-y-3">
        <label className="mr-auto flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={remember}
            onChange={(e) => setRemember(e.target.checked)}
          />
          {t("controls.dontShow")}
        </label>
        <Button variant="secondary" onClick={onClose}>
          {t("common.cancel")}
        </Button>
        <PlayButton data-autofocus onClick={() => onStart(remember)}>
          {t("library.play")}
        </PlayButton>
      </div>
    </Dialog>
  );
}
