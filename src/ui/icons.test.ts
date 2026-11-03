import { describe, expect, it } from "vitest";
import { ColumnIcon, ConsoleIcon, DdlIcon, DiagramIcon, MigrateIcon, objectIcon, RagIcon, RedisIcon, ReportIcon, RoutineIcon, SchemaIcon, SequenceIcon, TableIcon, tabIcon, ViewIcon } from "./icons";

describe("objectIcon: explorer tree object kind → its icon", () => {
  it.each([
    ["schema", SchemaIcon],
    ["table", TableIcon],
    ["partitioned", TableIcon],
    ["foreign", TableIcon],
    ["view", ViewIcon],
    ["matview", ViewIcon],
    ["sequence", SequenceIcon],
    ["routine", RoutineIcon],
    ["function", RoutineIcon],
    ["procedure", RoutineIcon],
    ["column", ColumnIcon],
  ])("%s", (kind, icon) => expect(objectIcon(kind)).toBe(icon));

  it("unknown kinds have none", () => {
    expect(objectIcon("trigger")).toBeUndefined();
    expect(objectIcon(undefined)).toBeUndefined();
  });
});

describe("tabIcon: what the tab shows → its icon", () => {
  it.each([
    ["console", undefined, ConsoleIcon],
    ["table", undefined, TableIcon],
    ["table", "view", ViewIcon],
    ["table", "matview", ViewIcon],
    ["ddl", "table", TableIcon],
    ["ddl", "view", ViewIcon],
    ["ddl", "routine", RoutineIcon],
    ["ddl", "function", RoutineIcon],
    ["ddl", "sequence", SequenceIcon],
    ["redis", undefined, RedisIcon],
    ["dba", undefined, ReportIcon],
    ["erd", undefined, DiagramIcon],
    ["migrate", undefined, MigrateIcon],
    ["rag", undefined, RagIcon],
  ])("%s %s", (kind, obj, icon) => expect(tabIcon(kind, obj)).toBe(icon));

  it("a DDL tab without (or with an unknown) object kind shows the DDL icon, not a routine's", () => {
    expect(tabIcon("ddl")).toBe(DdlIcon);
    expect(tabIcon("ddl", "trigger")).toBe(DdlIcon);
  });

  it("unknown tab kinds fall back to the DDL (SQL text) icon", () => expect(tabIcon("nope")).toBe(DdlIcon));

  it("the object kind only refines table and DDL tabs", () => expect(tabIcon("console", "view")).toBe(ConsoleIcon));
});
