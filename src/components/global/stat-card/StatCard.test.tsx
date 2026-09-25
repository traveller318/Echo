/**
 * SOURCE OF TRUTH KEYWORDS: StatCard test, stat card slots test, StatFigure test, hero size test, help tooltip test
 * WHAT:  Verifies StatCard renders each slot it is given and nothing for the others, names itself by its label,
 *        describes itself with its help sentence behind a named info button, sizes by `size`, and that StatFigure
 *        writes number parts with their unit words and real spaces.
 * WHY:   Every dashboard number goes through these two components (04 §6); a missing space or an unnamed group
 *        would break what screen readers announce on every card.
 * WHERE: Runs in the `web` Vitest project (jsdom).
 */
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { StatCard } from "./StatCard";
import { StatFigure } from "./StatFigure";

function slot(container: HTMLElement, name: string): Element | null {
  return container.querySelector(`[data-slot="stat-card-${name}"]`);
}

describe("StatCard", () => {
  it("renders every slot it is given as a labelled group", () => {
    const { container } = render(
      <StatCard
        label="Speaking speed"
        value="148"
        unit="wpm"
        trend="+4%"
        footer="Across 12 takes."
        help="Words per minute while you were speaking."
        size="hero"
      />,
    );
    const group = screen.getByRole("group", { name: "Speaking speed" });
    expect(group).toHaveAttribute("data-size", "hero");
    expect(group).toHaveAccessibleDescription("Words per minute while you were speaking.");
    expect(slot(container, "value")).toHaveTextContent("148 wpm");
    expect(slot(container, "value")).toHaveClass("text-display");
    expect(slot(container, "trend")).toHaveTextContent("+4%");
    expect(slot(container, "footer")).toHaveTextContent("Across 12 takes.");
    expect(screen.getByRole("button", { name: "About Speaking speed" })).toBeInTheDocument();
  });

  it("leaves out the slots it is not given", () => {
    const { container } = render(<StatCard label="Words" value="1,204" />);
    const group = screen.getByRole("group", { name: "Words" });
    expect(group).toHaveAttribute("data-size", "default");
    expect(group).not.toHaveAttribute("aria-describedby");
    expect(slot(container, "value")).toHaveClass("text-title1");
    expect(slot(container, "trend")).toBeNull();
    expect(slot(container, "footer")).toBeNull();
    expect(screen.queryByRole("button")).toBeNull();
  });
});

describe("StatFigure", () => {
  it("writes figures and unit words with spaces between them", () => {
    const { container } = render(
      <p>
        <StatFigure
          parts={[
            { value: "2", unit: "h" },
            { value: "5", unit: "min" },
          ]}
        />
      </p>,
    );
    expect(container).toHaveTextContent(/^2 h 5 min$/);
    expect(container.querySelectorAll("[data-slot='stat-unit']")).toHaveLength(2);
  });

  it("writes a plain count without a unit", () => {
    const { container } = render(
      <p>
        <StatFigure parts={[{ value: "1,204", unit: "" }]} />
      </p>,
    );
    expect(container).toHaveTextContent(/^1,204$/);
    expect(container.querySelector("[data-slot='stat-unit']")).toBeNull();
  });
});
