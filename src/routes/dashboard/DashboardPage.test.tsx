/**
 * SOURCE OF TRUTH KEYWORDS: DashboardPage test, dashboard route test, stat cards test, activity chart test, recent takes test, metrics range test
 * WHAT:  Verifies the Dashboard against mocked commands: every registry metric in its row with its formatted value
 *        (a dash for no data), the range picker re-reading the summary, the activity card's summary line and chart,
 *        the five recent takes (a click opens no detail sheet), "Show all" opening History, and the error retry.
 * WHY:   The page is laid out from the registry and fed only by metrics_summary / metrics_activity / history_list;
 *        this is the one place that whole flow runs short of Rust. jsdom layout comes from test/layout-stubs.ts.
 * WHERE: Runs in the `web` Vitest project with `@/bindings` mocked.
 */
import { QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { act, fireEvent as fire } from "@testing-library/react";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  ActivityDay,
  MetricSpec,
  MetricsSummary,
  NavItem,
  RegistryView,
  TranscriptSummary,
} from "@/bindings";
import { RegistryContext } from "@/hooks/use-registry";
import { createEchoQueryClient } from "@/lib/query-client";
import { DEFAULT_METRICS_RANGE, useDashboardStore } from "@/stores";
import { stubListLayout, stubResizeObserver } from "@/test/layout-stubs";

const DASHBOARD: NavItem = { id: "dashboard", label: "Dashboard", icon: "layout-dashboard", route: "/", order: 1 };
const NAV: readonly NavItem[] = [
  DASHBOARD,
  { id: "history", label: "History", icon: "history", route: "/history", order: 2 },
];

/** The registry's metric entries (registry/metrics.rs), in dashboard order. */
const METRICS: MetricSpec[] = [
  {
    id: "time-saved",
    label: "Time saved",
    help: "Time typing these words would take at your typing speed, minus the time you spent speaking.",
    unit: "duration",
    query: { kind: "summary", aggregate: "time_saved" },
    emphasis: "hero",
  },
  {
    id: "words",
    label: "Words",
    help: "Words delivered by completed takes.",
    unit: "count",
    query: { kind: "summary", aggregate: "words" },
    emphasis: "primary",
  },
  {
    id: "transcriptions",
    label: "Transcriptions",
    help: "Completed takes.",
    unit: "count",
    query: { kind: "summary", aggregate: "transcriptions" },
    emphasis: "primary",
  },
  {
    id: "speaking-wpm",
    label: "Speaking speed",
    help: "Words per minute while you were speaking.",
    unit: "wpm",
    query: { kind: "summary", aggregate: "speaking_wpm" },
    emphasis: "primary",
  },
  {
    id: "activity",
    label: "Activity",
    help: "Words per day over the last 30 days.",
    unit: "count",
    query: { kind: "activity", days: 30 },
    emphasis: "primary",
  },
  {
    id: "median-latency",
    label: "Median latency",
    help: "Typical time from stopping to pasted text, over your last 100 takes.",
    unit: "ms",
    query: { kind: "summary", aggregate: "median_latency" },
    emphasis: "secondary",
  },
  {
    id: "streak",
    label: "Streak",
    help: "Consecutive days with at least one completed take.",
    unit: "days",
    query: { kind: "summary", aggregate: "streak" },
    emphasis: "secondary",
  },
];

const REGISTRY: RegistryView = { settings: [], sections: [], hotkeys: [], nav: [...NAV], engines: [], metrics: METRICS, credits: [] };

function summary(range: MetricsSummary["range"], words: number | null, streak: number | null): MetricsSummary {
  return {
    range,
    values: [
      { aggregate: "time_saved", value: 7_500_000 },
      { aggregate: "words", value: words },
      { aggregate: "transcriptions", value: 12 },
      { aggregate: "speaking_wpm", value: 148.6 },
      { aggregate: "median_latency", value: null },
      { aggregate: "streak", value: streak },
    ],
  };
}

