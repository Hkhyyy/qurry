import { useEffect, useRef, useState } from "react";
import { missingForListing, TestElapsed } from "../../components/ConnectionDialog";
import { needsTlsOptIn } from "../../components/TlsFix";
import { api, ConnectionConfig, DbKind, DRIVER_MISSING, isSystemDb, meta, SCOPE_ALL, SchemaInfo, targetId } from "../../lib/api";
import { scopeChecked, toggleScoped } from "../../lib/scopeToggle";
import { Button, EmptyState, Notice, Text, TlsOptIn } from "../../ui";

type Scope = NonNullable<ConnectionConfig["scope"]>;

/** The form's settings, for listing a data source that isn't saved yet */
interface FormSettings {
  config: ConnectionConfig;
  password?: string;
  sshSecret?: string;
}

/**
 * Schemas tab of DataGrip Data Source properties: choose which DBs and schemas show in the tree. A saved source lists through its
 * saved settings; a new one connects with the form's settings, as Test Connection does, and keeps nothing (DataGrip does the same)
 */
export function SchemasTab({ connId, form, cancelRef, onAllowPlaintext, kind, defaultDb, scope, onChange }: { connId?: string; form: FormSettings; cancelRef: { current: (() => void) | null }; onAllowPlaintext: () => void; kind: DbKind; defaultDb: string; scope: Scope; onChange: (s: Scope) => void }) {
  const [dbs, setDbs] = useState<string[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [schemas, setSchemas] = useState<Record<string, SchemaInfo[] | "loading" | { error: string }>>({});
  const [openDb, setOpenDb] = useState<string | null>(null);
  // The dialog's settings now: Try Again and expanding a database use them as they are then (Allow unencrypted, a fix made after an error)
  const formNow = useRef(form);
  formNow.current = form;
  const [attempt, setAttempt] = useState(0);
  const retry = () => (setErr(null), setAttempt((n) => n + 1));
  const wrap = useRef<HTMLDivElement>(null);
  // Schemas being listed for an expanded database (a new source: each connects on its own), stopped when the tab or the dialog closes
  const expanding = useRef(new Set<string>());
  useEffect(() => () => expanding.current.forEach((id) => void api.cancelCall(id)), []);

  const missing = connId ? [] : missingForListing(form.config);
  const ready = missing.length === 0;
  useEffect(() => {
    if (connId) {
      meta.databases(connId).then(setDbs, (e) => setErr(String(e)));
      return;
    }
    if (!ready) return;
    // Cancel, Esc, leaving the tab and closing the dialog stop the connect, and with it its SSH / AWS SSM tunnel
    const id = crypto.randomUUID();
    let running = true;
    const done = () => {
      running = false;
      cancelRef.current = null;
    };
    const stop = () => (done(), void api.cancelCall(id));
    cancelRef.current = () => (stop(), setErr("Cancelled."));
    const f = formNow.current;
    api.formDatabases(f.config, f.password, f.sshSecret, id).then(
      (v) => running && (done(), setDbs(v)),
      (e) => running && (done(), setErr(String(e))),
    );
    return () => void (running && stop());
  }, [connId, attempt, cancelRef, ready]);

  // Try Again, Cancel and the list replace the button that had focus: keep focus in the dialog, so Esc still stops or closes
  useEffect(() => {
    if (document.activeElement === document.body) wrap.current?.querySelector<HTMLElement>("button, input")?.focus();
  }, [dbs, err]);

  return (
    <div ref={wrap} style={{ display: "contents" }}>
      {body()}
    </div>
  );

  function body() {
    if (!ready) {
      // Not an error: a new source isn't filled in yet. Say what's left, by tab
      const tabs = [...new Set(missing.map(([t]) => t))];
      const list = tabs.map((t) => `${missing.filter(([x]) => x === t).map(([, f]) => f).join(" and ")} on the ${t} tab`).join(", ");
      return <EmptyState>Fill in {list} to list the databases here.</EmptyState>;
    }
    if (err && !connId) {
      // Fixed where Test Connection fixes them: say so (the TLS opt-in is offered right here, as under a failed test)
      const fixedByTest = err.includes("Unknown SSH host key") ? "trust the SSH host key" : err.includes(DRIVER_MISSING) ? "install the driver" : null;
      const cfg = formNow.current.config;
      return (
        <>
          <Notice tone="danger">{"✕ " + err}</Notice>
          {fixedByTest && <Text as="div" tone="muted">Test Connection offers to {fixedByTest}; then Try Again.</Text>}
          {needsTlsOptIn(err) && <TlsOptIn host={cfg.host} production={!!cfg.options?.confirmChanges} inDialog onAllow={async () => (onAllowPlaintext(), retry())} />}
          <div className="ui-row conn-test-actions">
            <Button onClick={retry}>Try Again</Button>
          </div>
        </>
      );
    }
    if (err) return <Notice tone="danger">{err}</Notice>;
    if (!dbs && !connId)
      return (
        <EmptyState action={<Button onClick={() => cancelRef.current?.()}>Cancel Listing</Button>}>
          <TestElapsed label="Connecting with this dialog's settings…" />
        </EmptyState>
      );
    if (!dbs) return <EmptyState>Loading databases…</EmptyState>;

    // Same rules as Manage Shown Schemas (ScopePopup): no filter = All, stored as no filter
    const isAll = (v: string[]) => !v.length || (v.length === 1 && v[0] === SCOPE_ALL);
    const selDbs = scope.databases?.length ? scope.databases : [SCOPE_ALL];
    const allDbs = selDbs.includes(SCOPE_ALL);
    const toggleDb = (db: string) => {
      // System databases stay out of the list that replaces All, as in Manage Shown Schemas
      const next = toggleScoped(selDbs, db, dbs.filter((d) => d === defaultDb || !isSystemDb(kind, d)), defaultDb);
      onChange({ ...scope, databases: isAll(next) ? null : next });
    };
    const loadSchemas = (db: string) => {
      setOpenDb(openDb === db ? null : db);
      // Listed or listing: kept. Failed: expanding again tries again
      const cur = schemas[db];
      if (cur === "loading" || Array.isArray(cur)) return;
      setSchemas((s) => ({ ...s, [db]: "loading" }));
      const id = crypto.randomUUID();
      const f = formNow.current;
      if (!connId) expanding.current.add(id);
      (connId ? meta.schemas(targetId(connId, db)) : api.formSchemas(f.config, db, f.password, f.sshSecret, id))
        .then(
          (list) => setSchemas((s) => ({ ...s, [db]: list.filter((x) => !x.isSystem) })),
          (e) => setSchemas((s) => ({ ...s, [db]: { error: String(e) } })),
        )
        .finally(() => expanding.current.delete(id));
    };
    const schList = (db: string) => (scope.schemas?.[db]?.length ? scope.schemas[db] : [SCOPE_ALL]);
    const toggleSchema = (db: string, name: string) => {
      const sch = schemas[db];
      const next = toggleScoped(schList(db), name, Array.isArray(sch) ? sch.map((x) => x.name) : [], "public");
      const all = { ...(scope.schemas ?? {}) };
      if (isAll(next)) delete all[db];
      else all[db] = next;
      onChange({ ...scope, schemas: all });
    };

    return (
      <div className="scope-list">
        <label className="scope-row all">
          <input type="checkbox" checked={allDbs} onChange={() => toggleDb(SCOPE_ALL)} /> All databases
        </label>
        {dbs.map((db) => {
          const sch = schemas[db];
          const selSch = schList(db);
          return (
            <div key={db}>
              <div className="scope-row">
                <span className="twisty" onClick={() => loadSchemas(db)}>{openDb === db ? "▾" : "▸"}</span>
                <label>
                  <input type="checkbox" checked={scopeChecked(selDbs, db, defaultDb)} onChange={() => toggleDb(db)} /> {db}
                </label>
                <Text tone="muted">{selSch.includes(SCOPE_ALL) ? "all schemas" : `${selSch.length} schemas`}</Text>
              </div>
              {openDb === db && (
                <div className="scope-children">
                  {sch === "loading" && <Text as="div" tone="muted">Loading…</Text>}
                  {sch && typeof sch === "object" && "error" in sch && <Text as="div" tone="danger">{sch.error}</Text>}
                  {Array.isArray(sch) &&
                    sch.map((x) => (
                      <label key={x.name} className="scope-row">
                        <input type="checkbox" checked={scopeChecked(selSch, x.name, "public")} onChange={() => toggleSchema(db, x.name)} /> {x.name}
                      </label>
                    ))}
                </div>
              )}
            </div>
          );
        })}
      </div>
    );
  }
}
