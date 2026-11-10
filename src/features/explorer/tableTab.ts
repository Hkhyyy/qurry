import { meta } from "../../lib/api";
import { cancelWait, ReconnectFailed, waitOn } from "../../lib/reconnect";
import { setTableColumns } from "../../lib/statusbar";
import { allTabs, patch, updTab } from "../tabs/ops";
import type { Tab } from "../tabs/types";

/** The tabs waiting for their metadata (loadTableTab): the call id of that load, by tab id */
const metaCalls = new Map<string, string>();

/** Cancel on a table tab (its Stop, ⌘F2, the status bar's ✕) while it waits for its metadata: nothing runs on its session yet, so
 *  this stops that load (cancelWait) and the rows never load. False when the tab isn't waiting for it */
export function cancelTableLoad(tabId: string): boolean {
  const callId = metaCalls.get(tabId);
  if (callId) cancelWait(callId);
  return !!callId;
}

/**
 * A table tab just opened (Loading… in it at once): its metadata on `target` (the primary key, which makes it editable), then its
 * rows (`run`). A reconnect the metadata waits on shows in the toast and the status bar; when it fails or is cancelled the tab stops
 * loading and the toast keeps the reason with Retry, which loads it again; so does the tab's own Cancel then (cancelTableLoad). Any other metadata error: the rows still load, read-only,
 * and the status bar says why
 */
export async function loadTableTab(tab: Tab, target: string, sql: string, run: (tab: Tab, sql: string) => Promise<void>, setStatus: (s: string) => void): Promise<void> {
  const { schema, table } = tab.edit!;
  const load = async () => {
    patch(tab.id, { running: true, error: null });
    let failed: string | undefined;
    try {
      const d = await waitOn(
        target,
        (callId) => {
          metaCalls.set(tab.id, callId);
          return meta.table(target, schema, table, callId);
        },
        () => void load(),
      ).finally(() => metaCalls.delete(tab.id));
      updTab(tab.id, (t) => ({ ...t, edit: { schema, table, pk: d.columns.filter((x) => x.isPk).map((x) => x.name) } }));
      setTableColumns(target, schema, table, d.columns);
    } catch (e) {
      if (e instanceof ReconnectFailed) {
        patch(tab.id, { running: false });
        setStatus(String(e));
        return;
      }
      failed = `Read-only: couldn't read the columns of ${schema}.${table}: ${String(e)}`;
    }
    // Closed meanwhile: nothing to load into
    if (!allTabs().some((t) => t.id === tab.id)) return;
    await run(tab, sql);
    if (failed) setStatus(failed);
  };
  await load();
}
