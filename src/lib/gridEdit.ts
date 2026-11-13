// Submit's statements: qurry_shared::result_edit through WASM, the TUI's rules. DELETE runs first, then UPDATE, then INSERT; a row
// both edited and deleted is only deleted; a new row's cell never set is left out (the column default applies), one set to NULL is
// NULL; an all-default row is INSERT INTO t () VALUES () on MySQL and DEFAULT VALUES elsewhere. The GUI ends each with `;`
import type { Cell, ColumnMeta, DbKind, JoinEdit, ResultEditTarget } from "./api";
import { shared, sharedOr } from "./shared";

/** A new row's cells: undefined = never set (left out of the INSERT), null = set to NULL */
export type NewRow = (Cell | undefined)[];
/** Edited cells of the result's rows: row → column → value */
export type Changes = Record<number, Record<number, Cell>>;

const text = (c: Cell) => (c === null ? null : String(c));

function edits(changes: Changes): [number, number, string | null][] {
  return Object.entries(changes).flatMap(([r, ch]) => Object.entries(ch).map(([c, v]): [number, number, string | null] => [Number(r), Number(c), text(v)]));
}

/** The rows' raw text where Submit reads a key (edited and deleted rows); the other rows are left empty, so only those cross into WASM */
function keyRows(rows: Cell[][], changes: Changes, deleted: Iterable<number> = []): (string | null)[][] {
  const out: (string | null)[][] = rows.map(() => []);
  for (const r of [...Object.keys(changes).map(Number), ...deleted]) out[r] = rows[r].map(text);
  return out;
}

/** One table's statements; throws when there are rows to update or delete and the table has no primary key */
export function editStatements(kind: DbKind, to: ResultEditTarget, rows: Cell[][], changes: Changes, deleted: Set<number>, added: NewRow[]): string[] {
  const set = added.map((row) => row.flatMap((v, c): [number, string | null][] => (v === undefined ? [] : [[c, text(v)]])));
  const res = shared().editStatements(kind, to, keyRows(rows, changes, deleted), edits(changes), [...deleted], set);
  if ("Err" in res) throw new Error(res.Err);
  return res.Ok.map((s) => `${s};`);
}

/** A join's statements: per edited row, one UPDATE for each table its edits go to, found by that table's key */
export function joinStatements(kind: DbKind, j: JoinEdit, rows: Cell[][], changes: Changes): string[] {
  return shared().joinStatements(kind, j, keyRows(rows, changes), edits(changes)).map((s) => `${s};`);
}

/** What a failed Submit offers under its error: one line of what happened, and SQL to copy (Qurry never runs it) */
export interface ErrorFix {
  text: string;
  sql: string;
}

/** A failed Submit whose new row's key sequence is behind the table (qurry_shared::errors::sequence_behind, the TUI's rule). filled =
 *  the table's columns the database numbers (ColumnMeta.autoIncrement); undefined without WebAssembly */
export function sequenceBehind(kind: DbKind, error: string, schema: string, table: string, filled: string[]): ErrorFix | undefined {
  return sharedOr<ErrorFix | undefined>(undefined, (s) => s.sequenceBehind(kind, error, schema, table, filled) ?? undefined);
}

/** The fix under a failed Submit of one table's edits, if any. columns = the table's columns as loaded (lib/statusbar tableColumns),
 *  read only when the error is a duplicate of a key the INSERT left out */
export function submitFix(kind: DbKind, error: string, to: { schema: string; table: string; pk: string[] }, columns: () => ColumnMeta[] | undefined): ErrorFix | undefined {
  if (!sequenceBehind(kind, error, to.schema, to.table, to.pk)) return undefined;
  return sequenceBehind(kind, error, to.schema, to.table, (columns() ?? []).filter((c) => c.autoIncrement).map((c) => c.name));
}
