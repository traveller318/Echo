/**
 * SOURCE OF TRUTH KEYWORDS: CSP nonce, style nonce, adoptCspStyleNonce, get-nonce, setNonce, react-remove-scroll, production CSP, style-src
 * WHAT:  `adoptCspStyleNonce(doc)` reads the per-load nonce Tauri stamps on the HTML entry's placeholder `<style>`
 *        and hands it to `get-nonce`, so style tags that libraries inject at runtime carry it. Returns the nonce,
 *        or null when there is none (dev server, tests).
 * WHY:   The production CSP is `style-src 'self'` (02 §10). Radix Dialog and Sheet lock page scroll through
 *        react-remove-scroll, which inserts a `<style>` element — blocked without a nonce (05 §5 risk). Tauri
 *        adds a fresh random nonce to every `<style>` in a served HTML file and to the CSP; the empty placeholder
 *        in index.html / pill.html exists only to receive it. react-style-singleton reads `get-nonce`, so
 *        setting it once before React renders covers every later injection without loosening the CSP. The
 *        nonce is read from the `nonce` property: browsers hide the attribute value after parsing.
 * WHERE: Called by mountRoot (lib/mount-root.tsx) before the first render in both windows.
 */
import { setNonce } from "get-nonce";

/** The placeholder element in each HTML entry that Tauri stamps with the page's style nonce. */
export const CSP_NONCE_SELECTOR = "style[data-csp-nonce]";

export function adoptCspStyleNonce(doc: Document = document): string | null {
  const nonce = doc.querySelector<HTMLStyleElement>(CSP_NONCE_SELECTOR)?.nonce ?? "";
  if (nonce === "") {
    return null;
  }
  setNonce(nonce);
  return nonce;
}
