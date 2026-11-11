import Editor from "@monaco-editor/react";
import { useEffect, useRef } from "react";
import { markUnbalanced, monaco, setRunError } from "../lib/monaco";
import { bindModel, unbindModel } from "../lib/completion";
import { MONACO_THEME } from "../lib/theme";
import { font, fontStack } from "../lib/prefs";
import { caretText, setFocusWidgets } from "../lib/statusbar";
import { bindings, onKeymapChange } from "../lib/shortcuts";
import { markHover, shownMark, type MarkStmt, type StmtMark } from "../lib/stmtMark";
import { shared, sharedReady } from "../lib/shared";
import { keywordCase } from "../lib/keywordCase";
import type { DbKind } from "../lib/api";

/** Run request: passes editor content·caret·selection; the console decides what actually runs (DataGrip run rules) */
export interface RunRequest {
  text: string;
  offset: number;
  selection: { text: string; start: number; end: number } | null;
  /** Briefly highlight the ranges about to run and keep them for their gutter marks; gives how to mark them (nothing once the text changed after the press) */
  begin: (ranges: { start: number; end: number }[]) => MarkStmt | undefined;
  /** Explain Plan / Explain Analyse */
  explain?: "plan" | "analyze";
}

export interface EditorApi {
  run: (explain?: "plan" | "analyze") => void;
  insert: (text: string) => void;
  focus: () => void;
  /** Highlight what a Statements popup option would run (none = clear); scrolls its start into view, the caret stays */
  preview: (ranges: { start: number; end: number }[]) => void;
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
  /** The data source's kind: how Reformat Code, the keyword case and the unclosed quote and bracket marks read strings, comments and
   * quoted names (Redis, MongoDB and vector consoles: none of them runs) */
  kind: DbKind;
  onChange: (sql: string) => void;
  onRun: (req: RunRequest) => void;
  /** A statement's gutter mark was clicked: its Output line */
  onMarkClick?: (out: string) => void;
  /** How the console splits its script into statements (their places are what result tabs follow) */
  split?: (text: string) => { start: number; end: number }[];
  /** The caret moved: the statement it is in (from its first non-blank character to its end), null between statements */
  onCaret?: (stmt: number | null) => void;
  /** The text changed: its statements' texts before and after (once per change, only when they differ) */
  onStatements?: (before: string[], after: string[]) => void;
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

/** One character typed (or an auto-closed pair, or a newline with its indent) that can end a keyword: not a letter, digit, _ or $ */
const TYPED_END = /^(?:[^\p{L}\p{N}_$]|[([{'"`][)\]}'"`]|\r?\n[ \t]*)$/u;

export function ConsoleEditor({ tabId, connId, database, initialSql, language, kind, onChange, onRun, onMarkClick, split, onCaret, onStatements, registerApi, readOnly }: Props) {
  const modelUri = useRef<string | null>(null);
  useEffect(() => {
    if (modelUri.current) bindModel(modelUri.current, connId, database);
  }, [connId, database]);
  const onRunRef = useRef(onRun);
  onRunRef.current = onRun;
  const onMarkRef = useRef(onMarkClick);
  onMarkRef.current = onMarkClick;
  const kindRef = useRef(kind);
  kindRef.current = kind;
  const follow = useRef({ split, onCaret, onStatements });
  follow.current = { split, onCaret, onStatements };

  return (
    <Editor
      path={`console-${tabId}.sql`}
      defaultValue={initialSql}
      // Read-only (DDL): the text follows initialSql, so a reload shows the new text
      value={readOnly ? initialSql : undefined}
      defaultLanguage={language}
      theme={MONACO_THEME}
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
        // Gutter marks of executed statements (consoles only)
        glyphMargin: !readOnly,
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
        // The status bar's Ln, Col (and the selection's length), shown while focus is in this editor (its find bar too)
        const owner = ed.getContainerDomNode();
        const caret = () => {
          const pos = ed.getPosition();
          const sel = ed.getSelection();
          const m = ed.getModel();
          if (!pos || !m) return;
          const n = sel && !sel.isEmpty() ? m.getValueInRange(sel).length : 0;
          setFocusWidgets(owner, { pos: caretText(pos.lineNumber, pos.column, n) });
        };
        caret();
        ed.onDidChangeCursorSelection(caret);
        const unbalanced = !readOnly ? markUnbalanced(ed, () => kindRef.current) : undefined;
        const deco = ed.createDecorationsCollection();
        const preview = ed.createDecorationsCollection();
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
        // ── Gutter marks (DataGrip): each run statement's range is tracked through edits (decorations), its mark sits on its first line.
        // Editing its text drops it; once a statement of a run starts, the marks of earlier runs go (the gutter shows the latest run only).
        // `m` is unset until it starts
        type Mark = { id: string; text: string; m?: StmtMark };
        let marks: Mark[] = [];
        const glyphs = ed.createDecorationsCollection();
        const rangeOf = (x: Mark) => ed.getModel()?.getDecorationRange(x.id) ?? null;
        const drop = (gone: Mark[]) => {
          ed.getModel()?.deltaDecorations(gone.map((x) => x.id), []);
          marks = marks.filter((x) => !gone.includes(x));
        };
        const drawMarks = () =>
          glyphs.set(
            marks.flatMap((x) => {
              const r = rangeOf(x);
              if (!x.m || !r) return [];
              return [{ range: new monaco.Range(r.startLineNumber, 1, r.startLineNumber, 1), options: { glyphMarginClassName: `stmt-mark ${x.m.state}`, glyphMarginHoverMessage: { value: markHover(x.m) } } }];
            }),
          );
        const begin = (ranges: { start: number; end: number }[]): MarkStmt | undefined => {
          const m = ed.getModel();
          if (!m) return;
          highlight(ranges);
          // The next run: the last one's error mark goes
          setRunError(m, null);
          // Statements of an earlier press that never started (cancelled, or after an error) aren't kept
          drop(marks.filter((x) => !x.m));
          const ids = m.deltaDecorations(
            [],
            ranges.map((r) => {
              const s = m.getPositionAt(r.start);
              const e = m.getPositionAt(r.end);
              return { range: new monaco.Range(s.lineNumber, s.column, e.lineNumber, e.column), options: { stickiness: monaco.editor.TrackedRangeStickiness.NeverGrowsWhenTypingAtEdges } };
            }),
          );
          const run: Mark[] = ids.map((id) => ({ id, text: m.getValueInRange(m.getDecorationRange(id)!) }));
          marks.push(...run);
          return (i, s) => {
            const x = run[i];
            // Dropped meanwhile (its text was edited), or the editor is gone
            if (!x || !marks.includes(x) || !ed.getModel()) return;
            drop(marks.filter((o) => !run.includes(o)));
            x.m = s;
            drawMarks();
            // A failed statement: its error in the editor too, where the server points (else over the statement)
            const r = rangeOf(x);
            const model = ed.getModel()!;
            if (s.state === "error" && r) {
              const start = model.getOffsetAt(r.getStartPosition());
              const message = `${s.text ?? "Failed"}\n\nFix the statement and run it again (Output keeps the error).`;
              setRunError(model, { start, end: model.getOffsetAt(r.getEndPosition()), at: s.at === undefined ? undefined : start + s.at, message });
            }
          };
        };
        // Keyword case as you type (Settings › Appearance › Keyword case): a space, newline or punctuation typed after a keyword
        // re-cases it, outside strings, comments and quoted names; in the keystroke's undo step
        ed.onDidChangeModelContent((e) => {
          const m = ed.getModel();
          const c = e.changes[0];
          if (!m || e.isUndoing || e.isRedoing || e.isFlush || e.changes.length !== 1 || c.rangeLength !== 0 || !TYPED_END.test(c.text)) return;
          if (keywordCase() === "off" || !sharedReady()) return;
          // Not while Monaco dispatches the change: right after it, if nothing changed meanwhile
          const version = m.getVersionId();
          queueMicrotask(() => {
            if (ed.getModel() !== m || m.getVersionId() !== version) return;
            const r = shared().keywordCaseAt(m.getValue(), c.rangeOffset, kindRef.current, keywordCase());
            if (r) ed.executeEdits("keywordCase", [{ range: monaco.Range.fromPositions(m.getPositionAt(r.start), m.getPositionAt(c.rangeOffset)), text: r.text }]);
          });
        });
        ed.onDidChangeModelContent(() => {
          const m = ed.getModel();
          // An edit clears the run's error mark (a gutter mark stays until its own statement is edited)
          if (m) setRunError(m, null);
          if (!m || !marks.length) return;
          drop(
            marks.filter((x) => {
              const r = rangeOf(x);
              return !r || m.getValueInRange(r) !== x.text;
            }),
          );
          drawMarks();
        });
        // ── Result tabs follow the statements (#151): the script is split once per text version (on a change, never on a caret move)
        let stmts: { version: number; segs: { start: number; end: number }[]; texts: string[] } | null = null;
        const statements = () => {
          const m = ed.getModel();
          const fn = follow.current.split;
          if (!m || !fn || !sharedReady()) return null;
          if (stmts?.version !== m.getVersionId()) {
            const text = m.getValue();
            const segs = fn(text).map((s) => {
              const raw = text.slice(s.start, s.end);
              return { start: s.start + raw.length - raw.trimStart().length, end: s.end };
            });
            stmts = { version: m.getVersionId(), segs, texts: segs.map((s) => text.slice(s.start, s.end).trim()) };
          }
          return stmts;
        };
        statements();
        ed.onDidChangeModelContent(() => {
          const before = stmts?.texts;
          const after = statements()?.texts;
          if (before && after && (before.length !== after.length || before.some((t, i) => t !== after[i]))) follow.current.onStatements?.(before, after);
        });
        ed.onDidChangeCursorPosition((e) => {
          const m = ed.getModel();
          const st = follow.current.onCaret && statements();
          if (!m || !st) return;
          const at = m.getOffsetAt(e.position);
          const i = st.segs.findIndex((s) => s.start <= at && at <= s.end);
          follow.current.onCaret?.(i < 0 ? null : i);
        });
        ed.onMouseDown((e) => {
          if (e.target.type !== monaco.editor.MouseTargetType.GUTTER_GLYPH_MARGIN) return;
          const line = e.target.position?.lineNumber;
          const m = shownMark(marks.flatMap((x) => (x.m && rangeOf(x)?.startLineNumber === line ? [x.m] : [])));
          if (m) onMarkRef.current?.(m.out);
        });
        const run = (explain?: "plan" | "analyze") => {
          const m = ed.getModel();
          if (!m) return;
          // The statements as of this run, so an edit after it moves its result tabs (the module may have loaded since the last split)
          statements();
          const sel = ed.getSelection();
          const selection =
            sel && !sel.isEmpty() ? { text: m.getValueInRange(sel), start: m.getOffsetAt(sel.getStartPosition()), end: m.getOffsetAt(sel.getEndPosition()) } : null;
          // The offsets are into the text at the press: after an edit (a run waiting for the module, the Statements chooser) they'd mark other text
          const version = m.getAlternativeVersionId();
          const begun = (ranges: { start: number; end: number }[]) => (ed.getModel() === m && m.getAlternativeVersionId() === version ? begin(ranges) : undefined);
          onRunRef.current({ text: m.getValue(), offset: m.getOffsetAt(ed.getPosition()!), selection, begin: begun, explain });
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
          // The selection, or the statement at the caret (qurry_shared::format::reformat, as the TUI's ⌥⇧F): one undo step
          act("dbtool.reformat", "Reformat Code", "editor.reformat", () => {
            const m = ed.getModel();
            const sel = ed.getSelection();
            if (!m || !sel || !sharedReady()) return;
            const at = (p: monaco.IPosition) => m.getOffsetAt(p);
            const r = shared().reformat(m.getValue(), at(sel.getStartPosition()), at(sel.getEndPosition()), at(sel.getPosition()), kindRef.current, keywordCase());
            if (!r) return;
            ed.pushUndoStop();
            ed.executeEdits("reformat", [{ range: monaco.Range.fromPositions(m.getPositionAt(r.start), m.getPositionAt(r.end)), text: r.text }]);
            ed.pushUndoStop();
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
          // An underlined unknown name: its closest names (lib/completion.ts code actions)
          act("dbtool.quickFix", "Show Quick Fixes", "editor.quickFix", () => ed.trigger("k", "editor.action.quickFix", null));
        };
        register();
        const offKeymap = onKeymapChange(register);
        // addAction's commands·shortcuts·menus stay in the global registry, so dispose them manually when the editor closes
        ed.onDidDispose(() => {
          offKeymap();
          unbalanced?.dispose();
          setFocusWidgets(owner, undefined);
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
          preview: (ranges) => {
            const m = ed.getModel();
            if (!m) return;
            const range = (r: { start: number; end: number }) => monaco.Range.fromPositions(m.getPositionAt(r.start), m.getPositionAt(r.end));
            preview.set(ranges.map((r) => ({ range: range(r), options: { className: "stmt-preview" } })));
            if (ranges.length) ed.revealRangeInCenterIfOutsideViewport(range(ranges[0]).collapseToStart());
          },
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
