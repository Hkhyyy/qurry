import { IconButton, Spacer, StatusItem, Text } from "../../ui";
import { useStatusWidgets } from "../../lib/statusbar";
import { TransferBar, type TransferJob } from "../../components/Transfer";
import { keyTitle } from "../../lib/shortcuts";
import { RunningBar } from "../../components/RunningBar";

/** `busy`: what the focused tab is doing (a run, a load-more) and since when, shown instead of the status text; ■ cancels it */
function StatusBar({ run, status, busy, readOnly, update, onUpdate, jobs }: { run: { canRun: boolean; running: boolean; onRun: () => void; onCancel: () => void }; status: string; busy?: { label: string; since: number }; readOnly: boolean; update?: string; onUpdate: () => void; jobs: TransferJob[] }) {
  const w = useStatusWidgets();
  return (
    <div className="statusbar">
      <IconButton size="sm" tone="ok" disabled={!run.canRun} onClick={run.onRun} title={keyTitle("Execute", "editor.run")}>
        ▶
      </IconButton>
      <IconButton size="sm" tone="danger" disabled={!run.running} onClick={run.onCancel} title={keyTitle("Cancel Running Statements", "editor.cancel")}>
        ■
      </IconButton>
      {busy ? (
        <RunningBar label={busy.label} since={busy.since} />
      ) : (
        <Text truncate className="sb-text">
          {status}
        </Text>
      )}
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
    </div>
  );
}

export { StatusBar };
