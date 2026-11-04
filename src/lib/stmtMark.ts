// Gutter marks of a console's executed statements (DataGrip): what each mark shows and says on hover

/** A statement's run as its gutter mark shows it: running, then done or failed. `out` is its line in Output, `text` the result or the error */
export interface StmtMark {
  state: "running" | "ok" | "error";
  out: string;
  ts: string;
  text?: string;
}
/** Sets the gutter mark of the i-th statement of a run */
export type MarkStmt = (i: number, m: StmtMark) => void;

const escapeMd = (s: string) => s.replace(/[\\`*_{}[\]()#+\-.!|<>]/g, "\\$&");
/** A mark's hover (Markdown): when it ran, and its result (rows, duration) or its error */
export const markHover = (m: StmtMark) => (m.state === "running" ? `Running since ${m.ts}` : `${m.state === "ok" ? "Executed" : "Failed"} ${m.ts}  \n${escapeMd(m.text ?? "")}`);
