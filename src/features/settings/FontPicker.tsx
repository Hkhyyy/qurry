import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import type { CustomFont, FontFamily } from "../../lib/api";
import { ask, notice } from "../../lib/dialogs";
import { addCustomFont, DEFAULT_FONT_LABEL, fontOptions, fontStack, removeCustomFont, useCustomFonts } from "../../lib/prefs";
import { Button, Checkbox, IconButton, ListItem, listKey, PopupList, SearchField } from "../../ui";
import { Chevron, IcMinus } from "../../ui/icons";

/** Popup height: the same whatever the search matches, so it doesn't jump while typing */
const POP_H = 300;
const same = (a: string, b: string) => a.trim().toLowerCase() === b.trim().toLowerCase();

/**
 * Settings › Editor / Grid font family, like DataGrip's font combo box: a drop-down of the installed families with a search field
 * and "Monospaced only" (DataGrip's "Show only monospaced fonts"; on by default). Typing on the closed field opens it with that text;
 * ↑↓ PgUp PgDn move, ↩ picks (applied live), ⎋ closes. The field shows the family in itself; a saved family that isn't installed
 * says so and renders with the default. Opening it calls onOpen (Settings lists the installed fonts again). The drop-down is the ui kit's PopupList (as ComboBox's), here with a search box on top
 * Not in DataGrip: "Add Font File…" under the list copies a font file into Qurry and picks it; added fonts are listed first, marked
 * "custom", each with a remove button (asks first; a picker set to it goes back to the default unless a font of that name is installed). A file that can't be
 * loaded as a font is removed again with a notice
 */
export function FontPicker({ label, value, installed, onOpen, onPick }: { label: string; value: string; installed: FontFamily[] | null; onOpen: () => void; onPick: (family: string) => void }) {
  const [query, setQuery] = useState<string | null>(null);
  const [monoOnly, setMonoOnly] = useState(true);
  const [active, setActive] = useState(0);
  const btn = useRef<HTMLButtonElement>(null);
  const custom = useCustomFonts();
  const open = query !== null;
  const rows = fontOptions(installed, custom, value, monoOnly, query ?? "");
  const missing = fontOptions(installed, custom, value, false, "").some((r) => r.missing);

  const close = (refocus: boolean) => {
    setQuery(null);
    if (refocus) btn.current?.focus();
  };
  const pick = (family: string) => {
    onPick(family);
    close(true);
  };
  // The popup closes first: the file dialog or the confirm box takes the focus, and it comes back to the field
  const add = async () => {
    close(true);
    try {
      const f = await addCustomFont();
      if (f) onPick(f.name);
    } catch (e) {
      await notice(String(e), "Add Font File");
    }
    btn.current?.focus();
  };
  const remove = async (f: CustomFont) => {
    close(true);
    const ok = await ask(`Remove the font file "${f.name}" from Qurry? The copy in Qurry's fonts folder is deleted; the file you added it from is kept.`, { title: "Remove Font File", okLabel: "Remove", danger: true });
    if (ok) {
      try {
        await removeCustomFont(f, installed);
      } catch (e) {
        await notice(String(e), "Remove Font File");
      }
    }
    btn.current?.focus();
  };

  useEffect(() => {
    if (open) onOpen();
  }, [open]);

  // The current family is highlighted when the list opens or its rows change
  useEffect(() => {
    if (open) setActive(Math.max(0, rows.findIndex((r) => same(r.family, value))));
  }, [open, query, monoOnly, installed, custom]);

  const onKey = (e: KeyboardEvent) => {
    // ↩ on a button reached with ⇥ (Add Font File…, a remove button) presses it
    if (e.key === "Enter" && (e.target as HTMLElement).tagName === "BUTTON") return;
    const k = listKey(e.key, active, rows.length);
    if (k === null) return;
    if (k === "pick") pick(rows[active].family);
    else if (k === "close") close(true);
    else setActive(k);
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
        onClick={() => (open ? close(true) : setQuery(""))}
        onKeyDown={(e) => {
          if (["ArrowDown", "ArrowUp", "Enter", " "].includes(e.key)) setQuery("");
          else if (e.key.length === 1 && !e.metaKey && !e.ctrlKey && !e.altKey) setQuery(e.key);
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
        <PopupList
          anchor={btn}
          active={active}
          height={POP_H}
          minWidth={280}
          className="font-picker-pop"
          empty="No matching font"
          onClose={() => close(false)}
          onKeyDown={onKey}
          // Focus stays in the search box; Tab or a click elsewhere closes it (a click on the field toggles it there)
          onBlur={(e) => !e.currentTarget.contains(e.relatedTarget as Node) && e.relatedTarget !== btn.current && close(false)}
          head={
            <>
              <SearchField className="grow" autoFocus aria-label={`Search ${label.toLowerCase()}`} placeholder="Search" value={query ?? ""} onChange={(e) => setQuery(e.target.value)} />
              <Checkbox checked={monoOnly} onChange={setMonoOnly}>
                Monospaced only
              </Checkbox>
            </>
          }
          foot={
            <div className="font-picker-foot">
              <Button variant="link" onClick={add}>
                Add Font File…
              </Button>
              {installed === null && <span>Loading the installed fonts…</span>}
            </div>
          }
        >
          {rows.map((r, i) => {
            const f = r.custom && custom.find((c) => c.name === r.family);
            return (
              <ListItem
                key={r.family || "\0default"}
                title={r.family || DEFAULT_FONT_LABEL}
                hint={r.missing ? "(not installed)" : r.custom ? "custom" : undefined}
                trailing={
                  f && (
                    <IconButton size="sm" tone="danger" title={`Remove font file ${f.name}`} onClick={(e) => (e.stopPropagation(), remove(f))}>
                      <IcMinus />
                    </IconButton>
                  )
                }
                selected={i === active}
                onMouseEnter={() => setActive(i)}
                onClick={() => pick(r.family)}
              />
            );
          })}
        </PopupList>
      )}
    </>
  );
}
