/**
 * SOURCE OF TRUTH KEYWORDS: CSP nonce test, adoptCspStyleNonce test, get-nonce test
 * WHAT:  Verifies adoptCspStyleNonce hands the placeholder's nonce to get-nonce and does nothing without one.
 * WHY:   Without the nonce, Radix Dialog's scroll lock <style> is blocked by the production CSP (05 §5); the dev
 *        server and tests have no nonce and must not set an empty one.
 * WHERE: Runs in the `web` Vitest project (jsdom).
 */
import { getNonce } from "get-nonce";
import { afterEach, describe, expect, it } from "vitest";
import { adoptCspStyleNonce } from "./csp-nonce";

describe("adoptCspStyleNonce", () => {
  afterEach(() => {
    document.head.replaceChildren();
  });

  it("returns null and sets nothing when the page has no nonce", () => {
    const style = document.createElement("style");
    style.setAttribute("data-csp-nonce", "");
    document.head.append(style);
    expect(adoptCspStyleNonce()).toBeNull();
    expect(getNonce()).toBeUndefined();
  });

  it("adopts the nonce Tauri stamped on the placeholder", () => {
    const style = document.createElement("style");
    style.setAttribute("data-csp-nonce", "");
    style.nonce = "8230571";
    document.head.append(style);
    expect(adoptCspStyleNonce()).toBe("8230571");
    expect(getNonce()).toBe("8230571");
  });
});
