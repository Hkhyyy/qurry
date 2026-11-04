import { beforeEach, describe, expect, it, vi } from "vitest";
import golden from "../../crates/shared/tests/goldens/valueclass.json";
import type { Cell } from "./api";
import type { Alignments } from "./shared";
import { DEFAULT_TOKEN } from "./extract";

const saved = vi.hoisted(() => ({ overrides: {} as Record<string, string>, fail: false }));
vi.mock("./api", () => ({
  api: {
    gridAlign: async () => saved.overrides,
    // As the core does: re-read the file, change the one class (its default drops it), return what is saved now
    setGridAlign: async (cls: string, a: string) => {
      if (saved.fail) throw "Couldn't save the settings: permission denied";
      const next = { ...saved.overrides };
      if (a === (cls === "number" ? "right" : cls === "null" ? "column" : "left")) delete next[cls];
      else next[cls] = a;
      saved.overrides = next;
      return next;
    },
  },
}));

// A WebView that refuses WebAssembly: the module never loads
const wasm = vi.hoisted(() => ({ refused: false }));
vi.mock("./shared", async (orig) => {
  const real = await orig<typeof import("./shared")>();
  return { ...real, sharedReady: () => !wasm.refused && real.sharedReady(), sharedOr: <T,>(fb: T, f: Parameters<typeof real.sharedOr<T>>[1]) => (wasm.refused ? fb : real.sharedOr(fb, f)) };
});

const { alignOf, alignTable, cellClasses, loadGridAlign, setGridAlign } = await import("./gridAlign");

describe("grid alignment", () => {
  beforeEach(async () => {
    saved.overrides = {};
    saved.fail = false;
    wasm.refused = false;
    await loadGridAlign();
  });

  it("classes and default alignments match the shared golden (the TUI's rule)", () => {
    for (const e of golden) {
      const cls = cellClasses(e.type)(e.value as Cell);
      const column = ("column" in e ? (e.column as Cell[]) : [e.value as Cell]).map((v) => [v]);
      expect({ cls, align: alignTable({}, e.type, column)[cls] }, `${e.type} ${JSON.stringify(e.value)}`).toEqual({ cls: e.class, align: e.align });
    }
  });

  it("a DEFAULT not yet written is a NULL (it shows as <default>)", () => {
    expect(cellClasses("int4")(DEFAULT_TOKEN)).toBe("null");
  });

  it("NULL sits where its column's type does by default; Left / Right are saved, picking the default again drops the override", async () => {
    expect(alignOf("null")).toBe("column");
    const nullIn = (t: string, o: Alignments = {}) => alignTable(o, t).null;
    expect(["text", "int4", "timestamptz", "bool", "jsonb"].map((t) => nullIn(t))).toEqual(["left", "right", "left", "left", "left"]);
    expect(alignTable({}, "text")).toEqual({ string: "left", number: "right", dateTime: "left", boolean: "left", null: "left", jsonBinary: "left" });
    // It follows the column's class as the user set it
    expect(nullIn("numeric", { number: "center" })).toBe("center");
    // A column without a type name (SQLite expression, MongoDB) goes by its first non-null value
    expect(alignTable({}, "", [[null], [12], [7]]).null).toBe("right");
    expect(alignTable({}, "", [[null], ["a long text"]]).null).toBe("left");
    await setGridAlign("null", "right");
    expect(saved.overrides).toEqual({ null: "right" });
    expect(["text", "int4", "bool"].map((t) => nullIn(t, saved.overrides))).toEqual(["right", "right", "right"]);
    await setGridAlign("null", "left");
    expect(saved.overrides).toEqual({ null: "left" });
    expect(alignOf("null")).toBe("left");
    await setGridAlign("number", "center");
    await setGridAlign("null", "column");
    expect(saved.overrides).toEqual({ number: "center" });
  });

  it("reads what was saved (also by another app) at start, skipping values this version doesn't know", async () => {
    saved.overrides = { null: "left", boolean: "justify", string: "column" };
    await loadGridAlign();
    expect(alignOf("null")).toBe("left");
    expect(alignOf("boolean")).toBe("left");
    expect(alignOf("string")).toBe("left");
  });

  it("a change saved by another app (the TUI) while the GUI is open is kept, and shows after the next save", async () => {
    saved.overrides = { null: "left" };
    await setGridAlign("number", "center");
    expect(saved.overrides).toEqual({ null: "left", number: "center" });
    expect(alignOf("null")).toBe("left");
  });

  it("a save that fails rolls the setting back and says why", async () => {
    saved.fail = true;
    await expect(setGridAlign("null", "left")).rejects.toMatch("permission denied");
    expect(alignOf("null")).toBe("column");
  });

  it("without WebAssembly cells get no alignment (the stylesheet keeps numbers on the right)", () => {
    wasm.refused = true;
    expect(alignTable({ null: "left" }, "text")).toEqual({});
  });
});
