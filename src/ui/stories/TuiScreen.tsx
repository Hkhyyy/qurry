/** TUI scene (drawn on a virtual terminal by crates/tui/src/kit_snapshots.rs), with chars · colors as is */
export type Run = [string, string | null, string | null, boolean, boolean, boolean, boolean, boolean];
export interface Scene {
  name: string;
  desc: string;
  w: number;
  h: number;
  rows: Run[][];
}

const FG = "#abb2bf";

/**
 * Terminals give each char a fixed cell, but in web fonts symbols (⛁ ◧ ▦ ▾ ⌥ …) are drawn with other fonts and differ in width → lines shift.
 * Chars other than Latin letters · digits · common symbols each go in an exact 1ch box (two-cell chars as a group of 2ch — below)
 */
function cells(t: string) {
  const out: React.ReactNode[] = [];
  let ascii = "";
  for (const ch of t) {
    if (ch.charCodeAt(0) < 0x80) {
      ascii += ch;
      continue;
    }
    if (ascii) out.push(ascii), (ascii = "");
    out.push(
      <span key={out.length} style={{ display: "inline-block", width: "1ch", overflow: "hidden", textAlign: "center", verticalAlign: "top" }}>
        {ch}
      </span>,
    );
  }
  if (ascii) out.push(ascii);
  return out;
}
const BG = "#1e1f22";

export function TuiScreen({ scene }: { scene: Scene }) {
  return (
    <div>
      <div style={{ marginBottom: 8, color: "var(--ui-text-muted)" }}>{scene.desc}</div>
      <pre
        style={{
          display: "inline-block",
          margin: 0,
          padding: 8,
          background: BG,
          color: FG,
          font: "13px/1.25 'JetBrains Mono', Menlo, monospace",
          borderRadius: 6,
          boxShadow: "0 0 0 1px #393b40",
          // So two-cell Korean chars take exactly two cells
          fontVariantLigatures: "none",
        }}
      >
        {scene.rows.map((runs, y) => (
          <div key={y} style={{ height: "1.25em", whiteSpace: "pre" }}>
            {runs.map(([t, fg, bg, bold, ul, it, st, wide], i) => (
              <span
                key={i}
                style={{
                  color: fg ?? undefined,
                  background: bg ?? undefined,
                  fontWeight: bold ? 700 : undefined,
                  fontStyle: it ? "italic" : undefined,
                  textDecoration: [ul && "underline", st && "line-through"].filter(Boolean).join(" ") || undefined,
                  // Two-cell chars: exactly two cells regardless of web font width
                  ...(wide ? { display: "inline-block", width: "2ch", textAlign: "center", overflow: "hidden", verticalAlign: "top" } : {}),
                }}
              >
                {wide ? t : cells(t)}
              </span>
            ))}
          </div>
        ))}
      </pre>
    </div>
  );
}
