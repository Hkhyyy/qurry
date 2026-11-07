import { describe, expect, it, vi } from "vitest";
import { shared } from "./shared";

const saved = vi.hoisted(() => ({ value: "lower", fail: false }));
vi.mock("./api", () => ({
  api: {
    keywordCase: async () => saved.value,
    setKeywordCase: async (c: string) => {
      if (saved.fail) throw "Couldn't save the settings: permission denied";
      saved.value = c;
    },
  },
}));

const { keywordCase, loadKeywordCase, setKeywordCase } = await import("./keywordCase");

describe("keyword case setting", () => {
  it("is off until loaded, then the saved one; a failed save rolls back", async () => {
    expect(keywordCase()).toBe("off");
    await loadKeywordCase();
    expect(keywordCase()).toBe("lower");
    await setKeywordCase("upper");
    expect([keywordCase(), saved.value]).toEqual(["upper", "upper"]);
    saved.fail = true;
    await expect(setKeywordCase("off")).rejects.toMatch("permission denied");
    expect(keywordCase()).toBe("upper");
  });
});

// The WASM facade's offsets are UTF-16, as Monaco counts (the rules themselves are tested in qurry_shared::format)
describe("reformat and keywordCaseAt through WASM", () => {
  it("re-cases the keyword the typed character ends, after text with characters outside the BMP", () => {
    const text = "select '🎉한글' from t";
    const end = text.indexOf(" from") + 5;
    expect(shared().keywordCaseAt(text, end, "postgres", "upper")).toEqual({ start: end - 4, text: "FROM" });
    expect(shared().keywordCaseAt(text, end, "postgres", "off")).toBeUndefined();
    expect(shared().keywordCaseAt("select '🎉 from ", 14, "postgres", "upper")).toBeUndefined();
  });

  it("reformats the statement at the caret, or the selection", () => {
    const text = "select '🎉';\nselect a from t";
    const r = shared().reformat(text, 0, 0, text.length, "mysql", "off");
    expect(r).toEqual({ start: text.indexOf("select a"), end: text.length, text: "SELECT\n    a\nFROM\n    t" });
    expect(shared().reformat(text, 0, 11, 0, "postgres", "lower")).toEqual({ start: 0, end: 11, text: "select\n    '🎉'" });
    expect(shared().reformat("GET k", 0, 0, 0, "redis", "upper")).toBeUndefined();
  });
});
