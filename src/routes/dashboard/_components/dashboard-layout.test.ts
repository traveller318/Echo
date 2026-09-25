/**
 * SOURCE OF TRUTH KEYWORDS: dashboardLayout test, summaryValue test, metrics range options test, parseMetricsRange test
 * WHAT:  Verifies the registry metrics are sorted into hero, primary, activity and secondary rows in registry order
 *        (an activity series always charted), that summaryValue finds a card's number or null, and that the range
 *        picker offers every MetricsRange once, in order, and refuses anything else.
 * WHY:   The Dashboard lays itself out from these; a metric in the wrong row or a range the schema lets through
 *        would reach the user or Rust unchecked.
 * WHERE: Runs in the `web` Vitest project.
 */
import { describe, expect, it } from "vitest";
import type { MetricSpec, MetricsSummary } from "@/bindings";
import { dashboardLayout, summaryValue } from "./dashboard-layout";
import { METRICS_RANGE_OPTIONS, metricsRangeLabel, parseMetricsRange } from "./metrics-range";

function spec(id: string, query: MetricSpec["query"], emphasis: MetricSpec["emphasis"]): MetricSpec {
  return { id, label: id, help: `${id}.`, unit: "count", query, emphasis };
}

const METRICS: MetricSpec[] = [
  spec("time-saved", { kind: "summary", aggregate: "time_saved" }, "hero"),
  spec("words", { kind: "summary", aggregate: "words" }, "primary"),
  spec("activity", { kind: "activity", days: 30 }, "primary"),
  spec("latency", { kind: "summary", aggregate: "median_latency" }, "secondary"),
  spec("wpm", { kind: "summary", aggregate: "speaking_wpm" }, "primary"),
  spec("week", { kind: "activity", days: 7 }, "secondary"),
];

describe("dashboardLayout", () => {
  it("sorts metrics into rows by emphasis and charts every activity series", () => {
    const layout = dashboardLayout(METRICS);
    expect(layout.hero.map((metric) => metric.aggregate)).toEqual(["time_saved"]);
    expect(layout.primary.map((metric) => metric.aggregate)).toEqual(["words", "speaking_wpm"]);
    expect(layout.secondary.map((metric) => metric.aggregate)).toEqual(["median_latency"]);
    expect(layout.activity.map((metric) => [metric.spec.id, metric.days])).toEqual([
      ["activity", 30],
      ["week", 7],
    ]);
  });

  it("gives empty rows for an empty registry", () => {
    expect(dashboardLayout([])).toEqual({ hero: [], primary: [], activity: [], secondary: [] });
  });
});

describe("summaryValue", () => {
  const summary: MetricsSummary = {
    range: "all_time",
    values: [
      { aggregate: "words", value: 1204 },
      { aggregate: "streak", value: null },
    ],
  };

  it("finds a value by aggregate, and null for no data, a missing entry or no summary", () => {
    expect(summaryValue(summary, "words")).toBe(1204);
    expect(summaryValue(summary, "streak")).toBeNull();
    expect(summaryValue(summary, "time_saved")).toBeNull();
    expect(summaryValue(undefined, "words")).toBeNull();
  });
});

describe("metrics range options", () => {
  it("offers every range once, shortest first, with its label", () => {
    expect(METRICS_RANGE_OPTIONS.map((option) => option.value)).toEqual([
      "today",
      "last_7_days",
      "last_30_days",
      "all_time",
    ]);
    expect(METRICS_RANGE_OPTIONS.map((option) => option.label)).toEqual([
      "Today",
      "Last 7 days",
      "Last 30 days",
      "All time",
    ]);
    expect(metricsRangeLabel("last_7_days")).toBe("Last 7 days");
  });

  it("parses only a known range", () => {
    expect(parseMetricsRange("today")).toBe("today");
    for (const bad of ["", "forever", "Today", "last_7days"]) {
      expect(parseMetricsRange(bad)).toBeNull();
    }
  });
});
