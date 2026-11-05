import type { Meta, StoryObj } from "@storybook/react-vite";
import type { ReactNode } from "react";
import { IconButton, Spacer, Text } from "../../index";

// Agreed design for 0.3.2, #107: where the GUI shows "cols 3–8 of 14". Chosen: D (status bar).
// Not product code: replace it with the real components when the issue is built, then delete this story.
const meta: Meta = { title: "Design/0.3.2/Shown columns indicator (#107)", parameters: { layout: "fullscreen" } };
export default meta;
type S = StoryObj;

const noop = () => {};
const allCols = ["idx", "order_no", "user_idx", "payment_type", "tx_type", "sub_type", "contract_idx", "merchant_idx", "partner_idx", "sales_idx", "receiver_name", "receiver_contact", "receiver_bank_code", "created_at"];
const shown = allCols.slice(2, 8);
const rows = Array.from({ length: 9 }, (_, i) => [100 + i * 3, i % 2 ? "BANK" : "CARD", i % 3 ? "PAY" : "REFUND", "NORMAL", 3100 + i, 77 + i]);
const label = "cols 3–8 of 14";

function Grid({ overlay, scrollbar }: { overlay?: ReactNode; scrollbar?: ReactNode }) {
  return (
    <div style={{ position: "relative", flex: 1, minHeight: 0, overflow: "hidden", background: "var(--bg)" }}>
      <table style={{ borderCollapse: "collapse", fontFamily: "JetBrains Mono, Menlo, monospace", fontSize: 12, width: "100%" }}>
        <thead>
          <tr>
            {shown.map((c) => (
              <th key={c} style={{ textAlign: "left", padding: "4px 10px", color: "var(--muted)", borderBottom: "1px solid var(--border)", background: "var(--panel)", whiteSpace: "nowrap" }}>
                {c}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((r, i) => (
            <tr key={i} style={{ background: i % 2 ? "var(--panel)" : undefined }}>
              {r.map((v, j) => (
                <td key={j} style={{ padding: "3px 10px", textAlign: typeof v === "number" ? "right" : "left" }}>
                  {v}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
      {scrollbar ?? <FakeScrollbar />}
      {overlay}
    </div>
  );
}

/** A horizontal scrollbar like the one the grid has (thumb over columns 3–8 of 14) */
function FakeScrollbar({ children }: { children?: ReactNode }) {
  return (
    <div style={{ position: "absolute", left: 0, right: 0, bottom: 0, height: 10, display: "flex", alignItems: "center", gap: 8, padding: "0 6px" }}>
      <div style={{ position: "relative", flex: 1, height: 6 }}>
        <div style={{ position: "absolute", left: `${(2 / 14) * 100}%`, width: `${(6 / 14) * 100}%`, top: 0, bottom: 0, borderRadius: 3, background: "var(--muted)", opacity: 0.6 }} />
      </div>
      {children}
    </div>
  );
}

function Toolbar({ extra }: { extra?: ReactNode }) {
  return (
    <div className="grid-toolbar">
      <Text size="sm" className="muted">
        500+ rows
      </Text>
      {extra}
    </div>
  );
}

function Footer({ right }: { right?: ReactNode }) {
  return (
    <div className="statusbar">
      <IconButton size="sm" tone="ok" onClick={noop} title="Execute">
        ▶
      </IconButton>
      <IconButton size="sm" tone="danger" onClick={noop} title="Cancel">
        ■
      </IconButton>
      <Text truncate className="sb-text">
        500 rows retrieved
      </Text>
      <Spacer />
      {right}
      <span style={{ padding: "0 6px" }}>UTF-8</span>
    </div>
  );
}

function Frame({ title, note, children }: { title: string; note: string; children: ReactNode }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
      <Text size="sm">
        <b>{title}</b> — {note}
      </Text>
      <div style={{ display: "flex", flexDirection: "column", height: 300, border: "1px solid var(--border)", borderRadius: 6, overflow: "hidden" }}>{children}</div>
    </div>
  );
}

export const Chosen: S = {
  name: "Status bar: cols 3–8 of 14",
  render: () => (
    <div style={{ padding: 18, maxWidth: 760 }}>
      <Frame title="Shown columns in the status bar" note="for the focused result grid, next to UTF-8; hidden when every column fits; updates while scrolling sideways">
        <Toolbar />
        <Grid />
        <Footer right={<span style={{ padding: "0 6px" }}>{label}</span>} />
      </Frame>
    </div>
  ),
};
export const ChosenLight: S = { ...Chosen, name: "Status bar — light", globals: { theme: "light" } };
