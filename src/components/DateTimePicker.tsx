import { useState } from "react";
import { Select, TextInput } from "../ui";

/** DataGrip-style date · time cell editor popup: day / month / year, hour : minute : second */
const RE = /^(\d{4})-(\d{2})-(\d{2})(?:[ T](\d{2}):(\d{2}):(\d{2})(\.\d+)?)?(.*)$/;
const RE_TIME = /^(\d{2}):(\d{2}):(\d{2})(\.\d+)?(.*)$/;
const pad = (n: number, w = 2) => String(n).padStart(w, "0");
const daysIn = (y: number, m: number) => new Date(y, m, 0).getDate();

export const isTemporal = (typeName: string) => /^(TIMESTAMP|TIMESTAMPTZ|DATE|TIME|TIMETZ|DATETIME)$/.test(typeName.toUpperCase());

interface Parts {
  y: number; mo: number; d: number; h: number; mi: number; s: number; frac: string; rest: string;
}

function parse(value: string, typeName: string): Parts {
  const now = new Date();
  const base: Parts = { y: now.getFullYear(), mo: now.getMonth() + 1, d: now.getDate(), h: 0, mi: 0, s: 0, frac: "", rest: "" };
  const t = typeName.toUpperCase();
  if (t.startsWith("TIME") && !t.startsWith("TIMESTAMP")) {
    const m = RE_TIME.exec(value.trim());
    return m ? { ...base, h: +m[1], mi: +m[2], s: +m[3], frac: m[4] ?? "", rest: m[5] ?? "" } : base;
  }
  const m = RE.exec(value.trim());
  if (!m) return base;
  return { y: +m[1], mo: +m[2], d: +m[3], h: +(m[4] ?? 0), mi: +(m[5] ?? 0), s: +(m[6] ?? 0), frac: m[7] ?? "", rest: m[8] ?? "" };
}

function format(p: Parts, typeName: string): string {
  const t = typeName.toUpperCase();
  const date = `${pad(p.y, 4)}-${pad(p.mo)}-${pad(p.d)}`;
  const time = `${pad(p.h)}:${pad(p.mi)}:${pad(p.s)}${p.frac}`;
  if (t === "DATE") return date + p.rest;
  if (t.startsWith("TIME") && !t.startsWith("TIMESTAMP")) return time + p.rest;
  return `${date} ${time}${p.rest}`;
}

export function DateTimePicker({ value, typeName, onChange }: { value: string; typeName: string; onChange: (v: string) => void }) {
  const [p, setP] = useState<Parts>(() => parse(value, typeName));
  const t = typeName.toUpperCase();
  const showDate = !(t.startsWith("TIME") && !t.startsWith("TIMESTAMP"));
  const showTime = t !== "DATE";
  const months = Array.from({ length: 12 }, (_, i) => new Date(2000, i, 1).toLocaleString(navigator.language, { month: "long" }));

  const update = (patch: Partial<Parts>) => {
    const n = { ...p, ...patch };
    n.d = Math.min(n.d, daysIn(n.y, n.mo));
    setP(n);
    onChange(format(n, typeName));
  };
  const num = (v: string, min: number, max: number) => Math.min(max, Math.max(min, Number(v) || 0));

  return (
    <div className="dt-picker" onMouseDown={(e) => e.stopPropagation()}>
      {showDate && (
        <div className="dt-row">
          <Select value={p.d} onChange={(e) => update({ d: +e.target.value })} autoFocus>
            {Array.from({ length: daysIn(p.y, p.mo) }, (_, i) => i + 1).map((d) => (
              <option key={d} value={d}>{d}</option>
            ))}
          </Select>
          <Select value={p.mo} onChange={(e) => update({ mo: +e.target.value })}>
            {months.map((m, i) => (
              <option key={i} value={i + 1}>{m}</option>
            ))}
          </Select>
          <TextInput type="number" className="dt-year" value={p.y} min={1} max={9999} onChange={(e) => update({ y: num(e.target.value, 1, 9999) })} />
        </div>
      )}
      {showTime && (
        <div className="dt-row">
          <TextInput type="number" value={p.h} min={0} max={23} onChange={(e) => update({ h: num(e.target.value, 0, 23) })} />
          <span>:</span>
          <TextInput type="number" value={p.mi} min={0} max={59} onChange={(e) => update({ mi: num(e.target.value, 0, 59) })} />
          <span>:</span>
          <TextInput type="number" value={p.s} min={0} max={59} onChange={(e) => update({ s: num(e.target.value, 0, 59) })} />
        </div>
      )}
    </div>
  );
}
