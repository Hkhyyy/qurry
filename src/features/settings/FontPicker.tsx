import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import type { FontFamily } from "../../lib/api";
import { DEFAULT_FONT_LABEL, fontOptions, fontStack } from "../../lib/prefs";
import { Checkbox, List, ListItem, SearchField } from "../../ui";
import { Chevron } from "../../ui/icons";

/** Popup height: the same whatever the search matches, so it doesn't jump while typing */
const POP_H = 300;
const same = (a: string, b: string) => a.trim().toLowerCase() === b.trim().toLowerCase();

/**
 * Settings › Editor / Grid font family, like DataGrip's font combo box: a drop-down of the installed families with a search field
 * and "Monospaced only" (DataGrip's "Show only monospaced fonts"; on by default). Typing on the closed field opens it with that text;
 * ↑↓ PgUp PgDn move, ↩ picks (applied live), ⎋ closes. The field shows the family in itself; a saved family that isn't installed
 * says so and renders with the default
 */
export function FontPicker({ label, value, installed, onPick }: { label: string; value: string; installed: FontFamily[] | null; onPick: (family: string) => void }) {
  const [query, setQuery] = useState<string | null>(null);
  const [monoOnly, setMonoOnly] = useState(true);
  const [active, setActive] = useState(0);
  const [at, setAt] = useState({ left: 0, top: 0, width: 0 });
  const btn = useRef<HTMLButtonElement>(null);
  const pop = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const open = query !== null;
  const rows = fontOptions(installed, value, monoOnly, query ?? "");
  const missing = fontOptions(installed, value, false, "").some((r) => r.missing);

  const show = (q: string) => {
    const r = btn.current!.getBoundingClientRect();
    const width = Math.max(r.width, 280);
    // Below the field, or above it when the window has no room
    setAt({ left: Math.min(r.left, window.innerWidth - width - 8), top: r.bottom + 2 + POP_H > window.innerHeight ? r.top - 2 - POP_H : r.bottom + 2, width });
    setQuery(q);
  };
  const close = (refocus: boolean) => {
    setQuery(null);
    if (refocus) btn.current?.focus();
  };
  const pick = (family: string) => {
    onPick(family);
    close(true);
  };

  // The current family is highlighted when the list opens or its rows change; the highlighted row stays in view
  useEffect(() => {
    if (open) setActive(Math.max(0, rows.findIndex((r) => same(r.family, value))));
  }, [open, query, monoOnly, installed]);
  useLayoutEffect(() => {
    const row = list.current?.children[active];
    if (!open || !row) return;
    // By hand: scrollIntoView would also scroll the form under the popup
    const l = list.current!.getBoundingClientRect();
    const r = row.getBoundingClientRect();
    if (r.top < l.top) list.current!.scrollTop -= l.top - r.top;
    else if (r.bottom > l.bottom) list.current!.scrollTop += r.bottom - l.bottom;
  }, [open, active]);
  // The popup is fixed under the field: scrolling the form would leave it behind, so it closes
  useEffect(() => {
    if (!open) return;
    const onScroll = (e: Event) => !pop.current?.contains(e.target as Node) && close(false);
    window.addEventListener("scroll", onScroll, true);
    return () => window.removeEventListener("scroll", onScroll, true);
  }, [open]);

  const onKey = (e: KeyboardEvent) => {
    const step = { ArrowDown: 1, ArrowUp: -1, PageDown: 10, PageUp: -10 }[e.key];
    if (step) setActive((i) => Math.max(0, Math.min(rows.length - 1, i + step)));
    else if (e.key === "Enter" && rows[active]) pick(rows[active].family);
    else if (e.key === "Escape") close(true);
    else return;
    // Handled here: not the Settings dialog's ⎋ / ↩
    e.preventDefault();
  };

  return (
    <>
      <button
        ref={btn}
        type="button"
        className="ui-select font-picker"
        role="combobox"
        aria-label={label}
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => (open ? close(true) : show(""))}
        onKeyDown={(e) => {
          if (["ArrowDown", "ArrowUp", "Enter", " "].includes(e.key)) show("");
          else if (e.key.length === 1 && !e.metaKey && !e.ctrlKey && !e.altKey) show(e.key);
          else return;
          e.preventDefault();
        }}
      >
        <span className="font-picker-name" style={{ fontFamily: fontStack(value) }}>
          {value.trim() || DEFAULT_FONT_LABEL}
        </span>
        {missing && <span className="font-picker-note">(not installed)</span>}
        <span className="font-picker-arrow">
          <Chevron open />
        </span>
      </button>
      {open && (
        <div
          ref={pop}
          className="font-picker-pop"
          style={{ ...at, height: POP_H }}
          onKeyDown={onKey}
          // Clicks on the rows and the checkbox leave the focus in the search field (so ↑↓ ↩ keep working); focus leaving closes the popup
          onMouseDownCapture={(e) => (e.target as HTMLElement).tagName !== "INPUT" && e.preventDefault()}
          onBlur={(e) => {
            const to = e.relatedTarget as Node | null;
            // Tab or a click elsewhere; a click on the field toggles it there
            if (!pop.current?.contains(to) && to !== btn.current) close(false);
          }}
        >
          <div className="font-picker-head">
            <SearchField className="grow" autoFocus aria-label={`Search ${label.toLowerCase()}`} placeholder="Search" value={query ?? ""} onChange={(e) => setQuery(e.target.value)} />
            <Checkbox checked={monoOnly} onChange={setMonoOnly}>
              Monospaced only
            </Checkbox>
          </div>
          <List listRef={list} className="font-picker-list" empty="No matching font">
            {rows.map((r, i) => (
              <ListItem
                key={r.family || "\0default"}
                title={r.family || DEFAULT_FONT_LABEL}
                hint={r.missing ? "(not installed)" : undefined}
                selected={i === active}
                onMouseEnter={() => setActive(i)}
                onClick={() => pick(r.family)}
              />
            ))}
          </List>
          {installed === null && <div className="font-picker-foot">Loading the installed fonts…</div>}
        </div>
      )}
    </>
  );
}
