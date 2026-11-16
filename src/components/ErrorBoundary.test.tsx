// The boundary's lifecycle as React drives it (no DOM in these tests: the e2e spec errorBoundary.spec.ts mounts it for real)
import { isValidElement, type ReactElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { ErrorBoundary, ErrorFallback, errorDetails, type Scope } from "./ErrorBoundary";
import { api } from "../lib/api";

vi.mock("../lib/api", () => ({ api: { logFrontendError: vi.fn(() => Promise.resolve()) } }));

const Child = () => <p>console text</p>;

/** A boundary React would hold: its state set as React sets it (setState applied at once) */
function boundary(scope: Scope, onReload?: () => void) {
  const b = new ErrorBoundary({ scope, onReload, children: <Child /> });
  b.setState = ((u: any) => (b.state = { ...b.state, ...(typeof u === "function" ? u(b.state) : u) })) as any;
  return b;
}
/** What React does when a child throws while rendering */
function fail(b: ErrorBoundary, error: unknown) {
  b.state = { ...b.state, ...ErrorBoundary.getDerivedStateFromError(error) };
  b.componentDidCatch(error, { componentStack: "\n    at Child\n    at ConsolePane" });
}

describe("ErrorBoundary (#181)", () => {
  it("shows its children until one throws, then the tab's fallback with the message, Reload Tab and Copy Details", () => {
    const b = boundary("tab");
    expect(renderToStaticMarkup(b.render())).toBe("<p>console text</p>");
    fail(b, new TypeError("Cannot read properties of undefined (reading 'map')"));
    const html = renderToStaticMarkup(b.render());
    expect(html).toContain('role="alert"');
    expect(html).toContain("This tab hit an error");
    expect(html).toContain("TypeError: Cannot read properties of undefined (reading &#x27;map&#x27;)");
    expect(html).toContain(">Reload Tab</button>");
    expect(html).toContain(">Copy Details</button>");
    expect(html).not.toContain("console text");
    expect(b.state.componentStack).toContain("at ConsolePane");
    // Into qurry.log: the message, its stack and where in the tree
    expect(api.logFrontendError).toHaveBeenLastCalledWith({
      source: "boundary",
      message: "tab: TypeError: Cannot read properties of undefined (reading 'map')",
      stack: expect.stringContaining("TypeError"),
      componentStack: "\n    at Child\n    at ConsolePane",
    });
  });

  it("Reload Tab drops what the tab keeps outside (onReload), then mounts the children again (a new key), from scratch", () => {
    const onReload = vi.fn();
    const b = boundary("tab", onReload);
    const before = b.render() as ReactElement;
    fail(b, new Error("boom"));
    b.reload();
    expect(onReload).toHaveBeenCalledOnce();
    const after = b.render() as ReactElement;
    expect(isValidElement(after) && after.type).not.toBe(ErrorFallback);
    expect(after.key).not.toBe(before.key);
    expect(renderToStaticMarkup(after)).toBe("<p>console text</p>");
    // And catches the next error again
    fail(b, "again");
    expect(renderToStaticMarkup(b.render())).toContain("This tab hit an error");
  });

  it("the window's fallback reloads the window; a Settings page's says page", () => {
    const reload = vi.fn();
    vi.stubGlobal("location", { reload });
    const w = boundary("window");
    fail(w, new Error("boom"));
    const html = renderToStaticMarkup(w.render());
    expect(html).toContain("Qurry hit an error");
    expect(html).toContain(">Reload Window</button>");
    w.reload();
    expect(reload).toHaveBeenCalledOnce();
    vi.unstubAllGlobals();
    const p = boundary("page");
    fail(p, new Error("boom"));
    expect(renderToStaticMarkup(p.render())).toContain("This page hit an error");
  });

  it("Copy Details: the message, the stack and the components", () => {
    const e = Object.assign(new Error("boom"), { stack: "Error: boom\n    at render (index.js:1:2)" });
    expect(errorDetails(e, "\n    at Child")).toBe("Error: boom\n\nError: boom\n    at render (index.js:1:2)\n\nComponents:\n    at Child");
    expect(errorDetails("plain")).toBe("plain");
  });
});
