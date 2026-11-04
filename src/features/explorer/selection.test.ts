import { describe, expect, it } from "vitest";
import type { MenuEntry } from "../../ui";
import { clickSelect, forSelection, NO_SEL, NOT_ALL, ONE_NODE, withoutNode } from "./selection";

const vis = ["conn", "db", "orders", "users", "items", "v_sales"];

describe("tree selection", () => {
  it("a plain click selects one node", () => {
    const s = clickSelect({ ids: ["orders", "users"], anchor: "users", lead: "users" }, "items", "one", vis);
    expect(s).toEqual({ ids: ["items"], anchor: "items", lead: "items" });
  });

  it("⌘-click adds and removes a node", () => {
    let s = clickSelect(NO_SEL, "orders", "one", vis);
    s = clickSelect(s, "items", "toggle", vis);
    expect(s.ids).toEqual(["orders", "items"]);
    s = clickSelect(s, "orders", "toggle", vis);
    expect(s).toEqual({ ids: ["items"], anchor: "orders", lead: "orders" });
  });

  it("⌘-click drops a folder that was selected (folders aren't selectable nodes) and acts as a click on one", () => {
    expect(clickSelect({ ids: ["folder"], anchor: "folder", lead: "folder" }, "users", "toggle", vis).ids).toEqual(["users"]);
    expect(clickSelect({ ids: ["users"], anchor: "users", lead: "users" }, "folder", "toggle", vis).ids).toEqual(["folder"]);
  });

  it("⇧-click selects the visible nodes from the anchor, either direction, and keeps the anchor", () => {
    let s = clickSelect(NO_SEL, "orders", "one", vis);
    s = clickSelect(s, "items", "range", vis);
    expect(s).toEqual({ ids: ["orders", "users", "items"], anchor: "orders", lead: "items" });
    s = clickSelect(s, "db", "range", vis);
    expect(s.ids).toEqual(["db", "orders"]);
  });

  it("⇧-click with an anchor no longer on screen selects one node", () => {
    expect(clickSelect({ ids: ["gone"], anchor: "gone", lead: "gone" }, "users", "range", vis).ids).toEqual(["users"]);
  });

  it("a node that leaves the screen leaves the selection; the lead and anchor move to the last remaining node", () => {
    const s = { ids: ["orders", "users", "items"], anchor: "orders", lead: "items" };
    expect(withoutNode(s, "users")).toEqual({ ids: ["orders", "items"], anchor: "orders", lead: "items" });
    expect(withoutNode(s, "items")).toEqual({ ids: ["orders", "users"], anchor: "orders", lead: "users" });
    expect(withoutNode(withoutNode(s, "orders"), "items")).toEqual({ ids: ["users"], anchor: "users", lead: "users" });
    // Every node gone (Collapse All): nothing is selected, so nothing acts
    expect(["orders", "users", "items"].reduce(withoutNode, s)).toEqual(NO_SEL);
    // A row that wasn't selected changes nothing
    expect(withoutNode(s, "v_sales")).toEqual(s);
  });
});

describe("context menu for several nodes", () => {
  const ran: string[] = [];
  const items: MenuEntry[] = [
    { label: "Rename…", id: "tree.rename", action: () => ran.push("rename") },
    { separator: true },
    { label: "Refresh", id: "tree.refresh", action: () => ran.push("one refresh") },
    { label: "Object Actions", submenu: [{ label: "Drop…", id: "tree.remove", danger: true, action: () => ran.push("one drop") }] },
  ];

  it("runs the several-node command, greys out the rest with a tooltip", () => {
    const m = forSelection(items, { "tree.refresh": () => ran.push("all refresh"), "tree.remove": undefined });
    expect(m[0]).toMatchObject({ disabled: true, title: ONE_NODE });
    expect(m[1]).toEqual({ separator: true });
    expect(m[2]).not.toHaveProperty("disabled");
    (m[2] as { action: () => void }).action();
    expect(ran).toEqual(["all refresh"]);
    expect((m[3] as { submenu: MenuEntry[] }).submenu[0]).toMatchObject({ label: "Drop…", disabled: true, title: NOT_ALL });
  });
});
