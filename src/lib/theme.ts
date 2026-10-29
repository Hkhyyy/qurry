import { useSyncExternalStore } from "react";

/** Settings › Appearance › Theme. "system" = Sync with OS */
export type ThemePref = "dark" | "light" | "hc" | "system";
export type Theme = "dark" | "light" | "hc";

const KEY = "dbtool.theme";
const media = window.matchMedia("(prefers-color-scheme: dark)");
let pref: ThemePref = (() => {
  try {
    return (localStorage.getItem(KEY) as ThemePref) || "dark";
  } catch {
    return "dark";
  }
})();
const subs = new Set<() => void>();

const resolve = (): Theme => (pref === "system" ? (media.matches ? "dark" : "light") : pref);
let current: Theme = resolve();

function apply() {
  current = resolve();
  document.documentElement.dataset.theme = current;
  subs.forEach((f) => f());
}
media.addEventListener("change", () => pref === "system" && apply());
apply();

export function setThemePref(p: ThemePref) {
  pref = p;
  try {
    localStorage.setItem(KEY, p);
  } catch {
    /* Still applied for this run even if saving fails */
  }
  apply();
}
export const getThemePref = () => pref;
export const THEME_LABEL: Record<Theme, string> = { dark: "Dark", light: "Light", hc: "High contrast" };

/** Current theme (re-renders on change) */
export function useTheme(): Theme {
  return useSyncExternalStore(
    (f) => (subs.add(f), () => subs.delete(f)),
    () => current,
  );
}
export const monacoTheme = (t: Theme) => (t === "light" ? "dbtool-light" : t === "hc" ? "dbtool-hc" : "dbtool-dark");

/** Connection color (tree · tab background): blended faintly with the background — text stays readable, the strong color goes in the left stripe (connStripe) */
export const connBg = (color: string | null | undefined, t: Theme) =>
  !color ? undefined : t === "light" ? `color-mix(in srgb, ${color} 22%, #ffffff)` : t === "hc" ? color : `color-mix(in srgb, ${color} 35%, var(--panel))`;
/** Connection color stripe (tree left · tab top) — spot production DBs at a glance */
export const connStripe = (color: string | null | undefined, t: Theme) => (!color ? undefined : t === "light" ? `color-mix(in srgb, ${color} 75%, #ffffff)` : `color-mix(in srgb, ${color} 75%, #ffffff 25%)`);
