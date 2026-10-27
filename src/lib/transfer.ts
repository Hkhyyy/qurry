/** 데이터 내보내기·가져오기 (형식, 열 짝짓기, 진행 표시) */
export type DataFormat = "csv" | "tsv" | "json" | "sql" | "markdown";

export const FORMATS: { id: DataFormat; label: string; hint: string }[] = [
  { id: "csv", label: "CSV", hint: "Excel, spreadsheets" },
  { id: "tsv", label: "TSV", hint: "tab separated — pastes into spreadsheets as cells" },
  { id: "json", label: "JSON", hint: "array of objects" },
  { id: "sql", label: "SQL INSERT", hint: "one INSERT statement per row" },
  { id: "markdown", label: "Markdown", hint: "table for docs, issues, PRs" },
];

const norm = (s: string) => s.toLowerCase().replace(/[\s_-]/g, "");

/** 파일 열마다 넣을 컬럼 ("" = 건너뜀). table = null 이면 새 테이블 (열 이름 그대로) */
export function autoMap(file: string[], table: string[] | null): string[] {
  if (!table) return [...file];
  return file.map((f) => table.find((t) => norm(t) === norm(f)) ?? "");
}

/** 진행 막대: 전체를 알면 % (추정치라 넘어도 99% 까지), 모르면 행 수만 */
export function progressText(p: { rows: number; done: number; total: number }): { pct: number | null; text: string } {
  // 백업·복원은 행 대신 바이트로 나아간다
  const rows = p.rows === 0 && p.done > 0 ? `${(p.done / 1048576).toFixed(1)} MB` : `${p.rows.toLocaleString("en-US")} rows`;
  if (!p.total) return { pct: null, text: rows };
  const pct = Math.min(99, Math.floor((p.done / p.total) * 100));
  return { pct, text: `${rows} · ${pct}%` };
}
