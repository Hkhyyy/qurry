import type { Meta, StoryObj } from "@storybook/react-vite";
import { useEffect, useState, type ReactNode } from "react";
import { IconButton, ProgressBar, Spacer, Text, ToolButton, Toolbar, ToolbarSep } from "../../index";
import { IcClock, IcGear, IcMinus, IcPlus, IcPreview, IcReload, IcRevert, IcStop, IcSubmit } from "../../icons";

// Agreed design for 0.3.2, #107: run / stop as one toggling button, information-only status bar. Chosen: "6. Chosen" / the "Chosen" story.
// Not product code: replace it with the real components when the issue is built, then delete this story.
const meta: Meta = { title: "Design/0.3.2/Run and stop buttons (#107)", parameters: { layout: "fullscreen" } };
export default meta;
type S = StoryObj;

const noop = () => {};

function useElapsed() {
  const [t0] = useState(() => Date.now());
  const [now, setNow] = useState(t0);
  useEffect(() => {
    const h = setInterval(() => setNow(Date.now()), 100);
    return () => clearInterval(h);
  }, []);
  return ((now - t0) / 1000).toFixed(1) + " s";
}

/** How a variant places run / stop */
interface V {
  key: string;
  title: string;
  note: string;
  /** console toolbar: "both" = ▶ and ■, "run" = ▶ only, "toggle" = ▶ turns into ■ while running */
  consoleBar: "both" | "run" | "toggle";
  /** grid toolbar: "both" = ↻ and ■, "reload" = ↻ only, "toggle" = ↻ turns into ■ while running */
  gridBar: "both" | "reload" | "toggle";
  /** status bar: "buttons" = ▶ ■ before the indicator, "stop" = a ■ glued to the indicator, "none" = no buttons, "hoverX" = ✕ on hover */
  status: "buttons" | "stop" | "none" | "hoverX";
}

const VARIANTS: V[] = [
  { key: "today", title: "1. Today", note: "▶ ■ in the toolbars and again in the status bar", consoleBar: "both", gridBar: "both", status: "buttons" },
  { key: "toolbars", title: "2. Toolbars only", note: "▶ ■ / ↻ ■ next to what they act on; the status bar only shows information", consoleBar: "both", gridBar: "both", status: "none" },
  { key: "toggle", title: "3. One button that toggles", note: "▶ (or ↻) turns into ■ while it runs, in the same spot (VS Code / browsers); no separate ■", consoleBar: "toggle", gridBar: "toggle", status: "none" },
  { key: "status", title: "4. Stop only in the status bar", note: "toolbars keep ▶ / ↻ only; the one ■ sits on the running indicator, wherever you are", consoleBar: "run", gridBar: "reload", status: "stop" },
  { key: "pick", title: "6. Chosen: one button + ✕ on hover", note: "▶ / ↻ turn into ■ while running; hover the status bar indicator for a ✕", consoleBar: "toggle", gridBar: "toggle", status: "hoverX" },
  { key: "hover", title: "5. Toolbars + ✕ on the indicator", note: "as 2, plus a ✕ when you hover the running indicator", consoleBar: "both", gridBar: "both", status: "hoverX" },
];

function ConsoleBar({ running, mode, onRun = noop, onStop = noop }: { running: boolean; mode: V["consoleBar"]; onRun?: () => void; onStop?: () => void }) {
  const toggleStop = mode === "toggle" && running;
  return (
    <Toolbar className="console-toolbar">
      {toggleStop ? (
        <IconButton tone="danger" title="Cancel (⌘F2)" onClick={onStop}>
          <IcStop />
        </IconButton>
      ) : (
        <IconButton tone="ok" title="Execute (⌘↩)" disabled={running && mode !== "toggle"} onClick={onRun}>
          ▶
        </IconButton>
      )}
      <ToolButton onClick={noop}>Explain ▾</ToolButton>
      <IconButton title="Browse Query History" onClick={noop}>
        <IcClock />
      </IconButton>
      <IconButton title="Query Execution Settings" onClick={noop}>
        <IcGear />
      </IconButton>
      <ToolbarSep />
      <ToolButton onClick={noop}>Tx: Auto ▾</ToolButton>
      {mode === "both" && (
        <IconButton tone="danger" title="Cancel Running Statements (⌘F2)" disabled={!running} onClick={noop}>
          <IcStop />
        </IconButton>
      )}
    </Toolbar>
  );
}

