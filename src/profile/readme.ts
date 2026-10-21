import { useQuery } from "@tanstack/react-query";
import { commands } from "../bindings";
import { unwrap } from "../ipc";

export const readmeKeys = ["readme"] as const;
/** Invalidate it when the mod's files change, e.g. it's replaced. */
export const readmeKey = (modId: string) => [...readmeKeys, modId] as const;

/** The README shipped with a mod; `null` when it has none. Off for games. */
export function useReadme(modId: string | null) {
  return useQuery({
    queryKey: readmeKey(modId ?? ""),
    queryFn: () => unwrap(commands.modReadme(modId!)),
    enabled: modId !== null,
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
