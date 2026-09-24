/**
 * SOURCE OF TRUTH KEYWORDS: mountRoot, window bootstrap, React root, createRoot, StrictMode, MissingRootElementError, ROOT_ELEMENT_ID
 * WHAT:  Finds the `#root` element of the current window's HTML entry and renders a React tree into it under StrictMode.
 * WHY:   Both windows (main, pill) boot the same way; one helper keeps them identical. It throws a typed error
 *        when `#root` is missing instead of using a non-null assertion, which the lint bans.
 * WHERE: Called by src/main.tsx and src/pill.tsx; tested in mount-root.test.tsx.
 */
import { StrictMode, type ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";

export const ROOT_ELEMENT_ID = "root";

export class MissingRootElementError extends Error {
  constructor(elementId: string) {
    super(`Window entry has no #${elementId} element to mount into.`);
    this.name = "MissingRootElementError";
  }
}

export function mountRoot(children: ReactNode, doc: Document = document): Root {
  const container = doc.getElementById(ROOT_ELEMENT_ID);
  if (container === null) {
    throw new MissingRootElementError(ROOT_ELEMENT_ID);
  }
  const root = createRoot(container);
  root.render(<StrictMode>{children}</StrictMode>);
  return root;
}
