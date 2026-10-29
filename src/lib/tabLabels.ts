/** Tab name: distinguish table tabs with the same name (DataGrip) — schema.table if schemas differ, db.schema.table if the schema is also the same */
export interface TableTabKey {
  id: string;
  db: string;
  schema: string;
  table: string;
}

export function tableTabLabels(tabs: TableTabKey[]): Map<string, string> {
  const out = new Map<string, string>();
  for (const t of tabs) {
    const same = tabs.filter((x) => x.id !== t.id && x.table === t.table);
    if (!same.length) continue;
    out.set(t.id, same.some((x) => x.schema === t.schema) ? `${t.db}.${t.schema}.${t.table}` : `${t.schema}.${t.table}`);
  }
  return out;
}
