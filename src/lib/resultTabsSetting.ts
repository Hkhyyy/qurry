// Result tabs (Settings › Result tabs): unpinned result tabs kept per console (lib/resultTabs). Kept in settings.json
// ("resultTabs"), which the TUI reads too
import { useSyncExternalStore } from "react";
import { api } from "./api";

let current = 10;
const subs = new Set<() => void>();
const emit = () => subs.forEach((f) => f());

/** Read the saved number (at start). If that fails it stays 10 */
export async function loadResultTabs() {
  try {
    current = (await api.resultTabs()) || 10;
    emit();
  } catch {
    /* 10 */
  }
}

export const resultTabs = () => current;

/** Set and save; rolls back if saving fails */
export async function setResultTabs(n: number) {
  const prev = current;
  current = n;
  emit();
  try {
    await api.setResultTabs(n);
  } catch (e) {
    current = prev;
    emit();
    throw e;
  }
}

/** The saved number (re-renders on change) */
export function useResultTabs(): number {
  return useSyncExternalStore(
    (f) => (subs.add(f), () => subs.delete(f)),
    () => current,
  );
}
