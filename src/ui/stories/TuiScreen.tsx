/** TUI 장면 (crates/tui/src/kit_snapshots.rs 가 가상 터미널에 그린 것) 을 글자 · 색 그대로 */
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
 * 터미널은 글자마다 칸이 정해져 있지만, 웹 글꼴에서 기호 (⛁ ◧ ▦ ▾ ⌥ …) 는 다른 글꼴로 그려져 폭이 다르다 → 줄이 밀린다.
 * 영문 · 숫자 · 흔한 기호가 아닌 글자는 하나씩 정확히 1ch 상자에 넣는다 (두 칸 글자는 묶음째 2ch — 아래)
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
          // 한글 두 칸 글자가 정확히 두 칸이 되도록
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
                  // 두 칸 글자: 웹 글꼴 폭과 상관없이 정확히 두 칸
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
