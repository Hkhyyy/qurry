import { describe, expect, it } from "vitest";
import { listKey, matchOptions, placePopup } from "./popup";

const TYPES = ["integer", "bigint", "smallint", "numeric(10,2)", "text", "timestamp", "timestamptz", "time", "interval", "text[]"];

describe("matchOptions: free-text suggestions for a combo box", () => {
  it("lists everything for an empty query, in the given order", () => expect(matchOptions(TYPES, "  ")).toEqual(TYPES));
  it("prefix matches before the ones that only contain it, each in the given order", () => expect(matchOptions(TYPES, "int")).toEqual(["integer", "interval", "bigint", "smallint"]));
  it("ignores case", () => expect(matchOptions(TYPES, "BIG")).toEqual(["bigint"]));
  it("an exact match comes first, so ↩ keeps what was typed", () => expect(matchOptions(TYPES, "time")).toEqual(["time", "timestamp", "timestamptz"]));
  it("nothing matches: no rows (no popup)", () => expect(matchOptions(TYPES, "varchar(100)")).toEqual([]));
  it("punctuation is literal", () => expect(matchOptions(TYPES, "(10")).toEqual(["numeric(10,2)"]));
});

describe("listKey: the keyboard of a list popup", () => {
  it.each([
    ["ArrowDown", 3, 4],
    ["ArrowUp", 3, 2],
    ["PageDown", 3, 13],
    ["PageUp", 15, 5],
  ])("%s from %i → %i", (key, from, to) => expect(listKey(key, from, 50)).toBe(to));
  it("stays inside the list", () => {
    expect(listKey("ArrowUp", 0, 5)).toBe(0);
    expect(listKey("ArrowDown", 4, 5)).toBe(4);
    expect(listKey("PageUp", 3, 5)).toBe(0);
    expect(listKey("PageDown", 3, 5)).toBe(4);
  });
  it("↩ picks the highlighted row, ⎋ closes", () => {
    expect(listKey("Enter", 2, 5)).toBe("pick");
    expect(listKey("Escape", 2, 5)).toBe("close");
  });
  it("↩ in an empty list isn't handled (falls through to the field)", () => expect(listKey("Enter", 0, 0)).toBeNull());
  it("other keys aren't handled", () => {
    expect(listKey("a", 0, 5)).toBeNull();
    expect(listKey("Tab", 0, 5)).toBeNull();
  });
});

describe("placePopup: under the field, above it when there's no room, inside the window", () => {
  const view = { width: 1000, height: 800 };
  const field = { left: 300, top: 100, bottom: 126, width: 120 };
  it("right under the field, left-aligned to it", () => expect(placePopup(field, { width: 160, height: 200 }, view)).toEqual({ left: 300, top: 128 }));
  it("flips above when it doesn't fit below", () => expect(placePopup({ ...field, top: 700, bottom: 726 }, { width: 160, height: 200 }, view)).toEqual({ left: 300, top: 498 }));
  it("stays below when there's even less room above", () => expect(placePopup({ ...field, top: 150, bottom: 176 }, { width: 160, height: 700 }, view).top).toBe(178));
  it("never leaves the window on the right or the top", () => {
    expect(placePopup({ ...field, left: 900 }, { width: 160, height: 200 }, view).left).toBe(832);
    expect(placePopup({ ...field, top: 500, bottom: 526 }, { width: 160, height: 600 }, view).top).toBe(8);
  });
});
