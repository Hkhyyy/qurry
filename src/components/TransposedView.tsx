import type { Cell, QueryResult } from "../lib/api";
import { alignTable, CELL_KIND, cellClasses, useGridAlign } from "../lib/gridAlign";
import { useSharedReady } from "../lib/shared";
import { fracWidth, isTimeType, padFrac } from "../lib/timeFrac";
import { CutMark, cx } from "../ui";
import { cutLength, cutTitle, display } from "../features/grid/shared";

/** View as | Transpose: swap rows and columns */
export function TransposedView({ cols, rows, cutAt }: { cols: QueryResult["columns"]; rows: Cell[][]; cutAt: (d: number, c: number) => boolean }) {
  useSharedReady(); // time fractions and value classes come from the WASM module: render again once it has loaded
  const fracW = cols.map((c, ci) => (isTimeType(c.typeName) ? fracWidth(rows.map((r) => r[ci])) : 0));
  const overrides = useGridAlign();
  return (
    <div className="grid-scroll transposed">
      <table>
        <tbody>
          {cols.map((c, ci) => {
            const classOf = cellClasses(c.typeName);
            const aligns = alignTable(overrides, c.typeName, rows, ci);
            return (
              <tr key={ci}>
                <th>
                  <span className="col-name">{c.name}</span> <span className="col-type">{c.typeName.toLowerCase()}</span>
                </th>
                {rows.map((r, ri) => {
                  const cls = classOf(r[ci]);
                  const isCut = cutAt(ri, ci);
                  return (
                    <td key={ri} className={cx(CELL_KIND[cls], aligns[cls] && `al-${aligns[cls]}`)} title={isCut ? cutTitle(cutLength(r[ci])) : undefined}>
                      {fracW[ci] && typeof r[ci] === "string" ? padFrac(r[ci] as string, fracW[ci]) : display(r[ci])}
                      {isCut && <CutMark />}
                    </td>
                  );
                })}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
