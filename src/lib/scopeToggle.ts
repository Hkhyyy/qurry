import { SCOPE_ALL, SCOPE_DEFAULT } from "./api";

/**
 * Manage Shown Schemas: one click on a database line, or on a schema line of a database (DataGrip rules, same as the TUI's scope.rs).
 * cur = the stored list ("*" = all, "@default" = def), listed = the databases / schemas the popup lists,
 * def = what "@default" stands for (the source's default database, or public).
 * With All on, unchecking one keeps the others checked; checking the last unchecked one turns All back on.
 */
export function toggleScoped(cur: string[], name: string, listed: string[], def: string): string[] {
  if (name === SCOPE_ALL || name === SCOPE_DEFAULT) return cur.includes(name) ? cur.filter((x) => x !== name) : [...cur, name];
  if (cur.includes(SCOPE_ALL)) return [...cur.filter((x) => x === SCOPE_DEFAULT), ...listed.filter((x) => x !== name)];
  const next = cur.includes(name) ? cur.filter((x) => x !== name) : [...cur, name];
  const checked = (s: string) => next.includes(s) || (s === def && next.includes(SCOPE_DEFAULT));
  return listed.every(checked) ? [...next.filter((x) => x === SCOPE_DEFAULT), SCOPE_ALL] : next;
}
