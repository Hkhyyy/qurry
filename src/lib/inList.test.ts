import { describe, expect, it } from "vitest";
import { inList } from "./inList";

describe("inList", () => {
  it("한 열: 따옴표·쉼표·괄호, 중복과 NULL 은 뺀다", () => {
    expect(inList([["1.2.3.4"], ["5.6.7.8"], ["1.2.3.4"], [null], ["it's"]])).toBe("('1.2.3.4', '5.6.7.8', 'it''s')");
  });
  it("숫자는 따옴표 없이", () => {
    expect(inList([[1], [2], [2]])).toBe("(1, 2)");
  });
  it("여러 열: 행마다 묶음 ((a, b), …) — WHERE (x, y) IN …", () => {
    expect(inList([["a", 1], ["b", 2]])).toBe("(('a', 1), ('b', 2))");
  });
  it("값이 없으면 빈 목록 대신 NULL", () => {
    expect(inList([[null]])).toBe("(NULL)");
  });
});
