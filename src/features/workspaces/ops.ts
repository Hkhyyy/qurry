import type { ConnectionView } from "../../lib/api";

/** A workspace's Database Explorer list: its data sources in its order, ids of removed ones skipped (the explorer and This Workspace search) */
export const wsConns = (connIds: string[], conns: ConnectionView[]): ConnectionView[] =>
  connIds.map((id) => conns.find((c) => c.id === id)).filter((c): c is ConnectionView => !!c);
