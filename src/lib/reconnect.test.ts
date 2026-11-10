import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Reconnecting } from "./api";

const backend = { reconnecting: [] as Reconnecting[], asked: 0, cancelled: [] as string[] };
vi.mock("./api", async (orig) => ({
  ...(await orig<typeof import("./api")>()),
  api: {
    reconnecting: async () => (backend.asked++, backend.reconnecting),
    cancelCall: async (id: string) => void backend.cancelled.push(id),
  },
}));
const { cancelReconnect, cancelWait, ReconnectFailed, RECONNECT_POLL_MS, retryReconnect, useReconnect, waitOn, waitingToast } = await import("./reconnect");

const prod: Reconnecting = { id: "prod", message: "Reconnecting prod through AWS SSM", slow: "Taking longer…", failed: "Couldn't reconnect prod through AWS SSM", cancelled: "Cancelled while reconnecting prod through AWS SSM." };
/** A call that ends when the test says so */
const pending = () => {
  let ok!: (v: string) => void, fail!: (e: unknown) => void;
  const p = new Promise<string>((a, b) => ((ok = a), (fail = b)));
  return { p, ok, fail };
};
const toasts = () => useReconnect.getState().toasts;
const beat = () => vi.advanceTimersByTimeAsync(RECONNECT_POLL_MS);

beforeEach(() => {
  vi.useFakeTimers();
  Object.assign(backend, { reconnecting: [], asked: 0, cancelled: [] });
  useReconnect.setState({ toasts: [] });
});
// The loads have ended: the next beat stops the polling (as it does in the app)
afterEach(async () => {
  await beat();
  vi.useRealTimers();
});

describe("waitOn (#135)", () => {
  it("asks only while a load waits, and nothing shows for a source that doesn't reconnect", async () => {
    await beat();
    expect(backend.asked).toBe(0);
    const c = pending();
    const r = waitOn("dev::shop", () => c.p, () => {});
    backend.reconnecting = [prod];
    await beat();
    expect(backend.asked).toBe(1);
    expect(toasts()).toEqual([]);
    c.ok("done");
    expect(await r).toBe("done");
    await beat();
    expect(backend.asked).toBe(1);
  });

  it("while it waits on a source that reconnects: the toast and the status bar say so; it goes once the load ends", async () => {
    const c = pending();
    const r = waitOn("prod::orders", () => c.p, () => {});
    backend.reconnecting = [prod];
    await beat();
    expect(waitingToast(toasts())).toMatchObject({ phase: "waiting", id: "prod", message: prod.message, slow: prod.slow });
    c.ok("rows");
    await r;
    expect(toasts()).toEqual([]);
  });

  it("a failure keeps the reason and Retry, which runs the load again", async () => {
    const c = pending();
    const retry = vi.fn();
    const r = waitOn("prod", () => c.p, retry);
    backend.reconnecting = [prod];
    await beat();
    backend.reconnecting = [];
    c.fail("TargetNotConnected");
    const e = await r.catch((e) => e);
    expect(e).toBeInstanceOf(ReconnectFailed);
    expect(String(e)).toBe(`${prod.failed}: TargetNotConnected`);
    expect(toasts()).toEqual([{ phase: "failed", id: "prod", title: prod.failed, reason: "TargetNotConnected" }]);
    retryReconnect("prod");
    expect(retry).toHaveBeenCalledOnce();
    expect(toasts()).toEqual([]);
  });

  it("Cancel stops the source's loads by their call ids and leaves what it cancelled", async () => {
    const [a, b] = [pending(), pending()];
    const ids: string[] = [];
    const ra = waitOn("prod", (id) => (ids.push(id), a.p), () => {});
    const rb = waitOn("prod::other", (id) => (ids.push(id), b.p), () => {});
    backend.reconnecting = [prod];
    await beat();
    cancelReconnect("prod");
    expect(backend.cancelled).toEqual(ids);
    a.fail("Cancelled");
    b.fail("Cancelled");
    const errors = await Promise.all([ra, rb].map((r) => r.catch((e) => String(e))));
    expect(errors).toEqual([prod.cancelled, prod.cancelled]);
    expect(toasts()).toEqual([{ phase: "cancelled", id: "prod", text: prod.cancelled }]);
  });

  it("a load's own Cancel: that load alone while nothing shows, the source's reconnect once the toast says it reconnects", async () => {
    const [a, b] = [pending(), pending()];
    const ids: string[] = [];
    const ra = waitOn("prod", (id) => (ids.push(id), a.p), () => {});
    const rb = waitOn("prod::other", (id) => (ids.push(id), b.p), () => {});
    // Unique across webview reloads (the backend keeps a late cancel for a while), not a counter that starts again
    expect(ids[0]).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-/);
    expect(ids[0]).not.toBe(ids[1]);
    cancelWait(ids[0]);
    expect(backend.cancelled).toEqual([ids[0]]);
    a.fail("Cancelled");
    expect(String(await ra.catch((e) => e))).toBe("Cancelled");
    expect(toasts()).toEqual([]);
    backend.reconnecting = [prod];
    await beat();
    const c = pending();
    const rc = waitOn("prod", (id) => (ids.push(id), c.p), () => {});
    cancelWait(ids[2]);
    expect(backend.cancelled).toEqual([ids[0], ids[1], ids[2]]);
    expect(toasts()).toEqual([{ phase: "cancelled", id: "prod", text: prod.cancelled }]);
    b.fail("Cancelled");
    c.fail("Cancelled");
    expect(await Promise.all([rb, rc].map((r) => r.catch((e) => String(e))))).toEqual([prod.cancelled, prod.cancelled]);
  });

  it("a load cancelled as it answered drops the answer", async () => {
    const c = pending();
    let id = "";
    const r = waitOn("dev", (i) => ((id = i), c.p), () => {});
    cancelWait(id);
    c.ok("rows");
    const e = await r.catch((e) => e);
    expect(e).toBeInstanceOf(ReconnectFailed);
    expect(String(e)).toBe("Cancelled");
  });

  it("an error while nothing reconnects is the load's own", async () => {
    const c = pending();
    const r = waitOn("dev", () => c.p, () => {});
    c.fail("permission denied for table users");
    expect(await r.catch((e) => e)).toBe("permission denied for table users");
    expect(toasts()).toEqual([]);
  });
});
