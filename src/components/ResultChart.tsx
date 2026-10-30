import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { QueryResult } from "../lib/api";
import { buildChart, ChartKind, COUNT, defaultAxes, niceTicks, numericColumns, shortNumber } from "../lib/chart";
import { Select, Spacer, Text, ToolButton, Toolbar, ToolbarSep } from "../ui";

/** Series colors (visible on both dark · light themes) */
const COLORS = ["#4e9af1", "#e5a540", "#59b36b", "#d9605a", "#a77fd8", "#4fb6b0", "#d47fb0", "#9aa0a6", "#c9b458", "#6c8ebf"];
/** Number of X values drawn in bar · line charts */
const MAX_POINTS = 500;
const M = { top: 12, right: 16, bottom: 34, left: 56 };

/**
 * Result table as a chart (View as › Chart): line · bar · pie. One X, several Y (numeric columns or row count).
 * Equal X values are merged. Only loaded rows are used (redrawn when more are loaded)
 */
export function ResultChart({ result }: { result: QueryResult }) {
  const nums = useMemo(() => numericColumns(result), [result]);
  const colKey = result.columns.map((c) => c.name).join("\u0001");
  const [kind, setKind] = useState<ChartKind>("bar");
  const [axes, setAxes] = useState(() => defaultAxes(result));
  // For a different result (columns changed), pick the axes again
  useEffect(() => setAxes(defaultAxes(result)), [colKey]);
  const ys = kind === "pie" ? axes.ys.slice(0, 1) : axes.ys;
  const data = useMemo(() => buildChart(result, axes.x, ys, kind), [result, axes, kind]);
  const shown = kind === "pie" ? data : { labels: data.labels.slice(0, MAX_POINTS), series: data.series.map((s) => ({ ...s, values: s.values.slice(0, MAX_POINTS) })) };

  const area = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 600, h: 300 });
  useLayoutEffect(() => {
    const el = area.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setSize({ w: el.clientWidth, h: el.clientHeight }));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  const [hover, setHover] = useState<{ i: number; x: number; y: number } | null>(null);

  const toggleY = (y: number) => {
    const has = axes.ys.includes(y);
    const next = has ? axes.ys.filter((v) => v !== y) : [...axes.ys, y];
    setAxes({ ...axes, ys: next.length ? next : [y] });
  };
  const yName = (y: number) => (y === COUNT ? "count" : result.columns[y].name);

  return (
    <div className="chart-wrap">
      <Toolbar>
        {(["bar", "line", "pie"] as ChartKind[]).map((k) => (
          <ToolButton key={k} active={kind === k} onClick={() => setKind(k)}>
            {k === "bar" ? "Bar" : k === "line" ? "Line" : "Pie"}
          </ToolButton>
        ))}
        <ToolbarSep />
        <Text tone="muted" size="sm">
          X
        </Text>
        <Select value={axes.x} onChange={(e) => setAxes({ ...axes, x: Number(e.target.value) })}>
          {result.columns.map((c, i) => (
            <option key={i} value={i}>
              {c.name}
            </option>
          ))}
        </Select>
        <ToolbarSep />
        <Text tone="muted" size="sm">
          {kind === "pie" ? "Value" : "Y"}
        </Text>
        {[...nums.filter((i) => i !== axes.x), COUNT].map((y) => (
          <ToolButton key={y} active={ys.includes(y)} title={y === COUNT ? "Number of rows for each X" : `Sum of ${yName(y)} for each X`} onClick={() => (kind === "pie" ? setAxes({ ...axes, ys: [y] }) : toggleY(y))}>
            {ys.includes(y) && kind !== "pie" && ys.length > 1 && <span className="chart-dot" style={{ background: COLORS[ys.indexOf(y) % COLORS.length] }} />}
            {yName(y)}
          </ToolButton>
        ))}
        <Spacer />
        <Text tone="muted" size="sm">
          {result.rows.length.toLocaleString("en-US")}
          {result.truncated ? "+ loaded" : ""} rows · {data.labels.length.toLocaleString("en-US")} {kind === "pie" ? "slices" : "points"}
          {kind !== "pie" && data.labels.length > MAX_POINTS ? ` (first ${MAX_POINTS})` : ""}
        </Text>
      </Toolbar>
      <div className="chart-area" ref={area} onMouseLeave={() => setHover(null)}>
        {!shown.labels.length ? (
          <div className="chart-empty">Nothing to draw — pick a number column for Y, or count rows.</div>
        ) : kind === "pie" ? (
          <Pie data={shown} w={size.w} h={size.h} hover={hover} setHover={setHover} />
        ) : (
          <Axes data={shown} kind={kind} w={size.w} h={size.h} hover={hover} setHover={setHover} />
        )}
        {hover && shown.labels[hover.i] !== undefined && (
          <div className="chart-tip" style={{ left: Math.min(hover.x + 12, size.w - 180), top: Math.max(hover.y - 10, 0) }}>
            <b>{shown.labels[hover.i]}</b>
            {shown.series.map((s, j) => (
              <div key={s.name}>
                <span className="chart-dot" style={{ background: COLORS[(kind === "pie" ? hover.i : j) % COLORS.length] }} />
                {s.name}: {s.values[hover.i].toLocaleString("en-US", { maximumFractionDigits: 4 })}
                {kind === "pie" && ` (${((s.values[hover.i] / s.values.reduce((a, b) => a + b, 0)) * 100).toFixed(1)}%)`}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

type Hover = { i: number; x: number; y: number } | null;
type ChartProps = { data: { labels: string[]; series: { name: string; values: number[] }[] }; w: number; h: number; hover: Hover; setHover: (h: Hover) => void };

/** Bar · line: ticks · X labels (skipped when overlapping) · hovered X highlight */
function Axes({ data, kind, w, h, hover, setHover }: ChartProps & { kind: ChartKind }) {
  const pw = Math.max(10, w - M.left - M.right);
  const ph = Math.max(10, h - M.top - M.bottom);
  const all = data.series.flatMap((s) => s.values);
  const ticks = niceTicks(Math.min(0, ...all), Math.max(0, ...all));
  const [lo, hi] = [ticks[0], ticks[ticks.length - 1]];
  const y = (v: number) => M.top + ph - ((v - lo) / (hi - lo || 1)) * ph;
  const n = data.labels.length;
  const band = pw / n;
  const cx = (i: number) => M.left + band * i + band / 2;
  const every = Math.max(1, Math.ceil((n * 70) / pw));
  const bars = data.series.length;
  const bw = Math.max(1, Math.min(48, (band * 0.8) / bars));
  const cut = (s: string) => (s.length > 12 ? `${s.slice(0, 11)}…` : s);
  return (
    <svg
      width={w}
      height={h}
      className="chart-svg"
      onMouseMove={(e) => {
        const r = (e.currentTarget as SVGSVGElement).getBoundingClientRect();
        const i = Math.floor((e.clientX - r.left - M.left) / band);
        setHover(i >= 0 && i < n ? { i, x: e.clientX - r.left, y: e.clientY - r.top } : null);
      }}
    >
      {ticks.map((t) => (
        <g key={t}>
          <line className={t === 0 ? "chart-zero" : "chart-grid"} x1={M.left} x2={M.left + pw} y1={y(t)} y2={y(t)} />
          <text className="chart-tick" x={M.left - 6} y={y(t)} textAnchor="end" dominantBaseline="middle">
            {shortNumber(t)}
          </text>
        </g>
      ))}
      {hover && <rect className="chart-hover" x={M.left + band * hover.i} y={M.top} width={band} height={ph} />}
      {data.labels.map((l, i) =>
        i % every === 0 ? (
          <text key={i} className="chart-tick" x={cx(i)} y={M.top + ph + 16} textAnchor="middle">
            {cut(l)}
          </text>
        ) : null,
      )}
      {kind === "bar"
        ? data.series.map((s, j) =>
            s.values.map((v, i) => {
              const x0 = cx(i) - (bw * bars) / 2 + bw * j;
              const [a, b] = [y(Math.max(0, v)), y(Math.min(0, v))];
              return <rect key={`${j}-${i}`} x={x0} y={a} width={Math.max(1, bw - 1)} height={Math.max(0, b - a)} fill={COLORS[j % COLORS.length]} />;
            }),
          )
        : data.series.map((s, j) => (
            <g key={j}>
              <polyline fill="none" stroke={COLORS[j % COLORS.length]} strokeWidth={2} points={s.values.map((v, i) => `${cx(i)},${y(v)}`).join(" ")} />
              {n <= 60 && s.values.map((v, i) => <circle key={i} cx={cx(i)} cy={y(v)} r={hover?.i === i ? 4 : 2.5} fill={COLORS[j % COLORS.length]} />)}
            </g>
          ))}
    </svg>
  );
}

/** Pie: slices + legend on the right (name · share) */
function Pie({ data, w, h, hover, setHover }: ChartProps) {
  const vals = data.series[0]?.values ?? [];
  const total = vals.reduce((a, b) => a + b, 0) || 1;
  const legendW = Math.min(220, w * 0.4);
  const r = Math.max(10, Math.min(w - legendW - 40, h - 24) / 2);
  const [cx, cy] = [20 + r, h / 2];
  let a0 = -Math.PI / 2;
  const slices = vals.map((v, i) => {
    const a1 = a0 + (v / total) * Math.PI * 2;
    const large = a1 - a0 > Math.PI ? 1 : 0;
    const p = (a: number) => `${cx + r * Math.cos(a)},${cy + r * Math.sin(a)}`;
    const d = vals.length === 1 ? `M ${cx - r},${cy} a ${r},${r} 0 1,0 ${2 * r},0 a ${r},${r} 0 1,0 ${-2 * r},0` : `M ${cx},${cy} L ${p(a0)} A ${r},${r} 0 ${large} 1 ${p(a1)} Z`;
    a0 = a1;
    return <path key={i} d={d} fill={COLORS[i % COLORS.length]} className={hover?.i === i ? "chart-slice on" : "chart-slice"} onMouseMove={(e) => {
      const b = (e.currentTarget.ownerSVGElement as SVGSVGElement).getBoundingClientRect();
      setHover({ i, x: e.clientX - b.left, y: e.clientY - b.top });
    }} />;
  });
  return (
    <svg width={w} height={h} className="chart-svg">
      {slices}
      {data.labels.map((l, i) => (
        <g key={i} transform={`translate(${cx + r + 28}, ${Math.max(14, cy - (data.labels.length * 18) / 2) + i * 18})`}>
          <rect width={10} height={10} y={-9} fill={COLORS[i % COLORS.length]} />
          <text className="chart-legend" x={16}>
            {l.length > 22 ? `${l.slice(0, 21)}…` : l} · {((vals[i] / total) * 100).toFixed(1)}%
          </text>
        </g>
      ))}
    </svg>
  );
}