/** 30 days ending 2026-03-12: 90 words on 03-10, 120 on 03-12. */
const ACTIVITY: ActivityDay[] = Array.from({ length: 30 }, (_, index) => {
  const date = new Date(2026, 1, 11 + index);
  const text = `${String(date.getFullYear())}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
  const words = text === "2026-03-10" ? 90 : text === "2026-03-12" ? 120 : 0;
  return { date: text, words, transcriptions: words === 0 ? 0 : 1 };
});

function take(id: string, preview: string): TranscriptSummary {
  return {
    id,
    created_at: Date.now(),
    status: "done",
    preview,
    duration_ms: 2000,
    word_count: preview.split(" ").length,
    app_name: "notepad.exe",
    error_code: null,
    has_audio: true,
  };
}

const RECENT = [take("01K5ZQ9J3V7M8N2P4R6T8W0Y2B", "Pick up the milk."), take("01K5ZQ9J3V7M8N2P4R6T8W0Y2A", "Call Sam back.")];

const mocks = vi.hoisted(() => ({
  metricsSummary: vi.fn(),
  metricsActivity: vi.fn(),
  historyList: vi.fn(),
  historyCopy: vi.fn(),
  historyDelete: vi.fn(),
  sessionRetry: vi.fn(),
}));

vi.mock("@/bindings", () => ({
  commands: mocks,
  // No Rust events in this test: the page is rendered without the invalidation bridge.
  events: {},
}));

const { default: DashboardPage } = await import("./index");

let restoreLayout: () => void = () => undefined;
let restoreResize: () => void = () => undefined;

const saved = {
  scrollIntoView: Object.getOwnPropertyDescriptor(Element.prototype, "scrollIntoView"),
  hasPointerCapture: Object.getOwnPropertyDescriptor(Element.prototype, "hasPointerCapture"),
  releasePointerCapture: Object.getOwnPropertyDescriptor(Element.prototype, "releasePointerCapture"),
};

// Radix Select scrolls its list and checks pointer capture, which jsdom lacks.
beforeAll(() => {
  Object.defineProperty(Element.prototype, "scrollIntoView", { configurable: true, value: () => undefined });
  Object.defineProperty(Element.prototype, "hasPointerCapture", { configurable: true, value: () => false });
  Object.defineProperty(Element.prototype, "releasePointerCapture", { configurable: true, value: () => undefined });
});

afterAll(() => {
  for (const [name, descriptor] of Object.entries(saved)) {
    if (descriptor === undefined) {
      Reflect.deleteProperty(Element.prototype, name);
    } else {
      Object.defineProperty(Element.prototype, name, descriptor);
    }
  }
});

beforeEach(() => {
  restoreLayout = stubListLayout({ viewport: 400, row: 60 });
  restoreResize = stubResizeObserver({ width: 600, height: 160 });
  mocks.metricsSummary.mockImplementation((input: { range: MetricsSummary["range"] }) =>
    Promise.resolve({ status: "ok", data: summary(input.range, input.range === "today" ? 120 : 1204, 3) }),
  );
  mocks.metricsActivity.mockResolvedValue({ status: "ok", data: ACTIVITY });
  mocks.historyList.mockResolvedValue({ status: "ok", data: { items: RECENT, next_cursor: null } });
});

afterEach(() => {
  restoreLayout();
  restoreResize();
  vi.clearAllMocks();
  useDashboardStore.setState({ range: DEFAULT_METRICS_RANGE });
});

function renderPage() {
  const router = createMemoryRouter(
    [
      { path: "/", element: <DashboardPage nav={DASHBOARD} /> },
      { path: "/history", element: <p>History page</p> },
    ],
    { initialEntries: ["/"] },
  );
  return render(
    <QueryClientProvider client={createEchoQueryClient()}>
      <RegistryContext value={REGISTRY}>
        <RouterProvider router={router} />
      </RegistryContext>
    </QueryClientProvider>,
  );
}

function card(label: string): HTMLElement {
  return screen.getByRole("group", { name: label });
}

describe("DashboardPage", () => {
  it("shows every registry metric in its row with its formatted value", async () => {
    renderPage();
    expect(await screen.findByRole("group", { name: "Time saved" })).toHaveTextContent("2 h 5 min");
    expect(screen.getByRole("heading", { level: 1, name: "Dashboard" })).toBeInTheDocument();
    expect(card("Words")).toHaveTextContent("1,204");
    expect(card("Transcriptions")).toHaveTextContent("12");
    expect(card("Speaking speed")).toHaveTextContent("149 wpm");
    expect(card("Median latency")).toHaveTextContent("—");
    expect(card("Streak")).toHaveTextContent("3 days");
    expect(card("Time saved")).toHaveTextContent(METRICS[0]?.help ?? "");
    expect(mocks.metricsSummary).toHaveBeenCalledWith({ range: "all_time" });
  });

  it("reads the summary again for a newly picked range", async () => {
    renderPage();
    await screen.findByRole("group", { name: "Words" });
    const trigger = screen.getByRole("combobox", { name: "Show numbers for" });
    expect(trigger).toHaveTextContent("All time");
    act(() => {
      fire.keyDown(trigger, { key: "Enter" });
    });
    const today = await screen.findByRole("option", { name: "Today" });
    await act(async () => {
      fire.keyDown(today, { key: "Enter" });
      await Promise.resolve();
    });
    await waitFor(() => {
      expect(mocks.metricsSummary).toHaveBeenCalledWith({ range: "today" });
    });
    await waitFor(() => {
      expect(card("Words")).toHaveTextContent("120");
    });
    expect(useDashboardStore.getState().range).toBe("today");
  });

  it("charts the activity window from the registry's day count", async () => {
    renderPage();
    const activity = await screen.findByRole("group", { name: "Activity" });
    expect(await within(activity).findByText("210 words on 2 days")).toBeInTheDocument();
    expect(mocks.metricsActivity).toHaveBeenCalledWith({ days: 30 });
    // The chart's code loads lazily (Recharts is large to transform on first import), so allow it time.
    await waitFor(
      () => {
        // One bar per day with words (03-10 and 03-12); idle days leave a gap on the baseline.
        expect(activity.querySelectorAll(".recharts-bar-rectangle path")).toHaveLength(2);
      },
      { timeout: 15_000 },
    );
  });

  it("lists the five newest takes and opens nothing on a click", async () => {
    renderPage();
    fireEvent.click(await screen.findByText("Pick up the milk."));
    expect(mocks.historyList).toHaveBeenCalledWith({ search: null, cursor: null, limit: 5 });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("opens History from Show all", async () => {
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "Show all" }));
    expect(await screen.findByText("History page")).toBeInTheDocument();
  });

  it("shows dashes and invitations before the first take", async () => {
    mocks.metricsSummary.mockResolvedValue({
      status: "ok",
      data: { range: "all_time", values: METRICS.flatMap((spec) => (spec.query.kind === "summary" ? [{ aggregate: spec.query.aggregate, value: null }] : [])) },
    });
    mocks.metricsActivity.mockResolvedValue({
      status: "ok",
      data: ACTIVITY.map((day) => ({ ...day, words: 0, transcriptions: 0 })),
    });
    mocks.historyList.mockResolvedValue({ status: "ok", data: { items: [], next_cursor: null } });
    renderPage();
    expect(await screen.findByRole("group", { name: "Time saved" })).toHaveTextContent("—");
    expect(card("Streak")).toHaveTextContent("—");
    expect(await within(card("Activity")).findByText("No takes yet")).toBeInTheDocument();
    expect(await within(card("Recent takes")).findByText("No takes yet")).toBeInTheDocument();
  });

  it("offers a retry when the numbers cannot be read", async () => {
    mocks.metricsSummary.mockResolvedValueOnce({ status: "error", error: { code: "Storage" } });
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "Try again" }));
    expect(await screen.findByRole("group", { name: "Words" })).toHaveTextContent("1,204");
  });
});
