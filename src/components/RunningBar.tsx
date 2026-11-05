import { useEffect, useState } from "react";
import { ProgressBar, Text } from "../ui";

/** 12.3 s · 1 m 05 s */
export function elapsedText(ms: number): string {
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`;
  const s = Math.floor(ms / 1000);
  return `${Math.floor(s / 60)} m ${String(s % 60).padStart(2, "0")} s`;
}

/** What the focused tab is doing (a run, a load-more), in the status bar where its text is: progress bar (end unknown, so it
 * bounces) · label · time since `since` (the tab's start, so it goes on across tab switches). The status bar's ■ cancels it */
export function RunningBar({ label, since }: { label: string; since: number }) {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    const h = setInterval(() => setNow(Date.now()), 100);
    return () => clearInterval(h);
  }, []);
  return (
    <span className="running-bar" role="status">
      <ProgressBar pct={null} width={120} label={label} />
      <Text size="sm">
        {label}… <b className="running-time">{elapsedText(Math.max(0, now - since))}</b>
      </Text>
    </span>
  );
}
