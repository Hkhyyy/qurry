import { describe, expect, it } from "vitest";
import { fuzzyScore } from "./fuzzy";

describe("fuzzyScore", () => {
  it("order: exact > prefix > word start > middle > scattered letters", () => {
    const s = (t: string) => fuzzyScore("pay", t)!;
    expect(s("pay")).toBeGreaterThan(s("payment"));
    expect(s("payment")).toBeGreaterThan(s("user_payment"));
    expect(s("user_payment")).toBeGreaterThan(s("repay"));
    expect(s("repay")).toBeGreaterThan(fuzzyScore("upy", "user_payment")!);
  });

  it("null when the letters aren't there", () => {
    expect(fuzzyScore("xyz", "users")).toBeNull();
    expect(fuzzyScore("", "users")).toBe(0);
  });
});
