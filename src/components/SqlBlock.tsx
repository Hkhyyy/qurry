import { useMemo } from "react";
import { DbKind } from "../lib/api";
import { sharedOr, useSharedReady } from "../lib/shared";
import { CodeBlock } from "../ui";

/**
 * Characters colored at most; the rest (whole lines) stays plain so a preview of thousands of statements still opens at once.
 * Each colored piece costs about 3 µs to draw: 5,000 short UPDATEs (165,000 pieces) took 0.35–0.4 s colored in full, 0.08 s so
 */
const COLOR_LIMIT = 50_000;

/** The part of `sql` that gets colored: all of it, or up to the last line break before COLOR_LIMIT (COLOR_LIMIT itself in one long line) */
export function coloredPart(sql: string) {
  if (sql.length <= COLOR_LIMIT) return sql;
  const nl = sql.lastIndexOf("\n", COLOR_LIMIT);
  return sql.slice(0, nl > 0 ? nl : COLOR_LIMIT);
}

/** SQL shown in the console's colors (qurry_shared::highlight in `kind`'s dialect); plain until the WASM module has loaded */
export function SqlBlock({ sql, kind, className }: { sql: string; kind: DbKind; className?: string }) {
  const ready = useSharedReady();
  const spans = useMemo(() => sharedOr([], (s) => s.highlight(coloredPart(sql), kind)), [sql, kind, ready]);
  return (
    <CodeBlock className={className} spans={spans}>
      {sql}
    </CodeBlock>
  );
}
