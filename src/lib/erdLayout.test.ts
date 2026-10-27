import { describe, expect, it } from "vitest";
import { layout } from "./erdLayout";

const t = (name: string, cols = 3) => ({ key: name, width: 100, height: 20 + cols * 10 });

describe("erdLayout", () => {
  it("가리켜지는 테이블이 위, 가리키는 테이블이 아래 (겹치지 않게)", () => {
    const pos = layout([t("order_items"), t("orders"), t("customers"), t("products"), t("tags")], [
      { from: "orders", to: "customers" },
      { from: "order_items", to: "orders" },
      { from: "order_items", to: "products" },
    ]);
    const y = (k: string) => pos.get(k)!.y;
    expect(y("customers")).toBeLessThan(y("orders"));
    expect(y("orders")).toBeLessThan(y("order_items"));
    expect(y("products")).toBeLessThan(y("order_items"));
    // 겹치지 않는다
    const boxes = [...pos.entries()].map(([k, p]) => ({ k, ...p, w: 100, h: 50 }));
    for (const a of boxes) for (const b of boxes) if (a.k !== b.k) expect(a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y).toBe(true);
  });
  it("순환 관계도 끝난다", () => {
    const pos = layout([t("a"), t("b")], [{ from: "a", to: "b" }, { from: "b", to: "a" }, { from: "a", to: "a" }]);
    expect(pos.size).toBe(2);
  });
});
