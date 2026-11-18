import { describe, expect, it, vi } from "vitest";
import { searchPathSql, useSql } from "./api";

// A console's schema switch: qurry_shared::sql::search_path_sql / use_sql through WASM (the TUI sends the same)
describe("searchPathSql / useSql", () => {
  it("quotes the schema as the shared rule does", () => {
    expect(searchPathSql("billing")).toBe("SET search_path TO billing, public");
    expect(searchPathSql("Billing")).toBe('SET search_path TO "Billing", public');
    expect(searchPathSql('a"b')).toBe('SET search_path TO "a""b", public');
    expect(useSql("shop")).toBe("USE shop");
    expect(useSql("my`db")).toBe("USE `my``db`");
  });

  it("before the module has loaded: the same text from the dialect", async () => {
    vi.resetModules();
    vi.doMock("./shared", () => ({ sharedOr: <T,>(fallback: T) => fallback }));
    const early = await import("./api");
    expect(early.searchPathSql("Billing")).toBe('SET search_path TO "Billing", public');
    expect(early.searchPathSql('a"b')).toBe('SET search_path TO "a""b", public');
    expect(early.useSql("my`db")).toBe("USE `my``db`");
    vi.doUnmock("./shared");
  });
});
