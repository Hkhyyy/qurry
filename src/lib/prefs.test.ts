import { beforeEach, describe, expect, it, vi } from "vitest";

const updateOptions = vi.fn();
vi.mock("./monaco", () => ({ monaco: { editor: { getEditors: () => [{ updateOptions }] } } }));

let store: Record<string, string>;
let css: Record<string, string>;
const load = async () => {
  vi.resetModules();
  vi.stubGlobal("localStorage", { getItem: (k: string) => store[k] ?? null, setItem: (k: string, v: string) => void (store[k] = v) });
  vi.stubGlobal("document", { documentElement: { style: { setProperty: (k: string, v: string) => void (css[k] = v) } } });
  return import("./prefs");
};
beforeEach(() => {
  store = {};
  css = {};
  updateOptions.mockClear();
});

describe("font prefs", () => {
  it("defaults to the monospace stack and the old sizes", async () => {
    const p = await load();
    expect(p.font("editor")).toEqual({ family: "", size: 13 });
    expect(p.font("grid")).toEqual({ family: "", size: 12 });
    expect(css["--grid-font-family"]).toBe(p.MONO);
    expect(css["--grid-font-size"]).toBe("12px");
  });

  it("persists family and size and reads them back on the next start", async () => {
    let p = await load();
    p.setFont("grid", { family: "Fira Code", size: 14 });
    p.setFont("editor", { family: "Menlo" });
    p = await load();
    expect(p.font("grid")).toEqual({ family: "Fira Code", size: 14 });
    expect(p.font("editor")).toEqual({ family: "Menlo", size: 13 });
    expect(css["--grid-font-family"]).toBe(`"Fira Code", ${p.MONO}`);
    expect(css["--grid-font-size"]).toBe("14px");
  });

  it("keeps the editor size saved by earlier versions", async () => {
    store["dbtool.editorFontSize"] = "17";
    expect((await load()).font("editor").size).toBe(17);
  });

  it("applies the editor font to open editors live", async () => {
    const p = await load();
    p.setFont("editor", { family: "Hack", size: 15 });
    expect(updateOptions).toHaveBeenLastCalledWith({ fontFamily: `"Hack", ${p.MONO}`, fontSize: 15 });
  });

  it("clamps sizes (the grid's rows are 24px high)", async () => {
    const p = await load();
    expect(p.setFont("editor", { size: 99 }).size).toBe(32);
    expect(p.setFont("grid", { size: 40 }).size).toBe(16);
    expect(p.setFont("grid", { size: 2 }).size).toBe(9);
  });

  it("still works when storage is blocked", async () => {
    const p = await load();
    vi.stubGlobal("localStorage", {
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("blocked");
      },
    });
    expect(p.setFont("grid", { size: 13 }).size).toBe(13);
    expect(css["--grid-font-size"]).toBe("13px");
  });
});

describe("fontStack", () => {
  it("puts the chosen family before the monospace fallback", async () => {
    const { fontStack, MONO } = await load();
    expect(fontStack("")).toBe(MONO);
    expect(fontStack("  ")).toBe(MONO);
    expect(fontStack(" D2Coding ")).toBe(`"D2Coding", ${MONO}`);
  });
  it("can't break out of the CSS value", async () => {
    const { fontStack, MONO } = await load();
    expect(fontStack('x"; color: red')).toBe(`"x color: red", ${MONO}`);
  });
  it("keeps a stack saved by an earlier version (typed in) one family, so the picker shows it as not installed", async () => {
    const { fontStack, MONO } = await load();
    expect(fontStack("Fira Code, monospace")).toBe(`"Fira Code monospace", ${MONO}`);
  });
});

describe("fontOptions (Settings font picker rows)", () => {
  const installed = [
    { name: "Courier New", mono: true },
    { name: "Helvetica", mono: false },
    { name: "Menlo", mono: true },
  ];
  const names = (rows: { family: string }[]) => rows.map((r) => r.family);

  it("lists the default, then the installed families; monospaced only when asked", async () => {
    const { fontOptions } = await load();
    expect(names(fontOptions(installed, "", false, ""))).toEqual(["", "Courier New", "Helvetica", "Menlo"]);
    expect(names(fontOptions(installed, "", true, ""))).toEqual(["", "Courier New", "Menlo"]);
  });

  it("keeps the chosen family even when it is proportional and only monospaced are shown", async () => {
    const { fontOptions } = await load();
    expect(names(fontOptions(installed, "Helvetica", true, ""))).toEqual(["", "Courier New", "Helvetica", "Menlo"]);
    // Same family in another case (CSS matches family names case-insensitively): installed, not a missing row
    expect(fontOptions(installed, "menlo", true, "")).toEqual([{ family: "" }, { family: "Courier New" }, { family: "Menlo" }]);
  });

  it("puts a saved family that isn't installed on top, marked", async () => {
    const { fontOptions } = await load();
    expect(fontOptions(installed, " No Such Mono ", true, "").slice(0, 3)).toEqual([{ family: "" }, { family: "No Such Mono", missing: true }, { family: "Courier New" }]);
  });

  it("marks nothing missing while the list is loading or the OS can't list fonts", async () => {
    const { fontOptions } = await load();
    expect(fontOptions(null, "Hack", true, "")).toEqual([{ family: "" }, { family: "Hack", missing: false }]);
    expect(fontOptions([], "Hack", true, "")).toEqual([{ family: "" }, { family: "Hack", missing: false }]);
  });

  it("filters by a case-insensitive part of the name (the default row by its label)", async () => {
    const { fontOptions } = await load();
    expect(names(fontOptions(installed, "Gone", false, "NE"))).toEqual(["Gone", "Courier New"]);
    expect(names(fontOptions(installed, "", false, "MEN"))).toEqual(["Menlo"]);
    expect(names(fontOptions(installed, "", false, "default"))).toEqual([""]);
  });
});
