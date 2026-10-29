import { confirm, type ConfirmOptions } from "../ui";

/**
 * Confirm·text input dialogs. In the real app (Tauri), the dialog plugin replaces window.confirm with an async function (returns a Promise):
 * `if (!confirm(…)) return` passes without waiting for the answer since a Promise is always truthy. So always `await ask(…)`.
 * window.prompt doesn't exist in the Tauri webview (null) → in-app input dialog (TextDialog, registered by App)
 */
export async function ask(message: string, opts: Omit<ConfirmOptions, "message"> = {}): Promise<boolean> {
  // In-app confirm dialog (ui/ConfirmHost). Without it (tests), the browser confirm
  return (await confirm({ message, ...opts })) === true;
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
