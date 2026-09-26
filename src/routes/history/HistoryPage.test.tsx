/**
 * SOURCE OF TRUTH KEYWORDS: HistoryPage test, history route test, search takes test, no detail sheet test, copy retry delete test, clear history test
 * WHAT:  Verifies the History page against mocked commands: rows from `history_list`, the search sent as typed,
 *        the empty states, a row click opening nothing, Copy and Retry calling their commands with the take's id,
 *        Delete calling `history_delete` and Clear history calling `history_clear` only after their confirmations.
 * WHY:   The page wires DataList, the History hooks and the take actions together; this is the one place the whole
 *        flow runs short of Rust. jsdom layout comes from test/layout-stubs.ts.
 * WHERE: Runs in the `web` Vitest project with `@/bindings` mocked.
 */
import { QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { NavItem, TranscriptSummary } from "@/bindings";
import { createEchoQueryClient } from "@/lib/query-client";
import { useToastStore } from "@/stores/toast-store";
import { stubListLayout } from "@/test/layout-stubs";

const NAV: NavItem = { id: "history", label: "History", icon: "history", route: "/history", order: 2 };

function summary(id: string, preview: string | null, overrides: Partial<TranscriptSummary> = {}): TranscriptSummary {
  return {
    id,
    created_at: Date.now(),
    status: "done",
    preview,
    duration_ms: 2000,
    word_count: preview?.split(" ").length ?? null,
    app_name: "notepad.exe",
    error_code: null,
    has_audio: true,
    ...overrides,
  };
}

const TAKES = [
  summary("01K5ZQ9J3V7M8N2P4R6T8W0Y2B", "Pick up the milk."),
  summary("01K5ZQ9J3V7M8N2P4R6T8W0Y2A", null, { status: "failed", error_code: "Asr", word_count: null }),
];

const mocks = vi.hoisted(() => ({
  historyList: vi.fn(),
  historyCopy: vi.fn(),
  historyDelete: vi.fn(),
  historyClear: vi.fn(),
  sessionRetry: vi.fn(),
}));

vi.mock("@/bindings", () => ({
  HISTORY_SEARCH_MAX_CHARS: 200,
  commands: mocks,
  // No Rust events in this test: the page is rendered without the invalidation bridge.
  events: {},
}));

const { default: HistoryPage } = await import("./index");

let restoreLayout: () => void = () => undefined;

beforeEach(() => {
  restoreLayout = stubListLayout({ viewport: 600, row: 60 });
  mocks.historyList.mockImplementation((input: { search: string | null }) =>
    Promise.resolve({
      status: "ok",
      data: {
        items: input.search === null ? TAKES : TAKES.filter((take) => take.preview?.includes(input.search ?? "")),
        next_cursor: null,
      },
    }),
  );
  mocks.historyClear.mockResolvedValue({ status: "ok", data: null });
  mocks.historyCopy.mockResolvedValue({ status: "ok", data: null });
  mocks.historyDelete.mockResolvedValue({ status: "ok", data: null });
  mocks.sessionRetry.mockResolvedValue({ status: "ok", data: TAKES[1] });
});

afterEach(() => {
  restoreLayout();
  vi.clearAllMocks();
  useToastStore.setState({ toasts: [], nextId: 1 });
});

function renderPage() {
  return render(
    <QueryClientProvider client={createEchoQueryClient()}>
      <HistoryPage nav={NAV} />
    </QueryClientProvider>,
  );
}

function row(text: string): HTMLElement {
  const item = screen.getByText(text).closest<HTMLElement>("[role='listitem']");
  if (item === null) {
    throw new Error(`no row for ${text}`);
  }
  return item;
}

function toastTitles(): string[] {
  return useToastStore.getState().toasts.map((toast) => toast.title);
}

describe("HistoryPage", () => {
  it("lists every take, newest first, with its status", async () => {
    renderPage();
    expect(await screen.findByText("Pick up the milk.")).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 1, name: "History" })).toBeInTheDocument();
    expect(within(row("Pick up the milk.")).getByText("Done")).toBeInTheDocument();
    expect(within(row("The audio is saved. Retry to transcribe it.")).getByText("Failed")).toBeInTheDocument();
    expect(mocks.historyList).toHaveBeenCalledWith({ search: null, cursor: null, limit: 100 });
  });

  it("searches as the user types and says when nothing matches", async () => {
    renderPage();
    await screen.findByText("Pick up the milk.");
    const field = screen.getByRole("searchbox", { name: "Search takes" });
    fireEvent.change(field, { target: { value: "milk" } });
    await waitFor(() => {
      expect(mocks.historyList).toHaveBeenCalledWith({ search: "milk", cursor: null, limit: 100 });
    });
    fireEvent.change(field, { target: { value: "zebra" } });
    expect(await screen.findByText("No takes match")).toBeInTheDocument();
  });

  it("opens nothing when a row is clicked", async () => {
    renderPage();
    fireEvent.click(await screen.findByText("Pick up the milk."));
    fireEvent.keyDown(row("Pick up the milk."), { key: "Enter" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("copies and retries a take by id", async () => {
    renderPage();
    await screen.findByText("Pick up the milk.");
    const take = row("Pick up the milk.");
    fireEvent.click(within(take).getByRole("button", { name: "Copy" }));
    await waitFor(() => {
      expect(mocks.historyCopy).toHaveBeenCalledWith({ id: TAKES[0]?.id });
    });
    await waitFor(() => {
      expect(toastTitles()).toContain("Copied to the clipboard");
    });

    fireEvent.click(within(take).getByRole("button", { name: "Retry" }));
    await waitFor(() => {
      expect(mocks.sessionRetry).toHaveBeenCalledWith({ id: TAKES[0]?.id });
    });
  });

  it("deletes only after the confirmation", async () => {
    renderPage();
    await screen.findByText("Pick up the milk.");
    const take = row("Pick up the milk.");
    fireEvent.click(within(take).getByRole("button", { name: "Delete" }));
    const dialog = await screen.findByRole("dialog", { name: "Delete this take?" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
    expect(mocks.historyDelete).not.toHaveBeenCalled();

    fireEvent.click(within(take).getByRole("button", { name: "Delete" }));
    const confirm = await screen.findByRole("dialog", { name: "Delete this take?" });
    fireEvent.click(within(confirm).getByRole("button", { name: "Delete" }));
    await waitFor(() => {
      expect(mocks.historyDelete).toHaveBeenCalledWith({ id: TAKES[0]?.id });
    });
    await waitFor(() => {
      expect(toastTitles()).toContain("Take deleted");
    });
  });

  it("clears History only after the confirmation, promising the dashboard stays", async () => {
    renderPage();
    await screen.findByText("Pick up the milk.");
    const button = screen.getByRole("button", { name: "Clear history" });
    fireEvent.click(button);
    const dialog = await screen.findByRole("dialog", { name: "Clear all history?" });
    expect(dialog).toHaveTextContent("Your dashboard numbers and streak stay.");
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
    expect(mocks.historyClear).not.toHaveBeenCalled();

    fireEvent.click(button);
    const confirm = await screen.findByRole("dialog", { name: "Clear all history?" });
    fireEvent.click(within(confirm).getByRole("button", { name: "Clear history" }));
    await waitFor(() => {
      expect(mocks.historyClear).toHaveBeenCalledTimes(1);
    });
    await waitFor(() => {
      expect(toastTitles()).toContain("History cleared");
    });
  });

  it("invites the first take when History is empty, with nothing to clear", async () => {
    mocks.historyList.mockResolvedValue({ status: "ok", data: { items: [], next_cursor: null } });
    renderPage();
    expect(await screen.findByText("No takes yet")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Clear history" })).toBeDisabled();
  });
});
