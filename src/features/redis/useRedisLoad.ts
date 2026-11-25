import { useRef, useState } from "react";
import { cancelWait, waitOn } from "../../lib/reconnect";

/** One load of the Redis browser (#220): the key list's SCAN, the key counts, a key's value, Info / Config. `since`: when it started */
export type RedisLoad = { phase: "idle" } | { phase: "loading"; since: number } | { phase: "failed"; error: string };

export const CANCELLED = "Cancelled";

/**
 * A load as a database tab's: `run` calls the backend through waitOn (a reconnect it waits on shows in the toast and the status bar,
 * whose Retry runs `retry`), loading meanwhile; a failure keeps its reason here (shown with Retry) and on the status bar. `cancel`
 * stops it (the call, or the reconnect it waits on) and says so. A run that a newer one or a cancel replaced answers undefined
 */
export function useRedisLoad(onStatus: (s: string) => void) {
  const [state, setState] = useState<RedisLoad>({ phase: "idle" });
  const seq = useRef(0);
  const call = useRef<string | null>(null);
  const run = async <T>(target: string, fn: (callId: string) => Promise<T>, retry: () => void): Promise<T | undefined> => {
    const n = ++seq.current;
    setState({ phase: "loading", since: Date.now() });
    try {
      const v = await waitOn(target, (id) => ((call.current = id), fn(id)), retry);
      if (n !== seq.current) return undefined;
      setState({ phase: "idle" });
      return v;
    } catch (e) {
      if (n !== seq.current) return undefined;
      const error = String(e);
      setState({ phase: "failed", error });
      onStatus(error);
      return undefined;
    } finally {
      if (n === seq.current) call.current = null;
    }
  };
  const cancel = () => {
    const id = call.current;
    if (!id) return;
    seq.current++;
    call.current = null;
    cancelWait(id);
    setState({ phase: "failed", error: CANCELLED });
    onStatus(CANCELLED);
  };
  return { state, run, cancel, loading: state.phase === "loading" };
}
