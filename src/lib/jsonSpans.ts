// JSON colors in the result grid (keys, strings, numbers, true/false/null, punctuation): qurry_shared::format::json_spans
// through WASM, the TUI grid's tokenizer. A cell reads only its first JSON_CAP characters, and the pieces are kept per text,
// so a large jsonb costs the same as a short one and scrolling back to it is a map lookup. Until the module is loaded
// (or when the WebView refuses WebAssembly) JSON cells show plain text
import { shared, sharedOr, sharedReady } from "./shared";

/** qurry_shared::format::JsonKind in its order: WASM returns the index */
export const JSON_KINDS = ["key", "string", "number", "keyword", "punct"] as const;
export type JsonKind = (typeof JSON_KINDS)[number];
/** UTF-16 range in the cell's text */
export type JsonSpan = [start: number, end: number, kind: JsonKind];

/** Characters of a JSON cell rendered, more than a column can show: columns are at most 420 px at a 12 px font (~58 monospace characters), both scale with the grid font */
export const JSON_CAP = 200;
const MAX_CACHED = 2000;
const cache = new Map<string, JsonSpan[]>();

export const isJsonType = (typeName: string) => sharedOr(false, (s) => s.isJsonType(typeName));

/** What a JSON cell shows of `value` (`text`) and its colored pieces; no pieces without the module */
export function jsonCell(value: string): { text: string; spans: JsonSpan[] } {
  const text = value.length > JSON_CAP ? value.slice(0, JSON_CAP) : value;
  const hit = cache.get(text);
  if (hit) return { text, spans: hit };
  if (!sharedReady()) return { text, spans: [] };
  const flat = shared().jsonSpans(text, JSON_CAP);
  const spans: JsonSpan[] = [];
  for (let i = 0; i < flat.length; i += 3) spans.push([flat[i], flat[i + 1], JSON_KINDS[flat[i + 2]]]);
  if (cache.size >= MAX_CACHED) cache.clear();
  cache.set(text, spans);
  return { text, spans };
}
