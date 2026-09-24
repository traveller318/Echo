/**
 * SOURCE OF TRUTH KEYWORDS: motion test, springs test, transitionFor test, reduced motion test, token reader test
 * WHAT:  Verifies the named springs match docs/04 §3.7, transitionFor swaps them for a token-timed tween under
 *        reduced motion, and the token readers parse CSS values or fail with MissingDesignTokenError.
 * WHY:   The pill's feel depends on these numbers, and the reduced-motion path must never guess a duration when
 *        the stylesheet is missing.
 * WHERE: Runs in the `web` Vitest project (jsdom).
 */
import { afterEach, describe, expect, it } from "vitest";
import {
  MissingDesignTokenError,
  PILL_ENTER_FROM,
  PILL_EXIT_TO,
  PILL_REST,
  readDurationToken,
  readEaseToken,
  readLengthToken,
  SPRINGS,
  stillPose,
  transitionFor,
} from "./motion";

describe("motion tokens", () => {
  afterEach(() => {
    document.documentElement.removeAttribute("style");
  });

  it("holds the four named springs of docs/04", () => {
    expect(SPRINGS).toEqual({
      pillEnter: { type: "spring", stiffness: 420, damping: 32, mass: 0.9 },
      pillMorph: { type: "spring", stiffness: 500, damping: 38, mass: 1 },
      pillExit: { type: "spring", stiffness: 380, damping: 36, mass: 1 },
      press: { type: "spring", stiffness: 700, damping: 40, mass: 1 },
    });
  });

  it("returns the spring when motion is allowed", () => {
    expect(transitionFor("pillMorph", false)).toBe(SPRINGS.pillMorph);
  });

  it("replaces the spring with the --duration-base / --ease-standard tween under reduced motion", () => {
    document.documentElement.style.setProperty("--duration-base", "200ms");
    document.documentElement.style.setProperty("--ease-standard", "cubic-bezier(0.2, 0, 0, 1)");
    expect(transitionFor("pillEnter", true)).toEqual({ type: "tween", duration: 0.2, ease: [0.2, 0, 0, 1] });
  });

  it("reads seconds and milliseconds", () => {
    document.documentElement.style.setProperty("--duration-slow", "0.32s");
    expect(readDurationToken("--duration-slow")).toBe(0.32);
  });

  it("reads lengths in px", () => {
    document.documentElement.style.setProperty("--pill-width-recording", "184px");
    expect(readLengthToken("--pill-width-recording")).toBe(184);
    document.documentElement.style.setProperty("--pill-width-recording", "11.5rem");
    expect(() => readLengthToken("--pill-width-recording")).toThrow(MissingDesignTokenError);
  });

  it("holds the pill poses of docs/04 and keeps only their opacity under reduced motion", () => {
    expect(PILL_ENTER_FROM).toEqual({ opacity: 0, scale: 0.92, y: 12 });
    expect(PILL_EXIT_TO).toEqual({ opacity: 0, scale: 0.96, y: 8 });
    expect(stillPose(PILL_ENTER_FROM, true)).toEqual({ opacity: 0, scale: 1, y: 0 });
    expect(stillPose(PILL_REST, false)).toBe(PILL_REST);
  });

  it("fails loudly instead of guessing when a token is missing or malformed", () => {
    expect(() => readDurationToken("--duration-base")).toThrow(MissingDesignTokenError);
    document.documentElement.style.setProperty("--ease-standard", "ease-in-out");
    expect(() => readEaseToken("--ease-standard")).toThrow(MissingDesignTokenError);
  });
});
