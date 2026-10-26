import { describe, expect, it } from "vitest";
import { autoMap, progressText } from "./transfer";

describe("transfer", () => {
  it("파일 열을 같은 이름의 컬럼에 붙인다 (대소문자·공백·_ 무시), 없으면 건너뛴다", () => {
    expect(autoMap(["ID", "User Name", "extra"], ["id", "user_name", "created_at"])).toEqual(["id", "user_name", ""]);
  });
  it("새 테이블이면 파일 열 이름 그대로", () => {
    expect(autoMap(["a", "b"], null)).toEqual(["a", "b"]);
  });
  it("진행: 전체를 알면 %, 모르면 행 수만", () => {
    expect(progressText({ rows: 1234, done: 1234, total: 5000 })).toEqual({ pct: 24, text: "1,234 rows · 24%" });
    expect(progressText({ rows: 1234, done: 1234, total: 0 })).toEqual({ pct: null, text: "1,234 rows" });
    // 추정치를 넘으면 99% 에서 멈춘다
    expect(progressText({ rows: 9000, done: 9000, total: 5000 }).pct).toBe(99);
  });
});
