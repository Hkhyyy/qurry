import { describe, expect, it } from "vitest";
import { keyTree, parentPaths, visibleRows, type KeyNode } from "./keyTree";

const show = (ns: KeyNode<string>[]): string[] => ns.map((n) => (n.kind === "folder" ? `/${n.name} ${n.count}` : n.item));

describe("keyTree", () => {
  it("Redis 키를 : 로 끊어 폴더로 묶는다 (폴더 먼저, 이름순)", () => {
    const top = keyTree(["token", "dev:sessions:{2}", "prod:x", "dev:admin:{1}", "dev:sessions:{1}"], (k) => k);
    expect(show(top)).toEqual(["/dev 3", "/prod 1", "token"]);
    const dev = top[0].kind === "folder" ? top[0].children : [];
    expect(show(dev)).toEqual(["/admin 1", "/sessions 2"]);
    const sessions = dev[1].kind === "folder" ? dev[1].children : [];
    expect(show(sessions)).toEqual(["dev:sessions:{2}", "dev:sessions:{1}"]);
    expect(dev[1].kind === "folder" && dev[1].path).toBe("dev:sessions");
  });
});

describe("visibleRows", () => {
  it("펼친 폴더만 안을 보이고, 깊이를 단다", () => {
    const tree = keyTree(["dev:sessions:{1}", "dev:sessions:{2}", "prod:x", "token"], (k) => k);
    const show = (open: string[]) =>
      visibleRows(tree, new Set(open)).map((r) => "  ".repeat(r.depth) + (r.kind === "folder" ? `/${r.name} ${r.count}` : r.item));
    expect(show([])).toEqual(["/dev 2", "/prod 1", "token"]);
    expect(show(["dev", "dev:sessions"])).toEqual(["/dev 2", "  /sessions 2", "    dev:sessions:{1}", "    dev:sessions:{2}", "/prod 1", "token"]);
  });
  it("키의 위 폴더 경로", () => {
    expect(parentPaths("dev:sessions:{1}")).toEqual(["dev", "dev:sessions"]);
    expect(parentPaths("token")).toEqual([]);
  });
});
