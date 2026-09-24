/**
 * SOURCE OF TRUTH KEYWORDS: Button test, button variants test, button type default, asChild button test
 * WHAT:  Verifies Button defaults to a non-submitting secondary md button, applies variant and size tokens,
 *        merges overrides, and renders on its child with asChild.
 * WHY:   A <button> defaults to type="submit", so a Button inside a settings form would submit on every click;
 *        the variant set is fixed by docs/04 §6.
 * WHERE: Runs in the `web` Vitest project (jsdom).
 */
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Button } from "./Button";

describe("Button", () => {
  it("is a non-submitting secondary md button by default", () => {
    render(<Button>Save</Button>);
    const button = screen.getByRole("button", { name: "Save" });
    expect(button).toHaveAttribute("type", "button");
    expect(button.className).toContain("bg-fill");
    expect(button.className).toContain("h-control");
  });

  it.each([
    ["primary", "bg-accent"],
    ["secondary", "bg-fill"],
    ["ghost", "bg-transparent"],
    ["destructive", "bg-record"],
  ] as const)("styles the %s variant with tokens", (variant, token) => {
    render(<Button variant={variant}>Go</Button>);
    expect(screen.getByRole("button").className).toContain(token);
  });

  it("sizes icon buttons to the hit target", () => {
    render(
      <Button size="icon-sm" aria-label="Close">
        x
      </Button>,
    );
    expect(screen.getByRole("button", { name: "Close" }).className).toContain("size-hit");
  });

  it("keeps an explicit type and merges class overrides", () => {
    render(
      <Button type="submit" className="px-6">
        Send
      </Button>,
    );
    const button = screen.getByRole("button");
    expect(button).toHaveAttribute("type", "submit");
    expect(button.className).toContain("px-6");
    expect(button.className).not.toContain("px-4");
  });

  it("renders its styles on the child with asChild", () => {
    render(
      <Button asChild variant="ghost">
        <a href="#history">History</a>
      </Button>,
    );
    const link = screen.getByRole("link", { name: "History" });
    expect(link).not.toHaveAttribute("type");
    expect(link.className).toContain("bg-transparent");
  });
});
