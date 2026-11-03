import { createElement, type ReactElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { CellEditor, ComboBox, SearchField, TextArea, TextInput } from ".";

const noop = () => {};
const OFF = ['autoComplete="off"', 'autoCorrect="off"', 'autoCapitalize="off"', 'spellCheck="false"', 'writingsuggestions="false"'];
const html = (el: ReactElement) => renderToStaticMarkup(el);

describe("ui-kit text fields: no WebKit autocorrect, capitalisation, spelling or writing suggestions (identifiers, not prose)", () => {
  it.each([
    ["TextInput", createElement(TextInput, { value: "", onChange: noop })],
    ["TextArea", createElement(TextArea, { value: "", onChange: noop })],
    ["SearchField", createElement(SearchField, { value: "", onChange: noop })],
    ["ComboBox", createElement(ComboBox, { value: "", options: ["a"], onChange: noop })],
    ["CellEditor", createElement(CellEditor, { width: 100, value: "", onChange: noop, onKeyDown: noop, onBlurOut: noop })],
  ])("%s", (_, el) => {
    const out = html(el).toLowerCase();
    for (const a of OFF) expect(out).toContain(a.toLowerCase());
  });
  it("prose fields (comments) opt back into spell check only", () => {
    const out = html(createElement(TextInput, { prose: true, value: "", onChange: noop })).toLowerCase();
    expect(out).toContain('spellcheck="true"');
    expect(out).toContain('autocorrect="off"');
  });
});
