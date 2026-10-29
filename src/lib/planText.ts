import type { PlanNode } from "../components/ConsolePane";

const n2 = (v: number) => (Number.isInteger(v) ? String(v) : v.toFixed(2));

/** Plan as indented text (for pasting to share): EXPLAIN text shape + shares · warnings */
export function planText(root: PlanNode): string {
  const out: string[] = [];
  const walk = (n: PlanNode, depth: number) => {
    const pad = depth ? "  ".repeat(depth - 1) + "->  " : "";
    const est = n.total !== undefined && n.total !== null ? ` (cost=${n2(n.startup ?? 0)}..${n2(n.total)}${n.rows !== undefined ? ` rows=${n.rows}` : ""})` : "";
    const act =
      n.actualTime !== undefined && n.actualTime !== null ? ` (actual time=${n.actualTime.toFixed(3)} ms${n.actualRows !== undefined ? ` rows=${n.actualRows}` : ""}${n.loops !== undefined ? ` loops=${n.loops}` : ""})` : "";
    out.push(`${pad}${n.op}${n.detail ? ` ${n.detail}` : ""}${est}${act}  [${n.pct.toFixed(0)}%]`);
    const sub = "  ".repeat(depth) + (depth ? "    " : "  ");
    for (const w of n.warnings) out.push(`${sub}⚠ ${w}`);
    n.children.forEach((c) => walk(c, depth + 1));
  };
  walk(root, 0);
  return out.join("\n");
}
