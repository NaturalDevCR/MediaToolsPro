import { describe, expect, it } from "vitest";
import {
  clamp,
  formatDisplayTime,
  normalizeTrimBounds,
  parseTimeInput,
} from "./time";

describe("time utilities", () => {
  it("formats display times with optional hours and decimals", () => {
    expect(formatDisplayTime(5)).toBe("00:05");
    expect(formatDisplayTime(95.5)).toBe("01:35.5");
    expect(formatDisplayTime(3661)).toBe("01:01:01");
  });

  it("parses seconds and colon-separated time input", () => {
    expect(parseTimeInput("95.5")).toBe(95.5);
    expect(parseTimeInput("01:35.5")).toBe(95.5);
    expect(parseTimeInput("1:02:03")).toBe(3723);
    expect(parseTimeInput("not-time")).toBeNull();
    expect(parseTimeInput("")).toBeNull();
  });

  it("clamps numeric values", () => {
    expect(clamp(-1, 0, 10)).toBe(0);
    expect(clamp(12, 0, 10)).toBe(10);
    expect(clamp(5, 0, 10)).toBe(5);
  });

  it("keeps trim bounds inside duration with a minimum gap", () => {
    expect(normalizeTrimBounds(100, -5, 120)).toEqual({ start: 0, end: 100 });
    expect(normalizeTrimBounds(10, 8, 2, "end")).toEqual({ start: 8, end: 8.1 });
    expect(normalizeTrimBounds(10, 8, 8.02, "start")).toEqual({ start: 7.92, end: 8.02 });
    expect(normalizeTrimBounds(0, 8, 9)).toEqual({ start: 0, end: 0 });
  });
});
