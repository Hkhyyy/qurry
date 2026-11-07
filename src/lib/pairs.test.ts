import { describe, expect, it } from "vitest";
import { pairKey } from "./pairs";
import { shared } from "./shared";

/** Types `key` at the `|` (or over the «…» selection) and shows the field with its caret / selection the same way */
function ty(field: string, key: string, kind: "postgres" | "mysql" = "postgres"): string | undefined {
  const a = field.includes("«") ? field.indexOf("«") : field.indexOf("|");
  const b = field.includes("«") ? field.indexOf("»") - 1 : a;
  const value = field.replace(/[|«»]/g, "");
  const r = pairKey(value, a, b, key, kind);
  if (!r) return undefined;
  const v = r.value;
  return r.selStart === r.selEnd ? v.slice(0, r.selStart) + "|" + v.slice(r.selStart) : v.slice(0, r.selStart) + "«" + v.slice(r.selStart, r.selEnd) + "»" + v.slice(r.selEnd);
}

describe("pairKey (table tab WHERE / ORDER BY)", () => {
  it("closes quotes and brackets, types over the closing one, ⌫ removes an empty pair", () => {
    expect(ty("id in |", "(")).toBe("id in (|)");
    expect(ty("name = |", "'")).toBe("name = '|'");
    expect(ty("name = 'ab|'", "'")).toBe("name = 'ab'|");
    expect(ty("(1|)", ")")).toBe("(1)|");
    expect(ty("x = '|'", "Backspace")).toBe("x = |");
    expect(ty("(a|)", "Backspace")).toBeUndefined();
  });

  it("wraps a selection, leaves strings and comments alone", () => {
    expect(ty("«a + b» > 1", "(")).toBe("(«a + b») > 1");
    expect(ty("'a |'", "(")).toBeUndefined();
    expect(ty("x -- |", "'")).toBeUndefined();
    expect(ty("x # |", "(", "mysql")).toBeUndefined();
    expect(ty("|", "x")).toBeUndefined();
    expect(ty("|", "Enter")).toBeUndefined();
  });

  it("UTF-16 offsets around non-ASCII text", () => {
    expect(ty("이름 = |", "'")).toBe("이름 = '|'");
    expect(ty("«😀»", "'")).toBe("'«😀»'");
  });

  it("unbalanced: UTF-16 ranges of what is left open", () => {
    expect(shared().unbalanced("select '😀', (1", "postgres")).toEqual([{ start: 13, end: 14, message: "Unclosed '('" }]);
    expect(shared().unbalanced("select 1", "postgres")).toEqual([]);
  });
});
