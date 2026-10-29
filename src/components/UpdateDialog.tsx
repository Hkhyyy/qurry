import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, isReleaseUrl, UpdateInfo } from "../lib/api";
import { Button, Hint, Modal, Notice, Spacer, Text } from "../ui";

/** New version notice: release notes + Update & Restart (downloads, replaces and restarts in the app). If that fails, download from the release page */
/** beforeInstall: what to do before restarting (save the workspace, ask about uncommitted changes). false cancels */
export function UpdateDialog({ info, onClose, beforeInstall }: { info: UpdateInfo; onClose: () => void; beforeInstall?: () => boolean | Promise<boolean> }) {
  const r = info.latest!;
  // Only open release URLs of this repository (otherwise the release list)
  const open = (u: string) => {
    const url = isReleaseUrl(u) ? u : "https://github.com/Hkhyyy/qurry/releases";
    openUrl(url).catch(() => window.open(url, "_blank"));
  };
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let off: (() => void) | undefined;
    listen<[number, number | null]>("update-progress", ({ payload: [got, total] }) => {
      const mb = (n: number) => (n / 1048576).toFixed(1);
      setProgress(total ? `Downloading… ${mb(got)} / ${mb(total)} MB (${Math.round((got / total) * 100)}%)` : `Downloading… ${mb(got)} MB`);
    })
      .then((u) => (off = u))
      .catch(() => {});
    return () => off?.();
  }, []);
  const install = async () => {
    if (beforeInstall && !(await beforeInstall())) return;
    setBusy(true);
    setError(null);
    setProgress("Checking…");
    try {
      await api.installUpdate(); // the app restarts on success
    } catch (e) {
      setError(String(e));
      setProgress(null);
      setBusy(false);
    }
  };
  const close = () => !busy && onClose();
  return (
    <Modal
      title={<>Qurry {r.version} is available</>}
      size="lg"
      onClose={close}
      footer={
        <>
          <Button onClick={() => open(r.url)}>Release Page</Button>
          <Spacer />
          <Button disabled={busy} onClick={onClose}>Later</Button>
          <Button variant="primary" autoFocus disabled={busy} onClick={install}>
            {busy ? "Updating…" : "Update & Restart"}
          </Button>
        </>
      }
    >
      <Text as="div" tone="muted">You have {info.current}. Update &amp; Restart downloads it, replaces this app and opens it again (open tabs are kept).</Text>
      {/* Only the changes, per version (install instructions are on the Release Page) */}
      <div className="update-notes">
        {info.changes?.length ? (
          info.changes.map((v) => (
            <div key={v.version} className="update-version">
              <div className="update-version-title">{v.version}</div>
              <ul>
                {v.changes.length ? v.changes.map((c, i) => <li key={i}>{c}</li>) : <li className="muted">No notes</li>}
              </ul>
            </div>
          ))
        ) : (
          <Text as="div" tone="muted">See the release page for what changed.</Text>
        )}
      </div>
      {progress && <Hint>{progress}</Hint>}
      {error && <Notice tone="danger">{error}</Notice>}
    </Modal>
  );
}
