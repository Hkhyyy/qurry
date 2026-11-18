// The columns a user hid and pinned in a result grid. Table tabs keep them per table in settings.json "columnLayout" through the core
// (api.columnLayout / api.setColumnLayout), which the TUI reads and writes too. Which column shows where is the shared rule
// (qurry_shared::format::column_view), by the keys widths are kept under (widthKeys)
import type { ColumnLayout } from "./api";
import { sharedOr, type ColumnView } from "./shared";

/** The display order of a result with column `keys` under `layout`: pinned first, hidden left out. Until the module loads, every
 *  column in result order (nothing hidden or pinned) */
export const columnView = (keys: string[], layout: ColumnLayout): ColumnView =>
  sharedOr({ view: keys.map((_, i) => i), pinned: 0, hidden: 0 }, (s) => s.columnView(keys, layout));
