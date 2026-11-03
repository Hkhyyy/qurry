import { useEffect, useMemo, useState } from "react";
import { api, secretKey, setUpdateCheckEnabled, UpdateInfo, updateCheckEnabled } from "../../lib/api";
import type { UpdateChannel } from "../../lib/api";
import { KeymapEditor } from "../../components/KeymapEditor";
import { Button, Checkbox, Field, List, ListItem, Modal, Select, Text, TextInput } from "../../ui";
import { getThemePref, setThemePref, ThemePref, useTheme } from "../../lib/theme";
import { fontInstalled, fontStack, FontTarget, KNOWN_MONO, setFont, SIZES, useFonts } from "../../lib/prefs";
import { isMac, sk } from "../shortcuts/labels";

/** Settings › Appearance › Editor / Grid font: a family (the installed fonts of KNOWN_MONO are suggested, any other can be typed; shown in itself) and a size. Applied as you type */
function FontField({ target, label, hint }: { target: FontTarget; label: string; hint?: string }) {
  const { family, size } = useFonts()[target];
  const [min, max] = SIZES[target];
  const name = family.trim();
  const missing = name !== "" && !fontInstalled(name);
  // The hint is always there with two lines reserved (.settings-font), so the warning coming and going as you type doesn't move the fields below
  return (
    <Field className="settings-font" label={label} hint={<>{missing ? `"${name}" isn't installed, so the default monospace font is used. Pick a font from the list, or clear the field for the default.` : hint}</>}>
      <div className="ui-row">
        <TextInput grow list="settings-fonts" aria-label={`${label} family`} placeholder="Default monospace" spellCheck={false} value={family} style={{ fontFamily: fontStack(family) }} onChange={(e) => setFont(target, { family: e.target.value })} />
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
export function SettingsDialog({ onClose, onUpdate, onChannel, initialPage = "appearance" }: { onClose: () => void; onUpdate: (u: UpdateInfo) => void; onChannel: (u: UpdateInfo | null) => void; initialPage?: "appearance" | "keymap" }) {
  const [page, setPage] = useState<"appearance" | "keymap" | "passwords">(initialPage);
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
  const [pref, setPref] = useState<ThemePref>(getThemePref());
  const theme = useTheme();
  const pick = (p: ThemePref) => {
    setPref(p);
    setThemePref(p);
  };
  const sync = pref === "system";
  const fonts = useMemo(() => KNOWN_MONO.filter(fontInstalled), []);
  const fontKeys = [sk("font.bigger"), sk("font.smaller")].filter(Boolean).join(" / ");
  return (
    <Modal
      title="Settings"
      size="xl"
      fixed
      className="settings"
      onClose={onClose}
      footer={
        <>
          {page === "appearance" && <span className="ui-modal-foot-hint">Quick switch theme: {sk("theme")}</span>}
          <Button variant="primary" onClick={onClose}>
            OK
          </Button>
        </>
      }
    >
      <div className="settings-body">
        <List className="settings-nav">
          <ListItem title="Appearance" selected={page === "appearance"} onClick={() => setPage("appearance")} />
          <ListItem title="Keymap" selected={page === "keymap"} onClick={() => setPage("keymap")} />
          <ListItem title="Passwords" selected={page === "passwords"} onClick={() => setPage("passwords")} />
        </List>
        {page === "keymap" && <KeymapEditor />}
        {page === "passwords" && <PasswordSettings />}
        {page === "appearance" && (
          <div className="settings-form">
            <Field label="Theme">
              <div className="ui-row">
                <Select value={sync ? theme : pref} disabled={sync} onChange={(e) => pick(e.target.value as ThemePref)}>
                  <option value="dark">Dark</option>
                  <option value="light">Light</option>
                  <option value="hc">High contrast</option>
                </Select>
                <Checkbox checked={sync} onChange={(v) => pick(v ? "system" : theme)}>
                  Sync with OS
                </Checkbox>
              </div>
            </Field>
            <FontField target="editor" label="Editor font" hint={fontKeys && `Size also with ${fontKeys} in the editor.`} />
            <FontField target="grid" label="Grid font" />
            <datalist id="settings-fonts">
              {fonts.map((f) => (
                <option key={f} value={f} />
              ))}
            </datalist>
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
