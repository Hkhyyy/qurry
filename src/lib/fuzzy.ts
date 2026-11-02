// qurry_shared::fuzzy through WASM. The GUI keeps its JetBrains-style scoring (D2 of the 0.3.0 architecture batch: the fuzzyScore rows
// of crates/shared/tests/DIVERGENCES.md); the TUI palette and the MCP server score without word starts
import { sharedOr } from "./shared";

/** JetBrains-style matching: substring > word start (_, ., uppercase) > scattered letters in order. Higher score comes first. Nothing matches until the module is loaded */
export function fuzzyScore(query: string, text: string): number | null {
  return sharedOr(query ? null : 0, (s) => s.fuzzyScore(query, text, true) ?? null);
}
