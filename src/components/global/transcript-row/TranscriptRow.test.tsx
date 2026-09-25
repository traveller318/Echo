/**
 * SOURCE OF TRUTH KEYWORDS: TranscriptRow test, status badge test, preview placeholder test, take meta test
 * WHAT:  Verifies a take row: time, duration, words and app on the first line; the preview, or the status's
 *        placeholder when there is no text; and a badge with glyph and label for every status.
 * WHY:   Status is never colour alone (04 §7) and missing measurements must not read as zero; the row is shared by
 *        History and the Dashboard.
 * WHERE: Runs in the `web` Vitest project.
 */
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { TranscriptStatus, TranscriptSummary } from "@/bindings";
import { TranscriptRow } from "./TranscriptRow";
import { TranscriptStatusBadge } from "./TranscriptStatusBadge";
import { TRANSCRIPT_STATUS_LOOK } from "./transcript-status";

const NOW = new Date(2026, 8, 25, 18, 0).getTime();

function take(overrides: Partial<TranscriptSummary> = {}): TranscriptSummary {
  return {
    id: "01K5ZQ9J3V7M8N2P4R6T8W0Y2A",
    created_at: new Date(2026, 8, 25, 14, 5).getTime(),
    status: "done",
    preview: "Meet me at the station at five.",
    duration_ms: 3200,
    word_count: 7,
    app_name: "notepad.exe",
    error_code: null,
    has_audio: true,
    ...overrides,
  };
}

describe("TranscriptRow", () => {
  it("shows the meta line and the preview of a completed take", () => {
    render(<TranscriptRow take={take()} now={NOW} />);
    expect(screen.getByText("Meet me at the station at five.")).toBeInTheDocument();
    expect(screen.getByText(/3 s · 7 words · notepad\.exe/)).toBeInTheDocument();
    expect(screen.getByText("Done")).toBeInTheDocument();
    expect(document.querySelector("time")).toHaveAttribute("datetime", new Date(take().created_at).toISOString());
  });

  it("says what the status means when there is no text, and leaves missing values out", () => {
    render(
      <TranscriptRow
        take={take({ status: "failed", preview: null, word_count: null, duration_ms: null, app_name: null })}
        now={NOW}
      />,
    );
    expect(screen.getByText(TRANSCRIPT_STATUS_LOOK.failed.placeholder)).toBeInTheDocument();
    expect(screen.getByText("Failed")).toBeInTheDocument();
    expect(screen.queryByText(/words/)).not.toBeInTheDocument();
    expect(screen.queryByText(/0 s/)).not.toBeInTheDocument();
  });
});

describe("TranscriptStatusBadge", () => {
  it("pairs every status with a glyph and a label", () => {
    const statuses = Object.keys(TRANSCRIPT_STATUS_LOOK) as TranscriptStatus[];
    expect(statuses).toHaveLength(6);
    for (const status of statuses) {
      const { container, unmount } = render(<TranscriptStatusBadge status={status} />);
      expect(screen.getByText(TRANSCRIPT_STATUS_LOOK[status].label)).toBeInTheDocument();
      expect(container.querySelector("svg")).not.toBeNull();
      unmount();
    }
  });
});
