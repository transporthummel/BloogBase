// Per-device UI preferences. Kept in localStorage for now; they move to the
// settings table once a settings screen needs to share them with the backend.

import { librarySorts, type LibrarySort } from "./library/sort";

export type Theme = "dark" | "light" | "system";
export type Language = "en" | "de";

const THEME_KEY = "bloogbase.theme";
const LANGUAGE_KEY = "bloogbase.language";
const SETUP_STEP_KEY = "bloogbase.setupStep";

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function write(key: string, value: string | null) {
  try {
    if (value === null) localStorage.removeItem(key);
    else localStorage.setItem(key, value);
  } catch {
    // Not fatal: the preference just won't survive a restart.
  }
}

export function getTheme(): Theme {
  const value = read(THEME_KEY);
  return value === "light" || value === "system" ? value : "dark";
}

export function setTheme(theme: Theme) {
  write(THEME_KEY, theme);
  applyTheme();
}

const lightQuery = window.matchMedia?.("(prefers-color-scheme: light)");

/** Sets `data-theme` on <html> from the stored preference and the OS setting. */
export function applyTheme() {
  const theme = getTheme();
  const light = theme === "light" || (theme === "system" && !!lightQuery?.matches);
  document.documentElement.dataset.theme = light ? "light" : "dark";
}

lightQuery?.addEventListener("change", applyTheme);

export function getLanguage(): Language {
  const stored = read(LANGUAGE_KEY);
  if (stored === "en" || stored === "de") return stored;
  return navigator.language.toLowerCase().startsWith("de") ? "de" : "en";
}

export function setLanguage(language: Language) {
  write(LANGUAGE_KEY, language);
}

/** The setup wizard step to resume at after an interruption or restart. */
export function getSetupStep(): string | null {
  return read(SETUP_STEP_KEY);
}

export function setSetupStep(step: string | null) {
  write(SETUP_STEP_KEY, step);
}

const LIBRARY_SORT_KEY = "bloogbase.librarySort";

export function getLibrarySort(): LibrarySort {
  const value = read(LIBRARY_SORT_KEY);
  return librarySorts.find((s) => s === value) ?? "recent";
}

export function setLibrarySort(sort: LibrarySort) {
  write(LIBRARY_SORT_KEY, sort);
}

const SEEN_TRIVIA_KEY = "bloogbase.seenTrivia";

/** The library fun facts this device has shown already. */
export function getSeenTrivia(): string[] {
  try {
    const value: unknown = JSON.parse(read(SEEN_TRIVIA_KEY) ?? "[]");
    return Array.isArray(value) ? value.filter((v) => typeof v === "string") : [];
  } catch {
    return [];
  }
}

export function setSeenTrivia(facts: string[]) {
  write(SEEN_TRIVIA_KEY, JSON.stringify(facts));
}

const SHOW_TRIVIA_KEY = "bloogbase.showTrivia";

/** Whether the library shows a fun fact. On unless turned off. */
export function getShowTrivia(): boolean {
  return read(SHOW_TRIVIA_KEY) !== "false";
}

export function setShowTrivia(show: boolean) {
  write(SHOW_TRIVIA_KEY, show ? null : "false");
}
