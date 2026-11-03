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
});
