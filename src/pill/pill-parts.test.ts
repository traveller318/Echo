/**
 * SOURCE OF TRUTH KEYWORDS: waveform levels test, levelFromRms test, pushLevel test, PillHitAreaRegistry test, hit area dedupe test
 * WHAT:  Verifies the waveform's decibel mapping and smoothing, and that the hit-area registry reports whole-pixel
 *        rectangles covering each button, only when they change, and drops unmounted buttons.
 * WHY:   Bars that sit at the floor during speech, or a stop button Rust thinks is elsewhere (so clicks fall through to
 *        the app behind), would both look like a broken pill.
 * WHERE: Runs in the `web` Vitest project.
 */
import { describe, expect, it, vi } from "vitest";
import { levelFromRms, pushLevel, silentLevels, WAVEFORM_BARS } from "./_components";
import { PillHitAreaRegistry } from "./hit-areas";

describe("waveform levels", () => {
  it("maps RMS on a decibel scale with a -60 dB floor", () => {
    expect(levelFromRms(0)).toBe(0);
    expect(levelFromRms(Number.NaN)).toBe(0);
    expect(levelFromRms(0.001)).toBe(0);
    expect(levelFromRms(1)).toBe(1);
    expect(levelFromRms(0.1)).toBeCloseTo(2 / 3);
    expect(levelFromRms(4)).toBe(1);
  });

  it("scrolls in new levels with light smoothing and keeps the bar count", () => {
    const start = silentLevels();
    expect(start).toHaveLength(WAVEFORM_BARS);
    const once = pushLevel(start, 1);
    expect(once).toHaveLength(WAVEFORM_BARS);
    expect(once.at(-1)).toBeCloseTo(0.65);
    const twice = pushLevel(once, 1);
    expect(twice.at(-1)).toBeGreaterThan(0.65);
    expect(twice.at(-2)).toBeCloseTo(0.65);
  });
});

function button(rect: { left: number; top: number; right: number; bottom: number }): Element {
  const element = document.createElement("button");
  element.getBoundingClientRect = () =>
    DOMRect.fromRect({ x: rect.left, y: rect.top, width: rect.right - rect.left, height: rect.bottom - rect.top });
  return element;
}

describe("PillHitAreaRegistry", () => {
  it("reports whole-pixel areas that cover each button, only on change", () => {
    const report = vi.fn();
    const registry = new PillHitAreaRegistry(report);
    const stop = button({ left: 150.4, top: 28.6, right: 178.2, bottom: 56.1 });
    const unregister = registry.register(stop);
    expect(report).toHaveBeenLastCalledWith([{ x: 150, y: 28, width: 29, height: 29 }]);
    registry.measure();
    expect(report).toHaveBeenCalledTimes(1);
    unregister();
    expect(report).toHaveBeenLastCalledWith([]);
    registry.dispose();
  });
});
