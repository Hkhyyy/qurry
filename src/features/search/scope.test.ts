import { describe, expect, it } from "vitest";
import type { ConnectionView } from "../../lib/api";
import { nextScope, scopeConns, shownScope } from "./scope";

const c = (id: string, kind = "postgres") => ({ id, name: id, kind, connected: true }) as ConnectionView;
const conns = [c("dev"), c("my", "mysql"), c("rd", "redis"), c("prod")];

describe("scopeConns", () => {
  it("This Workspace: the workspace's explorer list in its order, every kind, removed ids skipped", () => {
    expect(scopeConns("workspace", conns, ["my", "gone", "dev"]).map((x) => x.id)).toEqual(["my", "dev"]);
    expect(scopeConns("workspace", conns, [])).toEqual([]);
  });
  it("All Places: every data source", () => {
    expect(scopeConns("all", conns, ["dev"]).map((x) => x.id)).toEqual(["dev", "my", "rd", "prod"]);
  });
  it("Context Data Sources: the one in context; with none it is This Workspace", () => {
    expect(scopeConns("context", conns, ["dev", "my"], "prod").map((x) => x.id)).toEqual(["prod"]);
    expect(scopeConns("context", conns, ["dev", "my"]).map((x) => x.id)).toEqual(["dev", "my"]);
  });
});

describe("shownScope / nextScope", () => {
  it("a saved Context Data Sources without a context shows This Workspace", () => {
    expect(shownScope("context")).toBe("workspace");
    expect(shownScope("context", "dev")).toBe("context");
    expect(shownScope("all")).toBe("all");
  });
  it("cycles This Workspace → All Places → Context Data Sources, skipping Context when there is none", () => {
    expect(nextScope("workspace", "dev")).toBe("all");
    expect(nextScope("all", "dev")).toBe("context");
    expect(nextScope("context", "dev")).toBe("workspace");
    expect(nextScope("all")).toBe("workspace");
  });
});
