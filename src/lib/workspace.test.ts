import { describe, expect, it } from "vitest";
import { initials } from "./workspace";

describe("workspace bar", () => {
  it("initials: first letter of each word, or the first two letters of a single word", () => {
    expect(initials("Default")).toBe("DE");
    expect(initials("billing reports")).toBe("BR");
    expect(initials("prod-hotfix")).toBe("PH");
    expect(initials("  ")).toBe("?");
    expect(initials("주문 분석")).toBe("주분");
  });
});
