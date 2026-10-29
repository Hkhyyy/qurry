import { describe, expect, it } from "vitest";
import { isReleaseUrl } from "./api";

describe("isReleaseUrl", () => {
  it("the update dialog only opens this repository's release URLs (even if the cache in the config folder changed)", () => {
    expect(isReleaseUrl("https://github.com/Hkhyyy/qurry/releases/tag/v0.1.17")).toBe(true);
    expect(isReleaseUrl("https://github.com/Hkhyyy/qurry/releases/download/v0.1.17/Qurry_0.1.17_aarch64.dmg")).toBe(true);
    expect(isReleaseUrl("https://github.com/Hkhyyy/dbtool/releases/tag/v0.1.15")).toBe(true);
    expect(isReleaseUrl("https://evil.example/Qurry.dmg")).toBe(false);
    expect(isReleaseUrl("https://github.com/Hkhyyy/qurry.evil/releases/x")).toBe(false);
    expect(isReleaseUrl("http://github.com/Hkhyyy/qurry/releases/x")).toBe(false);
    expect(isReleaseUrl("javascript:alert(1)")).toBe(false);
  });
});
