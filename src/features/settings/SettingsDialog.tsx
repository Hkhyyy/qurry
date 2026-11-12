import { Fragment, useEffect, useRef, useState } from "react";
import { alignOf, setGridAlign, useGridAlign, VALUE_CLASSES } from "../../lib/gridAlign";
import { shared, useSharedReady, type Align, type KeywordCase } from "../../lib/shared";
import { loadStatusItems, resetStatusItems, setStatusItem, useStatusItems } from "../../lib/statusbar";
import { setKeywordCase, useKeywordCase } from "../../lib/keywordCase";
import { setResultTabs, useResultTabs } from "../../lib/resultTabsSetting";
import { api, openExternal, secretKey, setUpdateCheckEnabled, UpdateInfo, updateCheckEnabled } from "../../lib/api";
import type { FontFamily, UpdateChannel } from "../../lib/api";
import { KeymapEditor } from "../../components/KeymapEditor";
import { Button, Checkbox, cx, Field, Modal, Select, Text, TextInput, TreeRow } from "../../ui";
import { IcGitHub } from "../../ui/icons";
import { allThemes, currentTheme, setThemePick, setThemeSync, SYSTEM, useThemeState, type ThemeDef } from "../../lib/theme";
import { askDiscardColors, ThemeEditor } from "./ThemeEditor";
import { FontTarget, setFont, SIZES, useFonts } from "../../lib/prefs";
import { FontPicker } from "./FontPicker";
import { isMac, sk } from "../shortcuts/labels";

/** Settings › Appearance › Editor / Grid font: a family picked from the installed ones (FontPicker) and a size. Applied as you pick */
function FontField({ target, label, info, installed, onOpen }: { target: FontTarget; label: string; info?: string[]; installed: FontFamily[] | null; onOpen: () => void }) {
  const { family, size } = useFonts()[target];
  const [min, max] = SIZES[target];
  return (
    // group: a <label> would turn a click in the font list into a click on the picker field
    <Field group label={label} info={info}>
      <div className="ui-row">
        <FontPicker label={`${label} family`} value={family} installed={installed} onOpen={onOpen} onPick={(f) => setFont(target, { family: f })} />
        <Select aria-label={`${label} size`} value={size} onChange={(e) => setFont(target, { size: Number(e.target.value) })}>
          {Array.from({ length: max - min + 1 }, (_, i) => min + i).map((n) => (
            <option key={n} value={n}>
              {n} px
            </option>
          ))}
        </Select>
      </div>
    </Field>
  );
}

/** Settings › Appearance › Data grid › Alignment: where grid cells sit, per value class (shared with the TUI through settings.json) */
export function GridAlignField() {
  const overrides = useGridAlign();
  // The classes and their defaults come from the WASM module: without it (a WebView that refused WebAssembly) nothing can be picked
  const ready = useSharedReady();
  const [err, setErr] = useState<string | null>(null);
  return (
    <Field
      label="Alignment"
      group
      error={err ?? undefined}
      // Only the WebAssembly error stays visible (nothing can be picked without it)
      hint={ready ? undefined : "Alignment needs WebAssembly, which this WebView refused: update macOS (Safari) or webkit2gtk."}
      info={[
        "Where values sit in result grids, by kind of value.",
        "NULL › Column type follows the column: left in text, right in numbers. Pick Right for the 0.3.0 look.",
        ...(ready ? [] : ["Without WebAssembly, numbers are on the right and everything else on the left."]),
        "Shared with the TUI.",
      ]}
    >
      <div className="align-grid">
        {VALUE_CLASSES.map(({ cls, label }) => (
          <Fragment key={cls}>
            <label htmlFor={`align-${cls}`} className={cx("align-label", cls in overrides && "custom")}>
              {label}
            </label>
            <Select
              id={`align-${cls}`}
              aria-label={`${label} alignment`}
              disabled={!ready}
              value={ready ? alignOf(cls, overrides) : ""}
              onChange={(e) => {
                setErr(null);
                setGridAlign(cls, e.target.value as Align).catch((x) => setErr(`Couldn't save the alignment (${x}). It is back to what it was.`));
              }}
            >
              {!ready && <option value="">—</option>}
              {cls === "null" && <option value="column">Column type</option>}
              <option value="left">Left</option>
              <option value="center">Center</option>
              <option value="right">Right</option>
            </Select>
          </Fragment>
        ))}
      </div>
    </Field>
  );
}

/** Settings › Appearance › Data grid › Status bar: the items the status bar shows for the focused grid, by group (the status bar's right-click
 *  checklist, shared with the TUI through settings.json) */
