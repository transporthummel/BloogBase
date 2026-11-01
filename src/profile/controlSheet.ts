import type { Engine, Firing, Game } from "../bindings";

/** Keys pressed together, e.g. `["ctrl", "alt"]`. Letters and digits stand for themselves. */
export type Chord = string[];

export interface Binding {
  keys: Chord;
  /** Under `controls.action` (`controls.cheat` for cheats). */
  action: string;
  /** BloogBase's one-button firing changed it from what the game shipped with. */
  changed?: boolean;
}

/** A game's default keys and its known cheats, all usable without start parameters. */
export interface Sheet {
  engine: Engine;
  /** What the arrow keys do, under `controls.move`. */
  move: Engine;
  actions: Binding[];
  game: Binding[];
  window: Binding[];
  cheats: Binding[];
  /** Cheats held with F10, and the keys that allow them first, if any. */
  debug?: { enable?: Chord; cheats: Binding[] };
}

/**
 * Whether Space shoots (and Enter opens the status window) in Keen 1-3:
 * through BloogBase's one-button firing, or a mod that does it itself.
 */
export function spaceFires(enabled: boolean | undefined, firing: Firing | null | undefined) {
  return firing === "builtIn" || (firing === "patchable" && !!enabled);
}

/**
 * The keys from the games' own help and source code (Keen 1's HELPTEXT,
 * Keen Dreams' source), the cheats from KeenWiki. Cheats that need a start
 * parameter are left out, as BloogBase can't set one.
 */
export function sheetFor(game: Pick<Game, "id" | "engine">, space: boolean, mac: boolean): Sheet {
  const ctrl = mac ? "cmd" : "ctrl";
  const window: Binding[] = [
    { keys: ["alt", "enter"], action: "fullscreen" },
    { keys: [ctrl, "f5"], action: "screenshot" },
    { keys: [ctrl, "f7"], action: "video" },
  ];
  switch (game.engine) {
    case "vorticons":
      return {
        engine: "vorticons",
        move: "vorticons",
        actions: [
          { keys: ["ctrl"], action: "jump" },
          { keys: ["alt"], action: game.id === "keen1" ? "pogoFound" : "pogo" },
          space
            ? { keys: ["space"], action: "fire", changed: true }
            : { keys: ["ctrl", "alt"], action: "fire" },
        ],
        game: [
          space
            ? { keys: ["enter"], action: "status", changed: true }
            : { keys: ["space"], action: "status" },
          { keys: ["esc"], action: "menu" },
          { keys: ["f1"], action: "help" },
          { keys: ["f2"], action: "sound" },
          { keys: ["f3"], action: "keys" },
          { keys: ["f5"], action: "save" },
        ],
        window,
        cheats: [
          { keys: ["c", "t", "space"], action: game.id === "keen1" ? "ctSpaceKeen1" : "ctSpace" },
          { keys: ["g", "o", "d"], action: "god" },
          { keys: ["shiftLeft", "tab"], action: "skip" },
        ],
      };
    case "galaxy":
      return {
        engine: "galaxy",
        move: "galaxy",
        actions: [
          { keys: ["ctrl"], action: "jump" },
          { keys: ["alt"], action: "pogo" },
          { keys: ["space"], action: "fire" },
        ],
        game: [
          { keys: ["enter"], action: "status" },
          { keys: ["esc"], action: "menu" },
          { keys: ["f1"], action: "help" },
        ],
        window,
        cheats: [{ keys: ["b", "a", "t"], action: "bat" }],
        debug: {
          enable: ["a", "2", "enter"],
          cheats: [
            { keys: ["f10", "g"], action: "god" },
            { keys: ["f10", "i"], action: "items" },
            { keys: ["f10", "j"], action: "jump" },
            { keys: ["f10", "w"], action: "warp" },
            { keys: ["f10", "y"], action: "secrets" },
            { keys: ["f10", "e"], action: "end" },
          ],
        },
      };
    case "dreams":
      return {
        engine: "dreams",
        move: "dreams",
        actions: [
          { keys: ["ctrl"], action: "jump" },
          { keys: ["alt"], action: "throw" },
        ],
        game: [
          { keys: ["space"], action: "status" },
          { keys: ["esc"], action: "menu" },
        ],
        window,
        cheats: [],
        debug: {
          cheats: [
            { keys: ["f10", "g"], action: "god" },
            { keys: ["f10", "i"], action: "itemsDreams" },
            { keys: ["f10", "j"], action: "jump" },
            { keys: ["f10", "w"], action: "warp" },
            { keys: ["f10", "s"], action: "slow" },
          ],
        },
      };
  }
}
