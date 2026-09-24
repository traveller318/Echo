/**
 * SOURCE OF TRUTH KEYWORDS: mountRoot test, window bootstrap test, MissingRootElementError test
 * WHAT:  Verifies mountRoot renders into #root and fails with a typed error when #root is missing.
 * WHY:   Both windows depend on this helper; a silent failure would leave a blank window with no signal.
 * WHERE: Runs in the `web` Vitest project (jsdom).
 */
import { act, screen } from "@testing-library/react";
import type { Root } from "react-dom/client";
import { afterEach, describe, expect, it } from "vitest";
import { MissingRootElementError, mountRoot, ROOT_ELEMENT_ID } from "./mount-root";

describe("mountRoot", () => {
  afterEach(() => {
    document.body.replaceChildren();
  });

  it("renders the given tree into the #root element", () => {
    const container = document.createElement("div");
    container.id = ROOT_ELEMENT_ID;
    document.body.append(container);

    let root: Root | null = null;
    act(() => {
      root = mountRoot(<p>Mounted</p>);
    });

    expect(screen.getByText("Mounted")).toBeInTheDocument();
    expect(container).toContainElement(screen.getByText("Mounted"));
    act(() => {
      root?.unmount();
    });
  });

  it("throws MissingRootElementError when the entry has no #root element", () => {
    expect(() => mountRoot(<p>Never mounted</p>)).toThrow(MissingRootElementError);
    expect(screen.queryByText("Never mounted")).not.toBeInTheDocument();
  });
});
