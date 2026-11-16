// The boundary's lifecycle as React drives it (no DOM in these tests: e2e/error-boundary.spec.ts mounts it for real)
import { isValidElement, type ReactElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AGAIN_MS, ErrorBoundary, ErrorFallback, errorDetails, rollbackAsk, type Scope } from "./ErrorBoundary";
import { api } from "../lib/api";
import { recoveryNotice } from "../features/recovery";

vi.mock("../lib/api", () => ({ api: { logFrontendError: vi.fn(() => Promise.resolve()) } }));
vi.mock("../features/recovery", () => ({ recoveryNotice: vi.fn(), reloadWindow: vi.fn(), uncommitted: vi.fn(() => 0) }));

const Child = () => <p>console text</p>;

/** A boundary React would hold: its state set as React sets it (setState applied at once) */
function boundary(scope: Scope, onReload?: () => void, label?: string) {
  const b = new ErrorBoundary({ scope, onReload, label, children: <Child /> });
  b.setState = ((u: any) => (b.state = { ...b.state, ...(typeof u === "function" ? u(b.state) : u) })) as any;
  return b;
}
/** What React does when a child throws while rendering */
function fail(b: ErrorBoundary, error: unknown) {
  b.state = { ...b.state, ...ErrorBoundary.getDerivedStateFromError(error) };
  b.componentDidCatch(error, { componentStack: "\n    at Child\n    at ConsolePane" });
}
const html = (b: ErrorBoundary) => renderToStaticMarkup(b.render());

afterEach(() => {
  vi.useRealTimers();
  vi.clearAllMocks();
});

describe("ErrorBoundary (#181)", () => {
  it("a tab's first error: logged, dropped what broke it (onReload), mounted again by itself with a toast", () => {
    const onReload = vi.fn();
    const b = boundary("tab", onReload, "console [DEV] shop_app");
    const before = b.render() as ReactElement;
    fail(b, new TypeError("Cannot read properties of undefined (reading 'map')"));
    expect(api.logFrontendError).toHaveBeenLastCalledWith({
      source: "boundary",
      message: "tab: TypeError: Cannot read properties of undefined (reading 'map')",
      stack: expect.stringContaining("TypeError"),
      componentStack: "\n    at Child\n    at ConsolePane",
    });
    expect(onReload).toHaveBeenCalledOnce();
    expect(recoveryNotice).toHaveBeenCalledWith("Recovered from an error in console [DEV] shop_app — details in the log");
    const after = b.render() as ReactElement;
    expect(isValidElement(after) && after.type).not.toBe(ErrorFallback);
    expect(after.key).not.toBe(before.key);
    expect(html(b)).toBe("<p>console text</p>");
  });

  it("an error again within AGAIN_MS shows the tab's fallback (no loop); Reload Tab mounts it again; later errors recover again", () => {
    vi.useFakeTimers();
    const onReload = vi.fn();
    const b = boundary("tab", onReload);
    fail(b, new Error("boom"));
    vi.advanceTimersByTime(AGAIN_MS - 1000);
    fail(b, new Error("boom again"));
    expect(api.logFrontendError).toHaveBeenCalledTimes(2);
    expect(onReload).toHaveBeenCalledOnce();
    const h = html(b);
    expect(h).toContain('role="alert"');
    expect(h).toContain("This tab hit an error");
    expect(h).toContain("Error: boom again");
    expect(h).toContain(">Reload Tab</button>");
    expect(h).toContain(">Copy Details</button>");
    expect(h).not.toContain("console text");
    b.reload();
    expect(onReload).toHaveBeenCalledTimes(2);
    expect(html(b)).toBe("<p>console text</p>");
    vi.advanceTimersByTime(AGAIN_MS + 1);
    fail(b, new Error("much later"));
    expect(html(b)).toBe("<p>console text</p>");
  });

  it("the window: recovers by itself once, then Recover and Reload Window; a Settings page shows its fallback at once", () => {
    const w = boundary("window");
    fail(w, new Error("boom"));
    expect(recoveryNotice).toHaveBeenCalledWith("Recovered from an error — details in the log");
    expect(html(w)).toBe("<p>console text</p>");
    fail(w, new Error("boom"));
    const h = html(w);
    expect(h).toContain("Qurry hit an error");
    expect(h).toContain(">Recover</button>");
    expect(h).toContain(">Reload Window</button>");
    const p = boundary("page");
    fail(p, new Error("boom"));
    expect(html(p)).toContain("This page hit an error");
  });

  it("the window's fallback asks before a reload rolls back uncommitted changes", () => {
    expect(rollbackAsk(1)).toBe("1 console has uncommitted changes. Reload rolls them back.");
    expect(rollbackAsk(2)).toBe("2 consoles have uncommitted changes. Reload rolls them back.");
  });

  it("Copy Details: the message, the stack and the components", () => {
    const e = Object.assign(new Error("boom"), { stack: "Error: boom\n    at render (index.js:1:2)" });
    expect(errorDetails(e, "\n    at Child")).toBe("Error: boom\n\nError: boom\n    at render (index.js:1:2)\n\nComponents:\n    at Child");
    expect(errorDetails("plain")).toBe("plain");
  });
});
