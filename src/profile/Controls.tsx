import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { ChevronRight, Info } from "lucide-react";
import { Fragment, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import {
  commands,
  type Controller,
  type Engine,
  type Game,
  type GameKeys,
  type Profile,
} from "../bindings";
import { buttonName, padLayout } from "../gamepad/buttons";
import { unwrap } from "../ipc";
import { gameSettingsKey } from "../settings/queries";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { PlayButton } from "../ui/PlayButton";
import {
  type Binding,
  type Chord,
  keysInEffect,
  type PadInput,
  type PadKeys,
  type PadSheet,
  padFor,
  type Sheet,
  sheetFor,
  spaceFires,
} from "./controlSheet";
import { connectedController, controllerConnectedKey, type ControlsView } from "./controller";
import { oneButtonFiringKey, profileFiringKey } from "./firing";
import { Keycap } from "./Keycap";
import { keyLabel } from "./keyLabel";
import { type Callout, PadDrawing } from "./PadDrawing";

const mac = () => /Mac/.test(navigator.userAgent);

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

type HeadingTag = "h2" | "h3";

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

function Group({
  title,
  heading: Heading,
  children,
}: {
  title: string;
  heading: HeadingTag;
  children: ReactNode;
}) {
  return (
    <section className="grid content-start gap-3 rounded-xl border border-border bg-surface p-4">
      <Heading className="text-xs font-semibold tracking-wide text-muted uppercase">
        {title}
      </Heading>
      {children}
    </section>
  );
}

/** The movement keys in an inverted T. */
function ArrowCluster({ moving, upDown }: { moving: Sheet["moving"]; upDown: boolean }) {
  return (
    <div className="grid w-max grid-cols-3 gap-1">
      <span />
      <Keycap id={moving.up.id} changed={moving.up.changed} unused={!upDown} />
      <span />
      <Keycap id={moving.left.id} changed={moving.left.changed} />
      <Keycap id={moving.down.id} changed={moving.down.changed} unused={!upDown} />
      <Keycap id={moving.right.id} changed={moving.right.changed} />
    </div>
  );
}

const noKeys: GameKeys = {
  up: null,
  down: null,
  left: null,
  right: null,
  jump: null,
  pogo: null,
  fire: null,
};

const engineDot: Record<Engine, string> = {
  vorticons: "bg-vorticons",
  galaxy: "bg-galaxy",
  dreams: "bg-dreams",
};

/** Two or more choices, one of them on. */
function Choice<T extends string>({
  label,
  options,
  value,
  onChange,
}: {
  label: string;
  options: { value: T; label: string }[];
  value: T;
  onChange: (value: T) => void;
}) {
  return (
    <div role="group" aria-label={label} className="flex flex-wrap gap-1">
      {options.map((o) => (
        <Button
          key={o.value}
          variant={o.value === value ? "secondary" : "ghost"}
          aria-pressed={o.value === value}
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </Button>
      ))}
    </div>
  );
}

/**
 * The keys of a game or mod and, folded away, its cheats, or its controller
 * layout. With one-button firing on (or a mod that has it), Keen 1-3 shoot
 * with Space here too.
 */
export function ControlsSheet({
  profile,
  game,
  heading = "h2",
  view: initialView,
}: {
  profile: Profile;
  game: Game;
  /** Its groups' headings: under the page title, or under a dialog's (h2). */
  heading?: HeadingTag;
  /** The view it opens on; else the controller's if one is connected. */
  view?: ControlsView;
}) {
  const { t } = useTranslation();
  const connected = useQuery({
    queryKey: controllerConnectedKey,
    queryFn: connectedController,
    enabled: initialView === undefined,
    staleTime: 0,
  });
  // Kept for this visit only.
  const [chosen, setChosen] = useState<ControlsView | undefined>(initialView);
  const view = chosen ?? (connected.data ? "controller" : "keyboard");
  // Where Rust can't tell (or wasn't asked), the pad the app sees.
  const layout = connected.data ?? padLayout();
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
  const settings = useQuery({
    queryKey: gameSettingsKey,
    queryFn: () => unwrap(commands.gameSettings()),
  });
  const space = spaceFires(enabled.data, firing.data);
  const keys = keysInEffect(settings.data?.keys ?? noKeys, game, profile.versionId, enabled.data);
  const sheet = sheetFor(game, space, mac(), keys);
  // A copy the patch fits, with the setting off: worth a pointer.
  const offerSpace = firing.data === "patchable" && enabled.data === false;

  // Waits for the answer (at most a moment), so the view doesn't switch.
  const ready = chosen !== undefined || !connected.isPending;
  const windowGroup = (
    <Group title={t("controls.window")} heading={heading}>
      <Bindings items={sheet.window} prefix="action" />
    </Group>
  );

  return (
    <div className="grid gap-4">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <span className="flex items-center gap-2 font-pixel text-xs leading-relaxed">
          <span className={`size-2.5 rounded-sm ${engineDot[sheet.engine]}`} aria-hidden />
          {t(`controls.series.${sheet.engine}`)}
        </span>
        <span className="text-sm text-muted">{t(`controls.remap.${sheet.engine}`)}</span>
        <span className="ml-auto">
          <Choice
            label={t("controls.view.label")}
            value={view}
            onChange={setChosen}
            options={[
              { value: "keyboard", label: t("controls.view.keyboard") },
              { value: "controller", label: t("controls.view.controller") },
            ]}
          />
        </span>
      </div>
      {profile.modId !== null && (
        <p className="flex items-start gap-2 text-sm text-muted">
          <Info className="mt-0.5 size-4 shrink-0" aria-hidden />
          {t("controls.mod")}
        </p>
      )}

      {ready && view === "controller" && (
        <ControllerSheet pad={padFor(game.engine, space)} layout={layout} heading={heading}>
          {windowGroup}
        </ControllerSheet>
      )}
      {ready && view === "keyboard" && (
        <KeyboardSheet sheet={sheet} heading={heading} offerSpace={offerSpace}>
          {windowGroup}
        </KeyboardSheet>
      )}
    </div>
  );
}

function KeyboardSheet({
  sheet,
  heading,
  offerSpace,
  children,
}: {
  sheet: Sheet;
  heading: HeadingTag;
  offerSpace: boolean;
  /** The window keys. */
  children: ReactNode;
}) {
  const { t } = useTranslation();
  return (
    <>
      <div className="grid gap-4 sm:grid-cols-2">
        <Group title={t("controls.moving")} heading={heading}>
          <div className="flex flex-wrap items-center gap-4">
            <ArrowCluster moving={sheet.moving} upDown={sheet.engine !== "vorticons"} />
            <p className="max-w-56 text-sm">{t(`controls.move.${sheet.move}`)}</p>
          </div>
        </Group>
        <Group title={t("controls.actions")} heading={heading}>
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
        <Group title={t("controls.inGame")} heading={heading}>
          <Bindings items={sheet.game} prefix="action" />
        </Group>
        {children}
      </div>

      <Cheats sheet={sheet} />
    </>
  );
}

/** Inputs named in words, under `controls.pad.input`; the rest by their label. */
const inputNames: Record<Controller, Partial<Record<PadInput, string>>> = {
  xbox: { ls: "ls", rs: "rs", dpad: "dpad", view: "view", menu: "menu" },
  playstation: {
    ls: "ls",
    rs: "rs",
    dpad: "dpad",
    a: "cross",
    b: "circle",
    x: "square",
    y: "triangle",
  },
};

/** The controller drawn with what its inputs do, playing or with View held. */
function ControllerSheet({
  pad,
  layout,
  heading: Heading,
  children,
}: {
  pad: PadSheet;
  layout: Controller;
  heading: HeadingTag;
  /** The window keys, on the keyboard only. */
  children: ReactNode;
}) {
  const { t } = useTranslation();
  const [layer, setLayer] = useState<"play" | "menu">("play");
  /** The button names the texts mention. */
  const names = { view: buttonName(layout, "view"), lbName: buttonName(layout, "lb") };
  const callouts: Callout[] = pad[layer].map((b) => ({
    input: b.input,
    lines: [
      t(`controls.${b.label}`, { ...names, ...b.values }),
      ...(b.more ? [t(`controls.pad.${b.more}`, { ...names, ...b.values })] : []),
    ],
  }));
  const inputName = (input: PadInput) => {
    const name = inputNames[layout][input];
    if (name) return t(`controls.pad.input.${name}`);
    return input === "lt" ||
      input === "rt" ||
      input === "lb" ||
      input === "rb" ||
      input === "view" ||
      input === "menu"
      ? buttonName(layout, input)
      : input.toUpperCase();
  };
  return (
    <>
      <section className="grid gap-3 rounded-xl border border-border bg-surface p-4">
        <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-2">
          <Heading className="text-xs font-semibold tracking-wide text-muted uppercase">
            {t("controls.view.controller")}
          </Heading>
          <Choice
            label={t("controls.pad.layer.label")}
            value={layer}
            onChange={setLayer}
            options={[
              { value: "play", label: t("controls.pad.layer.play") },
              { value: "menu", label: t("controls.pad.layer.menu", names) },
            ]}
          />
        </div>
        <PadDrawing callouts={callouts} layout={layout} />
        <ul className="sr-only">
          {callouts.map((c) => (
            <li key={c.input}>
              {inputName(c.input)}: {c.lines.join(", ")}
            </li>
          ))}
        </ul>
        {layer === "menu" && <MenuKeys keys={pad.menuKeys} inputName={inputName} names={names} />}
      </section>
      <div className="grid gap-4 sm:grid-cols-2">
        <section className="grid content-start gap-2 rounded-xl border border-border bg-surface p-4">
          <Heading className="text-xs font-semibold tracking-wide text-muted uppercase">
            {t("controls.pad.notes")}
          </Heading>
          <ul className="grid list-disc gap-1.5 pl-4 text-sm">
            <li>{t("controls.pad.plugIn")}</li>
            <li>{t("controls.pad.ownKeys")}</li>
            <li>{t("controls.pad.diagonals")}</li>
          </ul>
        </section>
        {children}
      </div>
    </>
  );
}

const directions = ["up", "right", "down", "left"] as const;

/** Which direction types which key with View (and LB) held. */
function MenuKeys({
  keys,
  inputName,
  names,
}: {
  keys: PadKeys[];
  inputName: (input: PadInput) => string;
  /** The button names the texts mention. */
  names: { view: string; lbName: string };
}) {
  const { t } = useTranslation();
  const rows = keys.flatMap((k) => [
    { name: inputName(k.input), keys: k.keys },
    ...(k.lb ? [{ name: `${inputName(k.input)} + ${names.lbName}`, keys: k.lb }] : []),
  ]);
  return (
    <table className="w-max max-w-full text-sm">
      <caption className="mb-2 text-left text-xs text-muted">
        {t("controls.pad.typed", names)}
      </caption>
      <thead>
        <tr className="text-xs text-muted">
          <th scope="col" className="sr-only">
            {t("controls.pad.input.label")}
          </th>
          {directions.map((d) => (
            <th key={d} scope="col" className="px-2 font-semibold">
              <span aria-hidden>{keyLabel(d, t)}</span>
              <span className="sr-only">{t(`controls.pad.direction.${d}`)}</span>
            </th>
          ))}
          <th scope="col" className="px-2 font-semibold">
            {t("controls.pad.press")}
          </th>
        </tr>
      </thead>
      <tbody>
        {rows.map((r) => (
          <tr key={r.name}>
            <th scope="row" className="py-1 pr-3 text-left font-normal text-muted">
              {r.name}
            </th>
            {[0, 1, 2, 3, 4].map((i) => (
              <td key={i} className="px-2 py-1 text-center">
                {r.keys[i] !== undefined ? <Keycap id={r.keys[i]} /> : null}
              </td>
            ))}
          </tr>
        ))}
      </tbody>
    </table>
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

/** Shown before a series' first start: the keys or the controller, then Play. */
export function ControlsDialog({
  profile,
  game,
  view,
  onStart,
  onClose,
}: {
  profile: Profile;
  game: Game;
  /** The view it opens on. */
  view: ControlsView;
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
      <ControlsSheet profile={profile} game={game} heading="h3" view={view} />
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
