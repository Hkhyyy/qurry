import { useEffect, useState } from "react";
import { Button, ProgressBar, Text } from "../ui";

/** 12.3 s · 1 m 05 s */
export function elapsedText(ms: number): string {
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`;
  const s = Math.floor(ms / 1000);
  return `${Math.floor(s / 60)} m ${String(s % 60).padStart(2, "0")} s`;
}

/** Running bar (above the result area): progress bar (end unknown, so it bounces) · elapsed time · Cancel */
export function RunningBar({ running, label = "Executing", onCancel }: { running: boolean; label?: string; onCancel?: () => void }) {
  const [start, setStart] = useState<number | null>(null);
  const [now, setNow] = useState(0);
  useEffect(() => {
    if (!running) return setStart(null);
    const s = Date.now();
    setStart(s);
    setNow(s);
    const h = setInterval(() => setNow(Date.now()), 100);
    return () => clearInterval(h);
  }, [running]);
  if (!running || start === null) return null;
  return (
    <div className="running-bar" role="status">
      <ProgressBar pct={null} width={140} />
      <Text size="sm">
        {label}… <b className="running-time">{elapsedText(now - start)}</b>
      </Text>
      {onCancel && (
        <Button size="sm" onClick={onCancel}>
          Cancel
        </Button>
      )}
    </div>
  );
}
