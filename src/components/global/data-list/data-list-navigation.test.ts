/**
 * SOURCE OF TRUTH KEYWORDS: data list navigation test, nextActiveIndex test, key table test, clampIndex test
 * WHAT:  Verifies the DataList key table: each navigation key's target, stopping at both ends, page moves of at
 *        least one row, and non-navigation keys and empty lists answering null.
 * WHY:   Keyboard navigation is a hard requirement of the main window (04 §7); the table is pure, so every edge is
 *        checked here without a DOM.
 * WHERE: Runs in the `web` Vitest project.
 */
import { describe, expect, it } from "vitest";
import { clampIndex, isNavigationKey, nextActiveIndex } from "./data-list-navigation";

describe("nextActiveIndex", () => {
  it("moves one row with the arrows and stops at the ends", () => {
    expect(nextActiveIndex("ArrowDown", 3, 10, 5)).toBe(4);
    expect(nextActiveIndex("ArrowUp", 3, 10, 5)).toBe(2);
    expect(nextActiveIndex("ArrowDown", 9, 10, 5)).toBe(9);
    expect(nextActiveIndex("ArrowUp", 0, 10, 5)).toBe(0);
  });

  it("jumps to the ends with Home and End", () => {
    expect(nextActiveIndex("Home", 7, 10, 5)).toBe(0);
    expect(nextActiveIndex("End", 2, 10, 5)).toBe(9);
  });

  it("moves a viewport of rows with the page keys, at least one", () => {
    expect(nextActiveIndex("PageDown", 2, 100, 7.8)).toBe(9);
    expect(nextActiveIndex("PageUp", 9, 100, 7.8)).toBe(2);
    expect(nextActiveIndex("PageDown", 98, 100, 7)).toBe(99);
    expect(nextActiveIndex("PageDown", 0, 100, 0)).toBe(1);
  });

  it("ignores other keys and empty lists", () => {
    expect(nextActiveIndex("Enter", 1, 10, 5)).toBeNull();
    expect(nextActiveIndex("a", 1, 10, 5)).toBeNull();
    expect(nextActiveIndex("ArrowDown", 0, 0, 5)).toBeNull();
    expect(isNavigationKey("Tab")).toBe(false);
    expect(isNavigationKey("End")).toBe(true);
  });
});

describe("clampIndex", () => {
  it("keeps an index inside the list", () => {
    expect(clampIndex(-3, 5)).toBe(0);
    expect(clampIndex(8, 5)).toBe(4);
    expect(clampIndex(2, 5)).toBe(2);
    expect(clampIndex(4, 0)).toBe(0);
  });
});
