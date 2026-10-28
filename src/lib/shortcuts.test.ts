import { describe, expect, it } from "vitest";
import { keyLabel, SHORTCUTS } from "./shortcuts";

describe("keyLabel", () => {
  it("Shift 없는 ⌘/⌃/⌥+글자는 소문자 (대문자 = Shift 가 필요한 키)", () => {
    expect(keyLabel("⌘K")).toBe("⌘k");
    expect(keyLabel("⌥⌘Y · ⌃⇧F · ⇧⌘A")).toBe("⌥⌘y · ⌃⇧F · ⇧⌘A");
    expect(keyLabel("Run (⌘↩) · ⌘F2 · ⌘[")).toBe("Run (⌘↩) · ⌘F2 · ⌘[");
    expect(keyLabel("Ctrl+K · Ctrl+Shift+K · Alt+Enter")).toBe("Ctrl+k · Ctrl+Shift+K · Alt+Enter");
    expect(keyLabel("⌘B / ⌘↓ · Delete")).toBe("⌘b / ⌘↓ · Delete");
  });

  it("소스의 화면 글에 대문자 단축키가 남아 있지 않다", () => {
    // shortcuts.ts 의 "Mod+Alt+O" 는 표시가 아니라 키 입력과 맞춰 보는 형식이라 뺀다 (mock 은 가짜 데이터)
    const files = import.meta.glob<string>(["../**/*.{ts,tsx}", "!../**/*.test.ts", "!./shortcuts.ts", "!./mock.ts"], { query: "?raw", import: "default", eager: true });
    expect(Object.keys(files).length).toBeGreaterThan(10);
    const bad: string[] = [];
    for (const [f, text] of Object.entries(files)) {
      text.split("\n").forEach((line, i) => {
        if (keyLabel(line) !== line) bad.push(`${f}:${i + 1}: ${line.trim().slice(0, 100)}`);
      });
    }
    expect(bad).toEqual([]);
  });
});

describe("global shortcuts", () => {
  it("워크스페이스는 ⌘1~9 (터미널이 아니니 ⌥ 가 아니다), 탐색기는 ⌘0, 겹치는 기본 키 없음", () => {
    const keys = (id: string) => SHORTCUTS.find((s) => s.id === id)?.keys;
    expect(keys("ws.1")).toEqual(["Mod+1"]);
    expect(keys("ws.9")).toEqual(["Mod+9"]);
    expect(keys("explorer")).toEqual(["Mod+0"]);
    const seen = new Map<string, string>();
    const dup: string[] = [];
    for (const s of SHORTCUTS.filter((x) => x.scope === "global"))
      for (const k of s.keys) {
        if (seen.has(k)) dup.push(`${k}: ${seen.get(k)} / ${s.id}`);
        seen.set(k, s.id);
      }
    expect(dup).toEqual([]);
  });
});
