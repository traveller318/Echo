/**
 * SOURCE OF TRUTH KEYWORDS: format test, formatDuration test, formatClock test, formatBytes test, formatTakeTime test, metric unit format test
 * WHAT:  Verifies every formatter's output for typical, boundary and missing values in a fixed locale.
 * WHY:   The dashboard, history, models and settings all read numbers through lib/format.ts; a wrong rounding or
 *        unit here shows up everywhere at once (and negative time saved must read as negative).
 * WHERE: Runs in the `web` Vitest project.
 */
import { describe, expect, it } from "vitest";
import {
  formatBytes,
  formatClock,
  formatCount,
  formatDays,
  formatDuration,
  formatMetricValue,
  formatMilliseconds,
  formatMinutes,
  formatSeconds,
  formatSettingInt,
  formatTakeTime,
  formatWords,
  formatWpm,
  MISSING_VALUE,
} from "./format";

const LOCALE = "en-US";

describe("formatCount", () => {
  it("groups thousands and rounds to a whole number", () => {
    expect(formatCount(12345.6, LOCALE)).toBe("12,346");
    expect(formatCount(0, LOCALE)).toBe("0");
  });
});

describe("formatDuration", () => {
  it("shows the two largest non-zero units", () => {
    expect(formatDuration(7_500_000, LOCALE)).toBe("2 h 5 min");
    expect(formatDuration(192_000, LOCALE)).toBe("3 min 12 s");
    expect(formatDuration(45_000, LOCALE)).toBe("45 s");
    expect(formatDuration(3_605_000, LOCALE)).toBe("1 h");
  });

  it("rounds to whole seconds and reads zero as 0 s", () => {
    expect(formatDuration(0, LOCALE)).toBe("0 s");
    expect(formatDuration(400, LOCALE)).toBe("0 s");
    expect(formatDuration(1_600, LOCALE)).toBe("2 s");
  });

  it("marks negative time saved with a minus sign, but never a negative zero", () => {
    expect(formatDuration(-125_000, LOCALE)).toBe("−2 min 5 s");
    expect(formatDuration(-300, LOCALE)).toBe("0 s");
  });
});

describe("formatClock", () => {
  it("shows m:ss, then h:mm:ss past an hour", () => {
    expect(formatClock(7_000)).toBe("0:07");
    expect(formatClock(765_000)).toBe("12:45");
    expect(formatClock(3_723_000)).toBe("1:02:03");
  });

  it("never shows a negative or partial second", () => {
    expect(formatClock(-50)).toBe("0:00");
    expect(formatClock(59_999)).toBe("0:59");
  });
});

describe("unit formatters", () => {
  it("label wpm, ms, seconds, minutes and days", () => {
    expect(formatWpm(142.4, LOCALE)).toBe("142 wpm");
    expect(formatMilliseconds(1234, LOCALE)).toBe("1,234 ms");
    expect(formatSeconds(3_000, LOCALE)).toBe("3 s");
    expect(formatSeconds(3_500, LOCALE)).toBe("3.5 s");
    expect(formatMinutes(15, LOCALE)).toBe("15 min");
    expect(formatDays(1, LOCALE)).toBe("1 day");
    expect(formatDays(7, LOCALE)).toBe("7 days");
  });
});

describe("formatBytes", () => {
  it("uses binary units with Windows labels and one decimal below 10", () => {
    expect(formatBytes(1, LOCALE)).toBe("1 byte");
    expect(formatBytes(512, LOCALE)).toBe("512 bytes");
    expect(formatBytes(1536, LOCALE)).toBe("1.5 KB");
    expect(formatBytes(702_545_920, LOCALE)).toBe("670 MB");
    expect(formatBytes(1.4 * 1024 ** 3, LOCALE)).toBe("1.4 GB");
  });

  it("treats a negative size as empty", () => {
    expect(formatBytes(-5, LOCALE)).toBe("0 bytes");
  });
});

describe("formatMetricValue", () => {
  it("formats by the registry unit and shows an em dash when there is no value", () => {
    expect(formatMetricValue("duration", 7_500_000, LOCALE)).toBe("2 h 5 min");
    expect(formatMetricValue("count", 1200, LOCALE)).toBe("1,200");
    expect(formatMetricValue("wpm", 150, LOCALE)).toBe("150 wpm");
    expect(formatMetricValue("ms", 184, LOCALE)).toBe("184 ms");
    expect(formatMetricValue("days", 3, LOCALE)).toBe("3 days");
    expect(formatMetricValue("count", null, LOCALE)).toBe(MISSING_VALUE);
  });
});

describe("formatSettingInt", () => {
  it("formats by the setting unit, and a unit-less int as a count", () => {
    expect(formatSettingInt("milliseconds", 3000, LOCALE)).toBe("3 s");
    expect(formatSettingInt("minutes", 15, LOCALE)).toBe("15 min");
    expect(formatSettingInt("days", 0, LOCALE)).toBe("0 days");
    expect(formatSettingInt("words_per_minute", 40, LOCALE)).toBe("40 wpm");
    expect(formatSettingInt(null, 2500, LOCALE)).toBe("2,500");
  });
});

describe("formatWords", () => {
  it("counts words with the right noun", () => {
    expect(formatWords(1, LOCALE)).toBe("1 word");
    expect(formatWords(1204, LOCALE)).toBe("1,204 words");
    expect(formatWords(0, LOCALE)).toBe("0 words");
  });
});

describe("formatTakeTime", () => {
  const now = new Date(2026, 8, 25, 18, 30).getTime();

  it("shows only the time for a take from today", () => {
    expect(formatTakeTime(new Date(2026, 8, 25, 14, 5).getTime(), now, LOCALE)).toBe("2:05 PM");
  });

  it("adds the day this year and the year before that", () => {
    expect(formatTakeTime(new Date(2026, 8, 12, 9, 0).getTime(), now, LOCALE)).toBe("Sep 12, 9:00 AM");
    expect(formatTakeTime(new Date(2025, 11, 31, 23, 59).getTime(), now, LOCALE)).toBe("Dec 31, 2025, 11:59 PM");
  });
});
