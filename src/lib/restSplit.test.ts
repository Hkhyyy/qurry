import { describe, expect, it } from "vitest";
import { dialectOf } from "./api";

describe("REST console (vector DBs)", () => {
  it("one request runs from a method line to the next method line (blank lines in the body included)", () => {
    const t = "# list\nGET /collections\n\nPOST /collections/x/points/search\n{\n  vector: [1, 2],\n\n  limit: 3\n}\nDELETE /collections/y";
    const parts = dialectOf("qdrant").split!(t).map((s) => t.slice(s.start, s.end));
    expect(parts).toEqual(["GET /collections", "POST /collections/x/points/search\n{\n  vector: [1, 2],\n\n  limit: 3\n}", "DELETE /collections/y"]);
  });
});
