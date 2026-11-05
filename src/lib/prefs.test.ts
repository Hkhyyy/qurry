import { beforeEach, describe, expect, it, vi } from "vitest";

const updateOptions = vi.fn();
vi.mock("./monaco", () => ({ monaco: { editor: { getEditors: () => [{ updateOptions }], remeasureFonts: () => {} } } }));
const api = vi.hoisted(() => ({ customFonts: vi.fn(), fontFile: vi.fn(async () => new ArrayBuffer(4)), removeFontFile: vi.fn(), addFontFile: vi.fn() }));
vi.mock("./api", () => ({ api }));

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
    expect(names(fontOptions(installed, [], "", false, ""))).toEqual(["", "Courier New", "Helvetica", "Menlo"]);
    expect(names(fontOptions(installed, [], "", true, ""))).toEqual(["", "Courier New", "Menlo"]);
  });

  it("keeps the chosen family even when it is proportional and only monospaced are shown", async () => {
    const { fontOptions } = await load();
    expect(names(fontOptions(installed, [], "Helvetica", true, ""))).toEqual(["", "Courier New", "Helvetica", "Menlo"]);
    // Same family in another case (CSS matches family names case-insensitively): installed, not a missing row
    expect(fontOptions(installed, [], "menlo", true, "")).toEqual([{ family: "" }, { family: "Courier New" }, { family: "Menlo" }]);
  });

  it("puts a saved family that isn't installed on top, marked", async () => {
    const { fontOptions } = await load();
    expect(fontOptions(installed, [], " No Such Mono ", true, "").slice(0, 3)).toEqual([{ family: "" }, { family: "No Such Mono", missing: true }, { family: "Courier New" }]);
  });

  it("marks nothing missing while the list is loading or the OS can't list fonts", async () => {
    const { fontOptions } = await load();
    expect(fontOptions(null, [], "Hack", true, "")).toEqual([{ family: "" }, { family: "Hack", missing: false }]);
    expect(fontOptions([], [], "Hack", true, "")).toEqual([{ family: "" }, { family: "Hack", missing: false }]);
  });

  it("filters by a case-insensitive part of the name (the default row by its label)", async () => {
    const { fontOptions } = await load();
    expect(names(fontOptions(installed, [], "Gone", false, "NE"))).toEqual(["Gone", "Courier New"]);
    expect(names(fontOptions(installed, [], "", false, "MEN"))).toEqual(["Menlo"]);
    expect(names(fontOptions(installed, [], "", false, "default"))).toEqual([""]);
  });
});

describe("fontOptions with added font files", () => {
  const installed = [
    { name: "Hack", mono: true },
    { name: "Helvetica", mono: false },
  ];
  const custom = [
    { name: "hack", file: "hack.ttf" },
    { name: "Inter Display", file: "Inter Display.woff2" },
  ];

  it("lists the added fonts after the default, marked custom, whatever monospaced-only says; an installed family of the same name is left out", async () => {
    const { fontOptions } = await load();
    expect(fontOptions(installed, custom, "", true, "")).toEqual([{ family: "" }, { family: "hack", custom: true }, { family: "Inter Display", custom: true }]);
    expect(fontOptions(installed, custom, "", false, "").map((r) => r.family)).toEqual(["", "hack", "Inter Display", "Helvetica"]);
  });

  it("an added font set as the family is not a missing row; another family is marked missing only when the OS lists fonts", async () => {
    const { fontOptions } = await load();
    expect(fontOptions([], custom, "inter display", true, "")).toEqual([{ family: "" }, { family: "hack", custom: true }, { family: "Inter Display", custom: true }]);
    expect(fontOptions(installed, custom, "Gone", true, "")[1]).toEqual({ family: "Gone", missing: true });
    // Still loading, or the OS can't list fonts (Windows): an installed choice like Consolas isn't flagged because a font was added
    expect(fontOptions(null, custom, "Consolas", true, "")[1]).toEqual({ family: "Consolas", missing: false });
    expect(fontOptions([], custom, "Consolas", true, "")[1]).toEqual({ family: "Consolas", missing: false });
  });
});

describe("added font files", () => {
  const files: { name: string; file: string }[] = [];
  const added = new Set<string>();
  const load2 = async () => {
    const p = await load();
    const fonts = { add: (f: { family: string }) => added.add(f.family), delete: (f: { family: string }) => added.delete(f.family) };
    vi.stubGlobal("document", { documentElement: { style: { setProperty: (k: string, v: string) => void (css[k] = v) } }, fonts });
    vi.stubGlobal(
      "FontFace",
      class {
        constructor(
          public family: string,
          public bytes: ArrayBuffer,
        ) {}
        load = async () => {
          if (this.family === "Broken") throw new Error("invalid font data");
          return this;
        };
      },
    );
    return p;
  };
  beforeEach(() => {
    added.clear();
    files.splice(0, files.length, { name: "Fira Code", file: "Fira Code.ttf" });
    api.customFonts.mockImplementation(async () => [...files]);
    api.removeFontFile.mockImplementation(async (file: string) => void files.splice(files.findIndex((f) => f.file === file), 1));
  });

  it("registers each added font with FontFace; removing one unregisters it and sets a font that used it back to the default", async () => {
    const p = await load2();
    await p.loadCustomFonts();
    expect([...added]).toEqual(["Fira Code"]);
    expect(api.fontFile).toHaveBeenCalledWith("Fira Code.ttf");
    p.setFont("editor", { family: "fira code" });
    p.setFont("grid", { family: "Menlo" });
    await p.removeCustomFont(files[0], [{ name: "Menlo", mono: true }]);
    expect([...added]).toEqual([]);
    expect(p.font("editor").family).toBe("");
    expect(p.font("grid").family).toBe("Menlo");
  });

  it("removing an added font named like an installed one keeps a field set to it (the installed font renders again)", async () => {
    const p = await load2();
    await p.loadCustomFonts();
    p.setFont("editor", { family: "Fira Code" });
    await p.removeCustomFont(files[0], [{ name: "fira code", mono: true }]);
    expect([...added]).toEqual([]);
    expect(p.font("editor").family).toBe("Fira Code");
  });

  it("an added file that can't be loaded as a font is removed again and throws the reason", async () => {
    const p = await load2();
    api.addFontFile.mockImplementation(async () => {
      const f = { name: "Broken", file: "Broken.ttf" };
      files.push(f);
      return f;
    });
    await expect(p.addCustomFont()).rejects.toBe("Broken couldn't be loaded as a font (the file may be damaged or incomplete)");
    expect(api.removeFontFile).toHaveBeenLastCalledWith("Broken.ttf");
    expect(files.map((f) => f.name)).toEqual(["Fira Code"]);
    expect([...added]).toEqual(["Fira Code"]);
  });
});