function GridBar({ running, mode, onRun = noop, onStop = noop }: { running: boolean; mode: V["gridBar"]; onRun?: () => void; onStop?: () => void }) {
  const toggleStop = mode === "toggle" && running;
  return (
    <div className="grid-toolbar">
      {toggleStop ? (
        <IconButton size="sm" tone="danger" title="Cancel (⌘F2)" onClick={onStop}>
          <IcStop />
        </IconButton>
      ) : (
        <IconButton size="sm" title="Reload Page (⌘R)" disabled={running && mode !== "toggle"} onClick={onRun}>
          <IcReload />
        </IconButton>
      )}
      <IconButton size="sm" title="History" onClick={noop}>
        <IcClock />
      </IconButton>
      {mode === "both" && (
        <IconButton size="sm" tone="danger" title="Cancel (⌘F2)" disabled={!running} onClick={noop}>
          <IcStop />
        </IconButton>
      )}
      <ToolbarSep />
      {[IcPlus, IcMinus, IcRevert, IcPreview, IcSubmit].map((I, i) => (
        <IconButton key={i} size="sm" title="Edit" disabled onClick={noop}>
          <I />
        </IconButton>
      ))}
      <Text size="sm" className="muted">
        3000+ rows
      </Text>
    </div>
  );
}

function Indicator({ label, mode, onStop = noop }: { label: string; mode: V["status"]; onStop?: () => void }) {
  const t = useElapsed();
  const [hover, setHover] = useState(false);
  return (
    <span onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)} style={{ display: "inline-flex", alignItems: "center", gap: 8, padding: "0 6px" }} role="status">
      <ProgressBar pct={null} width={120} label={label} />
      <Text size="sm">
        {label}… <b className="running-time">{t}</b>
      </Text>
      {mode === "stop" && (
        <IconButton size="sm" tone="danger" title="Cancel (⌘F2)" onClick={noop}>
          <IcStop />
        </IconButton>
      )}
      {mode === "hoverX" && (
        <span title="Cancel (⌘F2)" onClick={onStop} style={{ visibility: hover ? "visible" : "hidden", cursor: "pointer", color: "var(--danger)", padding: "0 2px" }}>
          ✕
        </span>
      )}
    </span>
  );
}

function StatusBar({ mode, running, label, pos, done = "3000+ rows retrieved", onStop = noop }: { mode: V["status"]; running: boolean; label: string; pos: string; done?: string; onStop?: () => void }) {
  return (
    <div className="statusbar">
      {mode === "buttons" && (
        <>
          <IconButton size="sm" tone="ok" title="Execute" disabled={running} onClick={noop}>
            ▶
          </IconButton>
          <IconButton size="sm" tone="danger" title="Cancel" disabled={!running} onClick={noop}>
            ■
          </IconButton>
        </>
      )}
      {running ? (
        <Indicator label={label} mode={mode} onStop={onStop} />
      ) : (
        <Text truncate className="sb-text">
          {done}
        </Text>
      )}
      <Spacer />
      {mode !== "buttons" && (
        <>
          {pos.startsWith("Row") && <span style={{ padding: "0 6px" }}>cols 3–8 of 14</span>}
          <span style={{ padding: "0 6px" }}>{pos}</span>
        </>
      )}
      <span style={{ padding: "0 6px" }}>UTF-8</span>
    </div>
  );
}

