import { describe, expect, it, vi } from "vitest";
import { shared } from "./shared";

describe("shared wasm", () => {
  it("answers synchronously (initialized by the vitest setup file)", () => {
    expect(shared().ping("héllo 한글")).toEqual({ text: "héllo 한글", chars: 8 });
  });

  it("titles a failed Commit's notice from the Rust error text", () => {
    const lost = "Connection reset by peer (os error 54) — connection lost while committing, so it is unknown whether the changes were committed. Check the data before you run them again (the next run reconnects).";
    expect(shared().commitFailedTitle(lost)).toBe("Commit outcome unknown");
    expect(shared().commitFailedTitle("Session closed")).toBe("Commit failed");
  });

  it("finds a failed statement's error spot as a UTF-16 offset (PostgreSQL counts characters; an emoji is two units here)", () => {
    const sql = "select '😀', x fro t";
    expect(shared().errorPosition('syntax error at or near "t" (position 19)', sql)).toBe(sql.lastIndexOf("t"));
    const my = "1064 (42000): You have an error in your SQL syntax; check the manual that corresponds to your MySQL server version for the right syntax to use near 'fro t' at line 2";
    expect(shared().errorPosition(my, "select 1,\n  x fro t")).toBe("select 1,\n  x ".length);
    expect(shared().errorPosition('relation "t" does not exist', "select * from t")).toBeUndefined();
    expect(shared().withoutPosition('relation "t" does not exist (position 15)')).toBe('relation "t" does not exist');
  });

  it("Redis Terminal key names: the typed key first, quoted when it has spaces or quotes; unknown key counts show the name only", () => {
    expect(shared().redisKeyChoices("user:1", ["user:10", "user:1"]).map((c) => c.label)).toEqual(["user:1", "user:10"]);
    expect(shared().redisKeyChoices("user:1", ["user:1"])).toEqual([]);
    expect(shared().redisKeyChoices("us", ["user 1"])).toEqual([{ label: "user 1", insert: '"user 1"' }]);
    expect(shared().redisDbLabel("db0", -1)).toBe("db0");
  });

  it("words a failed load-more as the TUI does", () => {
    expect(shared().moreRowsFailed("Session closed")).toBe("Couldn't load more rows: Session closed");
  });

  it("hints a local Host with AWS SSM (the instance itself) in the TUI's words", () => {
    expect(shared().ssmLocalHost(" localhost ")).toContain("With AWS SSM, localhost is the instance itself: Host and Port are the database as seen from the instance");
    expect(shared().ssmLocalHost("127.0.0.1")).toContain("127.0.0.1 is the instance itself");
    expect(shared().ssmLocalHost("::1")).toBeUndefined();
    expect(shared().ssmLocalHost("mydb.abc.ap-northeast-2.rds.amazonaws.com")).toBeUndefined();
  });

  it("a freed completion index finds nothing instead of throwing (a request may still hold it)", () => {
    const idx = shared().completionIndex([{ schema: "public", name: "users", columns: [["id", "int8"]] }]);
    const text = "select * from users where status = ";
    expect(idx.suggest("select * from u", 15, "", false).items.length).toBeGreaterThan(0);
    idx.free();
    expect(idx.suggest("select * from u", 15, "", false).items).toEqual([]);
    expect(idx.hintAt(text, text.length, "", false)).toBeUndefined();
    idx.free();
  });

  it("offers no value hints in a JOIN's ON condition, but keeps them in the WHERE after it", () => {
    const idx = shared().completionIndex([
      { schema: "public", name: "memo", columns: [["admin_user_idx", "int8"]] },
      { schema: "public", name: "admin_user", columns: [["idx", "int8"], ["status", "text"]] },
    ]);
    const on = "select * from memo as m\nleft join admin_user as au\n  on m.admin_user_idx = ";
    expect(idx.hintAt(on, on.length, "", false)).toBeUndefined();
    const where = `${on}au.idx\nwhere au.status = '`;
    expect(idx.hintAt(where, where.length, "", false)).toMatchObject({ table: "admin_user", column: "status" });
    idx.free();
  });

  it("completes a table tab's filter field as its query, with UTF-16 carets", () => {
    const idx = shared().completionIndex([
      { schema: "public", name: "users", columns: [["id", "int8"], ["user_name", "text"], ["이름", "text"]] },
      { schema: "public", name: "orders", columns: [["user_id", "int8"]] },
    ]);
    const labels = (s: { items: { label: string }[] }) => s.items.map((i) => i.label);
    const w = idx.suggestFilter("public", "users", false, "us", 2, "", false, false);
    expect(w.from).toBe(0);
    // The field belongs to its table: no table or schema names, typed or asked
    expect(labels(w)).toEqual(["user_name", "using"]);
    for (const explicit of [false, true]) expect(labels(idx.suggestFilter("public", "users", false, "orde", 4, "", false, explicit))).toEqual(["order"]);
    // The caret mid-text, after a surrogate pair and Hangul: `from` is in UTF-16 units of the field
    const text = "'😀' = 이름 and id > 1";
    const caret = text.indexOf("이름") + 1;
    const mid = idx.suggestFilter("public", "users", false, text, caret, "", false, false);
    expect([mid.from, labels(mid)]).toEqual([text.indexOf("이름"), ["이름"]]);
    expect(labels(idx.suggestFilter("public", "users", true, "id de", 5, "", false, false))).toEqual(["desc", "delete"]);
    expect(idx.suggestFilter("public", "users", false, "user_name like 'us", 18, "", false, false).items).toEqual([]);
    // ⌃Space at an empty word lists the table's columns first
    expect(labels(idx.suggestFilter("public", "users", false, "", 0, "", false, true)).slice(0, 3)).toEqual(["id", "user_name", "이름"]);
    idx.free();
  });

  it("throws before init, saying how to fix it", async () => {
    vi.resetModules();
    const fresh = await import("./shared");
    expect(() => fresh.shared()).toThrow(/not ready.*pnpm wasm/);
  });
});
