import { describe, expect, it } from "vitest";
import { keyTree, parentPaths, visibleRows, type KeyNode } from "./keyTree";

const show = (ns: KeyNode<string>[]): string[] => ns.map((n) => (n.kind === "folder" ? `/${n.name} ${n.count}` : n.item));

describe("keyTree", () => {
  it("splits Redis keys at : into folders (folders first, by name)", () => {
    const top = keyTree(["token", "dev:sessions:{2}", "prod:x", "dev:admin:{1}", "dev:sessions:{1}"], (k) => k);
    expect(show(top)).toEqual(["/dev 3", "/prod 1", "token"]);
    const dev = top[0].kind === "folder" ? top[0].children : [];
    expect(show(dev)).toEqual(["/admin 1", "/sessions 2"]);
    const sessions = dev[1].kind === "folder" ? dev[1].children : [];
    expect(show(sessions)).toEqual(["dev:sessions:{2}", "dev:sessions:{1}"]);
    expect(dev[1].kind === "folder" && dev[1].path).toBe("dev:sessions");
  });
  it("stops at 64 folder levels, so a key of thousands of : doesn't overflow the WASM stack", () => {
    const key = "a:".repeat(10_000) + "x";
    let nodes = keyTree([key], (k) => k);
    let depth = 0;
    for (; nodes.length === 1 && nodes[0].kind === "folder"; depth++) nodes = nodes[0].children;
    expect([depth, show(nodes)]).toEqual([64, [key]]);
    // The module still works afterwards
    expect(show(keyTree(["a:b"], (k) => k))).toEqual(["/a 1"]);
  });
});

describe("visibleRows", () => {
  it("shows the inside of expanded folders only, with depth", () => {
    const tree = keyTree(["dev:sessions:{1}", "dev:sessions:{2}", "prod:x", "token"], (k) => k);
    const show = (open: string[]) =>
      visibleRows(tree, new Set(open)).map((r) => "  ".repeat(r.depth) + (r.kind === "folder" ? `/${r.name} ${r.count}` : r.item));
    expect(show([])).toEqual(["/dev 2", "/prod 1", "token"]);
    expect(show(["dev", "dev:sessions"])).toEqual(["/dev 2", "  /sessions 2", "    dev:sessions:{1}", "    dev:sessions:{2}", "/prod 1", "token"]);
  });
  it("parent folder paths of a key", () => {
    expect(parentPaths("dev:sessions:{1}")).toEqual(["dev", "dev:sessions"]);
    expect(parentPaths("token")).toEqual([]);
  });
});
