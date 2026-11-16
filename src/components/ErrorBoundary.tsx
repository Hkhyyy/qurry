// A render that throws unmounts everything up to the nearest error boundary (#181): the window has one at its root, each editor
// tab and Settings page its own, so one failing view leaves the rest of the window working. Errors go to qurry.log.
// The window's boundary mounts the app again (Recover): tabs, console text and open transactions are kept (features/recovery)
import { Component, Fragment, useEffect, useState, type ErrorInfo, type ReactNode } from "react";
import { api } from "../lib/api";
import { recoveryNotice, reloadWindow, uncommitted } from "../features/recovery";
import { Button, CodeBlock, EmptyState, Notice, Row } from "../ui";

type Source = "boundary" | "error" | "unhandledrejection";

/** Into qurry.log: the message and stacks only (never query text or cell values); a log that can't be written is no new error */
export function logFrontendError(source: Source, message: string, stack?: string, componentStack?: string) {
  try {
    api.logFrontendError({ source, message, stack, componentStack }).catch(() => {});
  } catch {
    // No backend (Storybook, unit tests)
  }
}

const summary = (x: unknown) => (x instanceof Error ? { message: `${x.name}: ${x.message}`, stack: x.stack } : { message: String(x) });

/** window `error` and `unhandledrejection` into the log, for as long as the page lives (the status bar shows them too, in App) */
export function logWindowErrors() {
  window.addEventListener("error", (e) => {
    // "ResizeObserver loop …" only says a resize was put off to the next frame: harmless
    if (e.message?.includes("ResizeObserver loop")) return;
    logFrontendError("error", e.error instanceof Error ? summary(e.error).message : e.message, e.error?.stack ?? (e.filename ? `${e.filename}:${e.lineno}:${e.colno}` : undefined));
  });
  window.addEventListener("unhandledrejection", (e) => {
    const d = summary(e.reason);
    logFrontendError("unhandledrejection", d.message, d.stack);
  });
}

export type Scope = "window" | "tab" | "page";

const TITLE: Record<Scope, string> = { window: "Qurry hit an error", tab: "This tab hit an error", page: "This page hit an error" };
const RELOAD: Record<Scope, string> = { window: "Recover", tab: "Reload Tab", page: "Reload Page" };
/** A remount that fails again within this shows the fallback (no loop); the window's then also offers Reload Window */
export const AGAIN_MS = 10_000;

/** What Copy Details puts on the clipboard */
export const errorDetails = (error: unknown, componentStack?: string) => {
  const d = summary(error);
  return [d.message, d.stack, componentStack && `Components:${componentStack}`].filter(Boolean).join("\n\n");
};

/** "1 console has uncommitted changes. Reload rolls them back." */
export const rollbackAsk = (n: number) => `${n} console${n === 1 ? " has" : "s have"} uncommitted changes. Reload rolls them back.`;

/**
 * A boundary's fallback: what failed, Reload (the window's: Recover) and Copy Details. The window's also offers Reload Window once
 * a Recover failed again (`again`), asking first when consoles have uncommitted changes; ⌘r does the first button (the app's
 * keymap is gone with it)
 */
export function ErrorFallback({ scope, error, componentStack, onReload, again, uncommitted = 0, onReloadWindow }: {
  scope: Scope;
  error: unknown;
  componentStack?: string;
  onReload: () => void;
  again?: boolean;
  uncommitted?: number;
  onReloadWindow?: () => void;
}) {
  const [asking, setAsking] = useState(false);
  const offerReload = scope === "window" && again && !!onReloadWindow;
  const reloadWindow = () => (uncommitted && !asking ? setAsking(true) : onReloadWindow?.());
  const first = offerReload ? reloadWindow : onReload;
  useEffect(() => {
    if (scope !== "window") return;
    const onKey = (e: KeyboardEvent) => (e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "r" && (e.preventDefault(), first());
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [scope, first]);
  const copy = <Button onClick={() => navigator.clipboard?.writeText(errorDetails(error, componentStack)).catch(() => {})}>Copy Details</Button>;
  return (
    <div className={`error-fallback ${scope}`} role="alert">
      <EmptyState
        title={TITLE[scope]}
        action={
          asking ? (
            <Row gap={2} style={{ justifyContent: "center" }}>
              <Button variant="danger" onClick={onReloadWindow}>Reload and Roll Back</Button>
              <Button onClick={() => setAsking(false)}>Cancel</Button>
            </Row>
          ) : (
            <Row gap={2} style={{ justifyContent: "center" }}>
              {offerReload && <Button variant="primary" onClick={reloadWindow} title="Reload Window (⌘r)">Reload Window</Button>}
              <Button variant={offerReload ? "default" : "primary"} onClick={onReload} title={scope === "window" && !offerReload ? "Recover (⌘r)" : undefined}>{RELOAD[scope]}</Button>
              {copy}
            </Row>
          )
        }
      >
        <CodeBlock className="error-message">{summary(error).message}</CodeBlock>
        {asking && <Notice tone="warn">{rollbackAsk(uncommitted)}</Notice>}
      </EmptyState>
    </div>
  );
}

interface State {
  error: unknown;
  failed: boolean;
  componentStack?: string;
  /** Bumped by Reload / Recover: the children mount again from scratch */
  rev: number;
  /** When the children were last mounted again (the window's: a Recover, or the automatic one) */
  remounted?: number;
  /** This error came within AGAIN_MS of that */
  again?: boolean;
}

/** Catches a render error below it and mounts its children again; when that fails again within AGAIN_MS (or for a Settings page,
 *  at once) shows ErrorFallback instead, whose Reload / Recover remounts them. A toast says it recovered (`label`: the tab's name).
 *  `onReload`: first drops what the view keeps outside itself that may be what failed (a console's results) */
export class ErrorBoundary extends Component<{ scope: Scope; label?: string; onReload?: () => void; children: ReactNode }, State> {
  state: State = { error: null, failed: false, rev: 0 };

  static getDerivedStateFromError(error: unknown): Partial<State> {
    return { error, failed: true, componentStack: undefined };
  }

  componentDidCatch(error: unknown, info: ErrorInfo) {
    const d = summary(error);
    logFrontendError("boundary", `${this.props.scope}: ${d.message}`, d.stack, info.componentStack ?? undefined);
    const again = this.state.remounted !== undefined && Date.now() - this.state.remounted < AGAIN_MS;
    // The window and a tab mount again by themselves, once: the fallback only when that fails again soon after
    if (this.props.scope !== "page" && !again) {
      recoveryNotice(`Recovered from an error${this.props.label ? ` in ${this.props.label}` : ""} — details in the log`);
      return this.reload();
    }
    this.setState({ componentStack: info.componentStack ?? undefined, again });
  }

  reload = () => {
    this.props.onReload?.();
    this.setState((s) => ({ error: null, failed: false, componentStack: undefined, rev: s.rev + 1, remounted: Date.now(), again: false }));
  };

  render() {
    if (this.state.failed) {
      const win = this.props.scope === "window";
      return (
        <ErrorFallback
          scope={this.props.scope}
          error={this.state.error}
          componentStack={this.state.componentStack}
          onReload={this.reload}
          again={win && this.state.again}
          uncommitted={win ? uncommitted() : 0}
          onReloadWindow={win ? () => void reloadWindow() : undefined}
        />
      );
    }
    return <Fragment key={this.state.rev}>{this.props.children}</Fragment>;
  }
}
