/**
 * SOURCE OF TRUTH KEYWORDS: ProgressBar test, determinate progress test, indeterminate progress test, progressFraction test
 * WHAT:  Verifies the determinate bar reports its value and fills by the clamped fraction, the indeterminate bar
 *        drops aria-valuenow and uses the sweep, and progressFraction clamps bad input.
 * WHY:   Model downloads stream bytes at up to 10 Hz (02 §4.4); an unclamped or NaN fraction would overdraw the
 *        bar or break the progressbar semantics screen readers announce.
 * WHERE: Runs in the `web` Vitest project (jsdom).
 */
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ProgressBar } from "./ProgressBar";
import { progressFraction } from "./progress-fraction";

function indicator(): HTMLElement {
  const element = screen.getByRole("progressbar").querySelector<HTMLElement>('[data-slot="progress-bar-indicator"]');
  if (element === null) {
    throw new Error("progress bar has no indicator");
  }
  return element;
}

describe("progressFraction", () => {
  it("clamps to 0…1 and survives a zero total or NaN", () => {
    expect(progressFraction(50, 200)).toBe(0.25);
    expect(progressFraction(300, 200)).toBe(1);
    expect(progressFraction(-5, 200)).toBe(0);
    expect(progressFraction(10, 0)).toBe(0);
    expect(progressFraction(Number.NaN, 10)).toBe(0);
  });
});

describe("ProgressBar", () => {
  it("fills by the fraction and reports the value", () => {
    render(<ProgressBar value={30} max={120} aria-label="Downloading model" />);
    const bar = screen.getByRole("progressbar", { name: "Downloading model" });
    expect(bar).toHaveAttribute("data-state", "loading");
    expect(bar).toHaveAttribute("aria-valuenow", "30");
    expect(bar).toHaveAttribute("aria-valuemax", "120");
    expect(indicator().style.getPropertyValue("--echo-progress")).toBe("0.25");
    expect(indicator().className).toContain("scale-x-(--echo-progress)");
  });

  it("clamps a value past the total", () => {
    render(<ProgressBar value={500} max={100} aria-label="Verifying" />);
    expect(screen.getByRole("progressbar")).toHaveAttribute("data-state", "complete");
    expect(indicator().style.getPropertyValue("--echo-progress")).toBe("1");
  });

  it("sweeps without a value when the total is unknown", () => {
    render(<ProgressBar value={null} aria-label="Preparing" />);
    const bar = screen.getByRole("progressbar", { name: "Preparing" });
    expect(bar).toHaveAttribute("data-state", "indeterminate");
    expect(bar).not.toHaveAttribute("aria-valuenow");
    expect(indicator().className).toContain("motion-safe:animate-indeterminate");
    expect(indicator().style.getPropertyValue("--echo-progress")).toBe("");
  });

  it("falls back to a valid total when max is not positive", () => {
    render(<ProgressBar value={10} max={0} aria-label="Unknown total" />);
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuemax", "100");
  });
});
