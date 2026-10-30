/** Statements that don't change data (used to count "already committed changes" in the mid-script failure notice) */
export const isReadStmt = (sql: string) => {
  const s2 = sql.replace(/--.*$/gm, "").replace(/\/\*[\s\S]*?\*\//g, "").trim().toLowerCase();
  return /^(select|with|show|explain|values|table|describe|desc)\b/.test(s2) && !/\b(insert|update|delete|merge)\b/.test(s2);
};

// Whether the first word after leading comments is a schema-changing statement (shown as "schema change" in the Production confirmation)
export const DDL_RE = /^(?:\s*(?:--[^\n]*\n|\/\*[\s\S]*?\*\/))*\s*(create|alter|drop|truncate|comment|grant|revoke|rename)\b/i;
