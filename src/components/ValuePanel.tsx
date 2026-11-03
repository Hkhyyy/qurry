import Editor from "@monaco-editor/react";
import { useEffect, useRef, useState } from "react";
import type { monaco } from "../lib/monaco";
import { fontStack, useFonts } from "../lib/prefs";
import { formatSpec, shortcutLabel } from "../lib/shortcuts";
import { monacoTheme, useTheme } from "../lib/theme";
import { IconButton, Notice, Spacer, TextArea, ToolButton } from "../ui";

const isMac = navigator.platform.toLowerCase().includes("mac");

interface Props {
  /** "column : type" of the grid's current cell; undefined when no cell is selected */
  title?: string;
  /** The text shown: the draft typed for this cell, else its value (JSON pretty-printed) */
  text: string;
  /** Highlight and fold as JSON (Monaco); plain text otherwise */
  json: boolean;
  /** The cell is NULL (shown as a placeholder: the text is empty) */
  isNull: boolean;
  readOnly: boolean;
  /** Why the last apply was refused (invalid JSON in a json / jsonb column) */
  error: string | null;
  /** Bumped to move the focus into the panel (⇧↩ in the grid) */
  focusSeq: number;
  onChange: (text: string) => void;
  /** "{ }": JSON between pretty and one line */
  onToggleLayout: () => void;
  /** ⌘↩ */
  onApply: () => void;
  /** ⎋: back to the grid, the panel stays */
  onBack: () => void;
  onClose: () => void;
}

/** Value Editor side panel of the result grid (DataGrip's value editor panel): the current cell's value, JSON highlighted with folding */
export function ValuePanel(p: Props) {
  const [wrap, setWrap] = useState(true);
  const theme = useTheme();
  const editorFont = useFonts().editor;
  const editor = useRef<monaco.editor.IStandaloneCodeEditor | null>(null);
  const area = useRef<HTMLTextAreaElement>(null);
  // A focus asked for before Monaco mounted is done in onMount
  const wantFocus = useRef(false);
  useEffect(() => {
    if (!p.focusSeq) return;
    if (p.json) {
      if (editor.current?.getModel()) editor.current.focus();
      else wantFocus.current = true;
    } else area.current?.focus();
  }, [p.focusSeq]);

  // Before Monaco or the text area sees the key: ⌘↩ applies, ⎋ goes back to the grid (Monaco's own find and suggest widgets keep their ⎋)
  const onKeyDownCapture = (e: React.KeyboardEvent) => {
    const mod = isMac ? e.metaKey : e.ctrlKey;
    const apply = mod && e.key === "Enter";
    const back = e.key === "Escape" && !e.shiftKey && !(e.target as HTMLElement).closest(".find-widget, .suggest-widget");
    if (!apply && !back) return;
    e.preventDefault();
    e.stopPropagation();
    if (apply) {
      if (!p.readOnly) p.onApply();
    } else p.onBack();
  };

  const closeKeys = `${shortcutLabel("tab.close")} / ${formatSpec("Shift+Esc")}`;
  return (
    <div className="value-panel" onKeyDownCapture={onKeyDownCapture}>
      <div className="ve-head">
        <span className="ve-title">{p.title ?? "Value Editor"}</span>
        <Spacer />
        <ToolButton active={wrap} title="Toggle Soft-Wrap" onClick={() => setWrap((w) => !w)}>Wrap</ToolButton>
        {p.json && <ToolButton title="Toggle Formatting (JSON)" onClick={p.onToggleLayout}>{"{ }"}</ToolButton>}
        <IconButton title={`Close (${closeKeys})`} onClick={p.onClose}>×</IconButton>
      </div>
      {p.title === undefined ? (
        <div className="ve-empty muted">Select a cell to see its value</div>
      ) : p.json ? (
        <div className="ve-monaco">
          <Editor
            language="json"
            value={p.text}
            theme={monacoTheme(theme)}
            onChange={(v) => p.onChange(v ?? "")}
            options={{
              readOnly: p.readOnly,
              wordWrap: wrap ? "on" : "off",
              minimap: { enabled: false },
              folding: true,
              lineNumbers: "on",
              lineNumbersMinChars: 3,
              glyphMargin: false,
              fontSize: editorFont.size,
              fontFamily: fontStack(editorFont.family),
              automaticLayout: true,
              scrollBeyondLastLine: false,
              renderLineHighlight: "none",
              fixedOverflowWidgets: true,
              tabSize: 2,
              // As the console: textarea input, not Chromium's EditContext (WebKit/Tauri behaves the same)
              editContext: false,
            }}
            onMount={(ed) => {
              editor.current = ed;
              if (wantFocus.current) ed.focus();
              wantFocus.current = false;
            }}
          />
        </div>
      ) : (
        <TextArea
          ref={area}
          readOnly={p.readOnly}
          className={"ve-text" + (wrap ? "" : " nowrap")}
          value={p.text}
          placeholder={p.isNull ? "<null>" : undefined}
          onChange={(e) => p.onChange(e.target.value)}
        />
      )}
      {p.error && <Notice tone="danger" className="ve-error">{p.error}</Notice>}
      <div className="ve-foot muted">
        {p.readOnly ? "Read-only" : `${formatSpec("Mod+Enter")} apply`} · {formatSpec("Esc")} back to the grid · {closeKeys} close
      </div>
    </div>
  );
}
