import { describe, expect, it } from "vitest";
import { dialectOf } from "./api";

describe("REST console (vector DBs)", () => {
  it("메서드 줄부터 다음 메서드 줄 전까지가 요청 하나 (본문의 빈 줄 포함)", () => {
    const t = "# list\nGET /collections\n\nPOST /collections/x/points/search\n{\n  vector: [1, 2],\n\n  limit: 3\n}\nDELETE /collections/y";
    const parts = dialectOf("qdrant").split!(t).map((s) => t.slice(s.start, s.end));
    expect(parts).toEqual(["GET /collections", "POST /collections/x/points/search\n{\n  vector: [1, 2],\n\n  limit: 3\n}", "DELETE /collections/y"]);
  });
});
