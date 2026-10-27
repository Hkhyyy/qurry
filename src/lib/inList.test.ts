import { describe, expect, it } from "vitest";
import { inList, isNumericType } from "./inList";

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
  it("숫자 컬럼은 타입으로 정한다: numeric·bigint 가 글로 와도 따옴표 없이 (값 그대로)", () => {
    expect(inList([["1234.50"], ["12345678901234567890"]], ["numeric"])).toBe("(1234.50, 12345678901234567890)");
    expect(inList([["1"]], ["int8"])).toBe("(1)");
    expect(inList([["NaN"]], ["numeric"])).toBe("('NaN')");
    // 숫자처럼 보여도 문자 컬럼이면 따옴표 (전화번호 010…, 우편번호 앞 0)
    expect(inList([["010"]], ["varchar"])).toBe("('010')");
    expect(isNumericType("interval")).toBe(false);
    expect(isNumericType("BIGINT UNSIGNED")).toBe(true);
    expect(isNumericType("double precision")).toBe(true);
  });
  it("값이 없으면 빈 목록 대신 NULL", () => {
    expect(inList([[null]])).toBe("(NULL)");
  });
});
