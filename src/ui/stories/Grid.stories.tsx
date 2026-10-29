import type { Meta, StoryObj } from "@storybook/react-vite";
import { CellEditor, GridCell, GridHeaderCell, GridHeaderRow, GridRow, RowNumberCell } from "..";

/** Result grid cells (pieces used by ResultGrid): header row · header cell · row · row number · data cell · edit cell */
const meta: Meta = { title: "GUI/4 Grid" };
export default meta;

const W = [40, 90, 120, 110, 180];
const ROW = 24;

export const States: StoryObj = {
  render: () => (
    <div className="grid-scroll" style={{ position: "relative", width: 600, height: 230, border: "1px solid var(--ui-border)" }}>
      <div style={{ width: 560, height: 26 + ROW * 6, position: "relative" }}>
        <GridHeaderRow width={560} height={26}>
          <RowNumberCell width={W[0]} title="Select all" />
          <GridHeaderCell name="id" type="int8" pk width={W[1]} sort={{ dir: "asc" }} onSortClick={() => {}} />
          <GridHeaderCell name="user_name" type="varchar" width={W[2]} colSelected onSortClick={() => {}} />
          <GridHeaderCell name="amount" type="numeric" width={W[3]} sort={{ dir: "desc", order: 2 }} onSortClick={() => {}} />
          <GridHeaderCell name="memo" type="text" width={W[4]} onSortClick={() => {}} />
        </GridHeaderRow>
        <GridRow top={26} height={ROW} width={560}>
          <RowNumberCell width={W[0]}>1</RowNumberCell>
          <GridCell width={W[1]} kind="num">1</GridCell>
          <GridCell width={W[2]} selected>
            user_1
          </GridCell>
          <GridCell width={W[3]} kind="num">
            0.00
          </GridCell>
          <GridCell width={W[4]}>보통 칸</GridCell>
        </GridRow>
        <GridRow top={26 + ROW} height={ROW} width={560} odd>
          <RowNumberCell width={W[0]} selected>
            2
          </RowNumberCell>
          <GridCell width={W[1]} kind="num">2</GridCell>
          <GridCell width={W[2]} selected active>
            user_2 (커서)
          </GridCell>
          <GridCell width={W[3]} kind="num" changed>
            1234.50
          </GridCell>
          <GridCell width={W[4]} kind="null">
            &lt;null&gt;
          </GridCell>
        </GridRow>
        <GridRow top={26 + ROW * 2} height={ROW} width={560}>
          <RowNumberCell width={W[0]}>3</RowNumberCell>
          <GridCell width={W[1]} kind="num">3</GridCell>
          <GridCell width={W[2]} match>
            found (찾기)
          </GridCell>
          <GridCell width={W[3]} kind="num">2469.00</GridCell>
          <CellEditor width={W[4]} value="editing…" onChange={() => {}} onKeyDown={() => {}} onBlurOut={() => {}} more={{ title: "Choose date/time", onClick: () => {} }} />
        </GridRow>
        <GridRow top={26 + ROW * 3} height={ROW} width={560} added>
          <RowNumberCell width={W[0]}>+</RowNumberCell>
          <GridCell width={W[1]} kind="null">&lt;default&gt;</GridCell>
          <GridCell width={W[2]}>new row (추가)</GridCell>
          <GridCell width={W[3]} kind="num">10</GridCell>
          <GridCell width={W[4]}>—</GridCell>
        </GridRow>
        <GridRow top={26 + ROW * 4} height={ROW} width={560} odd deleted>
          <RowNumberCell width={W[0]}>5</RowNumberCell>
          <GridCell width={W[1]} kind="num">5</GridCell>
          <GridCell width={W[2]}>deleted (지움)</GridCell>
          <GridCell width={W[3]} kind="num">4938.00</GridCell>
          <GridCell width={W[4]}>—</GridCell>
        </GridRow>
      </div>
    </div>
  ),
};
