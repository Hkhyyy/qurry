import { Fragment, useEffect, useRef, useState } from "react";
import { alignOf, setGridAlign, useGridAlign, VALUE_CLASSES } from "../../lib/gridAlign";
import { useSharedReady, type Align } from "../../lib/shared";
import { api, secretKey, setUpdateCheckEnabled, UpdateInfo, updateCheckEnabled } from "../../lib/api";
import type { FontFamily, UpdateChannel } from "../../lib/api";
import { KeymapEditor } from "../../components/KeymapEditor";
import { Button, Checkbox, cx, Field, List, ListItem, Modal, Select, Text } from "../../ui";
import { allThemes, currentTheme, setThemePick, setThemeSync, SYSTEM, useThemeState, type ThemeDef } from "../../lib/theme";
import { askDiscardColors, ThemeEditor } from "./ThemeEditor";
import { FontTarget, setFont, SIZES, useFonts } from "../../lib/prefs";
import { FontPicker } from "./FontPicker";
import { isMac, sk } from "../shortcuts/labels";

/** Settings › Appearance › Editor / Grid font: a family picked from the installed ones (FontPicker) and a size. Applied as you pick */
function FontField({ target, label, hint, installed, onOpen }: { target: FontTarget; label: string; hint?: string; installed: FontFamily[] | null; onOpen: () => void }) {
  const { family, size } = useFonts()[target];
  const [min, max] = SIZES[target];
  return (
    // group: a <label> would turn a click in the font list into a click on the picker field
    <Field group label={label} hint={hint}>
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

/** Settings › Appearance › Alignment: where grid cells sit, per value class (shared with the TUI through settings.json) */
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
      hint={
        ready
          ? "Where cells sit in result grids, by the kind of value; the TUI uses the same setting. NULL sits where its column's type does by default (Column type: left in text, right in numbers); set NULL to Right to keep the 0.3.0 look."
          : "Alignment needs WebAssembly, which this WebView refused: update macOS (Safari) or webkit2gtk. Until then numbers are on the right and everything else on the left."
      }
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
    <Field label="Theme" group error={err ?? undefined} hint={sync ? `Follows the system appearance: ${current.name} now. The TUI uses the same themes (its terminal background picks dark or light).` : "The TUI uses the same theme."}>
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
      <Field label="Key" hint={msg ?? "Saved passwords are encrypted in secrets.json. Its key is in the config folder (secret.key) or in the system keychain. The terminal UI and the MCP server use the same key, so they ask for keychain access too."}>
        <div className="ui-row">
          <Select value={loc ?? ""} disabled={!loc} onChange={(e) => move(e.target.value === "keychain")}>
            <option value="file">In the config folder (no prompts)</option>
            <option value="keychain">In the system keychain</option>
          </Select>
        </div>
      </Field>
      <Text tone="muted">Before moving it to the keychain, update every Qurry app you use (Qurry, qurry-tui, qurry-mcp — e.g. brew upgrade qurry): older versions can't read the key there and stop with "secret.key is corrupted".</Text>
      {isMac && <Text tone="muted">macOS asks once per app (Qurry, qurry-tui, qurry-mcp) to allow keychain access, and again after each update while Qurry isn't code-signed.</Text>}
    </div>
  );
}

/** The update a check offers, or null: a check that finds nothing clears the status bar button (the TUI may have switched the shared channel) */
const offered = (u: UpdateInfo) => (u.latest ? u : null);
const upToDate = (u: UpdateInfo) => `You have the latest ${u.channel} version (${u.current}).`;

/** onChannel: the update the new channel (or a check that finds nothing) offers (null: none), so the status bar button follows the channel */
export function SettingsDialog({ onClose, onUpdate, onChannel, initialPage = "appearance" }: { onClose: () => void; onUpdate: (u: UpdateInfo) => void; onChannel: (u: UpdateInfo | null) => void; initialPage?: "appearance" | "theme" | "keymap" }) {
  const [page, setPage] = useState<"appearance" | "theme" | "keymap" | "passwords">(initialPage);
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
  const fontKeys = [sk("font.bigger"), sk("font.smaller")].filter(Boolean).join(" / ");
  return (
    <Modal
      title="Settings"
      size="xl"
      fixed
      className="settings"
      onClose={() => leave(onClose)}
      footer={
        <>
          {(page === "appearance" || page === "theme") && <span className="ui-modal-foot-hint">Quick switch theme: {sk("theme")}</span>}
          <Button variant="primary" onClick={() => leave(onClose)}>
            OK
          </Button>
        </>
      }
    >
      <div className="settings-body">
        <List className="settings-nav">
          <ListItem title="Appearance" selected={page === "appearance" || page === "theme"} onClick={() => go("appearance")} />
          <ListItem title="Keymap" selected={page === "keymap"} onClick={() => go("keymap")} />
          <ListItem title="Passwords" selected={page === "passwords"} onClick={() => go("passwords")} />
        </List>
        {page === "keymap" && <KeymapEditor />}
        {page === "passwords" && <PasswordSettings />}
        {page === "theme" && <ThemeEditor start={currentTheme().id} onBack={() => go("appearance")} onDirty={(d) => (themeDirty.current = d)} />}
        {page === "appearance" && (
          <div className="settings-form">
            <ThemeField onEdit={() => setPage("theme")} />
            <FontField target="editor" label="Editor font" hint={fontKeys && `Size also with ${fontKeys} in the editor.`} installed={fonts} onOpen={loadFonts} />
            <FontField target="grid" label="Grid font" installed={fonts} onOpen={loadFonts} />
            <GridAlignField />
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
              </div>
            </Field>
            <Field label="Channel" hint={chMsg ?? "Stable gets the releases made from a version tag. Beta also gets the pre-release built from every merge to main. The terminal UI uses the same channel."}>
              <div className="ui-row">
                <Select value={channel ?? ""} disabled={!channel} onChange={(e) => pickChannel(e.target.value as UpdateChannel)}>
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
