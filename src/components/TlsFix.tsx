import { api, ConnectionView } from "../lib/api";
import { shared, sharedReady, useSharedReady } from "../lib/shared";
import { TlsOptIn } from "../ui";

/** The error is one where "Allow unencrypted connection" helps (false while the WebAssembly module isn't loaded: the bare error stays) */
export const needsTlsOptIn = (msg: string | null | undefined) => !!msg && sharedReady() && shared().tlsOptIn(msg);

/**
 * Under a tree, console or table tab connect error that needs TLS: the one-click "Allow unencrypted" for the saved data source, then `then`
 * (load the tree again, or only reconnect). Nothing for other errors
 */
export function TlsFix({ conn, msg, then }: { conn: ConnectionView | undefined; msg: string | null | undefined; then: () => unknown }) {
  useSharedReady();
  if (!conn || !needsTlsOptIn(msg)) return null;
  return (
    <TlsOptIn
      host={conn.host}
      production={!!conn.options?.confirmChanges}
      onAllow={async () => {
        await api.allowPlaintext(conn.id);
        await then();
      }}
    />
  );
}
