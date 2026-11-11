import type { ConnectionView } from "../../lib/api";
import { api } from "../../lib/api";
import { allThemes, findTheme, setThemePick, SYSTEM, useThemeState } from "../../lib/theme";
import { ConnectionDialog } from "../../components/ConnectionDialog";
import { CopyRowsDialog, GenerateDataDialog } from "../../components/DataTools";
import { HistoryDialog } from "../../components/HistoryDialog";
import { KeymapDialog } from "../../components/KeymapDialog";
import { NewObjectDialog, TableDesigner } from "../../components/ObjectEditors";
import { ProductionLog } from "../../components/ProductionLog";
import { schemaChanged } from "../../components/SchemaTree";
import { RecentFiles } from "../../components/Switcher";
import { TextDialog } from "../../components/TextDialog";
import { BackupDialog, ExportDialog, ImportDialog } from "../../components/Transfer";
import { UpdateDialog } from "../../components/UpdateDialog";
import { CodeBlock, ConfirmHost, Overlay } from "../../ui";
import type { useExplorer } from "../explorer/useExplorer";
import type { useJobs } from "../jobs/useJobs";
import { SettingsDialog } from "../settings/SettingsDialog";
import { editorApis, useTabsStore } from "../tabs/store";
import type { useTabs } from "../tabs/useTabs";
import type { useWorkspaces } from "../workspaces/useWorkspaces";
import {
  setBackupReq,
  setCopyReq,
  setDialog,
  setExportReq,
  setGenReq,
  setHistoryFor,
  setImportReq,
  setKeymapOpen,
  setObjEdit,
  setProdLog,
  setQuickDoc,
  setSchemeOpen,
  setSettingsOpen,
  setUpdate,
  setUpdateOpen,
  useDialogsStore,
} from "./store";

// The dialogs, in two groups mounted where App had them: overlays of the same layer stack in mount order (the Switcher, Search Everywhere,
// Full-text Search and Recent Files come between, then the status bar)

type Deps = {
  conns: ConnectionView[];
  setStatus: (s: string) => void;
} & Pick<ReturnType<typeof useTabs>, "openConsole"> &
  Pick<ReturnType<typeof useExplorer>, "startBackup"> &
  Pick<ReturnType<typeof useJobs>, "startExport" | "startImport"> &
  Pick<ReturnType<typeof useWorkspaces>, "readyToQuit">;

/** Theme, Settings, data transfer and object dialogs, the text and confirm dialogs, Update and Keymap Reference */
export function Dialogs({ conns, setStatus, openConsole, startBackup, startExport, startImport, readyToQuit }: Deps) {
  const { settings: themeSettings, current } = useThemeState();
  const settingsOpen = useDialogsStore((s) => s.settingsOpen);
  const schemeOpen = useDialogsStore((s) => s.schemeOpen);
  const keymapOpen = useDialogsStore((s) => s.keymapOpen);
  const update = useDialogsStore((s) => s.update);
  const updateOpen = useDialogsStore((s) => s.updateOpen);
  const exportReq = useDialogsStore((s) => s.exportReq);
  const importReq = useDialogsStore((s) => s.importReq);
  const genReq = useDialogsStore((s) => s.genReq);
  const objEdit = useDialogsStore((s) => s.objEdit);
  const copyReq = useDialogsStore((s) => s.copyReq);
  const backupReq = useDialogsStore((s) => s.backupReq);
  return (
    <>
      {schemeOpen &&
        (() => {
          // Quick Switch Theme (as DataGrip's ⌃` › Theme): built-in, then custom themes, Sync with OS, and the editor
          const pick = themeSettings.theme ?? "dark";
          const name = (id: string) => findTheme(themeSettings, id)?.name ?? id;
          const items = [
            ...allThemes(themeSettings).map((t) => ({ key: t.id, icon: pick === t.id ? "●" : "", title: t.name, hint: t.custom ? "custom" : "", open: true })),
            { key: SYSTEM, icon: pick === SYSTEM ? "●" : "", title: "Sync with OS", hint: `${name(themeSettings.sync.dark)} / ${name(themeSettings.sync.light)}, now ${current.name}`, open: true },
            { key: "", icon: "", title: "Edit Colors…", hint: "Settings › Appearance", open: true },
          ];
          return (
            <RecentFiles
              title="Theme"
              start={Math.max(0, items.findIndex((i) => i.key === pick))}
              items={items}
              onClose={() => setSchemeOpen(false)}
              onPick={(k) => {
                setSchemeOpen(false);
                if (k) setThemePick(k).catch((e) => setStatus(String(e)));
                else setSettingsOpen("theme");
              }}
            />
          );
        })()}
      {settingsOpen && (
        <SettingsDialog
          initialPage={settingsOpen === true ? undefined : settingsOpen}
          onClose={() => setSettingsOpen(false)}
          onUpdate={(u) => {
            setUpdate(u);
            setSettingsOpen(false);
            setUpdateOpen(true);
          }}
          onChannel={setUpdate}
        />
      )}
      {backupReq && <BackupDialog what={backupReq.db ?? backupReq.c.name} pg={backupReq.c.kind === "postgres"} onRun={startBackup} onClose={() => setBackupReq(null)} />}
      {exportReq && <ExportDialog what={exportReq.name} onExport={startExport} onClose={() => setExportReq(null)} />}
      {objEdit &&
        (() => {
          const done = (msg: string) => {
            setStatus(msg);
            setObjEdit(null);
            objEdit.refresh?.();
            schemaChanged(objEdit.conn.id, objEdit.db ?? (objEdit.conn.database || (objEdit.conn.kind === "postgres" ? "postgres" : "")));
          };
          const toConsole = (sql: string) => {
            openConsole(objEdit.conn, objEdit.db, sql);
            setObjEdit(null);
          };
          return objEdit.newKind ? (
            <NewObjectDialog kind={objEdit.newKind} target={objEdit.target} conn={objEdit.conn} schema={objEdit.schema} onDone={done} onOpenConsole={toConsole} onClose={() => setObjEdit(null)} />
          ) : (
            <TableDesigner target={objEdit.target} conn={objEdit.conn} schema={objEdit.schema} table={objEdit.table} onDone={done} onOpenConsole={toConsole} onClose={() => setObjEdit(null)} />
          );
        })()}
      {copyReq && (
        <CopyRowsDialog
          source={copyReq.target}
          schema={copyReq.schema}
          table={copyReq.table}
          conn={copyReq.conn}
          conns={conns}
          onDone={(msg) => {
            setStatus(msg);
            setCopyReq(null);
          }}
          onClose={() => setCopyReq(null)}
        />
      )}
      {genReq && (
        <GenerateDataDialog
          target={genReq.target}
          schema={genReq.schema}
          table={genReq.table}
          production={genReq.conn.options?.confirmChanges ? genReq.conn.name : undefined}
          onDone={(n) => {
            setStatus(`${n.toLocaleString("en-US")} test row${n === 1 ? "" : "s"} inserted into ${genReq.schema ? `${genReq.schema}.` : ""}${genReq.table}`);
            setGenReq(null);
          }}
          onClose={() => setGenReq(null)}
        />
      )}
      {importReq && <ImportDialog target={importReq.target} schema={importReq.schema} table={importReq.table} onImport={startImport} onClose={() => setImportReq(null)} />}
      <TextDialog />
      <ConfirmHost />
      {updateOpen && update?.latest && <UpdateDialog info={update} onClose={() => setUpdateOpen(false)} beforeInstall={readyToQuit} />}
      {keymapOpen && <KeymapDialog onClose={() => setKeymapOpen(false)} onEdit={() => (setKeymapOpen(false), setSettingsOpen("keymap"))} />}
    </>
  );
}