function Editor() {
  return (
    <div style={{ flex: 1, minHeight: 0, padding: "8px 14px", fontFamily: "JetBrains Mono, Menlo, monospace", fontSize: 13, background: "var(--bg)" }}>
      <span style={{ color: "#cf8e6d" }}>select</span> * <span style={{ color: "#cf8e6d" }}>from</span> orders;
    </div>
  );
}

function Pane({ children }: { children: ReactNode }) {
  return <div style={{ display: "flex", flexDirection: "column", height: 150, border: "1px solid var(--border)", borderRadius: 6, overflow: "hidden" }}>{children}</div>;
}

function Board({ running }: { running: boolean }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 20, padding: 18 }}>
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 18 }}>
        <Text size="sm" className="muted">
          Console tab
        </Text>
        <Text size="sm" className="muted">
          Table tab
        </Text>
      </div>
      {VARIANTS.map((v) => (
        <div key={v.key} style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          <Text size="sm">
            <b>{v.title}</b> — {v.note}
          </Text>
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 18 }}>
            <Pane>
              <ConsoleBar running={running} mode={v.consoleBar} />
              <Editor />
              <StatusBar mode={v.status} running={running} label="Executing" pos="Ln 2, Col 11" />
            </Pane>
            <Pane>
              <GridBar running={running} mode={v.gridBar} />
              <div style={{ flex: 1, background: "var(--bg)" }} />
              <StatusBar mode={v.status} running={running} label="Loading more rows" pos="Row 3, Col 5" />
            </Pane>
          </div>
        </div>
      ))}
    </div>
  );
}

export const Running: S = { name: "Compare — while running", render: () => <Board running /> };
export const Idle: S = { name: "Compare — idle", render: () => <Board running={false} /> };
export const RunningLight: S = { name: "Compare — while running, light", globals: { theme: "light" }, render: () => <Board running /> };

/** Try it: click ▶ / ↻ to start (it stops by itself after 8 s), click ■ or hover the indicator and click ✕ to cancel */
function Live() {
  const [c, setC] = useState<"idle" | "run" | "done" | "cancel">("idle");
  const [g, setG] = useState<"idle" | "run" | "done" | "cancel">("idle");
  useEffect(() => {
    if (c !== "run") return;
    const h = setTimeout(() => setC("done"), 8000);
    return () => clearTimeout(h);
  }, [c]);
  useEffect(() => {
    if (g !== "run") return;
    const h = setTimeout(() => setG("done"), 8000);
    return () => clearTimeout(h);
  }, [g]);
  const text = (s: string, ok: string) => (s === "cancel" ? "Cancelled" : s === "done" ? ok : "Ready");
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 10, padding: 18 }}>
      <Text size="sm">
        <b>One button + ✕ on hover</b> — click ▶ / ↻ to start. While it runs the same button is ■; hover the status bar indicator for ✕. Both cancel.
      </Text>
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 18 }}>
        <Pane>
          <ConsoleBar running={c === "run"} mode="toggle" onRun={() => setC("run")} onStop={() => setC("cancel")} />
          <Editor />
          <StatusBar mode="hoverX" running={c === "run"} label="Executing" pos="Ln 2, Col 11" done={text(c, "1 statement executed in 8.0 s")} onStop={() => setC("cancel")} />
        </Pane>
        <Pane>
          <GridBar running={g === "run"} mode="toggle" onRun={() => setG("run")} onStop={() => setG("cancel")} />
          <div style={{ flex: 1, background: "var(--bg)" }} />
          <StatusBar mode="hoverX" running={g === "run"} label="Loading more rows" pos="Row 3, Col 5" done={text(g, "3500+ rows retrieved")} onStop={() => setG("cancel")} />
        </Pane>
      </div>
    </div>
  );
}
export const Chosen: S = { name: "Chosen — one button + ✕ on hover (try it)", render: () => <Live /> };
export const ChosenLight: S = { name: "Chosen — light", globals: { theme: "light" }, render: () => <Live /> };