export function StatusBarField() {
  const on = useStatusItems();
  const ready = useSharedReady();
  const [err, setErr] = useState<string | null>(null);
  // The TUI may have changed them since
  useEffect(() => void loadStatusItems(), []);
  const save = (p: Promise<void>) => {
    setErr(null);
    p.catch((e) => setErr(String(e)));
  };
  const info = ready ? shared().statusBarInfo() : null;
  const groups = info ? [...new Set(info.items.map((i) => i.group))] : [];
  return (
    <Field
      label="Status bar"
      group
      error={err ?? undefined}
      hint={ready ? "Position and Shown columns are on by default." : "The status bar items need WebAssembly, which this WebView refused: update macOS (Safari) or webkit2gtk."}
      info={[
        "Which items the status bar shows on the right, for the focused grid.",
        "Position: the current cell (Row 3, Col 4), or the selected rows and columns (Rows 5–26 (22) · Col 4; 22 rows when they aren't one range).",
        "Not null: the selected cells that have a value.",
        "Sum, Avg, Min, Max: over the numbers in the selection.",
        "Size: the current cell's size in bytes and its column's declared limit, when the table's metadata gives it (a table tab, or a console result of a table). The bytes are UTF-8 as sent (binary: its length), which can differ from what the server stores in another character set (latin1).",
        "Right-click the status bar to change them there. Shared with the TUI.",
      ]}
    >
      {info && (
        <div className="sb-field">
          {groups.map((g) => (
            <Fragment key={g}>
              <span className="sb-field-label">{g}</span>
              <div className="sb-field-row">
                {info.items
                  .filter((i) => i.group === g)
                  .map((i) => (
                    <Checkbox key={i.id} checked={on.includes(i.id)} onChange={(v) => save(setStatusItem(i.id, v))}>
                      {i.id === "size" ? `${i.label} (${i.hint})` : i.label}
                    </Checkbox>
                  ))}
              </div>
            </Fragment>
          ))}
          <span />
          <div>
            <Button size="sm" onClick={() => save(resetStatusItems())}>
              Restore Defaults
            </Button>
          </div>
        </div>
      )}
    </Field>
  );
}

/** Settings › Appearance › Console › Keyword case: SQL keywords' letter case as you type in a console and in Reformat Code (shared with the TUI) */
export function KeywordCaseField() {
  const kc = useKeywordCase();
  const [err, setErr] = useState<string | null>(null);
  const reformat = sk("editor.reformat");
  return (
    <Field
      label="Keyword case"
      error={err ?? undefined}
      info={[
        `SQL keywords' case as you type in a console (after a space, a new line or punctuation) and in Reformat Code${reformat ? ` (${reformat})` : ""}.`,
        "Never inside strings, comments or quoted names.",
        "Off: typing is left alone; Reformat Code still writes UPPER.",
        "Shared with the TUI.",
      ]}
    >
      <div className="ui-row">
        <Select
          aria-label="Keyword case"
          value={kc}
          onChange={(e) => {
            setErr(null);
            setKeywordCase(e.target.value as KeywordCase).catch((x) => setErr(`Couldn't save the keyword case (${x}). It is back to what it was.`));
          }}
        >
          <option value="off">Off</option>
          <option value="upper">UPPER</option>
          <option value="lower">lower</option>
        </Select>
      </div>
    </Field>
  );
}

/** Settings › Appearance › Console › Result tabs: unpinned result tabs kept per console (lib/resultTabs; shared with the TUI). Saved as typed; a value
 *  that isn't a whole number of 1 or more says so and isn't saved */
export function ResultTabsField() {
  const n = useResultTabs();
  const [text, setText] = useState(String(n));
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => setText(String(n)), [n]);
  return (
    <Field label="Result tabs" error={err ?? undefined} info={["Unpinned result tabs kept per console; past it, the least recently shown closes. Shared with the TUI."]}>
      <div className="ui-row">
        <TextInput
          aria-label="Result tabs"
          className="w-port"
          type="number"
          min={1}
          value={text}
          aria-invalid={!!err}
          onChange={(e) => {
            const v = e.target.value;
            setText(v);
            const k = Number(v);
            if (v.trim() === "" || !Number.isInteger(k) || k < 1) return setErr("Enter 1 or more.");
            setErr(null);
            setResultTabs(k).catch((x) => setErr(`Couldn't save the result tabs (${x}). It is back to what it was.`));
          }}
        />
      </div>
    </Field>
  );
}

