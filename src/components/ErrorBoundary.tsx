// A render that throws unmounts everything up to the nearest error boundary (#181): the window has one at its root, each editor
// tab and Settings page its own, so one failing view leaves the rest of the window working. Errors go to qurry.log
import { Component, Fragment, useEffect, type ErrorInfo, type ReactNode } from "react";
import { api } from "../lib/api";
import { Button, CodeBlock, EmptyState, Row } from "../ui";

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
const RELOAD: Record<Scope, string> = { window: "Reload Window", tab: "Reload Tab", page: "Reload Page" };

/** What Copy Details puts on the clipboard */
export const errorDetails = (error: unknown, componentStack?: string) => {
  const d = summary(error);
  return [d.message, d.stack, componentStack && `Components:${componentStack}`].filter(Boolean).join("\n\n");
};

/** A boundary's fallback: what failed, Reload and Copy Details. The window's also reloads on ⌘r (its keymap is gone with the app) */
export function ErrorFallback({ scope, error, componentStack, onReload }: { scope: Scope; error: unknown; componentStack?: string; onReload: () => void }) {
  useEffect(() => {
    if (scope !== "window") return;
    const onKey = (e: KeyboardEvent) => (e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "r" && (e.preventDefault(), onReload());
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [scope, onReload]);
  return (
    <div className={`error-fallback ${scope}`} role="alert">
      <EmptyState
        title={TITLE[scope]}
        action={
          <Row gap={2} style={{ justifyContent: "center" }}>
            <Button variant="primary" onClick={onReload} title={scope === "window" ? "Reload Window (⌘r)" : undefined}>{RELOAD[scope]}</Button>
            <Button onClick={() => navigator.clipboard?.writeText(errorDetails(error, componentStack)).catch(() => {})}>Copy Details</Button>
          </Row>
        }
      >
        <CodeBlock className="error-message">{summary(error).message}</CodeBlock>
      </EmptyState>
    </div>
  );
}

interface State {
  error: unknown;
  failed: boolean;
  componentStack?: string;
  /** Bumped by Reload: the children mount again from scratch */
  rev: number;
}

/** Catches a render error below it and shows ErrorFallback instead; Reload remounts the children (the window's reloads the page).
 *  `onReload`: first drops what the view keeps outside itself that may be what failed (a console's results) */
export class ErrorBoundary extends Component<{ scope: Scope; onReload?: () => void; children: ReactNode }, State> {
  state: State = { error: null, failed: false, rev: 0 };

  static getDerivedStateFromError(error: unknown): Partial<State> {
    return { error, failed: true, componentStack: undefined };
  }

  componentDidCatch(error: unknown, info: ErrorInfo) {
    this.setState({ componentStack: info.componentStack ?? undefined });
    const d = summary(error);
    logFrontendError("boundary", `${this.props.scope}: ${d.message}`, d.stack, info.componentStack ?? undefined);
  }

  reload = () => {
    if (this.props.scope === "window") return location.reload();
    this.props.onReload?.();
    this.setState((s) => ({ error: null, failed: false, componentStack: undefined, rev: s.rev + 1 }));
  };

  render() {
    if (this.state.failed) return <ErrorFallback scope={this.props.scope} error={this.state.error} componentStack={this.state.componentStack} onReload={this.reload} />;
    return <Fragment key={this.state.rev}>{this.props.children}</Fragment>;
  }
}
