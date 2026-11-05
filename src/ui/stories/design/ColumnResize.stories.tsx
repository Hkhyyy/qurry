import type { Meta, StoryObj } from "@storybook/react-vite";
import { useRef, useState, type ReactNode } from "react";
import { Text } from "../../index";
import { TuiScreen, type Run, type Scene } from "../TuiScreen";

// Agreed design for 0.3.2, #109: resizing result grid columns (GUI drag / double-click fit / menu, TUI keys).
// Not product code: replace it with the real components when the issue is built, then delete this story.
const meta: Meta = { title: "Design/0.3.2/Column resize (#109)", parameters: { layout: "fullscreen" } };
export default meta;
type S = StoryObj;

const cols = ["idx", "order_no", "receiver_name", "memo", "created_at"];
const rows = [
  [486, "O-20261101", "김현교", "first order after the promotion; customer asked for a call before delivery", "2026-11-01 10:10"],
  [487, "O-20261102", "이수진", null, "2026-11-02 10:11"],
  [488, "O-20261103", "박민수", "refund requested", "2026-11-03 10:12"],
  [489, "O-20261104", "최지우", "gift wrap", "2026-11-04 10:13"],
];
const computed = [64, 110, 120, 160, 140];
const text = (v: unknown) => (v == null ? "<null>" : String(v));
const fit = (i: number) => Math.min(Math.max(...rows.map((r) => text(r[i]).length * 7.3 + 20), cols[i].length * 7.6 + 48, 64), 900);

