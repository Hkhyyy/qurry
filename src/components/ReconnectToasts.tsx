import { useEffect, useState } from "react";
import { Button, Text, Toast, Toasts } from "../ui";
import { cancelReconnect, dismissReconnect, RECONNECT_SLOW_MS, retryReconnect, useReconnect, type ReconnectToast } from "../lib/reconnect";
import { elapsedText } from "./RunningBar";

/** One data source's toast (#135): while a load waits on its reconnect, what and how long with Cancel (past 10 s, why it may take a
 *  while); a failure keeps the reason and Retry; Cancel leaves what it cancelled. `now`: the clock (stories hold it still) */
export function ReconnectToastView({ t, now, onCancel, onRetry, onClose }: { t: ReconnectToast; now: number; onCancel: () => void; onRetry: () => void; onClose: () => void }) {
  if (t.phase === "waiting") {
    const ms = Math.max(0, now - t.since);
    return (
      <Toast actions={<Button size="sm" onClick={onCancel}>Cancel</Button>}>
        <Text>
          {t.message}… <b className="running-time">{elapsedText(ms)}</b>
        </Text>
        {ms > RECONNECT_SLOW_MS && (
          <Text tone="muted" size="sm">
            {t.slow}
          </Text>
        )}
      </Toast>
    );
  }
  if (t.phase === "failed")
    return (
      <Toast tone="danger" onClose={onClose} actions={<Button size="sm" variant="primary" onClick={onRetry}>Retry</Button>}>
        <Text strong>{t.title}</Text>
        <Text size="sm" mono tone="muted" className="reconnect-reason">
          {t.reason}
        </Text>
      </Toast>
    );
  return (
    <Toast onClose={onClose}>
      <Text tone="muted">{t.text}</Text>
    </Toast>
  );
}

/** The app's reconnect toasts, bottom right above the status bar */
export function ReconnectToasts() {
  const toasts = useReconnect((s) => s.toasts);
  const [now, setNow] = useState(Date.now);
  const ticking = toasts.some((t) => t.phase === "waiting");
  useEffect(() => {
    if (!ticking) return;
    const h = setInterval(() => setNow(Date.now()), 100);
    return () => clearInterval(h);
  }, [ticking]);
  if (!toasts.length) return null;
  return (
    <Toasts>
      {toasts.map((t) => (
        <ReconnectToastView key={t.id} t={t} now={now} onCancel={() => cancelReconnect(t.id)} onRetry={() => retryReconnect(t.id)} onClose={() => dismissReconnect(t.id)} />
      ))}
    </Toasts>
  );
}
