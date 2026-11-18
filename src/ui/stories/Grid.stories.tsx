import type { Meta, StoryObj } from "@storybook/react-vite";
import { ReactNode, useEffect, useRef, useState } from "react";
import { CellEditor, CheckListPopover, GridCell, GridHead, GridHeaderCell, GridHeaderRow, GridRow, JsonText, Menu, RowNumberCell } from "..";
import type { Cell } from "../../lib/api";
import { alignTable, CELL_KIND, cellClasses } from "../../lib/gridAlign";
import { jsonCell } from "../../lib/jsonSpans";
import type { Alignments } from "../../lib/shared";
import { ValuePanel } from "../../components/ValuePanel";

/** Result grid cells (pieces used by ResultGrid): header strip · header row · header cell · row · row number · data cell · edit cell */
const meta: Meta = { title: "GUI/4 Grid" };
export default meta;

const W = [40, 90, 120, 110, 180];
const ROW = 24;

/**
 * Layout used by ResultGrid: the header strip (GridHead) sits above the scroller, not inside it, and only follows its
 * scrollLeft, so the header never leaves the top edge whatever the scroll timing. The row-number corner sits beside the strip
 */
function Frame({ header, width, height, children }: { header: ReactNode; width: number; height: number; children: ReactNode }) {
  const body = useRef<HTMLDivElement>(null);
  return (
    <div style={{ position: "relative", width: 600, height: 300, border: "1px solid var(--ui-border)" }}>
      <div className="grid-wrap">
        <GridHead
          body={body}
          corner={
            <GridHeaderRow width={W[0]} height={26}>
              <RowNumberCell width={W[0]} title="Select all" />
            </GridHeaderRow>
          }
        >
          {header}
        </GridHead>
        <div className="grid-scroll" ref={body} tabIndex={0}>
          <div style={{ width, height, position: "relative" }}>{children}</div>
        </div>
      </div>
    </div>
  );
}

export const States: StoryObj = {
  render: () => (
    <Frame
      width={560}
      height={ROW * 5}
      header={
        <GridHeaderRow width={560 - W[0]} height={26}>
          <GridHeaderCell name="id" type="int8" pk width={W[1]} sort={{ dir: "asc" }} onSortClick={() => {}} />
          <GridHeaderCell name="user_name" type="varchar" width={W[2]} colSelected onSortClick={() => {}} />
          <GridHeaderCell name="amount" type="numeric" width={W[3]} sort={{ dir: "desc", order: 2 }} onSortClick={() => {}} />
          <GridHeaderCell name="memo" type="text" width={W[4]} onSortClick={() => {}} onResize={() => {}} />
        </GridHeaderRow>
      }
    >
      <GridRow top={0} height={ROW} width={560}>
        <RowNumberCell width={W[0]}>1</RowNumberCell>
        <GridCell width={W[1]} kind="num" align="right">1</GridCell>
        <GridCell width={W[2]} selected>
          user_1
        </GridCell>
        <GridCell width={W[3]} kind="num" align="right">
          0.00
        </GridCell>
        <GridCell width={W[4]}>plain cell</GridCell>
      </GridRow>
      <GridRow top={ROW} height={ROW} width={560} odd>
        <RowNumberCell width={W[0]} selected>
          2
        </RowNumberCell>
        <GridCell width={W[1]} kind="num" align="right">2</GridCell>
        <GridCell width={W[2]} selected active>
          user_2 (cursor)
        </GridCell>
        <GridCell width={W[3]} kind="num" align="right" changed>
          1234.50
        </GridCell>
        <GridCell width={W[4]} kind="null" align="right">
          &lt;null&gt;
        </GridCell>
      </GridRow>
      <GridRow top={ROW * 2} height={ROW} width={560}>
        <RowNumberCell width={W[0]}>3</RowNumberCell>
        <GridCell width={W[1]} kind="num" align="right">3</GridCell>
        <GridCell width={W[2]} match>
          found (find)
        </GridCell>
        <GridCell width={W[3]} kind="num" align="right">2469.00</GridCell>
        <CellEditor width={W[4]} value="editing…" onChange={() => {}} onKeyDown={() => {}} onBlurOut={() => {}} more={{ title: "Choose date/time", onClick: () => {} }} />
      </GridRow>
      <GridRow top={ROW * 3} height={ROW} width={560} added>
        <RowNumberCell width={W[0]}>+</RowNumberCell>
        <GridCell width={W[1]} kind="null" align="right">&lt;default&gt;</GridCell>
        <GridCell width={W[2]}>new row (added)</GridCell>
        <GridCell width={W[3]} kind="num" align="right">10</GridCell>
        <GridCell width={W[4]}>—</GridCell>
      </GridRow>
      <GridRow top={ROW * 4} height={ROW} width={560} odd deleted>
        <RowNumberCell width={W[0]}>5</RowNumberCell>
        <GridCell width={W[1]} kind="num" align="right">5</GridCell>
        <GridCell width={W[2]}>deleted</GridCell>
        <GridCell width={W[3]} kind="num" align="right">4938.00</GridCell>
        <GridCell width={W[4]}>—</GridCell>
      </GridRow>
    </Frame>
  ),
};