function Grid() {
  const [w, setW] = useState(computed);
  const [menu, setMenu] = useState<{ x: number; y: number; i: number } | null>(null);
  const drag = useRef<{ i: number; x: number; w: number } | null>(null);
  const set = (i: number, v: number) => setW((ws) => ws.map((x, j) => (j === i ? Math.max(40, Math.round(v)) : x)));
  const onMove = (e: React.MouseEvent) => drag.current && set(drag.current.i, drag.current.w + e.clientX - drag.current.x);
  return (
    <div onMouseMove={onMove} onMouseUp={() => (drag.current = null)} onMouseLeave={() => (drag.current = null)} onClick={() => setMenu(null)} style={{ position: "relative", overflow: "auto", border: "1px solid var(--border)", borderRadius: 6, background: "var(--bg)", fontFamily: "JetBrains Mono, Menlo, monospace", fontSize: 12, userSelect: "none" }}>
      <div style={{ display: "flex", background: "var(--panel)", borderBottom: "1px solid var(--border)", width: "max-content" }}>
        {cols.map((c, i) => (
          <div
            key={c}
            onContextMenu={(e) => {
              e.preventDefault();
              setMenu({ x: e.nativeEvent.offsetX + w.slice(0, i).reduce((a, b) => a + b, 0), y: 26, i });
            }}
            style={{ position: "relative", width: w[i], flex: "none", padding: "4px 10px", color: "var(--muted)", fontWeight: 600, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", boxSizing: "border-box" }}
            title="Drag the right edge to resize · double-click it to fit · right-click for more"
          >
            {c}
            <span
              onMouseDown={(e) => {
                e.stopPropagation();
                drag.current = { i, x: e.clientX, w: w[i] };
              }}
              onDoubleClick={() => set(i, fit(i))}
              style={{ position: "absolute", right: 0, top: 0, bottom: 0, width: 6, cursor: "col-resize", borderRight: "2px solid var(--accent)", opacity: 0.35 }}
            />
          </div>
        ))}
      </div>
      {rows.map((r, ri) => (
        <div key={ri} style={{ display: "flex", width: "max-content", background: ri % 2 ? "var(--panel)" : undefined }}>
          {r.map((v, i) => (
            <div key={i} style={{ width: w[i], flex: "none", padding: "3px 10px", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", boxSizing: "border-box", textAlign: typeof v === "number" ? "right" : "left", color: v == null ? "var(--muted)" : undefined, fontStyle: v == null ? "italic" : undefined }}>
              {text(v)}
            </div>
          ))}
        </div>
      ))}
      {menu && (
        <div style={{ position: "absolute", left: menu.x, top: menu.y, background: "var(--panel)", border: "1px solid var(--border)", borderRadius: 6, boxShadow: "0 6px 18px rgba(0,0,0,.35)", padding: 4, fontFamily: "inherit", fontSize: 12, zIndex: 2 }}>
          {[
            ["Fit Column", "double-click the edge", () => set(menu.i, fit(menu.i))],
            ["Fit All Columns", "", () => setW(cols.map((_, i) => fit(i)))],
            ["Reset Widths", "computed widths", () => setW(computed)],
            ["Narrow / Widen", "⌃⌥← / ⌃⌥→", () => {}],
          ].map(([l, h, f]) => (
            <div key={l as string} onClick={f as () => void} style={{ display: "flex", gap: 18, padding: "3px 10px", cursor: "pointer" }}>
              <span>{l as string}</span>
              <span style={{ marginLeft: "auto", color: "var(--muted)" }}>{h as string}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

const r = (t: string, fg: string | null = null, bg: string | null = null, bold = false): Run => [t, fg, bg, bold, false, false, false, false];
const CYAN = "#56b6c2";
const FG = "#abb2bf";
const DIM = "#5c6370";
const KEY = "#e5c07b";
// ASCII only: the TUI scene renderer gives every char one cell
const tuiRows = [
  ["486", "O-20261101", "kim", "first order after the promotion; call before delivery", "2026-11-01"],
  ["487", "O-20261102", "lee", "<null>", "2026-11-02"],
  ["488", "O-20261103", "park", "refund requested", "2026-11-03"],
];
const tuiCols = ["idx", "order_no", "name", "memo", "created_at"];

/** One TUI grid with the cursor on memo, the given widths, and a line saying which key was pressed */
function tuiScene(name: string, memoW: number, pressed: string, result: string): Scene {
  const widths = [4, 11, 5, memoW, 10];
  const inner = 1 + widths.reduce((a, b) => a + b + 1, 0);
  const W = inner + 2;
  const line = (cells: string[], bold: boolean, cur: boolean) => {
    const runs: Run[] = [r("┃", CYAN), r(" ")];
    cells.forEach((c, i) => {
      const t = (c.length > widths[i] ? c.slice(0, widths[i] - 1) + "…" : c).padEnd(widths[i]);
      runs.push(cur && i === 3 ? r(t, "#1e1f22", CYAN, true) : r(t, i === 3 && bold ? CYAN : FG, null, bold));
      runs.push(r(" "));
    });
    runs.push(r("┃", CYAN));
    return runs;
  };
  const title = " orders ";
  const lines: Run[][] = [
    [r("┏", CYAN), r(title, CYAN, null, true), r("━".repeat(W - 2 - title.length), CYAN), r("┓", CYAN)],
    line(tuiCols, true, false),
    ...tuiRows.map((rw, i) => line(rw, false, i === 0)),
    [r("┗" + "━".repeat(W - 2) + "┛", CYAN)],
    [r(" pressed: ", DIM), r(pressed, KEY, null, true), r("   → " + result, FG)],
    [r(" keys: ", DIM), r("<", KEY, null, true), r(" narrower  ", DIM), r(">", KEY, null, true), r(" wider  ", DIM), r("=", KEY, null, true), r(" fit to content", DIM)],
  ];
  return { name, desc: result, w: Math.max(W, 60), h: lines.length, rows: lines };
}

function Frame({ title, note, children }: { title: string; note: string; children: ReactNode }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
      <Text size="sm">
        <b>{title}</b> — {note}
      </Text>
      {children}
    </div>
  );
}

export const Gui: S = {
  name: "GUI — drag, double-click to fit, header menu (try it)",
  render: () => (
    <div style={{ padding: 18, display: "flex", flexDirection: "column", gap: 14 }}>
      <Frame title="Result grid" note="drag a header's right edge · double-click the edge to fit the content · right-click a header for Fit Column / Fit All Columns / Reset Widths · ⌃⌥← / ⌃⌥→ on the focused column">
        <Grid />
      </Frame>
      <Text size="sm" className="muted">
        Table tabs remember widths per table (shared by GUI and TUI); console results keep them for that result. Reset Widths goes back to the computed ones.
      </Text>
    </div>
  ),
};

export const Tui: S = {
  name: "TUI — < > = on the current column",
  render: () => (
    <div style={{ padding: 18, display: "flex", flexDirection: "column", gap: 18 }}>
      <Text size="sm">The cursor is on the memo column (highlighted cell). Its value is longer than the computed width, so it ends in "…".</Text>
      <Frame title="1. As computed" note="memo is 16 cells wide and cut">
        <TuiScreen scene={tuiScene("start", 16, "(nothing)", "memo 16 cells, cut")} />
      </Frame>
      <Frame title="2. Press > three times" note="each > widens the current column by 4 cells">
        <TuiScreen scene={tuiScene("wider", 28, "> > >", "memo 28 cells")} />
      </Frame>
      <Frame title="3. Press =" note="fits the column to its longest loaded value; the width is saved for this table">
        <TuiScreen scene={tuiScene("fit", 53, "=", "memo fits its content (53 cells)")} />
      </Frame>
    </div>
  ),
};
