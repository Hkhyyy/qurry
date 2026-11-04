import Editor from "@monaco-editor/react";
import { useEffect, useRef } from "react";
import { format as formatSql } from "sql-formatter";
import { monaco } from "../lib/monaco";
import { bindModel, unbindModel } from "../lib/completion";
import { monacoTheme, useTheme } from "../lib/theme";
import { font, fontStack } from "../lib/prefs";
import { setStatusWidget } from "../lib/statusbar";
import { bindings, onKeymapChange } from "../lib/shortcuts";

/** Run request: passes editor content·caret·selection; the console decides what actually runs (DataGrip run rules) */
export interface RunRequest {
  text: string;
  offset: number;
  selection: { text: string; start: number; end: number } | null;
  /** Briefly highlight the range to run in the editor (not once the text changed after the press) */
  highlight: (ranges: { start: number; end: number }[]) => void;
  /** Explain Plan / Explain Analyse */
  explain?: "plan" | "analyze";
}

export interface EditorApi {
  run: (explain?: "plan" | "analyze") => void;
  insert: (text: string) => void;
  focus: () => void;
  /** Selected text, or the whole console if none */
  text: () => string;
}

interface Props {
  tabId: string;
  connId: string;
  /** The console's database by name (the connection's default included): completion inserts its MySQL tables bare */
  database: string;
  initialSql: string;
  language: string;
  onChange: (sql: string) => void;
  onRun: (req: RunRequest) => void;
  registerApi: (tabId: string, api: EditorApi) => void;
  readOnly?: boolean;
}

const { KeyMod: M, KeyCode: K } = monaco;

const MONACO_KEYS: Record<string, number> = {
  "/": K.Slash, "\\": K.Backslash, ";": K.Semicolon, "'": K.Quote, ",": K.Comma, ".": K.Period, "[": K.BracketLeft, "]": K.BracketRight, "`": K.Backquote, "-": K.Minus, "=": K.Equal,
  Enter: K.Enter, Backspace: K.Backspace, Delete: K.Delete, Esc: K.Escape, Tab: K.Tab, Space: K.Space,
  Up: K.UpArrow, Down: K.DownArrow, Left: K.LeftArrow, Right: K.RightArrow, Home: K.Home, End: K.End, PageUp: K.PageUp, PageDown: K.PageDown, Insert: K.Insert,
};

/** lib/shortcuts notation → Monaco keys (Mod = ⌘/Ctrl, Ctrl = macOS ⌃) */
function monacoKey(spec: string): number | null {
  const parts = spec.split("+");
  const key = parts.pop()!;
  const code = /^[A-Z]$/.test(key) ? (K as any)["Key" + key] : /^\d$/.test(key) ? (K as any)["Digit" + key] : /^F\d{1,2}$/.test(key) ? (K as any)[key] : MONACO_KEYS[key];
  if (code === undefined) return null;
  const mods: Record<string, number> = { Mod: M.CtrlCmd, Ctrl: M.WinCtrl, Alt: M.Alt, Shift: M.Shift };
  return parts.reduce((n, m) => n | (mods[m] ?? 0), code as number);
}
const keysOf = (id: string) => bindings(id).map(monacoKey).filter((n): n is number => n !== null);

