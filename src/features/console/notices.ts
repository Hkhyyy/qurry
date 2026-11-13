import type { Notice } from "../../lib/api";

/** A statement's server notices as Output lines, the same text as the TUI (qurry_core::notice::Notice::line): `NOTICE: …`;
 *  warnings in the warn style */
export function noticeLines(notices: Notice[] | undefined): { level: "info" | "warn"; text: string }[] {
  return (notices ?? []).map((n) => ({ level: n.level === "WARNING" ? "warn" : "info", text: n.level ? `${n.level}: ${n.message}` : n.message }));
}
