import { useCallback, useEffect, useState, type ReactNode } from "react";
import { meta, type ForeignKeyMeta, type TableStructure } from "../lib/api";
import { constraintLabel, fkSide } from "../lib/structure";
import { Button, Hint, Notice, Row, SectionTitle, Table, Td, Text, Th } from "../ui";
import { KeyIcon } from "../ui/icons";

/** DDL tab → Structure: a read-only view of a table in sections (DataGrip's object editor, without the editing) */
export function TableStructureView({ target, schema, table, onShowSql }: { target: string; schema: string; table: string; onShowSql: () => void }) {
  const [state, setState] = useState<{ s?: TableStructure; err?: string }>({});
  const load = useCallback(() => {
    setState({});
    meta.structure(target, schema, table).then(
      (s) => setState({ s }),
      (e) => setState({ err: String(e) }),
    );
  }, [target, schema, table]);
  useEffect(load, [load]);

  if (state.err)
    return (
      <div className="ddl-structure">
        <Notice tone="danger" className="ddl-structure-error">
          <div role="alert">
            Couldn't load the structure of {schema}.{table}: {state.err}
            <Row gap={3} style={{ marginTop: 8 }}>
              <Button size="sm" onClick={load}>Retry</Button>
              <Button size="sm" onClick={onShowSql}>Show SQL</Button>
              <Text tone="muted">The SQL view still has the full DDL.</Text>
            </Row>
          </div>
        </Notice>
      </div>
    );
  const s = state.s;
  if (!s)
    return (
      <div className="ddl-structure">
        <Text tone="muted">Loading the structure of {schema}.{table}…</Text>
      </div>
    );
  const fkRows = (list: ForeignKeyMeta[], incoming: boolean) =>
    list.map((f) => (
      <tr key={`${f.schema}.${f.table}.${f.name}`}>
        <Td mono>{f.name}</Td>
        <Td mono>{incoming ? fkSide(f, "from") : f.columns.join(", ")}</Td>
        <Td mono>{incoming ? f.refColumns.join(", ") : fkSide(f, "to")}</Td>
        <Td>{f.onUpdate}</Td>
        <Td>{f.onDelete}</Td>
      </tr>
    ));
  return (
    <div className="ddl-structure">
      <Section title="Columns" count={s.columns.length} head={["Name", "Type", "Nullable", "Default", "Comment"]}>
        {s.columns.map((c) => (
          <tr key={c.name}>
            <Td mono>
              {c.name}
              {c.isPk && (
                <span className="ddl-pk" title="Primary key">
                  <KeyIcon />
                </span>
              )}
            </Td>
            <Td mono>{c.dataType}</Td>
            <Td muted={!c.nullable}>{c.nullable ? "yes" : "no"}</Td>
            <Td mono>{c.defaultValue}</Td>
            <Td>{c.comment}</Td>
          </tr>
        ))}
      </Section>
      <Section title="Keys and constraints" count={s.constraints.length} head={["Name", "Kind", "Definition"]}>
        {s.constraints.map((c) => (
          <tr key={c.name}>
            <Td mono>{c.name}</Td>
            <Td>{constraintLabel(c.kind)}</Td>
            <Td mono>{c.definition}</Td>
          </tr>
        ))}
      </Section>
      <Section title="Indexes" count={s.indexes.length} head={["Name", "Columns", "Kind", "Definition"]}>
        {s.indexes.map((i) => (
          <tr key={i.name}>
            <Td mono>{i.name}</Td>
            <Td mono>{i.columns.join(", ")}</Td>
            <Td>{i.isPrimary ? "Primary" : i.isUnique ? "Unique" : ""}</Td>
            <Td mono muted>{i.definition}</Td>
          </tr>
        ))}
      </Section>
      <Section title="Foreign keys" count={s.foreignKeys?.length} head={["Name", "Columns", "References", "On update", "On delete"]}>
        {s.foreignKeys && fkRows(s.foreignKeys, false)}
      </Section>
      <Section title="Referenced by" count={s.referencedBy?.length} head={["Name", "From", "Columns", "On update", "On delete"]}>
        {s.referencedBy && fkRows(s.referencedBy, true)}
      </Section>
      <Section title="Triggers" count={s.triggers?.length} head={["Name", "Timing", "Events", "Definition"]}>
        {s.triggers?.map((t) => (
          <tr key={t.name}>
            <Td mono>{t.name}</Td>
            <Td>{t.timing}</Td>
            <Td>{t.events}</Td>
            <Td mono muted>{t.definition}</Td>
          </tr>
        ))}
      </Section>
      <Section title="Options and comment" head={["Option", "Value"]} available={!!s.options}>
        {[...(s.options ?? []), ...(s.comment ? [["Comment", s.comment]] : [])].map(([k, v]) => (
          <tr key={k}>
            <Td>{k}</Td>
            <Td mono={k !== "Comment"}>{v}</Td>
          </tr>
        ))}
      </Section>
    </div>
  );
}

/** One section: a title with the row count and a table; "None" when empty, a note when the source doesn't report it (count undefined) */
function Section({ title, count, head, available = count !== undefined, children }: { title: string; count?: number; head: string[]; available?: boolean; children: ReactNode }) {
  const label = count === undefined ? title : `${title} ${count}`;
  const empty = !available || (Array.isArray(children) && children.length === 0) || !children;
  return (
    <section className="ddl-section" aria-label={label}>
      <SectionTitle>
        {title}
        {count !== undefined && <span className="ddl-count">{count}</span>}
      </SectionTitle>
      {!available ? (
        <Hint>Not available for this data source</Hint>
      ) : empty ? (
        <Hint>None</Hint>
      ) : (
        <Table>
          <thead>
            <tr>
              {head.map((h) => (
                <Th key={h}>{h}</Th>
              ))}
            </tr>
          </thead>
          <tbody>{children}</tbody>
        </Table>
      )}
    </section>
  );
}
