import { describe, expect, it } from "vitest";
import { removedDespite } from "./api";

describe("removedDespite", () => {
  it("only the password-left-behind error means the data source is gone (its tabs close)", () => {
    expect(removedDespite("The data source was removed, but its saved password couldn't be: secrets.json is damaged")).toBe(true);
    expect(removedDespite(new Error("connection-access.json is damaged (EOF)"))).toBe(false);
    expect(removedDespite("Permission denied (os error 13)")).toBe(false);
  });
});
