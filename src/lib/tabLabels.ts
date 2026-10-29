/** 탭 이름: 같은 이름의 테이블 탭이 여럿 열렸으면 구분되게 (DataGrip) — 스키마가 다르면 schema.table, 스키마도 같으면 db.schema.table */
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
