import { describe, expect, it } from "vitest";
import { generateValue } from "./generate-value";

describe("generateValue", () => {
  it("skips biased bytes (>= 248) and maps the rest with modulo 62", () => {
    const feed = [255, 248, 0, 61, 62, 247];
    let i = 0;
    const fill = (b: Uint8Array) => {
      for (let k = 0; k < b.length; k++) b[k] = feed[i++ % feed.length];
    };
    // accepted: 0->A, 61->9, 62->62%62=0->A, 247->247%62=61->9
    expect(generateValue(fill, 4)).toBe("A9A9");
  });
});
