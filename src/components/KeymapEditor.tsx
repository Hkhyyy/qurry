import { useEffect, useRef, useState } from "react";
import { bind, bindableIn, bindings, eventSpec, formatSpec, isCustom, onKeymapChange, removeKey, resetAllShortcuts, resetShortcut, Scope, SCOPE_LABEL, SHORTCUTS } from "../lib/shortcuts";
import { Button, EmptyState, IconButton, SearchField, Spacer, Text } from "../ui";

/** Settings › Keymap: change command shortcuts. Pressing a key records it; the key is removed from overlapping commands in the same scope */
export function KeymapEditor() {
  const [, setVer] = useState(0);
  useEffect(() => onKeymapChange(() => setVer((n) => n + 1)), []);
  const [q, setQ] = useState("");
  const [rec, setRec] = useState<{ id: string; add: boolean } | null>(null);
  const [note, setNote] = useState<string | null>(null);
  // The button that started recording: it is disabled while recording, so focus goes back to it afterwards (Esc then closes Settings)
  const from = useRef<HTMLElement | null>(null);

  // While recording, intercept keys for the whole window (App's shortcuts pause while .keymap-recording is present)
  useEffect(() => {
    if (!rec) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape" && !e.shiftKey && !e.metaKey && !e.ctrlKey && !e.altKey) return setRec(null);
      const spec = eventSpec(e);
      if (!spec) return; // Only modifiers pressed
      const s = SHORTCUTS.find((x) => x.id === rec.id);
      if (!s) return setRec(null);
      if (!bindableIn(spec, s.scope))
        return setNote(`${formatSpec(spec)} can't be a command shortcut — add ⌘/Ctrl, ⌃ or ⌥, or use an F key${s.scope === "tree" || s.scope === "grid" ? " (here also Delete, Insert or Backspace)" : ""}`);
      const title = s.title;
      const taken = bind(rec.id, spec, rec.add);
      setNote(`${title}: ${formatSpec(spec)}${taken.length ? ` · removed from ${taken.map((t) => t.title).join(", ")}` : ""}`);
      setRec(null);
    };
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      from.current?.focus();
    };
  }, [rec]);

  const f = q.trim().toLowerCase();
  const scopes: Scope[] = ["global", "tree", "editor", "grid"];
  // A search matches a command's title, its keys, or the name of its section ("grid" lists the Data Grid's commands)
  const sections = scopes
    .map((scope) => ({ scope, rows: SHORTCUTS.filter((s) => s.scope === scope && (!f || SCOPE_LABEL[scope].toLowerCase().includes(f) || s.title.toLowerCase().includes(f) || bindings(s.id).some((k) => formatSpec(k).toLowerCase().includes(f) || k.toLowerCase().includes(f)))) }))
    .filter((x) => x.rows.length);
  return (
    <div className={"keymap-editor" + (rec ? " keymap-recording" : "")}>
      <SearchField className="keymap-search" placeholder="Search commands or shortcuts" value={q} onChange={(e) => setQ(e.target.value)} />
      <div className="keymap-editor-list">
        {!sections.length && <EmptyState>No command or shortcut matches “{q.trim()}”</EmptyState>}
        {sections.map(({ scope, rows }) => {
          return (
            <div key={scope} className="keymap-section">
              <div className="keymap-title">{SCOPE_LABEL[scope]}</div>
              {rows.map((s) => {
                const recording = rec?.id === s.id;
                return (
                  <div key={s.id} className={"keymap-edit-row" + (isCustom(s.id) ? " custom" : "")}>
                    <span className="keymap-desc">{s.title}</span>
                    <span className="keymap-chips">
                      {recording ? (
                        <span className="keymap-chip rec">Press keys… (Esc cancels)</span>
                      ) : bindings(s.id).length ? (
                        bindings(s.id).map((k) => (
                          <span key={k} className="keymap-chip">
                            {formatSpec(k)}
                            <IconButton size="sm" tone="danger" title="Remove this shortcut" onClick={() => removeKey(s.id, k)}>×</IconButton>
                          </span>
                        ))
                      ) : (
                        <Text tone="muted">none</Text>
                      )}
                    </span>
                    <Button size="sm" onClick={(e) => ((from.current = e.currentTarget), setRec({ id: s.id, add: false }))} disabled={!!rec}>Change</Button>
                    <Button size="sm" title="Add another shortcut" onClick={(e) => ((from.current = e.currentTarget), setRec({ id: s.id, add: true }))} disabled={!!rec}>+</Button>
                    <Button size="sm" title="Reset to default" onClick={() => resetShortcut(s.id)} disabled={!!rec || !isCustom(s.id)}>Reset</Button>
                  </div>
                );
              })}
            </div>
          );
        })}
      </div>
      <div className="keymap-editor-foot">
        <span className="hint">{note ?? "Saved to keymap.json in the config folder. Moving, typing, Enter, Esc and copy / paste keys are fixed (Keymap Reference lists them)."}</span>
        <Spacer />
        <Button size="sm" onClick={() => (resetAllShortcuts(), setNote("All shortcuts reset to defaults"))} disabled={!!rec}>Reset All</Button>
      </div>
    </div>
  );
}