export function ConsoleEditor({ tabId, connId, database, initialSql, language, onChange, onRun, registerApi, readOnly }: Props) {
  const modelUri = useRef<string | null>(null);
  useEffect(() => {
    if (modelUri.current) bindModel(modelUri.current, connId, database);
  }, [connId, database]);
  const theme = useTheme();
  const onRunRef = useRef(onRun);
  onRunRef.current = onRun;

  return (
    <Editor
      path={`console-${tabId}.sql`}
      defaultValue={initialSql}
      // Read-only (DDL): the text follows initialSql, so a reload shows the new text
      value={readOnly ? initialSql : undefined}
      defaultLanguage={language}
      theme={monacoTheme(theme)}
      onChange={(v) => onChange(v ?? "")}
      options={{
        minimap: { enabled: false },
        fontSize: font("editor").size,
        fontFamily: fontStack(font("editor").family),
        automaticLayout: true,
        scrollBeyondLastLine: false,
        renderLineHighlight: "all",
        tabSize: 2,
        fixedOverflowWidgets: true,
        readOnly,
        // Use textarea input instead of Chromium's EditContext (to match WebKit/Tauri behavior)
        editContext: false,
        quickSuggestions: { other: true, strings: false, comments: false },
        suggestOnTriggerCharacters: true,
      }}
      onMount={(ed) => {
        const model = ed.getModel();
        if (model) {
          modelUri.current = model.uri.toString();
          bindModel(modelUri.current, connId, database);
        }
        const caret = () => {
          const pos = ed.getPosition();
          const sel = ed.getSelection();
          const m = ed.getModel();
          if (!pos || !m) return;
          const n = sel && !sel.isEmpty() ? m.getValueInRange(sel).length : 0;
          setStatusWidget("caret", `${pos.lineNumber}:${pos.column}${n ? ` (${n} chars)` : ""}`);
        };
        ed.onDidChangeCursorSelection(caret);
        ed.onDidFocusEditorText(caret);
        const deco = ed.createDecorationsCollection();
        const highlight = (ranges: { start: number; end: number }[]) => {
          const m = ed.getModel();
          if (!m) return;
          deco.set(
            ranges.map((r) => {
              const s = m.getPositionAt(r.start);
              const e = m.getPositionAt(r.end);
              return { range: new monaco.Range(s.lineNumber, s.column, e.lineNumber, e.column), options: { className: "exec-highlight" } };
            }),
          );
          setTimeout(() => deco.clear(), 700);
        };
        const run = (explain?: "plan" | "analyze") => {
          const m = ed.getModel();
          if (!m) return;
          const sel = ed.getSelection();
          const selection =
            sel && !sel.isEmpty() ? { text: m.getValueInRange(sel), start: m.getOffsetAt(sel.getStartPosition()), end: m.getOffsetAt(sel.getEndPosition()) } : null;
          // The offsets are into the text at the press: after an edit (a run waiting for the module, the Statements chooser) they'd mark other text
          const version = m.getAlternativeVersionId();
          const mark = (ranges: { start: number; end: number }[]) => ed.getModel() === m && m.getAlternativeVersionId() === version && highlight(ranges);
          onRunRef.current({ text: m.getValue(), offset: m.getOffsetAt(ed.getPosition()!), selection, highlight: mark, explain });
        };
        // ── DataGrip editor keymap (keys are editor.* in lib/shortcuts; re-registered on change) ──
        let registered: monaco.IDisposable[] = [];
        const act = (id: string, label: string, shortcut: string, fn: () => void) =>
          registered.push(ed.addAction({ id, label, keybindings: keysOf(shortcut), run: fn }));
        const register = () => {
          registered.forEach((d) => d.dispose());
          registered = [];
          registered.push(ed.addAction({ id: "dbtool.run", label: "Execute", keybindings: keysOf("editor.run"), contextMenuGroupId: "navigation", contextMenuOrder: 0, run: () => run() }));
          registered.push(ed.addAction({ id: "dbtool.explain", label: "Explain Plan", keybindings: keysOf("editor.explain"), contextMenuGroupId: "navigation", contextMenuOrder: 1, run: () => run("plan") }));
          registered.push(ed.addAction({ id: "dbtool.explainAnalyse", label: "Explain Analyse", keybindings: keysOf("editor.explainAnalyze"), contextMenuGroupId: "navigation", contextMenuOrder: 2, run: () => run("analyze") }));
          act("dbtool.reformat", "Reformat Code", "editor.reformat", () => {
            const m = ed.getModel();
            if (!m) return;
            const sel = ed.getSelection();
            const whole = !sel || sel.isEmpty();
            const range = whole ? m.getFullModelRange() : sel!;
            try {
              const out = formatSql(m.getValueInRange(range), { language: language === "mysql" ? "mysql" : "postgresql", keywordCase: "upper", tabWidth: 4 });
              ed.executeEdits("reformat", [{ range, text: out }]);
            } catch {
              /* leave unparsable SQL as is */
            }
          });
          act("dbtool.duplicate", "Duplicate Line or Selection", "editor.duplicate", () => ed.trigger("k", "editor.action.copyLinesDownAction", null));
          act("dbtool.deleteLine", "Delete Line", "editor.deleteLine", () => ed.trigger("k", "editor.action.deleteLines", null));
          act("dbtool.moveUp", "Move Line Up", "editor.moveUp", () => ed.trigger("k", "editor.action.moveLinesUpAction", null));
          act("dbtool.moveDown", "Move Line Down", "editor.moveDown", () => ed.trigger("k", "editor.action.moveLinesDownAction", null));
          act("dbtool.extend", "Extend Selection", "editor.extend", () => ed.trigger("k", "editor.action.smartSelect.expand", null));
          act("dbtool.shrink", "Shrink Selection", "editor.shrink", () => ed.trigger("k", "editor.action.smartSelect.shrink", null));
          act("dbtool.gotoLine", "Go to Line/Column", "editor.gotoLine", () => ed.trigger("k", "editor.action.gotoLine", null));
          // DataGrip ⌘r: find/replace bar (regex .* · case · word toggles are in the Monaco bar)
          act("dbtool.replace", "Replace", "editor.replace", () => ed.trigger("k", "editor.action.startFindReplaceAction", null));
          act("dbtool.nextOcc", "Add Selection for Next Occurrence", "editor.nextOcc", () => ed.trigger("k", "editor.action.addSelectionToNextFindMatch", null));
          act("dbtool.allOcc", "Select All Occurrences", "editor.allOcc", () => ed.trigger("k", "editor.action.selectHighlights", null));
          act("dbtool.toggleCase", "Toggle Case", "editor.toggleCase", () => {
            const m = ed.getModel();
            const sel = ed.getSelection();
            if (!m || !sel || sel.isEmpty()) return;
            const t = m.getValueInRange(sel);
            ed.executeEdits("case", [{ range: sel, text: t === t.toUpperCase() ? t.toLowerCase() : t.toUpperCase() }]);
          });
          act("dbtool.blockComment", "Comment with Block Comment", "editor.blockComment", () => ed.trigger("k", "editor.action.blockComment", null));
        };
        register();
        const offKeymap = onKeymapChange(register);
        // addAction's commands·shortcuts·menus stay in the global registry, so dispose them manually when the editor closes
        ed.onDidDispose(() => {
          offKeymap();
          registered.forEach((d) => d.dispose());
          registered = [];
          if (modelUri.current) unbindModel(modelUri.current);
        });

        registerApi(tabId, {
          run,
          insert: (text) => {
            const sel = ed.getSelection();
            if (sel) ed.executeEdits("insert", [{ range: sel, text, forceMoveMarkers: true }]);
            ed.focus();
          },
          focus: () => ed.focus(),
          text: () => {
            const m = ed.getModel();
            const sel = ed.getSelection();
            if (!m) return "";
            return sel && !sel.isEmpty() ? m.getValueInRange(sel) : m.getValue();
          },
        });
        ed.focus();
      }}
    />
  );
}
