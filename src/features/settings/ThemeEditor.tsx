import { useEffect, useMemo, useState } from "react";
import { api } from "../../lib/api";
import { ask } from "../../lib/dialogs";
import { sharedOr, useSharedReady } from "../../lib/shared";
import { BUILT_IN, SYSTEM, TOKENS, customThemeName, deleteCustomTheme, findTheme, previewTheme, saveCustomTheme, setThemePick, useThemeState, type CustomTheme, type Palette, type ThemeDef, type ThemeToken, type ThemeWarning } from "../../lib/theme";
import { Button, cx, Named, normalizeHex, Notice, Select, Text, TextInput } from "../../ui";
import { sameImages, type ThemeImages } from "../../lib/themeImages";
import { ThemeImagesSection } from "./ThemeImages";
import { ThemePreview } from "./ThemePreview";

// Settings › Appearance › Theme › Edit Colors…: a custom theme made from any theme (a built-in one, or a custom one to change).
// Basic colors first, the rest behind "Advanced colors"; each color has a picker and its hex. The whole window draws the colors being
// edited (previewTheme) and the preview, which gets most of the width, shows every state; a color's row is its name, swatch and hex,
// and pointing at the name says what it colors (above the preview) and outlines what draws it. Contrast below a
// minimum is warned about under the color's name and in the header, never fixed silently; a hex that isn't one is not applied. The
// theme's images (#202: Background and Corner) are set over the preview and drawn live in the window behind Settings (hold Peek)

/** Leaving the editor, or replacing its colors (Based on, Import…), with colors not saved as a theme asks first */
export const askDiscardColors = () => ask("The changed colors aren't saved as a theme. Discard them?", { title: "Theme Colors", okLabel: "Discard", danger: true });

const LABEL = Object.fromEntries(TOKENS.map((t) => [t.key, t.label]));
const GROUPS = [...new Set(TOKENS.map((t) => t.group))];

/** A color: swatch (the system picker) and its hex; a hex being typed applies once it is one, and is marked while it isn't */
function HexField({ label, value, onChange, onPick }: { label: string; value: string; onChange: (v: string) => void; onPick: () => void }) {
  const [draft, setDraft] = useState<string | null>(null);
  const bad = draft !== null && !normalizeHex(draft);
  return (
    <>
      <label className="ui-swatch clickable theme-swatch" style={{ background: value }} title="Pick a color" onMouseDown={onPick}>
        <input type="color" aria-label={`${label} color`} value={value} onChange={(e) => (setDraft(null), onChange(e.target.value))} />
      </label>
      <TextInput
        aria-label={`${label} hex`}
        className={cx("theme-hex", bad && "invalid")}
        value={draft ?? value}
        spellCheck={false}
        aria-invalid={bad}
        title={bad ? "Not a color (#rgb or #rrggbb): not applied" : undefined}
        onChange={(e) => {
          setDraft(e.target.value);
          const h = normalizeHex(e.target.value);
          if (h) onChange(h);
        }}
        onBlur={() => setDraft(null)}
      />
    </>
  );
}

function TokenRow({ t, value, edited, warning, onHot, onChange, onReset }: { t: ThemeToken; value: string; edited: boolean; warning?: ThemeWarning; onHot: (on: boolean) => void; onChange: (v: string) => void; onReset: () => void }) {
  const other = warning && (warning.fg === t.key ? warning.bg : warning.fg);
  return (
    <div className="theme-token" data-token={t.key}>
      <div className="theme-token-label" onMouseEnter={() => onHot(true)} onMouseLeave={() => onHot(false)}>
        <div className="theme-token-name">
          <span>{t.label}</span>
          {edited && <span className="theme-edited" title="Changed from the base theme" />}
        </div>
        {warning && (
          <div className="theme-warn" title={`${LABEL[warning.fg]} on ${LABEL[warning.bg]}: needs ${warning.min}:1. Pick a color further from it in lightness, or ↺ reset to the base theme's`}>
            ⚠ {warning.ratio.toFixed(1)}:1 on {LABEL[other!].split(",")[0]}
          </div>
        )}
      </div>
      <HexField label={t.label} value={value} onChange={onChange} onPick={() => onHot(false)} />
      <button className="ui-icon-btn ui-icon-btn-sm" title="Reset to base" aria-label={`Reset ${t.label}`} style={{ visibility: edited ? "visible" : "hidden" }} onClick={onReset}>
        ↺
      </button>
    </div>
  );
}

