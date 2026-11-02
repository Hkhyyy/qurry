import { sharedOr } from "./shared";

/** The start of qurry_shared::errors::COMMIT_UNKNOWN_ERR, for when the WASM module isn't there (the test checks it against the Rust text) */
export const COMMIT_UNKNOWN_MARK = "connection lost while committing";

/**
 * Title of the notice for a failed Commit, decided in Rust like the TUI's (qurry_shared::errors::commit_failed_title). Without the
 * WASM module the same title for a commit cut off by a lost connection (it may have gone through: "failed" would tell the user to
 * run the changes again), else one that claims neither outcome
 */
export const commitNoticeTitle = (msg: string) =>
  sharedOr(msg.includes(COMMIT_UNKNOWN_MARK) ? "Commit outcome unknown" : "Commit did not complete", (s) => s.commitFailedTitle(msg));
