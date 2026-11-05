import type { Meta, StoryObj } from "@storybook/react-vite";
import { TuiScreen, type Run, type Scene } from "../TuiScreen";

// Agreed design for 0.3.2, #107: the TUI grid's horizontal scrollbar. Chosen: D (line + label).
// Not product code: replace it with the real components when the issue is built, then delete this story.
const meta: Meta = { title: "Design/0.3.2/TUI grid scrollbar (#107)", parameters: { layout: "fullscreen" } };
export default meta;
type S = StoryObj;

const W = 78;
const CYAN = "#56b6c2";
const GRAY = "#5c6370";
const DIM = "#4b5263";
const FG = "#abb2bf";
const NUM = "#2aacb8";

const r = (t: string, fg: string | null = null, bg: string | null = null, bold = false): Run => [t, fg, bg, bold, false, false, false, false];

const cols = ["idx", "order_no", "user_idx", "payment_type", "tx_type", "sub_type"];
const widths = [7, 11, 9, 13, 10, 10];
const rows = [
  ["486", "O-20261101", "100", "CARD", "PAY", "NORMAL"],
  ["487", "O-20261102", "103", "BANK", "PAY", "NORMAL"],
  ["488", "O-20261103", "106", "CARD", "REFUND", "PARTIAL"],
  ["489", "O-20261104", "109", "CARD", "PAY", "NORMAL"],
  ["490", "O-20261105", "112", "POINT", "PAY", "NORMAL"],
];
const pad = (s: string, w: number, right = false) => (right ? s.padStart(w) : s.padEnd(w)).slice(0, w);

type Bottom = (focused: boolean) => Run[];

function scene(name: string, desc: string, focused: boolean, bottom: Bottom, light = false): Scene {
  const v = focused ? "┃" : "│";
  const h = focused ? "━" : "─";
  const border = focused ? CYAN : GRAY;
  const inner = W - 2;
  const lines: Run[][] = [];
  const title = " orders  500+ rows ";
  lines.push([r(focused ? "┏" : "┌", border), r(title, focused ? CYAN : FG, null, focused), r(h.repeat(inner - title.length), border), r(focused ? "┓" : "┐", border)]);
  const head = cols.map((c, i) => pad(c, widths[i])).join(" ");
  lines.push([r(v, border), r(pad(" " + head, inner), FG, null, true), r(v, border)]);
  for (const row of rows) {
    const cells: Run[] = [r(" ")];
    row.forEach((c, i) => {
      const numeric = i === 0 || i === 2;
      cells.push(r(pad(c, widths[i], numeric), numeric ? NUM : FG));
      cells.push(r(" "));
    });
    const used = 1 + widths.reduce((a, b) => a + b + 1, 0);
    cells.push(r(" ".repeat(Math.max(0, inner - used))));
    lines.push([r(v, border), ...cells, r(v, border)]);
  }
  lines.push([r(focused ? "┗" : "└", border), ...bottom(focused), r(focused ? "┛" : "┘", border)]);
  lines.push([r(" a add  d delete  ⌫ NULL  ⌃s submit  ← → columns", DIM)]);
  return { name, desc, w: W, h: lines.length, rows: lines, light };
}

const inner = W - 2;
const start = 18;
const len = 28;
const fill = (left: Run[], thumb: Run[], right: Run[]) => [...left, ...thumb, ...right];

/** D. Line + position label at the right end: which columns are shown */
const label: Bottom = (f) => {
  const text = " ◀ cols 3–8 of 14 ▶ ";
  const track = inner - text.length;
  const s = Math.round((start * track) / inner);
  const l = Math.round((len * track) / inner);
  return fill([r("─".repeat(s), DIM)], [r("━".repeat(l), f ? CYAN : FG, null, true)], [r("─".repeat(track - s - l), DIM), r(text, f ? CYAN : FG)]);
};

function Board() {
  return (
    <div style={{ display: "grid", gap: 18, padding: 18, gridTemplateColumns: "1fr 1fr" }}>
      {[true, false].map((focused) => (
        <div key={String(focused)} style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          <div style={{ fontSize: 13 }}>
            <b>{focused ? "Grid focused" : "Grid not focused"}</b> — the thumb is the border line drawn heavier in the accent color over the shown columns, the rest dim, and the shown range at the right end
          </div>
          <TuiScreen scene={scene(focused ? "focused" : "not focused", "", focused, label)} />
        </div>
      ))}
    </div>
  );
}

export const Chosen: S = { name: "Line + shown columns label", render: () => <Board /> };