/** start: the theme to start from (the one in use); commit: set to what Settings' OK runs (saveIfDirty) */
export function ThemeEditor({ start, onBack, onDirty, commit }: { start: string; onBack: () => void; onDirty: (dirty: boolean) => void; commit?: { current: (() => Promise<boolean>) | null } }) {
  const { settings } = useThemeState();
  const startCustom = settings.custom.find((c) => c.id === start);
  const [baseId, setBaseId] = useState(startCustom?.base ?? (BUILT_IN.some((t) => t.id === start) ? start : "dark"));
  const [over, setOver] = useState<Palette>(startCustom?.colors ?? {});
  const [name, setName] = useState(startCustom?.name ?? "");
  const [images, setImages] = useState<ThemeImages>(startCustom?.images ?? {});
  const hasImages = !!(images.background || images.corner);
  /** The custom theme being changed (Save replaces it), or null for a new one */
  const [editing, setEditing] = useState<string | null>(startCustom?.id ?? null);
  const [advanced, setAdvanced] = useState(false);
  const [hot, setHot] = useState<string | null>(null);
  const [msg, setMsg] = useState<{ tone: "ok" | "danger" | "warn"; text: string } | null>(null);
  const base = BUILT_IN.find((t) => t.id === baseId)!;
  const saved = editing ? settings.custom.find((c) => c.id === editing) : undefined;
  const colors: Palette = { ...base.colors, ...over };
  const changed = Object.keys(over).filter((k) => over[k] !== base.colors[k]).sort();
  // Saved colors come back sorted by name (settings.json): compare in that order
  const savedColors = saved && Object.keys(saved.colors).sort().map((k) => [k, saved.colors[k]]);
  const dirty = saved
    ? saved.name !== name.trim() || saved.base !== baseId || JSON.stringify(savedColors) !== JSON.stringify(changed.map((k) => [k, over[k]])) || !sameImages(saved.images, images)
    : changed.length > 0 || hasImages;
  const ready = useSharedReady();
  const warnings = useMemo(() => sharedOr<ThemeWarning[]>([], (s) => s.themeWarnings(colors)), [JSON.stringify(colors), ready]);
  const warnOf = (k: string) => warnings.find((w) => w.token === k);
  const draft: ThemeDef = { id: editing ?? "draft", name: name || base.name, kind: base.kind, custom: true, colors, images };
  // The whole window draws the colors and images being edited; leaving the editor goes back to the theme picked
  const key = JSON.stringify(colors) + base.kind + JSON.stringify(images);
  useEffect(() => previewTheme(draft), [key]);
  useEffect(() => () => previewTheme(null), []);
  useEffect(() => void onDirty(dirty), [dirty]);
  const point = (k: string, on: boolean) => {
    setHot(on ? k : null);
  };

  const pickBase = async (id: string) => {
    if (dirty && !(await askDiscardColors())) return;
    const t = findTheme(settings, id);
    const c = settings.custom.find((x) => x.id === id);
    setBaseId(c ? c.base : id);
    setOver(c ? c.colors : {});
    setName(c ? c.name : "");
    setImages(c?.images ?? {});
    setEditing(c ? c.id : null);
    setMsg(t?.custom ? { tone: "ok", text: `Changing "${t.name}": Save replaces it` } : null);
  };
  const theme = (as = name): CustomTheme => ({ id: "", name: as.trim(), base: baseId, colors: Object.fromEntries(changed.map((k) => [k, over[k]])), ...(hasImages && { images }) });
  /** Save (as new, or over the theme being changed) under the name field's name, or `as`; whether it was saved (else the message
   *  says why) */
  const save = async (asNew: boolean, as?: string): Promise<boolean> => {
    try {
      const t = await saveCustomTheme({ ...theme(as), id: asNew ? "" : editing ?? "" });
      setEditing(t.id);
      setName(t.name);
      // Sync with OS stays on when a theme is replaced (drawn already if it is one of its two); a new one is switched to
      const sync = settings.theme === SYSTEM;
      if (!sync || asNew) await setThemePick(t.id);
      const low = warnings.length ? ` (${warnings.length} below the contrast minimum, as warned)` : "";
      const how = !sync ? " and switched to it" : asNew ? " and switched to it: Sync with OS is off" : ", Sync with OS stays on";
      setMsg({ tone: "ok", text: `Saved "${t.name}"${how}${low}` });
      return true;
    } catch (e) {
      setMsg({ tone: "danger", text: `Not saved: ${e}` });
      return false;
    }
  };
  /** Settings' OK (as DataGrip's): changes are saved, over the custom theme being changed, or from a built-in one as a new theme named
   *  by the name field or "<Base> custom"; true when there was nothing to save or it was saved */
  const saveIfDirty = async () => !dirty || (saved ? save(false) : save(true, customThemeName(base.name, name, settings.custom.map((c) => c.name))));
  if (commit) commit.current = saveIfDirty;
  const remove = async () => {
    if (!saved || !(await ask(`Delete the custom theme "${saved.name}"? Where it is picked, ${base.name} takes its place (in the TUI too).`, { title: "Delete Theme", okLabel: "Delete", danger: true }))) return;
    try {
      await deleteCustomTheme(saved.id);
      setEditing(null);
      setMsg({ tone: "ok", text: `Deleted "${saved.name}"; its colors stay here as a new theme until you leave` });
    } catch (e) {
      setMsg({ tone: "danger", text: `Not deleted: ${e}` });
    }
  };
  const importFile = async () => {
    try {
      const got = await api.importTheme();
      if (!got || (dirty && !(await askDiscardColors()))) return;
      const t = got.theme;
      setBaseId(t.base);
      setOver(t.colors);
      setName(t.name);
      setImages(t.images ?? {});
      setEditing(null);
      const came = [t.images?.background && "background", t.images?.corner && "corner"].filter(Boolean);
      const with_ = came.length ? ` with its ${came.join(" and ")} image${came.length > 1 ? "s" : ""}` : "";
      const left = got.dropped.length ? `; left out: ${got.dropped.join("; ")}` : "";
      setMsg({ tone: got.dropped.length ? "warn" : "ok", text: `Imported "${t.name}" (based on ${BUILT_IN.find((b) => b.id === t.base)?.name})${with_}${left}. Check it, then Save as new theme` });
    } catch (e) {
      setMsg({ tone: "danger", text: `Not imported: ${e}` });
    }
  };
  const exportFile = async () => {
    try {
      const path = await api.exportTheme({ ...theme(), name: name.trim() || `${base.name} (custom)` });
      if (path) setMsg({ tone: "ok", text: `Exported to ${path}${hasImages ? " with its images (Qurry 0.3.5-beta.1 and older can't import such a file)" : ""}` });
    } catch (e) {
      setMsg({ tone: "danger", text: `Not exported: ${e}` });
    }
  };

  const row = (t: ThemeToken) => (
    <TokenRow
      key={t.key}
      t={t}
      value={colors[t.key]}
      edited={changed.includes(t.key)}
      warning={warnOf(t.key)}
      onHot={(on) => point(t.key, on)}
      onChange={(v) => (setOver((o) => ({ ...o, [t.key]: v })), setHot(null))}
      onReset={() => setOver(({ [t.key]: _, ...o }) => o)}
    />
  );
  const advancedTokens = TOKENS.filter((t) => !t.basic);
  const advancedWarnings = advancedTokens.filter((t) => warnOf(t.key)).length;
  return (
    <div className="theme-editor">
      <div className="theme-editor-head">
        <Button size="sm" variant="link" onClick={onBack}>
          ‹ Appearance
        </Button>
        <Named about="Theme colors" info={["Point at a color's name to see what it colors, above the preview, and where it's used in it.", "Colors below the contrast minimum are saved with their warning, never changed.", "An invalid hex is not applied."]}>
          <b>Theme colors</b>
        </Named>
        <span className="muted">
          {changed.length} {changed.length === 1 ? "color" : "colors"} changed from {base.name}
          {warnings.length > 0 && <span className="theme-warn"> · ⚠ {warnings.length} below the contrast minimum</span>}
        </span>
      </div>
      <div className="theme-editor-body">
        <div className="theme-editor-side">
          <div className="ui-row theme-editor-base">
            <span className="muted">Based on</span>
            <Select aria-label="Based on" value={editing ?? baseId} onChange={(e) => pickBase(e.target.value)}>
              {BUILT_IN.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.name}
                </option>
              ))}
              {settings.custom.length > 0 && (
                <optgroup label="Custom">
                  {settings.custom.map((t) => (
                    <option key={t.id} value={t.id}>
                      {t.name}
                    </option>
                  ))}
                </optgroup>
              )}
            </Select>
          </div>
          <div className="theme-tokens">
            <div className="theme-group">Basic colors</div>
            {TOKENS.filter((t) => t.basic).map(row)}
            <button className="theme-advanced" aria-expanded={advanced} onClick={() => setAdvanced((a) => !a)}>
              <span className="muted">{advanced ? "▾" : "▸"}</span>
              <b>Advanced colors</b>
              <span className="muted">{advancedTokens.length} more: surfaces, icons, grid states, tints, status</span>
              {!advanced && advancedWarnings > 0 && <span className="theme-warn">⚠ {advancedWarnings}</span>}
            </button>
            {advanced &&
              GROUPS.map((g) => {
                const ts = advancedTokens.filter((t) => t.group === g);
                return ts.length === 0 ? null : (
                  <div key={g}>
                    <div className="theme-group">{g}</div>
                    {ts.map(row)}
                  </div>
                );
              })}
          </div>
          <div className="theme-editor-actions">
            <div className="ui-row">
              <Button size="sm" onClick={importFile}>
                Import…
              </Button>
              <Button size="sm" onClick={exportFile}>
                Export…
              </Button>
              <Button size="sm" disabled={changed.length === 0} onClick={() => setOver({})}>
                Reset to base
              </Button>
              {saved && (
                <Button size="sm" variant="danger" onClick={remove}>
                  Delete
                </Button>
              )}
            </div>
            <div className="ui-row">
              <TextInput aria-label="Theme name" value={name} onChange={(e) => setName(e.target.value)} placeholder="Theme name" maxLength={64} style={{ flex: 1, minWidth: 120 }} />
              {/* The primary button is what OK does: Save over the custom theme being changed; from a built-in one, Save as new theme
                  (there is nothing to replace, so no Save) */}
              {saved && (
                <Button size="sm" variant="primary" disabled={!name.trim() || !dirty} onClick={() => save(false)}>
                  Save
                </Button>
              )}
              <Button size="sm" variant={saved ? "default" : "primary"} disabled={!name.trim()} onClick={() => save(true)}>
                Save as new theme
              </Button>
            </div>
            {/* Its own line, kept (an empty one too) so a message doesn't move the color list; it is never cut */}
            <div className="theme-editor-msg" role="status">
              {msg && <Notice tone={msg.tone}>{msg.text}</Notice>}
            </div>
          </div>
        </div>
        <div className="theme-editor-preview">
          <ThemeImagesSection images={images} p={colors} onChange={setImages} newTheme={!editing} />
          {/* One line: pointing at another color doesn't move the preview */}
          <Text size="sm" tone="muted" truncate>
            {`Preview${hot ? ` — ${LABEL[hot]}: ${TOKENS.find((t) => t.key === hot)?.desc}` : ""}`}
          </Text>
          <ThemePreview p={colors} hot={hot} images={images} />
        </div>
      </div>
    </div>
  );
}
