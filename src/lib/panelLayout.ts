/** Splitter sizes saved before 0.3.4 (react-resizable-panels 2) sit under the group's id with the panel ids sorted ("editors,explorer");
 *  version 4 reads its own key and looks the old one up in panel order, so the Database Explorer and Find widths came back as defaults
 *  after the update. Moves them to version 4's key once, then drops the old entry (#192). */
export function moveOldLayout(id: string, panelIds: string[], storage: Storage = localStorage) {
  const oldKey = `react-resizable-panels:${id}`;
  try {
    const old = storage.getItem(oldKey);
    if (!old) return;
    const layout = JSON.parse(old)[[...panelIds].sort().join(",")]?.layout;
    if (Array.isArray(layout) && layout.length === panelIds.length && layout.every((n) => typeof n === "number"))
      storage.setItem(`react-resizable-panels:${[id, ...panelIds].join(":")}`, JSON.stringify(Object.fromEntries(panelIds.map((p, i) => [p, layout[i]]))));
    storage.removeItem(oldKey);
  } catch {
    // Unreadable: the defaults stay
  }
}
