import { useEffect, useState } from "react";
import { Button, IconButton, Notice, Text } from "../ui";
import { IcStop } from "../ui/icons";
import { keyTitle } from "../lib/shortcuts";
import type { RedisLoad } from "../features/redis/useRedisLoad";
import { elapsedText } from "./RunningBar";

/** Past this the time shows beside Loading… (a quick load never flickers a number) */
const SHOW_TIME_MS = 1000;

/** ■ in the toolbar's ⟳ spot while a load runs (as the grid's one button) */
export const StopButton = ({ onClick }: { onClick: () => void }) => (
  <IconButton tone="danger" title={keyTitle("Cancel", "editor.cancel")} onClick={onClick}>
    <IcStop />
  </IconButton>
);

/** Loading… (or `label`…), and past a second how long, with ■ (`onCancel`) */
function Loading({ since, label = "Loading", onCancel }: { since: number; label?: string; onCancel?: () => void }) {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    const h = setInterval(() => setNow(Date.now()), 100);
    return () => clearInterval(h);
  }, []);
  const ms = Math.max(0, now - since);
  return (
    <div className="rd-load" role="status">
      <Text tone="muted">
        {label}…{ms > SHOW_TIME_MS && <b className="running-time"> {elapsedText(ms)}</b>}
      </Text>
      {onCancel && <StopButton onClick={onCancel} />}
    </div>
  );
}

/** The Terminal's line on its way (a reconnect it waits on has Cancel in the toast) */
export const Running = ({ since }: { since: number }) => <Loading since={since} label="Running" />;

/** A failed load: its reason in full and Retry (the status bar says it too) */
export const LoadFailed = ({ error, onRetry }: { error: string; onRetry: () => void }) => (
  <Notice tone="danger" className="rd-load-error">
    <span className="rd-load-why">{error}</span>
    <Button size="sm" onClick={onRetry}>
      Retry
    </Button>
  </Notice>
);

/** Where a Redis browser load's result goes, while it isn't there: Loading… with ■, or the failure with Retry; nothing when idle */
export function RedisLoadState({ load, onCancel, onRetry }: { load: RedisLoad; onCancel?: () => void; onRetry: () => void }) {
  if (load.phase === "loading") return <Loading since={load.since} onCancel={onCancel} />;
  if (load.phase === "failed") return <LoadFailed error={load.error} onRetry={onRetry} />;
  return null;
}
