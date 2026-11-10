import { create } from "zustand";
import { api, type Reconnecting } from "./api";

/** After this long reconnecting the toast adds why it may take a while (qurry_shared::errors::RECONNECT_SLOW) */
export const RECONNECT_SLOW_MS = 10_000;
/** How often the backend is asked which sources reconnect, while a load waits on one (nothing is asked otherwise) */
export const RECONNECT_POLL_MS = 1000;

/** What the toast (and the status bar, while it waits) says for one data source that reconnects (#135) */
export type ReconnectToast =
  | { phase: "waiting"; id: string; message: string; slow: string; since: number }
  | { phase: "failed"; id: string; title: string; reason: string }
  | { phase: "cancelled"; id: string; text: string };

/** What waitOn throws when its load failed while the source reconnected, or was cancelled then: the toast has it (with Retry), so
 *  nothing more is sent. String() is its text */
export class ReconnectFailed {
  constructor(readonly text: string) {}
  toString() {
    return this.text;
  }
}

/** The toasts, one per data source */
export const useReconnect = create<{ toasts: ReconnectToast[] }>()(() => ({ toasts: [] }));

/** A load waiting on a data source (by its call id): the source, and how to run it again (Retry) */
const waits = new Map<string, { base: string; retry: () => void }>();
/** The last reconnect seen for each source with a load waiting: the texts its failure and Cancel use */
const seen = new Map<string, Reconnecting>();
/** When the backend last listed each of those sources */
const listedAt = new Map<string, number>();
/** The failed loads' Retry, by source */
const retries = new Map<string, (() => void)[]>();
const cancelled = new Set<string>();
let timer: ReturnType<typeof setInterval> | undefined;

const toastOf = (id: string) => useReconnect.getState().toasts.find((t) => t.id === id);
const setToast = (id: string, t: ReconnectToast | null) =>
  useReconnect.setState((s) => ({ toasts: [...s.toasts.filter((x) => x.id !== id), ...(t ? [t] : [])] }));

async function tick() {
  if (!waits.size) {
    clearInterval(timer);
    timer = undefined;
    return;
  }
  const now = await api.reconnecting().catch(() => [] as Reconnecting[]);
  const bases = new Set([...waits.values()].map((w) => w.base));
  for (const r of now) {
    if (!bases.has(r.id)) continue;
    seen.set(r.id, r);
    listedAt.set(r.id, Date.now());
    if (toastOf(r.id)?.phase !== "waiting") setToast(r.id, { phase: "waiting", id: r.id, message: r.message, slow: r.slow, since: Date.now() });
  }
  // No longer listed: it reconnected and its loads go on, so the toast goes. After a beat's grace: the backend stops listing it a
  // moment before a load that couldn't reconnect fails
  for (const t of useReconnect.getState().toasts) {
    if (t.phase === "waiting" && !now.some((r) => r.id === t.id) && Date.now() - (listedAt.get(t.id) ?? 0) > RECONNECT_POLL_MS * 1.5) setToast(t.id, null);
  }
}

/** The reason in a load's error when the core says it couldn't reconnect the source ("<failed>: <reason>", its tunnel didn't open
 *  again; qurry_shared::errors::reconnect_reason). Undefined for any other error: the load's own, also after a reconnect */
const reconnectReason = (r: Reconnecting, e: string) => (e.startsWith(`${r.failed}: `) ? e.slice(r.failed.length + 2) : undefined);

/**
 * Runs `call` (a table_detail or list_objects with its call id) on `target`, a load that a reconnecting tunnel may hold up: while the
 * backend says the source reconnects, the toast and the status bar say so, with Cancel. When the core then says it couldn't
 * reconnect, the toast keeps the reason and Retry (`retry`) and this throws ReconnectFailed with that text; after Cancel, one saying
 * it was cancelled. Any other error is the call's own
 */
export async function waitOn<T>(target: string, call: (callId: string) => Promise<T>, retry: () => void): Promise<T> {
  const base = target.split("::")[0];
  // Unique across webview reloads: a cancel the backend keeps for a call that had just ended never matches a new one
  const callId = crypto.randomUUID();
  // A new load on a source whose last one failed or was cancelled: that toast goes
  if (toastOf(base) && toastOf(base)?.phase !== "waiting") dismissReconnect(base);
  waits.set(callId, { base, retry });
  timer ??= setInterval(tick, RECONNECT_POLL_MS);
  try {
    const v = await call(callId);
    // Cancelled as it answered: the answer is dropped
    if (cancelled.has(callId)) throw new ReconnectFailed(seen.get(base)?.cancelled ?? "Cancelled");
    return v;
  } catch (e) {
    if (e instanceof ReconnectFailed) throw e;
    const r = seen.get(base);
    if (cancelled.has(callId)) throw new ReconnectFailed(r?.cancelled ?? String(e));
    const reason = r && reconnectReason(r, String(e));
    if (!r || reason === undefined) throw e;
    retries.set(base, [...(retries.get(base) ?? []), retry]);
    setToast(base, { phase: "failed", id: base, title: r.failed, reason });
    throw new ReconnectFailed(String(e));
  } finally {
    waits.delete(callId);
    cancelled.delete(callId);
    if (![...waits.values()].some((w) => w.base === base)) {
      seen.delete(base);
      listedAt.delete(base);
      if (toastOf(base)?.phase === "waiting") setToast(base, null);
    }
  }
}

const stop = (callId: string) => {
  cancelled.add(callId);
  void api.cancelCall(callId);
};

/** Cancel: stops the source's waiting loads (the backend drops the connect they wait on) */
export function cancelReconnect(id: string) {
  const text = seen.get(id)?.cancelled;
  for (const [callId, w] of waits) if (w.base === id) stop(callId);
  if (text) setToast(id, { phase: "cancelled", id, text });
}

/** A table tab's own Cancel while its load waits (`callId`): while the toast says its source reconnects, that reconnect's Cancel
 *  (all its loads, as the toast's and the status bar's); otherwise this load alone, which then fails with "Cancelled" */
export function cancelWait(callId: string) {
  const w = waits.get(callId);
  if (!w) return;
  if (toastOf(w.base)?.phase === "waiting") cancelReconnect(w.base);
  else stop(callId);
}

/** Retry: runs the source's failed loads again */
export function retryReconnect(id: string) {
  const again = retries.get(id) ?? [];
  dismissReconnect(id);
  again.forEach((f) => f());
}

/** Closes the source's toast (failed or cancelled) */
export function dismissReconnect(id: string) {
  retries.delete(id);
  setToast(id, null);
}

/** The source the status bar's running indicator speaks for: the first one waiting */
export const waitingToast = (toasts: ReconnectToast[]) => toasts.find((t): t is Extract<ReconnectToast, { phase: "waiting" }> => t.phase === "waiting");
