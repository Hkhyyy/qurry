import { describe, expect, it } from "vitest";
import { draftAt, draftToApply, leftDraft, panelText, toggleJsonLayout, type PanelDraft } from "./valuePanel";

const draft = (over: Partial<PanelDraft> = {}): PanelDraft => ({ r: 1, c: 2, text: "new", base: "old", editable: true, ...over });

describe("value panel", () => {
  it("follows the selection: a draft belongs to its cell only", () => {
    expect(draftAt(draft(), { r: 1, c: 2 })).toBe(true);
    expect(draftAt(draft(), { r: 2, c: 2 })).toBe(false);
    expect(draftAt(null, { r: 1, c: 2 })).toBe(false);
  });

  it("hands over the draft when the selection leaves its cell", () => {
    expect(leftDraft(draft(), { r: 1, c: 2 })).toBeNull();
    expect(leftDraft(draft(), { r: 2, c: 2 })).toEqual(draft());
    expect(leftDraft(draft(), null)).toEqual(draft());
    expect(leftDraft(null, { r: 0, c: 0 })).toBeNull();
  });

  it("applies a changed draft, nothing when unchanged or read-only", () => {
    expect(draftToApply(draft(), "TEXT", "old")).toEqual({ text: "new" });
    expect(draftToApply(draft({ text: "old" }), "TEXT", "old")).toBeNull();
    expect(draftToApply(draft({ editable: false }), "TEXT", "old")).toBeNull();
  });

  it("shows jsonb pretty and writes a one-line value back on one line", () => {
    const original = '{"kind": "card", "n": 12345678901234567890}';
    const shown = panelText("JSONB", original);
    expect(shown).toEqual({ json: true, text: '{\n  "kind": "card",\n  "n": 12345678901234567890\n}' });
    const edited = shown.text.replace("card", "bank");
    expect(draftToApply(draft({ text: edited, base: shown.text }), "JSONB", original)).toEqual({ text: '{"kind":"bank","n":12345678901234567890}' });
    // A multi-line value keeps the panel's layout
    expect(draftToApply(draft({ text: edited, base: shown.text }), "JSON", '{\n"kind": "card"\n}')).toEqual({ text: edited });
  });

  it("refuses invalid JSON in a json / jsonb column, takes it in a text column", () => {
    const r = draftToApply(draft({ text: '{"a": ', base: '{"a": 1}' }), "jsonb", '{"a": 1}');
    expect(r && "error" in r && r.error).toMatch(/^Invalid JSON: /);
    expect(draftToApply(draft({ text: "[1,", base: "[\n  1\n]" }), "TEXT", "[1]")).toEqual({ text: "[1," });
  });

  it("toggles JSON between pretty and one line, leaves other text", () => {
    expect(toggleJsonLayout('{"a":[1]}')).toBe('{\n  "a": [\n    1\n  ]\n}');
    expect(toggleJsonLayout('{\n  "a": [\n    1\n  ]\n}')).toBe('{"a":[1]}');
    expect(toggleJsonLayout("plain")).toBe("plain");
  });
});
