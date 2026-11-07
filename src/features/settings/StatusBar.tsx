import { Spacer, StatusItem, Text } from "../../ui";
import { useStatusWidgets, type FocusWidgets } from "../../lib/statusbar";
import { TransferBar, type TransferJob } from "../../components/Transfer";
import { RunningBar } from "../../components/RunningBar";

/** Information only (DataGrip): what the focused tab is doing (`busy`: a run, a load-more, and since when, shown instead of the status
 * text; its ✕ on hover cancels it), then on the right the focused grid's shown columns, the focused editor's or grid's position, UTF-8.
 * `widgets`: what Storybook shows instead of the focused editor's or grid's */
function StatusBar({ status, busy, readOnly, update, onUpdate, jobs, widgets }: { status: string; busy?: { label: string; since: number; onCancel: () => void }; readOnly: boolean; update?: string; onUpdate: () => void; jobs: TransferJob[]; widgets?: FocusWidgets }) {
  const live = useStatusWidgets();
  const w = widgets ? { ...live, ...widgets } : live;
  return (
    <div className="statusbar">
      {busy ? (
        <RunningBar label={busy.label} since={busy.since} onCancel={busy.onCancel} />
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
      {w.cols && <StatusItem title="Columns shown in the grid">{w.cols}</StatusItem>}
      {w.pos && <StatusItem title={w.pos.startsWith("Row") ? "Current cell" : "Caret position"}>{w.pos}</StatusItem>}
      <StatusItem>UTF-8</StatusItem>
    </div>
  );
}

export { StatusBar };
