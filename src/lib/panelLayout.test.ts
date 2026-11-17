import { beforeEach, describe, expect, it } from "vitest";
import { moveOldLayout } from "./panelLayout";

class Mem implements Storage {
  m = new Map<string, string>();
  get length() { return this.m.size; }
  clear() { this.m.clear(); }
  getItem(k: string) { return this.m.get(k) ?? null; }
  key(i: number) { return [...this.m.keys()][i] ?? null; }
  removeItem(k: string) { this.m.delete(k); }
  setItem(k: string, v: string) { this.m.set(k, v); }
}

describe("moveOldLayout", () => {
  let s: Mem;
  beforeEach(() => (s = new Mem()));

  it("moves a size saved by version 2 (sorted ids, sizes in panel order) to version 4's key", () => {
    s.setItem("react-resizable-panels:dbtool-h", JSON.stringify({ "editors,explorer": { expandToSizes: {}, layout: [31.5, 68.5] }, editors: { expandToSizes: {}, layout: [100] } }));
    moveOldLayout("dbtool-h", ["explorer", "editors"], s);
    expect(JSON.parse(s.getItem("react-resizable-panels:dbtool-h:explorer:editors")!)).toEqual({ explorer: 31.5, editors: 68.5 });
    expect(s.getItem("react-resizable-panels:dbtool-h")).toBeNull();
  });

  it("replaces the defaults version 4 saved meanwhile, once", () => {
    s.setItem("react-resizable-panels:dbtool-v", JSON.stringify({ "find,top": { expandToSizes: {}, layout: [55, 45] } }));
    s.setItem("react-resizable-panels:dbtool-v:top:find", JSON.stringify({ top: 70, find: 30 }));
    moveOldLayout("dbtool-v", ["top", "find"], s);
    expect(JSON.parse(s.getItem("react-resizable-panels:dbtool-v:top:find")!)).toEqual({ top: 55, find: 45 });
    s.setItem("react-resizable-panels:dbtool-v:top:find", JSON.stringify({ top: 60, find: 40 }));
    moveOldLayout("dbtool-v", ["top", "find"], s);
    expect(JSON.parse(s.getItem("react-resizable-panels:dbtool-v:top:find")!)).toEqual({ top: 60, find: 40 });
  });

  it("leaves the defaults when there is nothing old, or it can't be read", () => {
    moveOldLayout("dbtool-h", ["explorer", "editors"], s);
    expect(s.length).toBe(0);
    s.setItem("react-resizable-panels:dbtool-h", "{not json");
    moveOldLayout("dbtool-h", ["explorer", "editors"], s);
    expect(s.getItem("react-resizable-panels:dbtool-h:explorer:editors")).toBeNull();
  });
});
