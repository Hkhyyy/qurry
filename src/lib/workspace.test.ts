import { describe, expect, it } from "vitest";
import { initials } from "./workspace";

describe("workspace bar", () => {
  it("머리글자: 단어가 여럿이면 각 첫 글자, 하나면 앞 두 글자", () => {
    expect(initials("Default")).toBe("DE");
    expect(initials("billing reports")).toBe("BR");
    expect(initials("prod-hotfix")).toBe("PH");
    expect(initials("  ")).toBe("?");
    expect(initials("주문 분석")).toBe("주분");
  });
});
