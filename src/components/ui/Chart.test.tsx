/**
 * SOURCE OF TRUTH KEYWORDS: Chart test, ChartContainer test, ChartTooltipContent test, Recharts in jsdom
 * WHAT:  Verifies ChartTooltipContent shows nothing while inactive or empty and otherwise a heading plus one row per
 *        series (swatch only where a colour is given), and that ChartContainer draws its Recharts child at the
 *        size it is given before any measurement.
 * WHY:   The tooltip is the chart's only place for exact numbers, and a chart that silently draws nothing in a
 *        container without a size is the classic Recharts failure.
 * WHERE: Runs in the `web` Vitest project (jsdom).
 */
import { render, screen } from "@testing-library/react";
import { Bar, BarChart } from "recharts";
import { describe, expect, it } from "vitest";
import { ChartContainer, ChartTooltipContent } from "./Chart";

const ROWS = [
  { key: "words", label: "Words", value: "120", color: "var(--color-chart-1)" },
  { key: "takes", label: "Takes", value: "3" },
];

describe("ChartTooltipContent", () => {
  it("renders nothing while inactive or without rows", () => {
    const { container, rerender } = render(<ChartTooltipContent active={false} title="Mar 12" rows={ROWS} />);
    expect(container).toBeEmptyDOMElement();
    rerender(<ChartTooltipContent active title="Mar 12" rows={[]} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("shows the heading and one row per series", () => {
    const { container } = render(<ChartTooltipContent active title="Thursday, March 12" rows={ROWS} />);
    expect(screen.getByText("Thursday, March 12")).toBeInTheDocument();
    expect(screen.getByText("Words").parentElement).toHaveTextContent("Words120");
    expect(screen.getByText("Takes").parentElement).toHaveTextContent("Takes3");
    const swatches = container.querySelectorAll<HTMLElement>("[data-slot='chart-tooltip-swatch']");
    expect(swatches).toHaveLength(1);
    expect(swatches[0]?.style.backgroundColor).toBe("var(--color-chart-1)");
  });
});

describe("ChartContainer", () => {
  it("draws its chart at the initial size before it is measured", () => {
    const { container } = render(
      <ChartContainer initialDimension={{ width: 300, height: 160 }}>
        <BarChart
          data={[
            { day: "a", value: 2 },
            { day: "b", value: 5 },
          ]}
        >
          <Bar dataKey="value" isAnimationActive={false} />
        </BarChart>
      </ChartContainer>,
    );
    expect(container.querySelector("[data-slot='chart']")).toHaveClass("h-chart");
    expect(container.querySelector("svg.recharts-surface")).not.toBeNull();
    expect(container.querySelectorAll(".recharts-bar-rectangle")).toHaveLength(2);
  });
});
