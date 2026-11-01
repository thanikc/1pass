import { describe, expect, it } from "vitest";
import { errorMessage } from "./errors";

describe("errorMessage", () => {
  it("maps WrongPassword", () => {
    expect(errorMessage("WrongPassword")).toBe("Wrong password.");
  });

  it("includes the reference of EntryNotFound", () => {
    expect(errorMessage({ EntryNotFound: "op://a/b/c" })).toContain("op://a/b/c");
  });

  it("never stringifies unknown input", () => {
    for (const e of [{ secret: "hunter2" }, 42, null, "Nope"]) {
      expect(errorMessage(e)).toBe("Something went wrong.");
    }
  });
});
