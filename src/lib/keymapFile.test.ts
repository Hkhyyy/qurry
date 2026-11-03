import { expect, it, vi } from "vitest";
import fixture from "../../crates/shared/tests/keymap/keymap.json";
import effective from "../../crates/shared/tests/keymap/effective.json";

// The "gui" section of a keymap.json saved by an earlier version, as the backend returns it
vi.mock("@tauri-apps/api/core", () => ({ invoke: async (cmd: string) => (cmd === "load_keymap" ? fixture.gui : undefined) }));

it("a saved keymap.json loads to the same GUI bindings as before (same ids and defaults)", async () => {
  const { bindings, loadKeymap, SHORTCUTS } = await import("./shortcuts");
  expect(await loadKeymap()).toBeNull();
  const want: Record<string, string[]> = effective.gui;
  // On failure: a command id or a default key changed, so users' keymap.json would load differently.
  // Keep the old id (it is what keymap.json stores) and the old default; change effective.json only for an intended default change
  expect(Object.fromEntries(Object.keys(want).map((id) => [id, bindings(id)]))).toEqual(want);
  // Commands added since start at their defaults: the file has nothing for them
  for (const s of SHORTCUTS.filter((x) => !(x.id in want))) expect([s.id, bindings(s.id)]).toEqual([s.id, s.keys]);
});
