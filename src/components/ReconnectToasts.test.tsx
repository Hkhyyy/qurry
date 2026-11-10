import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { ReconnectToast } from "../lib/reconnect";
import { ReconnectToastView } from "./ReconnectToasts";

const noop = () => {};
const html = (t: ReconnectToast, now: number) => renderToStaticMarkup(<ReconnectToastView t={t} now={now} onCancel={noop} onRetry={noop} onClose={noop} />);
const waiting: ReconnectToast = { phase: "waiting", id: "prod", message: "Reconnecting prod through AWS SSM", slow: "Taking longer than usual", since: 0 };

describe("the reconnect toast (#135)", () => {
  it("while waiting: the text, the time and Cancel; past 10 s also why it may take a while", () => {
    const h = html(waiting, 3200);
    expect(h).toContain("Reconnecting prod through AWS SSM… <b class=\"running-time\">3.2 s</b>");
    expect(h).toContain(">Cancel</button>");
    expect(h).toContain('role="status"');
    expect(h).not.toContain("Taking longer");
    expect(h).not.toContain('title="Close"');
    expect(html(waiting, 14_000)).toContain("Taking longer than usual");
  });

  it("failed: an alert with the reason and Retry, and a ×; cancelled: what it cancelled and a ×", () => {
    const h = html({ phase: "failed", id: "prod", title: "Couldn't reconnect prod through AWS SSM", reason: "TargetNotConnected" }, 0);
    expect(h).toContain('role="alert"');
    expect(h).toContain("tone-danger");
    expect(h).toContain("TargetNotConnected");
    expect(h).toContain(">Retry</button>");
    expect(h).toContain('title="Close"');
    const c = html({ phase: "cancelled", id: "prod", text: "Cancelled while reconnecting prod through AWS SSM." }, 0);
    expect(c).toContain("Cancelled while reconnecting prod");
    expect(c).not.toContain("Retry");
    expect(c).toContain('title="Close"');
  });
});