/** Query History, Production Log, Quick Documentation and the Data Source dialog: mounted last, above the popups */
export function TopDialogs({ conns, setStatus, refresh, openConsole }: { conns: ConnectionView[]; setStatus: (s: string) => void; refresh: () => Promise<void> } & Pick<ReturnType<typeof useTabs>, "openConsole">) {
  const tabs = useTabsStore((s) => s.tabs);
  const historyFor = useDialogsStore((s) => s.historyFor);
  const historyTab = useDialogsStore((s) => s.historyTab);
  const prodLog = useDialogsStore((s) => s.prodLog);
  const quickDoc = useDialogsStore((s) => s.quickDoc);
  const dialog = useDialogsStore((s) => s.dialog);
  return (
    <>
      {historyFor && (() => {
        const t = tabs.find((x) => x.id === historyFor);
        const c = conns.find((x) => x.id === t?.connId);
        return t && c ? (
          <HistoryDialog
            connId={c.id}
            title={`${c.name} Query History`}
            initialTab={historyTab}
            onPick={(sql) => {
              editorApis.get(t.id)?.insert(sql);
              setHistoryFor(null);
            }}
            onClose={() => setHistoryFor(null)}
          />
        ) : null;
      })()}

      {prodLog && (
        <ProductionLog
          connId={prodLog.connId}
          onOpenUndo={(e) => {
            const [base, db] = e.connId.split("::");
            const c = conns.find((x) => x.id === base);
            if (!c) return setStatus(`Data source "${e.connName}" no longer exists`);
            openConsole(c, db || undefined, e.undo);
            setProdLog(null);
          }}
          onClose={() => setProdLog(null)}
        />
      )}

      {quickDoc && (
        <Overlay variant="light" onClose={() => setQuickDoc(null)} onKeyDown={(e) => e.key === "Escape" && setQuickDoc(null)}>
          <div className="quick-doc" onMouseDown={(e) => e.stopPropagation()} tabIndex={-1} ref={(el) => el?.focus()} onKeyDown={(e) => e.key === "Escape" && setQuickDoc(null)}>
            <div className="qd-title">{quickDoc.title}</div>
            <CodeBlock className="qd-body">{quickDoc.text}</CodeBlock>
          </div>
        </Overlay>
      )}

      {dialog && (
        <ConnectionDialog
          edit={dialog.edit}
          onClose={() => setDialog(null)}
          onSaved={async (saved) => {
            setDialog(null);
            await refresh();
            try {
              setStatus(`Connecting to ${saved.name}…`);
              const v = await api.connect(saved.id);
              setStatus(`Connected: ${v.split(" on ")[0]}`);
            } catch (e) {
              setStatus(`Connection failed: ${String(e)}`);
            }
            refresh();
          }}
        />
      )}
    </>
  );
}
