// Keyword case (Settings › Appearance › Keyword case): the letter case SQL keywords get as you type in a console and in Reformat
// Code. Kept in settings.json ("keywordCase"), which the TUI reads too; the rule itself is qurry_shared::format through WASM
import { useSyncExternalStore } from "react";
import { api } from "./api";
import type { KeywordCase } from "./shared";

let current: KeywordCase = "off";
const subs = new Set<() => void>();
const emit = () => subs.forEach((f) => f());

/** Read the saved case (at start). If that fails it stays off */
export async function loadKeywordCase() {
  try {
    current = (await api.keywordCase()) ?? "off";
    emit();
  } catch {
    /* Off */
  }
}

export const keywordCase = () => current;

/** Pick and save; rolls back if saving fails */
export async function setKeywordCase(c: KeywordCase) {
  const prev = current;
  current = c;
  emit();
  try {
    await api.setKeywordCase(c);
  } catch (e) {
    current = prev;
    emit();
    throw e;
  }
}

/** The saved case (re-renders on change) */
export function useKeywordCase(): KeywordCase {
  return useSyncExternalStore(
    (f) => (subs.add(f), () => subs.delete(f)),
    () => current,
  );
}
