import { useSyncExternalStore } from "react";

/** Status bar right-side widgets (Line:Column, grid Aggregator). Used by whichever side was focused last */
let widgets: { caret?: string; aggregate?: string } = {};
const subs = new Set<() => void>();
export function setStatusWidget(k: "caret" | "aggregate", v: string | undefined) {
  if (widgets[k] === v) return;
  widgets = { ...widgets, [k]: v };
  subs.forEach((f) => f());
}
export const useStatusWidgets = () =>
  useSyncExternalStore(
    (f) => (subs.add(f), () => subs.delete(f)),
    () => widgets,
  );
