import { describe, expect, it } from "vitest";
import { tableTabLabels } from "./tabLabels";

describe("tableTabLabels", () => {
  it("leaves unique tables alone", () => {
    expect(tableTabLabels([{ id: "1", db: "app", schema: "public", table: "orders" }]).size).toBe(0);
  });
  it("adds the schema when the same table is open from two schemas", () => {
    const m = tableTabLabels([
      { id: "1", db: "app", schema: "public", table: "orders" },
      { id: "2", db: "app", schema: "billing", table: "orders" },
      { id: "3", db: "app", schema: "public", table: "users" },
    ]);
    expect([m.get("1"), m.get("2"), m.get("3")]).toEqual(["public.orders", "billing.orders", undefined]);
  });
  it("adds the database when the schema is the same too", () => {
    const m = tableTabLabels([
      { id: "1", db: "app", schema: "public", table: "orders" },
      { id: "2", db: "app_test", schema: "public", table: "orders" },
    ]);
    expect([m.get("1"), m.get("2")]).toEqual(["app.public.orders", "app_test.public.orders"]);
  });
});
