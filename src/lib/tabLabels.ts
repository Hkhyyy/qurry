// Tab name: distinguish table tabs with the same name (DataGrip) — schema.table if schemas differ, db.schema.table if the schema is also the same.
// qurry_shared::labels through WASM, as the TUI names its tabs. Until the module is loaded tabs keep their titles
import { sharedOr } from "./shared";

export interface TableTabKey {
  id: string;
  db: string;
  schema: string;
  table: string;
}

export function tableTabLabels(tabs: TableTabKey[]): Map<string, string> {
  const out = new Map<string, string>();
  sharedOr([], (s) => s.tableTabLabels(tabs)).forEach((label, i) => label !== undefined && out.set(tabs[i].id, label));
  return out;
}
