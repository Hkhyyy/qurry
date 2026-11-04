import { SCOPE_ALL, SCOPE_DEFAULT } from "./api";

/**
 * Manage Shown Schemas: one click on a schema line of a database (DataGrip rules, same as the TUI's scope.rs).
 * cur = that database's schema list ("*" = all, "@default" = public), listed = the schemas the popup lists.
 * With All on, unchecking a schema keeps the others checked; checking the last unchecked one turns All back on.
 */
export function toggleSchema(cur: string[], name: string, listed: string[]): string[] {
  if (name === SCOPE_ALL || name === SCOPE_DEFAULT) return cur.includes(name) ? cur.filter((x) => x !== name) : [...cur, name];
  if (cur.includes(SCOPE_ALL)) return [...cur.filter((x) => x === SCOPE_DEFAULT), ...listed.filter((x) => x !== name)];
  const next = cur.includes(name) ? cur.filter((x) => x !== name) : [...cur, name];
  const checked = (s: string) => next.includes(s) || (s === "public" && next.includes(SCOPE_DEFAULT));
  return listed.every(checked) ? [...next.filter((x) => x === SCOPE_DEFAULT), SCOPE_ALL] : next;
}