const COLS = 24;
const COL_W = 110;
const ROWS = 200;
const WIDE = W[0] + COLS * COL_W;

/** Many columns and rows: scroll down and sideways — the header stays on the top edge, the row number column stays frozen */
export const Scrolling: StoryObj = {
  render: () => (
    <Frame
      width={WIDE}
      height={ROWS * ROW}
      header={
        <GridHeaderRow width={WIDE - W[0]} height={26}>
          {Array.from({ length: COLS }, (_, c) => (
            <GridHeaderCell key={c} name={`col_${c + 1}`} type="text" width={COL_W} sort={c === 0 ? { dir: "asc" } : undefined} onSortClick={() => {}} />
          ))}
        </GridHeaderRow>
      }
    >
      {Array.from({ length: ROWS }, (_, r) => (
        <GridRow key={r} top={r * ROW} height={ROW} width={WIDE} odd={r % 2 === 1}>
          <RowNumberCell width={W[0]}>{r + 1}</RowNumberCell>
          {Array.from({ length: COLS }, (_, c) => (
            <GridCell key={c} width={COL_W}>{`r${r + 1}c${c + 1}`}</GridCell>
          ))}
        </GridRow>
      ))}
    </Frame>
  ),
};

/** Settings › Appearance › Data grid › Alignment: one column per value class, as ResultGrid classifies and aligns them */
const CLASSES: { name: string; type: string; vals: Cell[] }[] = [
  { name: "name", type: "text", vals: ["abc", null, "하나"] },
  { name: "amount", type: "numeric", vals: ["129000.00", null, "7.50"] },
  { name: "paid_at", type: "timestamptz", vals: ["2026-10-28 09:12", null, "2026-10-28 09:40"] },
  { name: "refunded", type: "bool", vals: [true, false, null] },
  { name: "meta", type: "jsonb", vals: ['{"pg":"toss"}', null, "{}"] },
];
const CW = 108;
const AW = W[0] + CW * CLASSES.length;

function Aligned({ overrides }: { overrides: Alignments }) {
  return (
    <Frame
      width={AW}
      height={ROW * 3}
      header={
        <GridHeaderRow width={AW - W[0]} height={26}>
          {CLASSES.map((c) => (
            <GridHeaderCell key={c.name} name={c.name} type={c.type} width={CW} />
          ))}
        </GridHeaderRow>
      }
    >
      {[0, 1, 2].map((r) => (
        <GridRow key={r} top={ROW * r} height={ROW} width={AW} odd={r % 2 === 1}>
          <RowNumberCell width={W[0]}>{r + 1}</RowNumberCell>
          {CLASSES.map((c) => {
            const v = c.vals[r];
            const cls = cellClasses(c.type)(v);
            return (
              <GridCell key={c.name} width={CW} kind={CELL_KIND[cls]} align={alignTable(overrides, c.type)[cls]}>
                {v === null ? "<null>" : String(v)}
              </GridCell>
            );
          })}
        </GridRow>
      ))}
    </Frame>
  );
}

/** By default numbers sit on the right, everything else on the left, and a NULL where its column's type sits (in line with the values above it) */
export const Alignment: StoryObj = { render: () => <Aligned overrides={{}} /> };
/** settings.json "gridAlign": { "null": "right", "number": "center" }: only the changed classes move (NULL on the right in every column, as in 0.3.0) */
export const AlignmentChanged: StoryObj = { name: "Alignment (changed)", render: () => <Aligned overrides={{ null: "right", number: "center" }} /> };

/** json / jsonb cells: keys, strings, numbers, true/false/null and punctuation in color, still one line (no pretty-printing).
 *  A large value renders only what a column can show; from the first thing that isn't JSON on, the rest is plain */
const JSON_ROWS: (string | null)[] = [
  '{"type":"payment.paid","amount":129000,"live":true,"meta":{"pg":"toss","coupon":null}}',
  '{"type":"refund","memo":"고객 요청 🙏","lines":[1,2.5,-3e2]}',
  JSON.stringify({ items: Array.from({ length: 3000 }, (_, i) => ({ sku: `A-${i}`, qty: i })) }),
  '{"broken": oops, "rest": 1}',
  null,
];
const JW = 360;
const JSON_W = W[0] + W[1] + JW;

