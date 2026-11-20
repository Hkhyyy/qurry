import { confirm, type ConfirmOptions } from "../ui";
import type { DbKind } from "./api";

/**
 * Confirm·text input dialogs. In the real app (Tauri), the dialog plugin replaces window.confirm with an async function (returns a Promise):
 * `if (!confirm(…)) return` passes without waiting for the answer since a Promise is always truthy. So always `await ask(…)`.
 * window.prompt doesn't exist in the Tauri webview (null) → in-app input dialog (TextDialog, registered by App)
 */
export async function ask(message: string, opts: Omit<ConfirmOptions, "message"> = {}): Promise<boolean> {
  // In-app confirm dialog (ui/ConfirmHost). Without it (tests), the browser confirm
  return (await confirm({ message, ...opts })) === true;
}

/** A message to acknowledge (OK only), e.g. a change that ran but whose result the Production log couldn't record */
export async function notice(message: string, title: string): Promise<void> {
  await confirm({ message, title, cancelLabel: null });
}

export interface TextRequest {
  title: string;
  value: string;
  /** Options to pick below the input (radio). choice is the initial pick */
  choices?: string[];
  choice?: number;
  resolve: (v: string | null, choice?: number) => void;
}
let show: ((r: TextRequest) => void) | null = null;

/** App registers the function that shows the input dialog */
export function registerTextDialog(fn: ((r: TextRequest) => void) | null) {
  show = fn;
}

/** Single-line input. null if cancelled */
export function askText(title: string, value = ""): Promise<string | null> {
  return new Promise((resolve) => (show ? show({ title, value, resolve }) : resolve(window.prompt(title, value))));
}

/** Single-line input + one choice (e.g. snippet name and visibility scope). null if cancelled */
export function askTextChoice(title: string, value: string, choices: string[], initial = 0): Promise<{ text: string; choice: number } | null> {
  return new Promise((resolve) => {
    const done = (v: string | null, choice?: number) => resolve(v === null ? null : { text: v, choice: choice ?? initial });
    if (show) show({ title, value, choices, choice: initial, resolve: done });
    else done(window.prompt(title, value));
  });
}

/** User parameters to fill in before a console run (ParamsDialog, which App holds): each name once, in order of appearance */
export interface ParamsRequest {
  names: string[];
  /** The console's last values, offered again */
  last: Record<string, string>;
  /** The statements that hold them, for the preview */
  sql: string;
  kind: DbKind;
  resolve: (values: Record<string, string> | null) => void;
}
let showParams: ((r: ParamsRequest) => void) | null = null;

/** App registers the function that shows the Parameters dialog */
export function registerParamsDialog(fn: ((r: ParamsRequest) => void) | null) {
  showParams = fn;
}

/** A value for each name. null if cancelled (without the dialog, as in tests: one prompt per name) */
export function askParams(names: string[], last: Record<string, string>, sql: string, kind: DbKind): Promise<Record<string, string> | null> {
  return new Promise((resolve) => {
    if (showParams) return showParams({ names, last, sql, kind, resolve });
    const values: Record<string, string> = {};
    for (const n of names) {
      const v = window.prompt(`:${n}`, last[n] ?? "");
      if (v === null) return resolve(null);
      values[n] = v;
    }
    resolve(values);
  });
}
