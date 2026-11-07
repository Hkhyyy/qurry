// The Redis Terminal's replies shown by type (the shape comes from qurry_shared::redis_cmd::reply_shape through WASM, as in the TUI)
import type { RedisReply } from "./api";
import type { RedisReplyShape } from "./shared";

/** A reply as a table: a hash as field / value, a sorted set with scores as member / score, set members and list elements as rows */
export interface ReplyTable {
  heads: string[];
  rows: string[][];
}

const text = (v: RedisReply) => (v === null ? "(nil)" : typeof v === "string" ? v : typeof v === "object" && !Array.isArray(v) ? v.status : String(v));

/** The table for a reply of this shape, or null to print it as redis-cli does (not an array, nested arrays, an odd pair list) */
export function replyTable(shape: RedisReplyShape, reply: RedisReply): ReplyTable | null {
  if (!Array.isArray(reply) || !reply.length || reply.some(Array.isArray)) return null;
  const xs = reply.map(text);
  const pairs = (heads: string[]) => (xs.length % 2 ? null : { heads, rows: xs.flatMap((x, i) => (i % 2 ? [] : [[x, xs[i + 1]]])) });
  switch (shape) {
    case "pairs":
      return pairs(["field", "value"]);
    case "scored":
      return pairs(["member", "score"]);
    case "members":
      return { heads: ["member"], rows: xs.map((x) => [x]) };
    case "list":
      return { heads: ["index", "value"], rows: xs.map((x, i) => [String(i), x]) };
    default:
      return null;
  }
}