/** The theme choices: built-in, then custom */
function ThemeOptions({ themes }: { themes: ThemeDef[] }) {
  const custom = themes.filter((t) => t.custom);
  const opt = (t: ThemeDef) => (
    <option key={t.id} value={t.id}>
      {t.name}
    </option>
  );
  return (
    <>
      {themes.filter((t) => !t.custom).map(opt)}
      {custom.length > 0 && <optgroup label="Custom">{custom.map(opt)}</optgroup>}
    </>
  );
}

/** Settings › Appearance › Theme: built-in and custom themes, or Sync with OS with a dark and a light theme of the user's choice
 *  (shared with the TUI through settings.json); Edit Colors… opens the theme editor */
export function ThemeField({ onEdit }: { onEdit: () => void }) {
  const { settings, current } = useThemeState();
  const [err, setErr] = useState<string | null>(null);
  const all = allThemes(settings);
  const sync = settings.theme === SYSTEM;
  const run = (p: Promise<void>) => {
    setErr(null);
    p.catch((e) => setErr(String(e)));
  };
  return (
    <Field
      label="Theme"
      group
      error={err ?? undefined}
      hint={sync ? `Following the system: ${current.name} now.` : undefined}
      info={[sync ? "The TUI uses the same themes; the terminal's background picks dark or light." : "The TUI uses the same theme."]}
    >
      <div className="ui-row">
        <Select aria-label="Theme" value={sync ? current.id : settings.theme ?? "dark"} disabled={sync} onChange={(e) => run(setThemePick(e.target.value))}>
          <ThemeOptions themes={all} />
        </Select>
        <Checkbox checked={sync} onChange={(v) => run(setThemePick(v ? SYSTEM : current.id))}>
          Sync with OS
        </Checkbox>
        <Button size="sm" onClick={onEdit}>
          Edit Colors…
        </Button>
      </div>
      {sync && (
        <div className="ui-row theme-sync">
          <span className="muted">Dark</span>
          <Select aria-label="Theme for a dark appearance" value={settings.sync.dark} onChange={(e) => run(setThemeSync({ ...settings.sync, dark: e.target.value }))}>
            <ThemeOptions themes={all.filter((t) => t.kind !== "light")} />
          </Select>
          <span className="muted">Light</span>
          <Select aria-label="Theme for a light appearance" value={settings.sync.light} onChange={(e) => run(setThemeSync({ ...settings.sync, light: e.target.value }))}>
            <ThemeOptions themes={all.filter((t) => t.kind === "light")} />
          </Select>
        </div>
      )}
    </Field>
  );
}

/** Settings › Passwords: whether to keep the password file's master key in a file or the OS keychain */
function PasswordSettings() {
  const [loc, setLoc] = useState<"file" | "keychain" | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  useEffect(() => {
    secretKey.location().then(setLoc, (e) => setMsg(String(e)));
  }, []);
  const move = async (keychain: boolean) => {
    setMsg(keychain ? "Waiting for the system keychain…" : "Moving the key back to the file…");
    try {
      setLoc(await secretKey.move(keychain));
      setMsg(null);
    } catch (e) {
      setMsg(String(e));
    }
  };
  return (
    <div className="settings-form">
      <Field
        label="Key"
        hint={msg ?? undefined}
        info={[
          "Saved passwords are encrypted in secrets.json; the key is in the config folder (secret.key) or in the system keychain.",
          "qurry-tui and qurry-mcp use the same key, so they ask for keychain access too.",
          ...(isMac ? ["macOS asks once per app (Qurry, qurry-tui, qurry-mcp), and again after each update while Qurry isn't code-signed."] : []),
          'Older versions can\'t read the key in the keychain and stop with "secret.key is corrupted" (update with e.g. brew upgrade qurry).',
        ]}
      >
        <div className="ui-row">
          <Select aria-label="Key" value={loc ?? ""} disabled={!loc} onChange={(e) => move(e.target.value === "keychain")}>
            <option value="file">In the config folder (no prompts)</option>
            <option value="keychain">In the system keychain</option>
          </Select>
        </div>
        {/* A warning before the move: shown while the key is still in the file */}
        {loc === "file" && (
          <Text size="sm" tone="warn">
            Update Qurry, qurry-tui and qurry-mcp before moving the key: older ones can't read it.
          </Text>
        )}
      </Field>
    </div>
  );
}

/** The update a check offers, or null: a check that finds nothing clears the status bar button (the TUI may have switched the shared channel) */
const offered = (u: UpdateInfo) => (u.latest ? u : null);
const upToDate = (u: UpdateInfo) => `You have the latest ${u.channel} version (${u.current}).`;

