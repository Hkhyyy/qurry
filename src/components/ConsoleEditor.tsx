import Editor from "@monaco-editor/react";
import { useEffect, useRef } from "react";
import { format as formatSql } from "sql-formatter";
import { monaco } from "../lib/monaco";
import { bindModel, unbindModel } from "../lib/completion";
import { monacoTheme, useTheme } from "../lib/theme";
import { editorFontSize } from "../lib/prefs";
import { setStatusWidget } from "../lib/statusbar";
import { bindings, onKeymapChange } from "../lib/shortcuts";

/** 실행 요청: 에디터 내용·캐럿·선택 영역을 넘기고, 실제로 무엇을 실행할지는 콘솔이 정한다 (DataGrip 실행 규칙) */
export interface RunRequest {
  text: string;
  offset: number;
  selection: { text: string; start: number; end: number } | null;
  /** 실행할 범위를 에디터에 잠깐 표시 */
  highlight: (ranges: { start: number; end: number }[]) => void;
  /** Explain Plan / Explain Analyse */
  explain?: "plan" | "analyze";
}

export interface EditorApi {
  run: (explain?: "plan" | "analyze") => void;
  insert: (text: string) => void;
  focus: () => void;
  /** 골라 둔 글, 없으면 콘솔 전체 */
  text: () => string;
}

interface Props {
  tabId: string;
  connId: string;
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

/** lib/shortcuts 표기 → Monaco 키 (Mod = ⌘/Ctrl, Ctrl = macOS ⌃) */
function monacoKey(spec: string): number | null {
  const parts = spec.split("+");
  const key = parts.pop()!;
  const code = /^[A-Z]$/.test(key) ? (K as any)["Key" + key] : /^\d$/.test(key) ? (K as any)["Digit" + key] : /^F\d{1,2}$/.test(key) ? (K as any)[key] : MONACO_KEYS[key];
  if (code === undefined) return null;
  const mods: Record<string, number> = { Mod: M.CtrlCmd, Ctrl: M.WinCtrl, Alt: M.Alt, Shift: M.Shift };
  return parts.reduce((n, m) => n | (mods[m] ?? 0), code as number);
}
const keysOf = (id: string) => bindings(id).map(monacoKey).filter((n): n is number => n !== null);

export function ConsoleEditor({ tabId, connId, initialSql, language, onChange, onRun, registerApi, readOnly }: Props) {
  const modelUri = useRef<string | null>(null);
  useEffect(() => {
    if (modelUri.current) bindModel(modelUri.current, connId);
  }, [connId]);
  const theme = useTheme();
  const onRunRef = useRef(onRun);
  onRunRef.current = onRun;

  return (
    <Editor
      path={`console-${tabId}.sql`}
      defaultValue={initialSql}
      defaultLanguage={language}
      theme={monacoTheme(theme)}
      onChange={(v) => onChange(v ?? "")}
      options={{
        minimap: { enabled: false },
        fontSize: editorFontSize(),
        fontFamily: "'JetBrains Mono', Menlo, monospace",
        automaticLayout: true,
        scrollBeyondLastLine: false,
        renderLineHighlight: "all",
        tabSize: 2,
        fixedOverflowWidgets: true,
        readOnly,
        // Chromium 의 EditContext 대신 textarea 입력을 쓴다 (WebKit/Tauri 와 동작을 맞춤)
        editContext: false,
        quickSuggestions: { other: true, strings: false, comments: false },
        suggestOnTriggerCharacters: true,
      }}
      onMount={(ed) => {
        const model = ed.getModel();
        if (model) {
          modelUri.current = model.uri.toString();
          bindModel(modelUri.current, connId);
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
          onRunRef.current({ text: m.getValue(), offset: m.getOffsetAt(ed.getPosition()!), selection, highlight, explain });
        };
        // ── DataGrip 에디터 키맵 (키는 lib/shortcuts 의 editor.*, 바뀌면 다시 등록) ──
        let registered: monaco.IDisposable[] = [];
        const act = (id: string, label: string, shortcut: string, fn: () => void) =>
          registered.push(ed.addAction({ id, label, keybindings: keysOf(shortcut), run: fn }));
        const register = () => {
          registered.forEach((d) => d.dispose());
          registered = [];
          registered.push(ed.addAction({ id: "dbtool.run", label: "Execute", keybindings: keysOf("editor.run"), contextMenuGroupId: "navigation", contextMenuOrder: 0, run: () => run() }));
          registered.push(ed.addAction({ id: "dbtool.explain", label: "Explain Plan", contextMenuGroupId: "navigation", contextMenuOrder: 1, run: () => run("plan") }));
          registered.push(ed.addAction({ id: "dbtool.explainAnalyse", label: "Explain Analyse", contextMenuGroupId: "navigation", contextMenuOrder: 2, run: () => run("analyze") }));
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
              /* 파싱 못 하는 SQL 은 그대로 둔다 */
            }
          });
          act("dbtool.duplicate", "Duplicate Line or Selection", "editor.duplicate", () => ed.trigger("k", "editor.action.copyLinesDownAction", null));
          act("dbtool.deleteLine", "Delete Line", "editor.deleteLine", () => ed.trigger("k", "editor.action.deleteLines", null));
          act("dbtool.moveUp", "Move Line Up", "editor.moveUp", () => ed.trigger("k", "editor.action.moveLinesUpAction", null));
          act("dbtool.moveDown", "Move Line Down", "editor.moveDown", () => ed.trigger("k", "editor.action.moveLinesDownAction", null));
          act("dbtool.extend", "Extend Selection", "editor.extend", () => ed.trigger("k", "editor.action.smartSelect.expand", null));
          act("dbtool.shrink", "Shrink Selection", "editor.shrink", () => ed.trigger("k", "editor.action.smartSelect.shrink", null));
          act("dbtool.gotoLine", "Go to Line/Column", "editor.gotoLine", () => ed.trigger("k", "editor.action.gotoLine", null));
          // DataGrip ⌘r: 찾기/바꾸기 바 (정규식 .* · 대소문자 · 단어 토글은 Monaco 바에 있다)
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
        // addAction 의 명령·단축키·메뉴는 전역 레지스트리에 남으므로 에디터가 닫힐 때 직접 해제한다
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
