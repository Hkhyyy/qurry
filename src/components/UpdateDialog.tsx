import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, isReleaseUrl, UpdateInfo } from "../lib/api";

/** 새 버전 안내: 릴리스 노트 + Update & Restart (앱 안에서 받아 바꿔 넣고 다시 켠다). 안 되면 릴리스 페이지에서 받는다 */
/** beforeInstall: 다시 켜기 전에 할 일 (작업 공간 저장, 커밋 안 한 변경 묻기). false 면 취소 */
export function UpdateDialog({ info, onClose, beforeInstall }: { info: UpdateInfo; onClose: () => void; beforeInstall?: () => boolean | Promise<boolean> }) {
  const r = info.latest!;
  // 이 저장소의 릴리스 주소만 연다 (아니면 릴리스 목록으로)
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
      await api.installUpdate(); // 성공하면 앱이 다시 켜진다
    } catch (e) {
      setError(String(e));
      setProgress(null);
      setBusy(false);
    }
  };
  const close = () => !busy && onClose();
  return (
    <div className="modal-backdrop" onMouseDown={close}>
      <div className="modal update" onMouseDown={(e) => e.stopPropagation()} onKeyDown={(e) => e.key === "Escape" && close()}>
        <div className="modal-title">Qurry {r.version} is available</div>
        <div className="hint" style={{ marginTop: 0 }}>
          You have {info.current}. Update &amp; Restart downloads it, replaces this app and opens it again (open tabs are kept).
        </div>
        {/* 바뀐 것만, 버전마다 (설치 안내는 Release Page 에) */}
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
            <div className="muted">See the release page for what changed.</div>
          )}
        </div>
        {progress && <div className="hint">{progress}</div>}
        {error && <div className="hint" style={{ color: "var(--danger)" }}>{error}</div>}
        <div className="modal-actions">
          <button onClick={() => open(r.url)}>Release Page</button>
          <span className="grow" />
          <button disabled={busy} onClick={onClose}>Later</button>
          <button className="primary" autoFocus disabled={busy} onClick={install}>
            {busy ? "Updating…" : "Update & Restart"}
          </button>
        </div>
      </div>
    </div>
  );
}
