import type { Meta, StoryObj } from "@storybook/react-vite";
import { ReactNode, useRef } from "react";
import { CellEditor, GridCell, GridHead, GridHeaderCell, GridHeaderRow, GridRow, RowNumberCell } from "..";
import type { Cell } from "../../lib/api";
import { alignTable, CELL_KIND, cellClasses } from "../../lib/gridAlign";
import type { Alignments } from "../../lib/shared";

/** Result grid cells (pieces used by ResultGrid): header strip · header row · header cell · row · row number · data cell · edit cell */
const meta: Meta = { title: "GUI/4 Grid" };
export default meta;

const W = [40, 90, 120, 110, 180];
const ROW = 24;

/**
 * Layout used by ResultGrid: the header strip (GridHead) sits above the scroller, not inside it, and only follows its
 * scrollLeft, so the header never leaves the top edge whatever the scroll timing
 */
function Frame({ header, width, height, children }: { header: ReactNode; width: number; height: number; children: ReactNode }) {
  const body = useRef<HTMLDivElement>(null);
  return (
    <div style={{ position: "relative", width: 600, height: 300, border: "1px solid var(--ui-border)" }}>
      <div className="grid-wrap">
        <GridHead body={body}>{header}</GridHead>
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
        <GridHeaderRow width={560} height={26}>
          <RowNumberCell width={W[0]} title="Select all" />
          <GridHeaderCell name="id" type="int8" pk width={W[1]} sort={{ dir: "asc" }} onSortClick={() => {}} />
          <GridHeaderCell name="user_name" type="varchar" width={W[2]} colSelected onSortClick={() => {}} />
          <GridHeaderCell name="amount" type="numeric" width={W[3]} sort={{ dir: "desc", order: 2 }} onSortClick={() => {}} />
          <GridHeaderCell name="memo" type="text" width={W[4]} onSortClick={() => {}} />
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
        <GridHeaderRow width={WIDE} height={26}>
          <RowNumberCell width={W[0]} title="Select all" />
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

/** Settings › Appearance › Alignment: one column per value class, as ResultGrid classifies and aligns them */
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
  const al = alignTable(overrides);
  return (
    <Frame
      width={AW}
      height={ROW * 3}
      header={
        <GridHeaderRow width={AW} height={26}>
          <RowNumberCell width={W[0]} />
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
              <GridCell key={c.name} width={CW} kind={CELL_KIND[cls]} align={al[cls]}>
                {v === null ? "<null>" : String(v)}
              </GridCell>
            );
          })}
        </GridRow>
      ))}
    </Frame>
  );
}

/** By default numbers and NULL sit on the right (a NULL stays in line with the numbers above it), everything else on the left */
export const Alignment: StoryObj = { render: () => <Aligned overrides={{}} /> };
/** settings.json "gridAlign": { "null": "left", "number": "center" }: only the changed classes move */
export const AlignmentChanged: StoryObj = { name: "Alignment (changed)", render: () => <Aligned overrides={{ null: "left", number: "center" }} /> };
