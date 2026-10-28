import { afterEach, describe, expect, it, vi } from "vitest";
import { ask, askText, askTextChoice, registerTextDialog } from "./dialogs";

describe("dialogs", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    registerTextDialog(null);
  });
  it("Tauri 의 async confirm 도 대답을 기다린다 (Promise 를 참으로 보지 않는다)", async () => {
    vi.stubGlobal("window", { confirm: () => Promise.resolve(false) });
    expect(await ask("drop?")).toBe(false);
    vi.stubGlobal("window", { confirm: () => Promise.resolve(true) });
    expect(await ask("drop?")).toBe(true);
    vi.stubGlobal("window", { confirm: () => false });
    expect(await ask("drop?")).toBe(false);
  });
  it("글 입력은 등록된 앱 창으로 (Tauri 에는 prompt 가 없다)", async () => {
    registerTextDialog((r) => r.resolve(r.value + "!"));
    expect(await askText("Name", "a")).toBe("a!");
    registerTextDialog((r) => r.resolve(null));
    expect(await askText("Name")).toBeNull();
  });
  it("이름과 함께 고르기 (스니펫: 이 Data Source 만 / 전체) — OK·Cancel 로 고르지 않는다", async () => {
    registerTextDialog((r) => {
      expect(r.choices).toEqual(["Only this data source", "All data sources"]);
      expect(r.choice).toBe(0);
      r.resolve("users", 1);
    });
    expect(await askTextChoice("Snippet name", "sel", ["Only this data source", "All data sources"])).toEqual({ text: "users", choice: 1 });
    registerTextDialog((r) => r.resolve(null));
    expect(await askTextChoice("Snippet name", "sel", ["a", "b"])).toBeNull();
  });
});
