import { IconButton, Spacer, StatusItem, Text } from "../../ui";
import { useStatusWidgets } from "../../lib/statusbar";
import { TransferBar, type TransferJob } from "../../components/Transfer";
import { keyTitle } from "../../lib/shortcuts";

function StatusBar({ run, onSettings, settingsKey, onKeymap, status, readOnly, update, onUpdate, jobs }: { run: { canRun: boolean; running: boolean; onRun: () => void; onCancel: () => void }; onSettings: () => void; settingsKey?: string; onKeymap: () => void; status: string; readOnly: boolean; update?: string; onUpdate: () => void; jobs: TransferJob[] }) {
  const w = useStatusWidgets();
  return (
    <div className="statusbar">
      <IconButton size="sm" tone="ok" disabled={!run.canRun} onClick={run.onRun} title={keyTitle("Execute", "editor.run")}>
        ▶
      </IconButton>
      <IconButton size="sm" tone="danger" disabled={!run.running} onClick={run.onCancel} title={keyTitle("Cancel Running Statements", "editor.cancel")}>
        ■
      </IconButton>
      <Text truncate className="sb-text">
        {status}
      </Text>
      <Spacer />
      <TransferBar jobs={jobs} />
      {update && (
        <StatusItem accent title="A new version is available" onClick={onUpdate}>
          ⬆ v{update} available
        </StatusItem>
      )}
      {w.aggregate && <StatusItem title="Aggregate of selected cells">{w.aggregate}</StatusItem>}
      {readOnly && <StatusItem title="Data source is read-only">🔒 Read-only</StatusItem>}
      {w.caret && <StatusItem title="Line:Column">{w.caret}</StatusItem>}
      <StatusItem>UTF-8</StatusItem>
      <IconButton size="sm" title={settingsKey ? `Settings (${settingsKey})` : "Settings"} onClick={onSettings}>
        ⚙
      </IconButton>
      <IconButton size="sm" title={keyTitle("Keymap Reference", "keymap")} onClick={onKeymap}>
        ⌨
      </IconButton>
    </div>
  );
}

export { StatusBar };
