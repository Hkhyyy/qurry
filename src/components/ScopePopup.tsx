import { useEffect, useRef, useState } from "react";
import { ConnectionView, meta, Scope, SCOPE_ALL, SCOPE_DEFAULT, SchemaInfo, targetId } from "../lib/api";
import { IconButton } from "../ui";

/**
 * DataGrip의 스키마 선택 팝업 (Data Source / DB 노드의 "N of M" 배지를 누르면 열림).
 * Enter 또는 바깥 클릭 = 적용, Esc = 취소.
 */
interface Props {
  conn: ConnectionView;
  dbs: string[];
  defaultDb: string;
  /** DB 노드 배지에서 열었으면 그 DB를 펼친 상태로 시작 */
  focusDb?: string;
  x: number;
  y: number;
  onApply: (scope: Scope) => void;
  onClose: () => void;
}

const SYSTEM = ["information_schema", "pg_catalog"];

export function ScopePopup({ conn, dbs, defaultDb, focusDb, x, y, onApply, onClose }: Props) {
  const [scope, setScope] = useState<Scope>(() => ({
    databases: conn.scope?.databases?.length ? conn.scope.databases : [SCOPE_ALL],
    schemas: { ...(conn.scope?.schemas ?? {}) },
  }));
  const [open, setOpen] = useState<Set<string>>(new Set(focusDb ? [focusDb] : []));
  const [schemas, setSchemas] = useState<Record<string, SchemaInfo[] | "loading" | string>>({});
  const ref = useRef<HTMLDivElement>(null);
  const scopeRef = useRef(scope);
  scopeRef.current = scope;

  const load = (db: string, force = false) => {
    if (schemas[db] && schemas[db] !== "loading" && !force) return;
    setSchemas((s) => ({ ...s, [db]: "loading" }));
    meta.schemas(targetId(conn.id, db)).then(
      (list) => setSchemas((s) => ({ ...s, [db]: list })),
      (e) => setSchemas((s) => ({ ...s, [db]: String(e) })),
    );
  };

  useEffect(() => {
    if (focusDb) load(focusDb);
    // 저장할 때 "전부"는 필터 없음으로 정리
    const apply = () => {
      const sc = scopeRef.current;
      const schemas = Object.fromEntries(Object.entries(sc.schemas ?? {}).filter(([, v]) => !(v.length === 1 && v[0] === SCOPE_ALL)));
      const databases = sc.databases && !(sc.databases.length === 1 && sc.databases[0] === SCOPE_ALL) ? sc.databases : null;
      onApply({ databases, schemas });
    };
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) apply();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Enter") {
        e.preventDefault();
        apply();
      } else if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    // 팝업을 연 클릭이 바로 "바깥 클릭"으로 처리되지 않도록 다음 틱에 등록
    const t = setTimeout(() => {
      window.addEventListener("mousedown", onDown, true);
      window.addEventListener("keydown", onKey, true);
    });
    return () => {
      clearTimeout(t);
      window.removeEventListener("mousedown", onDown, true);
      window.removeEventListener("keydown", onKey, true);
    };
  }, []);

  // ─ DB 체크 상태 ─
  const dbList = scope.databases ?? [];
  const allDbs = dbList.includes(SCOPE_ALL);
  const defDb = dbList.includes(SCOPE_DEFAULT);
  const dbChecked = (db: string) => allDbs || dbList.includes(db) || (defDb && db === defaultDb);
  const dbImplicit = (db: string) => !dbList.includes(db) && dbChecked(db); // 전체/기본 규칙으로만 체크됨
  const setDbs = (next: string[]) => setScope((s) => ({ ...s, databases: next.length ? next : null }));
  const toggleDbFlag = (flag: string) => setDbs(dbList.includes(flag) ? dbList.filter((d) => d !== flag) : [...dbList, flag]);
  const toggleDb = (db: string) => setDbs(dbList.includes(db) ? dbList.filter((d) => d !== db) : [...dbList, db]);

  // ─ 스키마 체크 상태 ─
  // 스키마 필터가 없는 DB = 전부 보임 → "All schemas" 체크 상태로 표시
  const schList = (db: string) => (scope.schemas && db in scope.schemas ? scope.schemas[db] : [SCOPE_ALL]);
  const setSch = (db: string, next: string[]) =>
    setScope((s) => {
      const all = { ...(s.schemas ?? {}) };
      if (next.length) all[db] = next;
      else delete all[db];
      return { ...s, schemas: all };
    });
  const toggleSch = (db: string, name: string) => {
    const cur = schList(db);
    setSch(db, cur.includes(name) ? cur.filter((x) => x !== name) : [...cur, name]);
  };
  const schChecked = (db: string, name: string) => {
    const cur = schList(db);
    return cur.includes(SCOPE_ALL) || cur.includes(name) || (cur.includes(SCOPE_DEFAULT) && name === "public");
  };

  const toggleOpen = (db: string) =>
    setOpen((o) => {
      const n = new Set(o);
      if (n.has(db)) n.delete(db);
      else {
        n.add(db);
        load(db);
      }
      return n;
    });

  const left = Math.min(x, window.innerWidth - 320);
  const top = Math.min(y, window.innerHeight - 200);

  return (
    <div className="scope-popup" ref={ref} style={{ left, top, maxHeight: window.innerHeight - top - 12 }}>
      <div className="sp-toolbar">
        <span className="grow" />
        <IconButton title="Refresh" onClick={() => [...open].forEach((db) => load(db, true))}>⟳</IconButton>
        <IconButton title="Expand All" onClick={() => { setOpen(new Set(dbs)); dbs.forEach((db) => load(db)); }}>⌄</IconButton>
        <IconButton title="Collapse All" onClick={() => setOpen(new Set())}>⌃</IconButton>
        <IconButton title="Close (Esc)" onClick={onClose}>—</IconButton>
      </div>
      <div className="sp-list">
        <div className="sp-row">
          <span className="sp-tw" />
          <input type="checkbox" checked={allDbs} onChange={() => toggleDbFlag(SCOPE_ALL)} />
          <span>All databases</span>
        </div>
        <div className="sp-row">
          <span className="sp-tw" />
          <input type="checkbox" checked={defDb} onChange={() => toggleDbFlag(SCOPE_DEFAULT)} />
          <span>Default database</span>
          <span className="sp-hint">({defaultDb})</span>
        </div>
        {dbs.map((db) => {
          const sch = schemas[db];
          const isOpen = open.has(db);
          const checked = dbChecked(db);
          const cur = schList(db);
          return (
            <div key={db}>
              <div className={"sp-row" + (focusDb === db ? " focus" : "")}>
                <span className="sp-tw" onClick={() => toggleOpen(db)}>{isOpen ? "⌄" : "›"}</span>
                <input
                  type="checkbox"
                  checked={checked}
                  ref={(el) => {
                    if (el) el.indeterminate = dbImplicit(db);
                  }}
                  onChange={() => toggleDb(db)}
                />
                <span className={checked ? "sp-strong" : ""} onDoubleClick={() => toggleOpen(db)}>{db}</span>
                {db === defaultDb && <span className="sp-hint">(Default database)</span>}
              </div>
              {isOpen && (
                <div className="sp-children">
                  {sch === "loading" && <div className="sp-row muted">Loading…</div>}
                  {typeof sch === "string" && sch !== "loading" && <div className="sp-row err">{sch}</div>}
                  {Array.isArray(sch) && (
                    <>
                      <div className="sp-row">
                        <input type="checkbox" checked={cur.includes(SCOPE_ALL)} onChange={() => toggleSch(db, SCOPE_ALL)} />
                        <span>All schemas</span>
                      </div>
                      <div className="sp-row">
                        <input type="checkbox" checked={cur.includes(SCOPE_DEFAULT)} onChange={() => toggleSch(db, SCOPE_DEFAULT)} />
                        <span>Default schema</span>
                        <span className="sp-hint">(public)</span>
                      </div>
                      {[...sch]
                        .sort((a, b) => Number(!SYSTEM.includes(a.name)) - Number(!SYSTEM.includes(b.name)) || a.name.localeCompare(b.name))
                        .map((s) => (
                          <div key={s.name} className="sp-row">
                            <input type="checkbox" checked={schChecked(db, s.name)} onChange={() => toggleSch(db, s.name)} />
                            <span>{s.name}</span>
                            {s.isSystem && <span className="sp-bolt" title="System schema">⚡</span>}
                            {s.name === "public" && <span className="sp-hint">(Default schema)</span>}
                          </div>
                        ))}
                    </>
                  )}
                </div>
              )}
            </div>
          );
        })}
      </div>
      <div className="sp-footer">Press Enter or click outside the list to apply</div>
    </div>
  );
}