/** The pages tree on the left (DataGrip's settings tree): a child page sits under its parent */
const PAGES = [
  { id: "appearance", label: "Appearance", parent: null },
  { id: "console", label: "Console", parent: "appearance" },
  { id: "grid", label: "Data grid", parent: "appearance" },
  { id: "updates", label: "Updates", parent: null },
  { id: "keymap", label: "Keymap", parent: null },
  { id: "passwords", label: "Passwords", parent: null },
] as const;
export type SettingsPage = (typeof PAGES)[number]["id"];
const hasKids = (id: SettingsPage) => PAGES.some((p) => p.parent === id);

/** Settings opens on the last page used when no page is asked for */
const LAST_PAGE = "dbtool.settingsPage";
const lastPage = (): SettingsPage => {
  try {
    const v = localStorage.getItem(LAST_PAGE);
    return PAGES.find((p) => p.id === v)?.id ?? "appearance";
  } catch {
    return "appearance";
  }
};

const REPO_URL = "https://github.com/Hkhyyy/qurry";

/** onChannel: the update the new channel (or a check that finds nothing) offers (null: none), so the status bar button follows the channel.
 *  initialPage: the page to open on (the theme editor is Appearance's); none: the last page used */
export function SettingsDialog({ onClose, onUpdate, onChannel, initialPage }: { onClose: () => void; onUpdate: (u: UpdateInfo) => void; onChannel: (u: UpdateInfo | null) => void; initialPage?: SettingsPage | "theme" }) {
  const [page, setPage] = useState<SettingsPage | "theme">(() => initialPage ?? lastPage());
  useEffect(() => {
    if (page === "theme") return;
    try {
      localStorage.setItem(LAST_PAGE, page);
    } catch {
      /* the next Settings opens on Appearance */
    }
  }, [page]);
  const [checkOnStart, setCheckOnStart] = useState(updateCheckEnabled());
  const [updMsg, setUpdMsg] = useState<string | null>(null);
  const [channel, setChannel] = useState<UpdateChannel | null>(null);
  const [chMsg, setChMsg] = useState<string | null>(null);
  useEffect(() => {
    api.updateChannel().then(setChannel, (e) => setChMsg(String(e)));
  }, []);
  const pickChannel = async (c: UpdateChannel) => {
    const was = channel;
    setChannel(c);
    // A Check Now answer was for the channel before
    setUpdMsg(null);
    try {
      // Answered from the saved check (both channels), never a new request
      const u = await api.setUpdateChannel(c);
      onChannel(u && offered(u));
      setChMsg(!u ? "Saved. Check Now looks for updates on this channel." : u.latest ? `Qurry ${u.latest.version} is available on this channel.` : upToDate(u));
    } catch (e) {
      // Not saved: the select shows the channel still in use
      setChannel(was);
      setChMsg(String(e));
    }
  };
  const checkNow = async () => {
    setUpdMsg("Checking…");
    try {
      const u = await api.checkUpdate(true);
      if (u.latest) onUpdate(u);
      else {
        onChannel(null);
        setUpdMsg(upToDate(u));
      }
    } catch (e) {
      setUpdMsg(String(e));
    }
  };
  // Leaving the theme editor with colors not saved asks first (they would be lost)
  const themeDirty = useRef(false);
  const leave = async (then: () => void) => {
    if (page === "theme" && themeDirty.current && !(await askDiscardColors())) return;
    themeDirty.current = false;
    then();
  };
  const go = (p: typeof page) => leave(() => setPage(p));
  // Listed when Settings opens and again each time a picker opens (a font installed meanwhile shows up); empty if it can't
  // (the pickers then offer the default and the saved family)
  const [fonts, setFonts] = useState<FontFamily[] | null>(null);
  const loadFonts = () => api.fontFamilies().then(setFonts, () => setFonts((f) => f ?? []));
  useEffect(() => {
    loadFonts();
  }, []);
  const fontKeys = [sk("font.bigger"), sk("font.smaller")].filter(Boolean).join(" and ");

  /** The row selected in the tree (the theme editor is Appearance's) */
  const shown: SettingsPage = page === "theme" ? "appearance" : page;
  // Every parent starts open, so the opened page's row shows
  const [open, setOpen] = useState<ReadonlySet<SettingsPage>>(() => new Set(PAGES.flatMap((p) => (p.parent ? [p.parent] : []))));
  const visible = PAGES.filter((p) => !p.parent || open.has(p.parent));
  const toggle = (id: SettingsPage) => setOpen((o) => new Set(o.has(id) ? [...o].filter((x) => x !== id) : [...o, id]));
  /** DataGrip's tree keys: ↑↓ the previous / next row, → opens a parent then goes to its first page, ← closes it or goes to the parent,
   *  Home / End the first / last row. The Modal doesn't see them */
  const treeKeys = (e: React.KeyboardEvent) => {
    const i = visible.findIndex((p) => p.id === shown);
    const cur = visible[i];
    let to: SettingsPage | null = null;
    if (e.key === "ArrowUp") to = visible[Math.max(0, i - 1)].id;
    else if (e.key === "ArrowDown") to = visible[Math.min(visible.length - 1, i + 1)].id;
    else if (e.key === "Home") to = visible[0].id;
    else if (e.key === "End") to = visible[visible.length - 1].id;
    else if (e.key === "ArrowRight") {
      if (hasKids(cur.id) && !open.has(cur.id)) toggle(cur.id);
      else if (hasKids(cur.id)) to = PAGES.find((p) => p.parent === cur.id)!.id;
    } else if (e.key === "ArrowLeft") {
      if (hasKids(cur.id) && open.has(cur.id)) toggle(cur.id);
      else if (cur.parent) to = cur.parent;
    } else return;
    e.preventDefault();
    e.stopPropagation();
    if (to && to !== shown) go(to);
  };
  return (
    <Modal
      title="Settings"
      size="xl"
      fixed
      className="settings"
      onClose={() => leave(onClose)}
      footer={
        <>
          {shown === "appearance" && <span className="ui-modal-foot-hint">Quick switch theme: {sk("theme")}</span>}
          <Button variant="primary" onClick={() => leave(onClose)}>
            OK
          </Button>
        </>
      }
    >
      <div className="settings-body">
        <div className="settings-nav settings-tree" role="tree" aria-label="Settings" tabIndex={0} onKeyDown={treeKeys}>
          {visible.map((p) => (
            <TreeRow
              key={p.id}
              depth={p.parent ? 1 : 0}
              leaf={!hasKids(p.id)}
              open={open.has(p.id)}
              label={p.label}
              selected={shown === p.id}
              onSelect={() => go(p.id)}
              onToggle={() => toggle(p.id)}
            />
          ))}
        </div>
        {page === "keymap" && <KeymapEditor />}
        {page === "passwords" && <PasswordSettings />}
        {page === "theme" && <ThemeEditor start={currentTheme().id} onBack={() => go("appearance")} onDirty={(d) => (themeDirty.current = d)} />}
        {page === "appearance" && (
          <div className="settings-form">
            <ThemeField onEdit={() => setPage("theme")} />
            <FontField target="editor" label="Editor font" info={fontKeys ? [`In the editor, ${fontKeys} change the size too.`] : undefined} installed={fonts} onOpen={loadFonts} />
            <FontField target="grid" label="Grid font" installed={fonts} onOpen={loadFonts} />
          </div>
        )}
        {page === "console" && (
          <div className="settings-form">
            <KeywordCaseField />
            <ResultTabsField />
          </div>
        )}
        {page === "grid" && (
          <div className="settings-form">
            <GridAlignField />
            <StatusBarField />
          </div>
        )}
        {page === "updates" && (
          <div className="settings-form">
            <Field label="Updates" hint={updMsg ?? undefined}>
              <div className="ui-row">
                <Checkbox
                  checked={checkOnStart}
                  onChange={(v) => {
                    setCheckOnStart(v);
                    setUpdateCheckEnabled(v);
                  }}
                >
                  Check for updates automatically
                </Checkbox>
                <Button size="sm" onClick={checkNow}>
                  Check Now
                </Button>
                <Button size="sm" className="settings-github" aria-label="Open GitHub" title="Open GitHub" onClick={() => openExternal(REPO_URL)}>
                  <IcGitHub />
                </Button>
              </div>
            </Field>
            <Field label="Channel" hint={chMsg ?? undefined} info={["Stable: releases made from a version tag.", "Beta: also the pre-release built from every merge to main.", "Shared with the TUI."]}>
              <div className="ui-row">
                <Select aria-label="Channel" value={channel ?? ""} disabled={!channel} onChange={(e) => pickChannel(e.target.value as UpdateChannel)}>
                  <option value="stable">Stable</option>
                  <option value="beta">Beta</option>
                </Select>
              </div>
            </Field>
          </div>
        )}
      </div>
    </Modal>
  );
}

export { offered, upToDate };
