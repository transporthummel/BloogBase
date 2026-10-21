import en from "./trivia.txt?raw";
import de from "./trivia.de.txt?raw";
import { getSeenTrivia, setSeenTrivia } from "../preferences";

function lines(raw: string) {
  return raw
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}

/** Made-up Commander Keen fun facts, one per line. The English text is a fact's id. */
export const trivia = lines(en);

/** Translations, line by line in the same order as the English file. */
export const triviaTranslations: Record<string, string[]> = { de: lines(de) };

/** A fact in the app language, or in English if there's no translation. */
export function triviaText(fact: string, language: string): string {
  return triviaTranslations[language]?.[trivia.indexOf(fact)] ?? fact;
}

/**
 * Picks a fact not in `seen` and returns it with the new seen list. Facts are
 * remembered by text, so edits to the file keep the rest seen. Once all have
 * been shown a new round starts, without repeating the last fact right away.
 */
export function pickTrivia(
  facts: string[],
  seen: string[],
  random = Math.random,
): { fact: string; seen: string[] } | null {
  const known = new Set(facts);
  let kept = seen.filter((f) => known.has(f));
  let left = facts.filter((f) => !kept.includes(f));
  if (left.length === 0) {
    kept = facts.length > 1 ? kept.slice(-1) : [];
    left = facts.filter((f) => !kept.includes(f));
  }
  if (left.length === 0) return null;
  const fact = left[Math.floor(random() * left.length)];
  return { fact, seen: [...kept, fact] };
}

let current: string | null | undefined;

/** The fact for this app start: picked once, then the same until a restart. */
export function startupTrivia(): string | null {
  if (current === undefined) {
    const picked = pickTrivia(trivia, getSeenTrivia());
    if (picked) setSeenTrivia(picked.seen);
    current = picked?.fact ?? null;
  }
  return current;
}
