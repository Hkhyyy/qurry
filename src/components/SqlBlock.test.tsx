import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { coloredPart, SqlBlock } from "./SqlBlock";

describe("SqlBlock", () => {
  it("draws the console's colors in the data source's dialect, the text unchanged", () => {
    const pg = renderToStaticMarkup(<SqlBlock sql={`UPDATE t SET a = 'x', n = 1 WHERE lower(b) IS NULL; -- c`} kind="postgres" />);
    expect(pg).toContain('<span class="ui-hl-keyword">UPDATE</span>');
    expect(pg).toContain('<span class="ui-hl-string">&#x27;x&#x27;</span>');
    expect(pg).toContain('<span class="ui-hl-number">1</span>');
    expect(pg).toContain('<span class="ui-hl-function">lower</span>');
    expect(pg).toContain('<span class="ui-hl-comment">-- c</span>');
    expect(pg.replace(/<[^>]+>/g, "")).toBe("UPDATE t SET a = &#x27;x&#x27;, n = 1 WHERE lower(b) IS NULL; -- c");
    // # is a comment in MySQL only
    expect(renderToStaticMarkup(<SqlBlock sql="# note" kind="mysql" />)).toContain('<span class="ui-hl-comment"># note</span>');
    expect(renderToStaticMarkup(<SqlBlock sql="# note" kind="postgres" />)).not.toContain("ui-hl-comment");
  });

  it("colors a large preview only up to its limit, at a line break, and the rest stays plain", () => {
    const line = "INSERT INTO t (a) VALUES ('" + "x".repeat(990) + "');";
    const sql = Array.from({ length: 200 }, () => line).join("\n");
    const head = coloredPart(sql);
    expect(head.length).toBeLessThan(sql.length);
    expect(sql.startsWith(head + "\n")).toBe(true);
    const html = renderToStaticMarkup(<SqlBlock sql={sql} kind="postgres" />);
    expect(html.match(/ui-hl-keyword">INSERT</g)!.length).toBe(head.split("\n").length);
    expect(html.replace(/<[^>]+>/g, "").replace(/&#x27;/g, "'")).toBe(sql);
    // One line longer than the limit: cut inside it, not nothing
    expect(coloredPart("x".repeat(300_000)).length).toBeGreaterThan(0);
    expect(coloredPart("select 1")).toBe("select 1");
  });
});
