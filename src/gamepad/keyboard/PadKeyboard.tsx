import { ArrowBigUp, ArrowBigUpDash, Check, Delete } from "lucide-react";
import { useEffect, useRef, useState, useSyncExternalStore, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { isPadKey, sendKey } from "../actions";
import type { PadAction } from "../pad";
import { backspace, insert, moveCaret, readField, writeField, type Field, type Text } from "./edit";
import {
  clamp,
  layout,
  nextShift,
  step,
  typed,
  type At,
  type Key,
  type Page,
  type Shift,
} from "./layout";
import { closeKeyboard, keyboardField, setKeyboardHandler, subscribe } from "./store";

/** Width of a key one unit wide, and the gap between keys, in rem. */
const UNIT = 3;
const GAP = 0.375;

function firstPage(field: Field): Page {
  return field instanceof HTMLInputElement && field.type === "number" ? "digits" : "letters";
}

/** Letters start on Q (the first letter row), digits on 1. */
function firstKey(page: Page): At {
  return page === "letters" ? { row: 1, col: 0 } : { row: 0, col: 0 };
}

/**
 * The controller's keyboard (plans/app-controller.md §6), docked at the
 * bottom of the window. The focus stays in the field, so it keeps its caret
 * and a `Dialog` around it keeps the keys; this keeps its own place on the
 * keys and takes the controller's buttons first (`keyboardAction`).
 */
export function PadKeyboard() {
  const field = useSyncExternalStore(subscribe, keyboardField);
  return field ? <Board key={keyId(field)} field={field} /> : null;
}

// A new field starts the keyboard afresh.
const ids = new WeakMap<Field, number>();
let nextId = 0;
function keyId(field: Field) {
  if (!ids.has(field)) ids.set(field, nextId++);
  return ids.get(field);
}

function Board({ field }: { field: Field }) {
  const { t, i18n } = useTranslation();
  const [page, setPage] = useState<Page>(() => firstPage(field));
  const [shift, setShift] = useState<Shift>("off");
  const [at, setAt] = useState<At>(() => firstKey(firstPage(field)));
  const keys = layout(i18n.language, page);
  const panel = useRef<HTMLDivElement>(null);

  const edit = (change: (t: Text) => Text) => writeField(field, change(readField(field)));

  const type = (text: string) => edit((v) => insert(v, text));

  const togglePage = () => {
    if (page === "digits") return;
    const next = page === "letters" ? "symbols" : "letters";
    setPage(next);
    setAt((a) => clamp(layout(i18n.language, next), a));
  };

  const close = () => {
    closeKeyboard();
    if (field.isConnected) field.focus({ preventScroll: true });
  };

  const done = () => {
    closeKeyboard();
    // Settings saves a number when it loses the focus.
    if (field instanceof HTMLInputElement && field.type === "number") return field.blur();
    // A field's own Enter rule (a mod's name while there's a warning) comes first.
    if (!sendKey(field, "Enter")) field.form?.requestSubmit();
  };

  const press = (key: Key) => {
    if (key.type === "char") {
      type(typed(key.char, shift));
      if (shift === "once") setShift("off");
      return;
    }
    switch (key.action) {
      case "shift":
        return setShift(nextShift);
      case "symbols":
        return togglePage();
      case "space":
        return type(" ");
      case "backspace":
        return edit(backspace);
      case "done":
        return done();
    }
  };

  const onAction = (a: PadAction) => {
    if (!field.isConnected) return closeKeyboard();
    switch (a) {
      case "up":
      case "down":
      case "left":
      case "right":
        return setAt((from) => step(keys, from, a));
      case "activate":
        return press(keys[at.row][at.col]);
      case "back":
        return close();
      case "x":
        return edit(backspace);
      case "y":
        return page === "digits" ? undefined : type(" ");
      case "lb":
        return edit((v) => moveCaret(v, -1));
      case "rb":
        return edit((v) => moveCaret(v, 1));
      case "lt":
        return togglePage();
      case "l3":
        return page === "letters" ? setShift(nextShift) : undefined;
      case "menu":
        return done();
    }
  };
  // The latest one, with this render's keys and place.
  const action = useRef(onAction);
  useEffect(() => {
    action.current = onAction;
  });

  useEffect(() => {
    setKeyboardHandler((a) => action.current(a));
    // A real key: the keyboard user carries on in the field.
    const onKey = (e: KeyboardEvent) => {
      if (!isPadKey(e)) closeKeyboard();
    };
    // Clicked elsewhere, the field left, the page or dialog gone.
    const onBlur = () => closeKeyboard();
    const gone = new MutationObserver(() => {
      if (!field.isConnected) closeKeyboard();
    });
    document.addEventListener("keydown", onKey, true);
    field.addEventListener("blur", onBlur);
    gone.observe(document.body, { childList: true, subtree: true });
    return () => {
      setKeyboardHandler(null);
      document.removeEventListener("keydown", onKey, true);
      field.removeEventListener("blur", onBlur);
      gone.disconnect();
    };
  }, [field]);

  // Room below the page and dialogs (index.css, Dialog), then the field
  // into view above the keys.
  useEffect(() => {
    const root = document.documentElement;
    const el = panel.current;
    if (!el) return;
    const fit = () => {
      root.style.setProperty("--bb-pad-keyboard", `${el.offsetHeight}px`);
      requestAnimationFrame(() => field.scrollIntoView?.({ block: "nearest" }));
    };
    fit();
    const resize = new ResizeObserver(fit);
    resize.observe(el);
    return () => {
      resize.disconnect();
      root.style.removeProperty("--bb-pad-keyboard");
    };
  }, [field]);

  const label = (key: Key): { content: ReactNode; name: string } => {
    if (key.type === "char") {
      const char = typed(key.char, shift);
      return { content: char, name: char };
    }
    switch (key.action) {
      case "shift": {
        const Icon = shift === "lock" ? ArrowBigUpDash : ArrowBigUp;
        return { content: <Icon className="size-5" aria-hidden />, name: t("pad.keyboard.shift") };
      }
      case "symbols":
        return {
          content: page === "symbols" ? "ABC" : "?123",
          name: t(page === "symbols" ? "pad.keyboard.letters" : "pad.keyboard.symbols"),
        };
      case "space":
        return { content: t("pad.keyboard.space"), name: t("pad.keyboard.space") };
      case "backspace":
        return {
          content: <Delete className="size-5" aria-hidden />,
          name: t("pad.keyboard.backspace"),
        };
      case "done":
        return {
          content: (
            <>
              <Check className="size-5" aria-hidden />
              {t("pad.keyboard.done")}
            </>
          ),
          name: t("pad.keyboard.done"),
        };
    }
  };

  return createPortal(
    <div
      ref={panel}
      role="group"
      aria-label={t("pad.keyboard.label")}
      data-pad-keyboard
      className="bb-keyboard-in fixed inset-x-0 bottom-0 z-[60] border-t border-border bg-surface px-4 pt-3 pb-2 shadow-card select-none"
    >
      <div className="mx-auto grid w-fit gap-1.5">
        {keys.map((row, r) => (
          <div key={r} className="flex justify-center gap-1.5">
            {row.map((key, c) => {
              const here = r === at.row && c === at.col;
              const { content, name } = label(key);
              const on = key.type === "action" && key.action === "shift" && shift !== "off";
              return (
                <button
                  key={c}
                  type="button"
                  tabIndex={-1}
                  aria-label={name}
                  aria-pressed={key.type === "action" && key.action === "shift" ? on : undefined}
                  data-current={here || undefined}
                  style={{ width: `${key.width * UNIT + (key.width - 1) * GAP}rem` }}
                  className={`flex h-12 items-center justify-center gap-2 rounded-lg border text-base font-semibold transition active:translate-y-px active:duration-0 ${
                    here
                      ? "border-accent bg-accent text-accent-ink shadow-card"
                      : `bg-surface-raised text-ink shadow-key hover:border-muted ${on ? "border-accent" : "border-border"}`
                  }`}
                  // The focus stays in the field.
                  onPointerDown={(e) => e.preventDefault()}
                  onClick={() => {
                    setAt({ row: r, col: c });
                    press(key);
                  }}
                >
                  {content}
                </button>
              );
            })}
          </div>
        ))}
        <Hints page={page} />
      </div>
    </div>,
    document.body,
  );
}

/** The face buttons in the colours of the Controls tab's drawing. */
const FACE = {
  a: "bg-pad-a",
  b: "bg-pad-b",
  x: "bg-pad-x",
  y: "bg-pad-y",
} as const;

function Face({ button }: { button: keyof typeof FACE }) {
  return (
    <span
      aria-hidden
      className={`inline-flex size-5 items-center justify-center rounded-full text-[0.7rem] font-bold text-pad-letter ${FACE[button]}`}
    >
      {button.toUpperCase()}
    </span>
  );
}

function Named({ children }: { children: ReactNode }) {
  return (
    <span
      aria-hidden
      className="inline-flex h-5 items-center rounded border border-border bg-surface-raised px-1.5 text-[0.7rem] font-semibold"
    >
      {children}
    </span>
  );
}

/** What the controller's buttons do here: the only button hints in the app. */
function Hints({ page }: { page: Page }) {
  const { t } = useTranslation();
  const hints: [ReactNode, string][] = [
    [<Face button="a" />, t("pad.keyboard.type")],
    [<Face button="x" />, t("pad.keyboard.backspace")],
    ...(page === "digits"
      ? []
      : ([
          [<Face button="y" />, t("pad.keyboard.space")],
          [<Named>LT</Named>, t("pad.keyboard.symbols")],
        ] as [ReactNode, string][])),
    ...(page === "letters"
      ? ([[<Named>L3</Named>, t("pad.keyboard.shift")]] as [ReactNode, string][])
      : []),
    [
      <>
        <Named>LB</Named>
        <Named>RB</Named>
      </>,
      t("pad.keyboard.caret"),
    ],
    [<Named>Menu</Named>, t("pad.keyboard.done")],
    [<Face button="b" />, t("pad.keyboard.close")],
  ];
  return (
    <ul className="mt-1 flex flex-wrap justify-center gap-x-4 gap-y-1 text-xs text-muted">
      {hints.map(([button, text]) => (
        <li key={text} className="flex items-center gap-1.5">
          {button}
          {text}
        </li>
      ))}
    </ul>
  );
}
