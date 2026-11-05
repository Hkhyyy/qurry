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
function tuiScene(name: string, widths: number[], note: string): Scene {
  const W = 78;
  const line = (cells: string[], color: string, bold = false, cur = -1) => {
    const runs: Run[] = [r("┃", CYAN), r(" ")];
    cells.forEach((c, i) => {
      const t = c.padEnd(widths[i]).slice(0, widths[i]);
      runs.push(i === cur ? r(t, "#1e1f22", CYAN, true) : r(t, color, null, bold));
      runs.push(r(" "));
    });
    const used = 2 + widths.reduce((a, b) => a + b + 1, 0);
    runs.push(r(" ".repeat(Math.max(0, W - 1 - used))), r("┃", CYAN));
    return runs;
  };
  const rowsT = rows.map((rw) => rw.map((v) => text(v)));
  const lines: Run[][] = [
    [r("┏", CYAN), r(" orders ", CYAN, null, true), r("━".repeat(W - 10), CYAN), r("┓", CYAN)],
    line(cols, FG, true),
    ...rowsT.map((rw, i) => line(rw, FG, false, i === 0 ? 3 : -1)),
    [r("┗" + "━".repeat(W - 2) + "┛", CYAN)],
    [r(` < narrower  > wider  = fit  ${note}`, DIM)],
  ];
  return { name, desc: note, w: W, h: lines.length, rows: lines };
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
    <div style={{ padding: 18, display: "grid", gridTemplateColumns: "1fr 1fr", gap: 18 }}>
      <Frame title="Before" note="memo is cut at its computed width">
        <TuiScreen scene={tuiScene("before", [5, 11, 13, 16, 16], "(memo: 16)")} />
      </Frame>
      <Frame title="After > > > (or =)" note="the current column (memo) widened; widths saved per table">
        <TuiScreen scene={tuiScene("after", [5, 11, 13, 30, 16], "(memo: 30)")} />
      </Frame>
    </div>
  ),
};
