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

/** The mark a line with several shows (and a click on it opens): failed, then running, then done. App.css orders its rules the same way */
export const shownMark = (ms: StmtMark[]) => ["error", "running", "ok"].map((st) => ms.find((m) => m.state === st)).find(Boolean);

const escapeMd = (s: string) => s.replace(/[\\`*_{}[\]()#+\-.!|<>]/g, "\\$&");
/** A mark's hover (Markdown): when it ran, and its result (rows, duration) or its error */
export const markHover = (m: StmtMark) => (m.state === "running" ? `Running since ${m.ts}` : `${m.state === "ok" ? "Executed" : "Failed"} ${m.ts}  \n${escapeMd(m.text ?? "")}`);