export const JsonColumn: StoryObj = {
  name: "JSON column",
  render: () => (
    <Frame
      width={JSON_W}
      height={ROW * JSON_ROWS.length}
      header={
        <GridHeaderRow width={JSON_W - W[0]} height={26}>
          <GridHeaderCell name="id" type="int8" width={W[1]} />
          <GridHeaderCell name="payload" type="jsonb" width={JW} />
        </GridHeaderRow>
      }
    >
      {JSON_ROWS.map((v, r) => (
        <GridRow key={r} top={ROW * r} height={ROW} width={JSON_W} odd={r % 2 === 1}>
          <RowNumberCell width={W[0]}>{r + 1}</RowNumberCell>
          <GridCell width={W[1]} kind="num" align="right">
            {r + 1}
          </GridCell>
          {v === null ? (
            <GridCell width={JW} kind="null">
              &lt;null&gt;
            </GridCell>
          ) : (
            <GridCell width={JW} selected={r === 1} active={r === 1}>
              <JsonText {...jsonCell(v)} />
            </GridCell>
          )}
        </GridRow>
      ))}
    </Frame>
  ),
};

/** Long values (Settings › Appearance › Data grid): a value past the limit comes cut and ends with a dim …, which stays in view however
 *  narrow the column (hover says where it was cut). The Value Editor shows a cut cell's text read-only while it reads the whole value
 *  by the row's key, then the whole value, editable; a result without a key keeps the cut text, saying why */
const CUT_JSON = JSON.stringify({ id: 1, items: Array.from({ length: 400 }, (_, k) => ({ sku: `A-${k}`, qty: k })) }).slice(0, 4096);
const CUT_TEXT = "long note ".repeat(600).slice(0, 4096);
const [DOC_W, NOTE_W] = [260, 200];
const CUT_W = W[0] + W[1] + DOC_W + NOTE_W;
const cutHover = "Cut at 4096 characters — the Value Editor (⇧↩) shows the whole value";
function CutPanel({ note, text, readOnly }: { note?: string; text: string; readOnly: boolean }) {
  return (
    <div style={{ width: 300, height: 220, border: "1px solid var(--ui-border)" }}>
      <ValuePanel title="note : text" text={text} json={false} isNull={false} readOnly={readOnly} note={note} error={null} focusSeq={0}
        onChange={() => {}} onToggleLayout={() => {}} onApply={() => {}} onBack={() => {}} onClose={() => {}} />
    </div>
  );
}
export const CutValues: StoryObj = {
  name: "Cut values (Long values)",
  render: () => (
    <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
      <Frame
        width={CUT_W}
        height={ROW * 3}
        header={
          <GridHeaderRow width={CUT_W - W[0]} height={26}>
            <GridHeaderCell name="id" type="int8" pk width={W[1]} />
            <GridHeaderCell name="doc" type="jsonb" width={DOC_W} />
            <GridHeaderCell name="note" type="text" width={NOTE_W} />
          </GridHeaderRow>
        }
      >
        <GridRow top={0} height={ROW} width={CUT_W}>
          <RowNumberCell width={W[0]}>1</RowNumberCell>
          <GridCell width={W[1]} kind="num" align="right">1</GridCell>
          <GridCell width={DOC_W} cut={cutHover}><JsonText {...jsonCell(CUT_JSON)} /></GridCell>
          <GridCell width={NOTE_W}>note 1</GridCell>
        </GridRow>
        <GridRow top={ROW} height={ROW} width={CUT_W} odd>
          <RowNumberCell width={W[0]}>2</RowNumberCell>
          <GridCell width={W[1]} kind="num" align="right">2</GridCell>
          <GridCell width={DOC_W}><JsonText {...jsonCell('{"id":2,"ok":true}')} /></GridCell>
          <GridCell width={NOTE_W} cut={cutHover} selected active>{CUT_TEXT}</GridCell>
        </GridRow>
        <GridRow top={ROW * 2} height={ROW} width={CUT_W}>
          <RowNumberCell width={W[0]}>3</RowNumberCell>
          <GridCell width={W[1]} kind="num" align="right">3</GridCell>
          <GridCell width={DOC_W} cut={cutHover}><JsonText {...jsonCell(CUT_JSON)} /></GridCell>
          <GridCell width={NOTE_W} cut={cutHover} align="right">short cut text</GridCell>
        </GridRow>
      </Frame>
      <div style={{ display: "flex", gap: 12, flexWrap: "wrap" }}>
        <CutPanel text={CUT_TEXT} readOnly note="Loading the whole value…" />
        <CutPanel text={CUT_TEXT + "long note the end"} readOnly={false} />
        <CutPanel text={CUT_TEXT} readOnly note="Cut at 4096 characters; the whole value needs a primary key (public.cut_log has none)" />
      </div>
    </div>
  ),
};

/** Hidden and pinned columns (the column header's menu, Columns…; settings.json "columnLayout" for a table tab). `id` and `user_name`
 *  are pinned: they stay at the left while the grid is scrolled right, and the line after `user_name` is where the others pass under.
 *  `email` is hidden: gone from the header and the rows, unchecked in Columns…, where Show All brings every column back */
