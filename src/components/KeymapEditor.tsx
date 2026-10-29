import { useEffect, useState } from "react";
import { bind, bindable, bindings, eventSpec, formatSpec, isCustom, onKeymapChange, removeKey, resetAllShortcuts, resetShortcut, Scope, SCOPE_LABEL, SHORTCUTS } from "../lib/shortcuts";
import { Button, IconButton, SearchField, Spacer, Text } from "../ui";

/** Settings › Keymap: change command shortcuts. Pressing a key records it; the key is removed from overlapping commands in the same scope */
export function KeymapEditor() {
  const [, setVer] = useState(0);
  useEffect(() => onKeymapChange(() => setVer((n) => n + 1)), []);
  const [q, setQ] = useState("");
  const [rec, setRec] = useState<{ id: string; add: boolean } | null>(null);
  const [note, setNote] = useState<string | null>(null);

  // While recording, intercept keys for the whole window (App's shortcuts pause while .keymap-recording is present)
  useEffect(() => {
    if (!rec) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape" && !e.shiftKey && !e.metaKey && !e.ctrlKey && !e.altKey) return setRec(null);
      const spec = eventSpec(e);
      if (!spec) return; // Only modifiers pressed
      if (!bindable(spec)) return setNote(`${formatSpec(spec)} can't be a command shortcut — add ⌘/Ctrl, ⌃ or ⌥, or use an F key`);
      const title = SHORTCUTS.find((s) => s.id === rec.id)?.title;
      const taken = bind(rec.id, spec, rec.add);
      setNote(`${title}: ${formatSpec(spec)}${taken.length ? ` · removed from ${taken.map((t) => t.title).join(", ")}` : ""}`);
      setRec(null);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [rec]);

  const f = q.trim().toLowerCase();
  const scopes: Scope[] = ["global", "tree", "editor"];
  return (
    <div className={"keymap-editor" + (rec ? " keymap-recording" : "")}>
      <SearchField className="keymap-search" placeholder="Search commands or shortcuts" value={q} onChange={(e) => setQ(e.target.value)} />
      <div className="keymap-editor-list">
        {scopes.map((scope) => {
          const rows = SHORTCUTS.filter((s) => s.scope === scope && (!f || s.title.toLowerCase().includes(f) || bindings(s.id).some((k) => formatSpec(k).toLowerCase().includes(f) || k.toLowerCase().includes(f))));
          if (!rows.length) return null;
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
                    <Button size="sm" onClick={() => setRec({ id: s.id, add: false })} disabled={!!rec}>Change</Button>
                    <Button size="sm" title="Add another shortcut" onClick={() => setRec({ id: s.id, add: true })} disabled={!!rec}>+</Button>
                    <Button size="sm" title="Reset to default" onClick={() => resetShortcut(s.id)} disabled={!!rec || !isCustom(s.id)}>Reset</Button>
                  </div>
                );
              })}
            </div>
          );
        })}
      </div>
      <div className="keymap-editor-foot">
        <span className="hint">{note ?? "Saved to keymap.json in the config folder. Arrow keys, grid and tree navigation keys are fixed."}</span>
        <Spacer />
        <Button size="sm" onClick={() => (resetAllShortcuts(), setNote("All shortcuts reset to defaults"))} disabled={!!rec}>Reset All</Button>
      </div>
    </div>
  );
}
