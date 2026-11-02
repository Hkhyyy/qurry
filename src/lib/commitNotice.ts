import { sharedOr } from "./shared";

/**
 * Title of the notice for a failed Commit, decided in Rust like the TUI's (qurry_shared::errors::commit_failed_title). Without the
 * WASM module a title that claims neither outcome: a commit cut off by a lost connection may have gone through, and "failed" would
 * tell the user to run the changes again (the message below it says which it was)
 */
export const commitNoticeTitle = (msg: string) => sharedOr("Commit did not complete", (s) => s.commitFailedTitle(msg));
