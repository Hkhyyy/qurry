import { afterEach, describe, expect, it, vi } from "vitest";
import { ask, askText, registerTextDialog } from "./dialogs";

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
});
