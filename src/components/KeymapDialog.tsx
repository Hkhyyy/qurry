import { useState } from "react";
import { KEYMAP, KeyRow, rowDesc, toOther } from "../lib/keymap";
import { isCustom, shortcutLabel } from "../lib/shortcuts";
import { Button, EmptyState, Kbd, Modal, SearchField, SectionTitle } from "../ui";

const isMac = navigator.platform.toLowerCase().includes("mac");

/** Key shown on the row: current key for rebindable commands, fixed label for the rest */
const keysOf = (r: KeyRow) => (r[0] === null ? shortcutLabel(r[1]) || "—" : isMac ? r[0] : toOther(r[0]));

/** Help › Keymap Reference: searchable shortcut list */
export function KeymapDialog({ onClose, onEdit }: { onClose: () => void; onEdit: () => void }) {
  const [q, setQ] = useState("");
  const f = q.trim().toLowerCase();
  const sections = KEYMAP.map((s) => ({
    ...s,
    rows: s.rows.filter((r) => !f || rowDesc(r).toLowerCase().includes(f) || keysOf(r).toLowerCase().includes(f)),
  })).filter((s) => s.rows.length);
  return (
    <Modal
      title="Keymap Reference"
      size="lg"
      className="keymap"
      fixed
      bodyClassName="fill"
      onClose={onClose}
      footer={
        <>
          <span className="ui-modal-foot-hint">Also: Find Action ({shortcutLabel("find.action")}) lists every command with its shortcut.</span>
          <Button onClick={onEdit}>Edit Keymap…</Button>
          <Button variant="primary" onClick={onClose}>
            Close
          </Button>
        </>
      }
    >
      <SearchField className="keymap-search" autoFocus placeholder="Search actions or shortcuts" value={q} onChange={(e) => setQ(e.target.value)} />
      <div className="keymap-body">
        {sections.map((s) => (
          <div key={s.title} className="keymap-section">
            <SectionTitle className="keymap-title">{s.title}</SectionTitle>
            {s.rows.map((r) => (
              <div key={s.title + r[0] + r[1]} className={"keymap-row" + (r[0] === null && isCustom(r[1]) ? " custom" : "")}>
                <Kbd className="keymap-keys">{keysOf(r)}</Kbd>
                <span className="keymap-desc">{rowDesc(r)}</span>
              </div>
            ))}
          </div>
        ))}
        {!sections.length && <EmptyState>Nothing found</EmptyState>}
      </div>
    </Modal>
  );
}
