import { meta } from "../../lib/api";
import { ReconnectFailed, waitOn } from "../../lib/reconnect";
import { setTableColumns } from "../../lib/statusbar";
import { allTabs, patch, updTab } from "../tabs/ops";
import type { Tab } from "../tabs/types";

/**
 * A table tab just opened (Loading… in it at once): its metadata on `target` (the primary key, which makes it editable), then its
 * rows (`run`). A reconnect the metadata waits on shows in the toast and the status bar; when it fails or is cancelled the tab stops
 * loading and the toast keeps the reason with Retry, which loads it again. Any other metadata error: the rows still load, read-only,
 * and the status bar says why
 */
export async function loadTableTab(tab: Tab, target: string, sql: string, run: (tab: Tab, sql: string) => Promise<void>, setStatus: (s: string) => void): Promise<void> {
  const { schema, table } = tab.edit!;
  const load = async () => {
    patch(tab.id, { running: true, error: null });
    let failed: string | undefined;
    try {
      const d = await waitOn(target, (callId) => meta.table(target, schema, table, callId), () => void load());
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
