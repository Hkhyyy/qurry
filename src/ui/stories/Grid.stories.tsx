import type { Meta, StoryObj } from "@storybook/react-vite";
import { ReactNode, useRef } from "react";
import { CellEditor, GridCell, GridHead, GridHeaderCell, GridHeaderRow, GridRow, RowNumberCell } from "..";

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
        <GridCell width={W[1]} kind="num">1</GridCell>
        <GridCell width={W[2]} selected>
          user_1
        </GridCell>
        <GridCell width={W[3]} kind="num">
          0.00
        </GridCell>
        <GridCell width={W[4]}>plain cell</GridCell>
      </GridRow>
      <GridRow top={ROW} height={ROW} width={560} odd>
        <RowNumberCell width={W[0]} selected>
          2
        </RowNumberCell>
        <GridCell width={W[1]} kind="num">2</GridCell>
        <GridCell width={W[2]} selected active>
          user_2 (cursor)
        </GridCell>
        <GridCell width={W[3]} kind="num" changed>
          1234.50
        </GridCell>
        <GridCell width={W[4]} kind="null">
          &lt;null&gt;
        </GridCell>
      </GridRow>
      <GridRow top={ROW * 2} height={ROW} width={560}>
        <RowNumberCell width={W[0]}>3</RowNumberCell>
        <GridCell width={W[1]} kind="num">3</GridCell>
        <GridCell width={W[2]} match>
          found (find)
        </GridCell>
        <GridCell width={W[3]} kind="num">2469.00</GridCell>
        <CellEditor width={W[4]} value="editing…" onChange={() => {}} onKeyDown={() => {}} onBlurOut={() => {}} more={{ title: "Choose date/time", onClick: () => {} }} />
      </GridRow>
      <GridRow top={ROW * 3} height={ROW} width={560} added>
        <RowNumberCell width={W[0]}>+</RowNumberCell>
        <GridCell width={W[1]} kind="null">&lt;default&gt;</GridCell>
        <GridCell width={W[2]}>new row (added)</GridCell>
        <GridCell width={W[3]} kind="num">10</GridCell>
        <GridCell width={W[4]}>—</GridCell>
      </GridRow>
      <GridRow top={ROW * 4} height={ROW} width={560} odd deleted>
        <RowNumberCell width={W[0]}>5</RowNumberCell>
        <GridCell width={W[1]} kind="num">5</GridCell>
        <GridCell width={W[2]}>deleted</GridCell>
        <GridCell width={W[3]} kind="num">4938.00</GridCell>
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
