import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import golden from "../../crates/shared/tests/goldens/jsonspans.json";
import { GridCell, JsonText } from "../ui";
import { isJsonType, JSON_CAP, JSON_KINDS, jsonCell } from "./jsonSpans";
import { shared } from "./shared";

describe("JSON cell colors", () => {
  it("match the shared golden through WASM (the TUI grid's tokenizer), as UTF-16 pieces", () => {
    for (const e of golden) {
      const flat = shared().jsonSpans(e.text, e.max);
      const got = [];
      for (let i = 0; i < flat.length; i += 3) got.push([e.text.slice(flat[i], flat[i + 1]), JSON_KINDS[flat[i + 2]]]);
      expect(got, e.id).toEqual(e.spans);
    }
  });

  it("color JSON columns only, not binary ones of the same value class", () => {
    expect(["jsonb", "JSON", "json"].map(isJsonType)).toEqual([true, true, true]);
    expect(["bytea", "LONGBLOB", "text", "_jsonb"].map(isJsonType)).toEqual([false, false, false, false]);
  });

  it("render the pieces inside the cell, one line", () => {
    const html = renderToStaticMarkup(
      <GridCell width={200}>
        <JsonText {...jsonCell('{"이름": "김", "n": -1.5, "ok": true, "x": null}')} />
      </GridCell>,
    );
    // Punctuation is the color of the span around the colored part
    expect(html).toBe(
      '<div class="cell" style="width:200px" role="gridcell"><span class="json-punct">{<span class="json-key">&quot;이름&quot;</span>: ' +
        '<span class="json-string">&quot;김&quot;</span>, <span class="json-key">&quot;n&quot;</span>: <span class="json-number">-1.5</span>, ' +
        '<span class="json-key">&quot;ok&quot;</span>: <span class="json-keyword">true</span>, <span class="json-key">&quot;x&quot;</span>: ' +
        '<span class="json-keyword">null</span>}</span></div>',
    );
    // Invalid from some point on: the rest is plain text
    expect(renderToStaticMarkup(<JsonText {...jsonCell('[1, oops]')} />)).toBe('<span class="json-punct">[<span class="json-number">1</span>,</span> oops]');
  });

  it("a large value: only what the cell can show is read and rendered, once per text", () => {
    const big = JSON.stringify({ items: Array.from({ length: 5000 }, (_, i) => ({ id: i, name: `item ${i}`, tags: ["a", "b"] })) });
    expect(big.length).toBeGreaterThan(100_000);
    const spy = vi.spyOn(shared(), "jsonSpans");
    // shared() returns the same object each call, so the spy sees jsonCell's calls
    const a = jsonCell(big);
    const b = jsonCell(big);
    expect(spy).toHaveBeenCalledTimes(1);
    expect(spy.mock.calls[0][0]).toHaveLength(JSON_CAP);
    expect(a.text).toBe(big.slice(0, JSON_CAP));
    expect(b.spans).toBe(a.spans);
    expect(a.spans[a.spans.length - 1][1]).toBeLessThanOrEqual(JSON_CAP);
    spy.mockRestore();
  });
});
