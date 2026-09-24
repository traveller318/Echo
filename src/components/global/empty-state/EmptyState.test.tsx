/**
 * SOURCE OF TRUTH KEYWORDS: EmptyState test, empty state slots test, icon title body action slots
 * WHAT:  Verifies EmptyState renders each slot it is given (icon, title, body, action), renders nothing for a
 *        slot it is not given, and uses the requested heading level for the title.
 * WHY:   EmptyState is filled entirely through slots (docs/04 §6); every "nothing here yet" in the app depends on
 *        empty slots staying absent (no stray icon circle or action row) and on the title being a heading.
 * WHERE: Runs in the `web` Vitest project (jsdom).
 */
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { EmptyState } from "./EmptyState";

function slot(container: HTMLElement, name: string): Element | null {
  return container.querySelector(`[data-slot="empty-state-${name}"]`);
}

describe("EmptyState", () => {
  it("renders every slot it is given", () => {
    const { container } = render(
      <EmptyState
        icon={<svg data-testid="icon" />}
        title="No takes yet"
        body="Press the hotkey and speak."
        action={<button type="button">Open Settings</button>}
      />,
    );
    expect(slot(container, "icon")).toContainElement(screen.getByTestId("icon"));
    expect(slot(container, "icon")).toHaveAttribute("aria-hidden", "true");
    expect(screen.getByRole("heading", { level: 2, name: "No takes yet" })).toBeInTheDocument();
    expect(screen.getByText("Press the hotkey and speak.")).toHaveAttribute("data-slot", "empty-state-body");
    expect(slot(container, "action")).toContainElement(screen.getByRole("button", { name: "Open Settings" }));
  });

  it("renders only the title when no other slot is given", () => {
    const { container } = render(<EmptyState title="Nothing found" />);
    expect(screen.getByRole("heading", { name: "Nothing found" })).toBeInTheDocument();
    expect(slot(container, "icon")).toBeNull();
    expect(slot(container, "body")).toBeNull();
    expect(slot(container, "action")).toBeNull();
  });

  it("accepts rich nodes in the title and body slots", () => {
    render(
      <EmptyState
        title={
          <>
            No results for <mark>hello</mark>
          </>
        }
        body={<span data-testid="rich-body">Try another word.</span>}
      />,
    );
    expect(screen.getByRole("heading")).toHaveTextContent("No results for hello");
    expect(screen.getByTestId("rich-body")).toBeInTheDocument();
  });

  it("uses the requested heading level and passes DOM props through", () => {
    const { container } = render(
      <EmptyState title="Models" titleAs="h3" className="p-4" aria-live="polite" />,
    );
    expect(screen.getByRole("heading", { level: 3, name: "Models" })).toBeInTheDocument();
    const root = container.querySelector('[data-slot="empty-state"]');
    expect(root).toHaveAttribute("aria-live", "polite");
    expect(root?.className).toContain("p-4");
    expect(root?.className).not.toContain("p-8");
  });
});
