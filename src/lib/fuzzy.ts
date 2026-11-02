// qurry_shared::fuzzy through WASM. The GUI keeps its JetBrains-style scoring (D2 of the 0.3.0 architecture batch: the fuzzyScore rows
// of crates/shared/tests/DIVERGENCES.md); the TUI palette and the MCP server score without word starts
import { shared, sharedReady } from "./shared";

/**
 * JetBrains-style matching: substring > word start (_, ., uppercase) > scattered letters in order. Higher score comes first.
 * Until the module is loaded (or when the WebView refused WebAssembly) a case-insensitive substring matches, unranked
 */
export function fuzzyScore(query: string, text: string): number | null {
  if (!sharedReady()) return text.toLowerCase().includes(query.toLowerCase()) ? 0 : null;
  return shared().fuzzyScore(query, text, true) ?? null;
}
