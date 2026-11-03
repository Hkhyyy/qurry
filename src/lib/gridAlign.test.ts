import { beforeEach, describe, expect, it, vi } from "vitest";
import golden from "../../crates/shared/tests/goldens/valueclass.json";
import type { Cell } from "./api";
import { DEFAULT_TOKEN } from "./extract";

const saved = vi.hoisted(() => ({ overrides: {} as Record<string, string>, fail: false }));
vi.mock("./api", () => ({
  api: {
    gridAlign: async () => saved.overrides,
    // As the core does: re-read the file, change the one class (its default drops it), return what is saved now
    setGridAlign: async (cls: string, a: string) => {
      if (saved.fail) throw "Couldn't save the settings: permission denied";
      const next = { ...saved.overrides };
      if (a === (cls === "number" || cls === "null" ? "right" : "left")) delete next[cls];
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
      expect({ cls, align: alignOf(cls, {}) }, `${e.type} ${JSON.stringify(e.value)}`).toEqual({ cls: e.class, align: e.align });
    }
  });

  it("a DEFAULT not yet written is a NULL (it shows as <default>)", () => {
    expect(cellClasses("int4")(DEFAULT_TOKEN)).toBe("null");
  });

  it("NULL is on the right by default; setting it back to Left is saved, picking the default again drops the override", async () => {
    expect(alignTable({})).toEqual({ string: "left", number: "right", dateTime: "left", boolean: "left", null: "right", jsonBinary: "left" });
    await setGridAlign("null", "left");
    expect(saved.overrides).toEqual({ null: "left" });
    expect(alignOf("null")).toBe("left");
    await setGridAlign("number", "center");
    await setGridAlign("null", "right");
    expect(saved.overrides).toEqual({ number: "center" });
  });

  it("reads what was saved (also by another app) at start, skipping values this version doesn't know", async () => {
    saved.overrides = { null: "left", boolean: "justify" };
    await loadGridAlign();
    expect(alignOf("null")).toBe("left");
    expect(alignOf("boolean")).toBe("left");
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
    expect(alignOf("null")).toBe("right");
  });

  it("without WebAssembly cells get no alignment (the stylesheet keeps numbers on the right)", () => {
    wasm.refused = true;
    expect(alignTable({ null: "left" })).toEqual({});
  });
});
