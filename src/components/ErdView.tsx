import { useEffect, useMemo, useRef, useState } from "react";
import { erd, ErdRelation, ErdTable, saveFile } from "../lib/api";
import { layout } from "../lib/erdLayout";

const ROW = 18;
const HEAD = 26;
const CHAR = 7.2;
const key = (s: string, n: string) => `${s}.${n}`;

/** ERD 탭 (DataGrip Diagrams › Show Diagram): 테이블·외래 키 그림. 테이블을 끌어 옮기고, 휠로 확대·축소, 빈 곳을 끌어 이동. 더블클릭 = 테이블 데이터 */
export function ErdView(p: { target: string; schemas: string[]; visible: boolean; onOpenTable: (schema: string, table: string) => void; onStatus: (s: string) => void }) {
  const [data, setData] = useState<{ tables: ErdTable[]; relations: ErdRelation[]; mermaid: string } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [moved, setMoved] = useState<Record<string, { x: number; y: number }>>({});
  // 왼쪽 여백: 같은 열 테이블 사이 선이 왼쪽으로 돌아 나간다
  const [view, setView] = useState({ x: 70, y: 20, k: 1 });
  const [hover, setHover] = useState<string | null>(null);
  const drag = useRef<{ kind: "pan" | "table"; key?: string; sx: number; sy: number; ox: number; oy: number } | null>(null);
  const svg = useRef<SVGSVGElement>(null);
  const [seen, setSeen] = useState(p.visible);
  useEffect(() => {
    if (p.visible) setSeen(true);
  }, [p.visible]);

  useEffect(() => {
    if (!seen) return;
    setData(null);
    erd(p.target, p.schemas).then(
      ([e, mermaid]) => {
        setData({ ...e, mermaid });
        setError(null);
        setMoved({});
      },
      (e) => setError(String(e)),
    );
  }, [p.target, p.schemas.join(","), seen]);

  // 테이블 크기와 자리
  const boxes = useMemo(() => {
    if (!data) return [];
    const main = (() => {
      const count = new Map<string, number>();
      data.tables.forEach((t) => count.set(t.schema, (count.get(t.schema) ?? 0) + 1));
      return [...count.entries()].sort((a, b) => b[1] - a[1])[0]?.[0];
    })();
    return data.tables.map((t) => {
      const title = t.schema === main ? t.name : `${t.schema}.${t.name}`;
      const longest = Math.max(title.length + 2, ...t.columns.map((c) => c.name.length + c.dataType.length + 4));
      return { key: key(t.schema, t.name), t, title, width: Math.min(360, Math.max(140, longest * CHAR + 24)), height: HEAD + t.columns.length * ROW + 6 };
    });
  }, [data]);
  const auto = useMemo(() => layout(boxes, (data?.relations ?? []).map((r) => ({ from: key(r.fromSchema, r.fromTable), to: key(r.toSchema, r.toTable) }))), [boxes, data]);
  const posOf = (k: string) => moved[k] ?? auto.get(k) ?? { x: 0, y: 0 };
  const box = (k: string) => boxes.find((b) => b.key === k);
  const fk = useMemo(() => {
    const s = new Set<string>();
    data?.relations.forEach((r) => r.fromColumns.forEach((c) => s.add(`${key(r.fromSchema, r.fromTable)}|${c}`)));
    return s;
  }, [data]);

  // 선: 가리키는 컬럼 줄 → 가리켜지는 테이블의 PK 줄 (가까운 옆으로)
  const edge = (r: ErdRelation) => {
    const a = box(key(r.fromSchema, r.fromTable));
    const b = box(key(r.toSchema, r.toTable));
    if (!a || !b) return null;
    const pa = posOf(a.key);
    const pb = posOf(b.key);
    const rowY = (bx: typeof a, col: string) => HEAD + Math.max(0, bx.t.columns.findIndex((c) => c.name === col)) * ROW + ROW / 2 + 2;
    const ya = pa.y + rowY(a, r.fromColumns[0]);
    const yb = pb.y + rowY(b, r.toColumns[0]);
    const self = a.key === b.key;
    // 위아래로 겹치면 (같은 열) 둘 다 왼쪽으로 돌아 나간다: 테이블을 가로지르지 않게
    const stacked = pa.x < pb.x + b.width && pb.x < pa.x + a.width;
    if (self || stacked) {
      const left = self ? pa.x + a.width : Math.min(pa.x, pb.x);
      const out = self ? 36 : -36 - Math.abs(ya - yb) * 0.05;
      const xa = self ? pa.x + a.width : pa.x;
      const xb = self ? pa.x + a.width : pb.x;
      return { d: `M ${xa} ${ya} C ${left + out} ${ya}, ${left + out} ${yb}, ${xb} ${yb}` };
    }
    const right = pa.x + a.width / 2 < pb.x + b.width / 2;
    const xa = right ? pa.x + a.width : pa.x;
    const xb = right ? pb.x : pb.x + b.width;
    const dx = Math.max(30, Math.abs(xb - xa) / 2);
    return { d: `M ${xa} ${ya} C ${xa + (right ? dx : -dx)} ${ya}, ${xb + (right ? -dx : dx)} ${yb}, ${xb} ${yb}` };
  };

  const onDown = (e: React.MouseEvent, k?: string) => {
    e.stopPropagation();
    const o = k ? posOf(k) : { x: view.x, y: view.y };
    drag.current = { kind: k ? "table" : "pan", key: k, sx: e.clientX, sy: e.clientY, ox: o.x, oy: o.y };
  };
  const onMove = (e: React.MouseEvent) => {
    const d = drag.current;
    if (!d) return;
    const dx = e.clientX - d.sx;
    const dy = e.clientY - d.sy;
    if (d.kind === "pan") setView((v) => ({ ...v, x: d.ox + dx, y: d.oy + dy }));
    else setMoved((m) => ({ ...m, [d.key!]: { x: d.ox + dx / view.k, y: d.oy + dy / view.k } }));
  };
  const onWheel = (e: React.WheelEvent) => {
    const r = svg.current?.getBoundingClientRect();
    if (!r) return;
    const k = Math.min(2.5, Math.max(0.2, view.k * (e.deltaY < 0 ? 1.1 : 1 / 1.1)));
    const mx = e.clientX - r.left;
    const my = e.clientY - r.top;
    setView((v) => ({ k, x: mx - ((mx - v.x) * k) / v.k, y: my - ((my - v.y) * k) / v.k }));
  };
  const fit = () => {
    const r = svg.current?.getBoundingClientRect();
    if (!r || !boxes.length) return;
    const xs = boxes.map((b) => posOf(b.key).x + b.width);
    const ys = boxes.map((b) => posOf(b.key).y + b.height);
    const k = Math.min(1.5, (r.width - 110) / Math.max(...xs), (r.height - 40) / Math.max(...ys));
    setView({ x: 70, y: 20, k: Math.max(0.2, k) });
  };
  const svgText = () => {
    const el = svg.current?.cloneNode(true) as SVGSVGElement | undefined;
    if (!el) return "";
    el.setAttribute("xmlns", "http://www.w3.org/2000/svg");
    el.querySelector("g")?.setAttribute("transform", "translate(70 20)");
    const xs = boxes.map((b) => posOf(b.key).x + b.width);
    const ys = boxes.map((b) => posOf(b.key).y + b.height);
    el.setAttribute("width", String(Math.max(...xs) + 90));
    el.setAttribute("height", String(Math.max(...ys) + 40));
    // 파일만 열어도 보이게 (앱의 CSS 없이): 밝은 색으로
    const style = document.createElementNS("http://www.w3.org/2000/svg", "style");
    style.textContent =
      "svg{background:#fff}.erd-box{fill:#fff;stroke:#9aa0a6}.erd-head{fill:#e8eefc;stroke:#9aa0a6}.erd-title{font:600 13px -apple-system,sans-serif;fill:#1f2328}" +
      ".erd-col{font:12px Menlo,monospace;fill:#1f2328}.erd-col.pk{font-weight:700}.erd-type{font:11px Menlo,monospace;fill:#6e7781}.erd-edge{fill:none;stroke:#8c959f;stroke-width:1.4}";
    el.insertBefore(style, el.firstChild);
    return el.outerHTML;
  };

  if (error) return <div className="test-err dba-err">{error}</div>;
  if (!data) return <div className="muted pad">Loading the diagram…</div>;
  const hi = (r: ErdRelation) => hover !== null && (hover === key(r.fromSchema, r.fromTable) || hover === key(r.toSchema, r.toTable));
  return (
    <div className="erd">
      <div className="dba-bar erd-bar">
        <span className="muted dba-help">
          {p.schemas.join(", ")} — {data.tables.length} tables, {data.relations.length} foreign keys · drag tables, wheel to zoom, drag the background to pan, double-click a table to open it
        </span>
        <span className="grow" />
        <button onClick={fit}>Fit</button>
        <button onClick={() => setMoved({})}>Re-layout</button>
        <button onClick={() => navigator.clipboard.writeText(data.mermaid).then(() => p.onStatus("Mermaid diagram copied"))}>Copy as Mermaid</button>
        <button onClick={() => saveFile(`${p.schemas.join("_")}.svg`, svgText(), "SVG image", "svg").then((f) => f && p.onStatus(`Saved ${f}`))}>Save SVG…</button>
      </div>
      <svg ref={svg} className="erd-canvas" onMouseDown={(e) => onDown(e)} onMouseMove={onMove} onMouseUp={() => (drag.current = null)} onMouseLeave={() => (drag.current = null)} onWheel={onWheel}>
        <defs>
          <marker id="erd-one" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="9" markerHeight="9" markerUnits="userSpaceOnUse" orient="auto-start-reverse">
            <path d="M 0 0 L 10 5 L 0 10 z" fill="var(--muted)" />
          </marker>
        </defs>
        <g transform={`translate(${view.x} ${view.y}) scale(${view.k})`}>
          {data.relations.map((r, i) => {
            const e = edge(r);
            return e ? <path key={i} d={e.d} className={"erd-edge" + (hi(r) ? " on" : "")} markerEnd="url(#erd-one)"><title>{`${r.fromTable}(${r.fromColumns.join(", ")}) → ${r.toTable}(${r.toColumns.join(", ")})`}</title></path> : null;
          })}
          {boxes.map((b) => {
            const { x, y } = posOf(b.key);
            return (
              <g key={b.key} transform={`translate(${x} ${y})`} className="erd-table" onMouseDown={(e) => onDown(e, b.key)} onMouseEnter={() => setHover(b.key)} onMouseLeave={() => setHover(null)} onDoubleClick={() => p.onOpenTable(b.t.schema, b.t.name)}>
                <rect width={b.width} height={b.height} rx={5} className="erd-box" />
                <rect width={b.width} height={HEAD} rx={5} className="erd-head" />
                <text x={10} y={17} className="erd-title">{b.title}</text>
                {b.t.columns.map((c, i) => (
                  <g key={c.name} transform={`translate(0 ${HEAD + i * ROW + 2})`}>
                    <text x={10} y={13} className={"erd-col" + (c.pk ? " pk" : "")}>
                      {c.pk ? "🔑 " : fk.has(`${b.key}|${c.name}`) ? "↗ " : ""}
                      {c.name}
                    </text>
                    <text x={b.width - 8} y={13} className="erd-type" textAnchor="end">
                      {c.dataType}
                      {c.nullable ? "" : " !"}
                    </text>
                  </g>
                ))}
              </g>
            );
          })}
        </g>
      </svg>
    </div>
  );
}
