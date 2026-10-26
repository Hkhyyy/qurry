import { describe, expect, it } from "vitest";
import { keyTree, type KeyNode } from "./keyTree";

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
