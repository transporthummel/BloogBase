import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { commands } from "../bindings";
import { unwrap } from "../ipc";

export const readmeKeys = ["readme"] as const;
/** Invalidate it when the mod's files change, e.g. it's replaced. */
export const readmeKey = (modId: string) => [...readmeKeys, modId] as const;

/**
 * The documents of a profile, the README first: those shipped with its mod
 * (with the variant played, its own if it has any), else those of its game.
 * Off for a mod waiting for its game's files.
 */
export function useReadmes(
  profile:
    | {
        modId: string | null;
        installId: string | null;
        activeVariant?: { id: string } | null;
      }
    | undefined,
) {
  const modId = profile?.modId ?? null;
  const installId = profile?.installId ?? null;
  // Another variant, other documents: under readmeKey, which invalidates all.
  const variant = profile?.activeVariant?.id ?? null;
  return useQuery({
    queryKey:
      modId !== null ? [...readmeKey(modId), variant] : [...readmeKeys, "game", installId ?? ""],
    queryFn: () =>
      unwrap(modId !== null ? commands.modReadmes(modId) : commands.gameReadmes(installId!)),
    enabled: profile !== undefined && (modId !== null || installId !== null),
    // Those of the variant played before until another's are read, so the
    // page's tabs don't wait for them again.
    placeholderData: modId !== null ? keepPreviousData : undefined,
  });
}

/** The documents in a folder or ZIP not installed yet. */
export function useReadmesAt(path: string) {
  return useQuery({
    queryKey: [...readmeKeys, "at", path],
    queryFn: () => unwrap(commands.readmesAt(path)),
  });
}

export type Part = { text: string } | { url: string };

const URL = /\bhttps?:\/\/[^\s<>"'`]+/gi;

/** Splits text into plain runs and http(s) links, without trailing punctuation. */
export function linkify(text: string): Part[] {
  const parts: Part[] = [];
  let at = 0;
  for (const m of text.matchAll(URL)) {
    const url = m[0].replace(/[.,;:!?)\]}]+$/, "");
    if (m.index > at) parts.push({ text: text.slice(at, m.index) });
    parts.push({ url });
    at = m.index + url.length;
  }
  if (at < text.length) parts.push({ text: text.slice(at) });
  return parts;
}
