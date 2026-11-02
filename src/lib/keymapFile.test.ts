import { expect, it, vi } from "vitest";
import fixture from "../../crates/shared/tests/keymap/keymap.json";
import effective from "../../crates/shared/tests/keymap/effective.json";

// The "gui" section of a keymap.json saved by an earlier version, as the backend returns it
vi.mock("@tauri-apps/api/core", () => ({ invoke: async (cmd: string) => (cmd === "load_keymap" ? fixture.gui : undefined) }));

it("a saved keymap.json loads to the same GUI bindings as before (same ids and defaults)", async () => {
  const { bindings, loadKeymap, SHORTCUTS } = await import("./shortcuts");
  await loadKeymap();
  const now = Object.fromEntries(SHORTCUTS.map((s) => [s.id, bindings(s.id)]));
  // On failure: a command id or a default key changed, so users' keymap.json would load differently.
  // Keep the old id (it is what keymap.json stores) and the old default; change effective.json only for an intended default change
  expect(now).toEqual(effective.gui);
});