const LAYOUT_COLS = [
  ["id", "int8"], ["user_name", "varchar"], ["email", "varchar"], ["amount", "numeric"], ["status", "varchar"], ["paid_at", "timestamptz"],
  ["city", "varchar"], ["memo", "text"], ["coupon", "varchar"], ["refunded", "bool"],
] as const;
const LW = 112;
function HiddenPinned() {
  const [hidden, setHidden] = useState<string[]>(["email"]);
  const [pinned, setPinned] = useState<string[]>(["id", "user_name"]);
  const names: string[] = LAYOUT_COLS.map(([n]) => n);
  const view = [...pinned, ...names.filter((n) => !pinned.includes(n))].filter((n) => !hidden.includes(n));
  const typeOf = (n: string) => LAYOUT_COLS[names.indexOf(n)][1];
  const width = W[0] + view.length * LW;
  const pin = (d: number) => (d < pinned.length ? { left: W[0] + d * LW, edge: d === pinned.length - 1 } : undefined);
  const body = useRef<HTMLDivElement>(null);
  // Scrolled right, as after a look at the far columns: the pinned ones are still at the left
  useEffect(() => void body.current?.scrollTo({ left: 260 }), []);
  const menuOn = "city";
  return (
    <div style={{ display: "flex", gap: 16, flexWrap: "wrap" }}>
      {/* transform: the menu and the popover (position: fixed) are placed in their own box, not the window */}
      <div style={{ position: "relative", width: 600, height: 420, transform: "translateZ(0)" }}>
        <div style={{ position: "relative", width: 600, height: 300, border: "1px solid var(--ui-border)" }}>
          <div className="grid-wrap">
            <GridHead body={body} corner={<GridHeaderRow width={W[0]} height={26}><RowNumberCell width={W[0]} title="Select all" /></GridHeaderRow>}>
              <GridHeaderRow width={width - W[0]} height={26}>
                {view.map((n, d) => (
                  <GridHeaderCell key={n} name={n} type={typeOf(n)} pk={n === "id"} width={LW} pin={pin(d)} colSelected={n === menuOn} onSortClick={() => {}} />
                ))}
              </GridHeaderRow>
            </GridHead>
            <div className="grid-scroll" ref={body} tabIndex={0}>
              <div style={{ width, height: ROW * 8, position: "relative" }}>
                {Array.from({ length: 8 }, (_, r) => (
                  <GridRow key={r} top={r * ROW} height={ROW} width={width} odd={r % 2 === 1}>
                    <RowNumberCell width={W[0]}>{r + 1}</RowNumberCell>
                    {view.map((n, d) => {
                      const num = n === "id" || n === "amount";
                      return (
                        <GridCell key={n} width={LW} pin={pin(d)} kind={num ? "num" : undefined} align={num ? "right" : undefined} selected={n === menuOn} active={r === 2 && n === "user_name"}>
                          {n === "id" ? r + 1 : n === "amount" ? `${(r + 1) * 1250}.00` : `${n}_${r + 1}`}
                        </GridCell>
                      );
                    })}
                  </GridRow>
                ))}
              </div>
            </div>
          </div>
        </div>
        <Menu
          at={{ x: 330, y: 40 }}
          onClose={() => {}}
          items={[
            { label: "Hide Column", action: () => setHidden((h) => [...h, menuOn]) },
            { label: "Pin Column", action: () => setPinned((p) => [...p, menuOn]) },
            { label: "Columns…", action: () => {} },
            { separator: true },
            { label: "Fit Column", shortcut: "double-click the edge", action: () => {} },
            { label: "Fit All Columns", action: () => {} },
            { label: "Reset Widths", shortcut: "computed widths", action: () => {}, disabled: true },
            { separator: true },
            { label: "Narrow Column", shortcut: "⌃⌥←", action: () => {} },
            { label: "Widen Column", shortcut: "⌃⌥→", action: () => {} },
          ]}
        />
      </div>
      <div style={{ position: "relative", width: 360, height: 420, transform: "translateZ(0)" }}>
        <CheckListPopover
          title="Columns…"
          items={LAYOUT_COLS.map(([n, t]) => ({ key: n, label: n, hint: t, mark: pinned.includes(n) ? "pinned" : undefined, checked: !hidden.includes(n) }))}
          onToggle={(k, on) => setHidden((h) => (on ? h.filter((x) => x !== k) : [...h, k]))}
          action={{ label: "Show All", onClick: () => setHidden([]), disabled: !hidden.length }}
          onClose={() => {}}
        />
      </div>
    </div>
  );
}
export const HiddenAndPinned: StoryObj = { name: "Hidden and pinned columns", render: () => <HiddenPinned /> };
