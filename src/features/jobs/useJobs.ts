import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../../lib/api";
import type { TransferJob } from "../../components/Transfer";
import type { DataFormat } from "../../lib/transfer";
import { setExportReq, setImportReq, useDialogsStore } from "../dialogs/store";
import type { Tab } from "../tabs/types";

/** Long tasks on the progress bar at the bottom (export, import, backup …), and the export / import that start from their dialogs */
export function useJobs({ setStatus }: { setStatus: (s: string) => void }) {
  const exportReq = useDialogsStore((s) => s.exportReq);
  const importReq = useDialogsStore((s) => s.importReq);
  // ───────── Data export / import (choose format → progress bar at the bottom) ─────────
  const [jobs, setJobs] = useState<TransferJob[]>([]);
  useEffect(() => {
    let off: (() => void) | undefined;
    listen<[string, number, number, number]>("transfer-progress", ({ payload: [id, rows, done, total] }) =>
      setJobs((js) => js.map((j) => (j.id === id ? { ...j, rows, done, total } : j))),
    )
      .then((u) => (off = u))
      .catch(() => {});
    return () => off?.();
  }, []);
  /** Put a task on the progress bar and run it. Remove it when done (silently if cancelled) */
  const runJob = async <T,>(label: string, run: (job: string) => Promise<T>): Promise<T | undefined> => {
    const id = crypto.randomUUID();
    setJobs((js) => [...js, { id, label, rows: 0, done: 0, total: 0 }]);
    try {
      return await run(id);
    } catch (e) {
      setStatus(String(e) === "Cancelled" ? `${label}: cancelled` : `${label}: ${String(e)}`);
    } finally {
      setJobs((js) => js.filter((j) => j.id !== id));
    }
  };
  const startExport = async (format: DataFormat, clipboard: boolean) => {
    const r0 = exportReq;
    setExportReq(null);
    if (!r0) return;
    const label = `Export ${r0.name}`;
    const r = await runJob(label, (job) => api.exportData(r0.target, r0.sql, format, clipboard, r0.name, r0.table, job));
    if (!r) return;
    if (r.text !== null) {
      await navigator.clipboard.writeText(r.text);
      setStatus(`Copied ${r.rows} rows to the clipboard`);
    } else setStatus(`Exported ${r.rows} rows to ${r.path}`);
  };
  const startImport = async (table: string, create: boolean, columns: [number, string][], label: string) => {
    const r0 = importReq;
    setImportReq(null);
    if (!r0) return;
    const n = await runJob(`Import ${label}`, (job) => api.importData(r0.target, { path: "", schema: r0.schema, table, create, columns }, job));
    if (n === undefined) return;
    setStatus(`Imported ${n} rows into ${r0.schema}.${table}`);
    if (create) r0.refresh?.();
  };

  const exportCsv = (tab: Tab | null) => {
    if (!tab?.lastSql) return;
    setExportReq({ target: tab.target, sql: tab.lastSql, name: tab.kind === "table" ? tab.title : "result", table: tab.edit ? [tab.edit.schema, tab.edit.table] : null });
  };
  return { jobs, runJob, startExport, startImport, exportCsv };
}
