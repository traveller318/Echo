/**
 * SOURCE OF TRUTH KEYWORDS: DictionaryPage test, dictionary route test, apply dictionary switch test, add term test, edit term test, delete term test, duplicate term test, search terms test, termFormSchema test
 * WHAT:  Verifies the Dictionary page against mocked commands: the switch and the saved terms of the registry's
 *        dictionary section (and nothing Settings owns), the switch saved through `settings_set`, a term added
 *        (trimmed, appended), edited in place and deleted as a whole-list save, a word already listed refused inline
 *        with nothing saved, the search, and the empty state; plus the pure term rules.
 * WHY:   The page wires the registry, the settings reads, DataList and the term dialog together; this is the one place
 *        that whole flow runs short of Rust. jsdom layout comes from test/layout-stubs.ts.
 * WHERE: Runs in the `web` Vitest project with `@/bindings` mocked.
 */
import { QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { NavItem, RegistryView, SettingEntry, SettingSpec, SettingsAvailability, TextPair } from "@/bindings";
import { RegistryContext } from "@/hooks";
import { createEchoQueryClient } from "@/lib/query-client";
import { stubListLayout } from "@/test/layout-stubs";
import { matchesTerm, termFormSchema, withoutTerm, withTerm } from "./_components/dictionary-terms";

const NAV: NavItem = { id: "dictionary", label: "Dictionary", icon: "book-open", route: "/dictionary", order: 2 };

const PAIRS = { kind: "pairs", max_pairs: 500, max_len: 100 } as const;

const SETTINGS: SettingSpec[] = [
  {
    key: "polish.remove_fillers",
    section: "polish",
    label: "Remove filler words",
    help: "Drop fillers.",
    kind: { kind: "bool" },
    default: { kind: "bool", value: true },
    restart_required: false,
    visible: true,
    requires: null,
  },
  {
    key: "dictionary.enabled",
    section: "dictionary",
    label: "Apply dictionary to transcripts",
    help: "Swap your saved terms into every dictation before it lands.",
    kind: { kind: "bool" },
    default: { kind: "bool", value: true },
    restart_required: false,
    visible: true,
    requires: null,
  },
  {
    key: "dictionary.entries",
    section: "dictionary",
    label: "Saved terms",
    help: "Words Echo should always write your way.",
    kind: PAIRS,
    default: { kind: "pairs", value: [] },
    restart_required: false,
    visible: true,
    requires: null,
  },
];

const REGISTRY: RegistryView = {
  settings: SETTINGS,
  sections: [
    { section: "polish", label: "Cleanup", page: "settings" },
    { section: "dictionary", label: "Dictionary", page: "dictionary" },
  ],
  hotkeys: [],
  nav: [NAV],
  engines: [],
  metrics: [],
  credits: [],
};

const TERMS: TextPair[] = [
  { from: "type script", to: "TypeScript" },
  { from: "bridge mind", to: "BridgeMind" },
];

function values(terms: TextPair[], enabled = true): SettingEntry[] {
  return [
    { key: "polish.remove_fillers", value: { kind: "bool", value: true } },
    { key: "dictionary.enabled", value: { kind: "bool", value: enabled } },
    { key: "dictionary.entries", value: { kind: "pairs", value: terms } },
  ];
}

const AVAILABILITY: SettingsAvailability = { caps: [], options: [] };

const mocks = vi.hoisted(() => ({
  settingsGetAll: vi.fn(),
  settingsAvailability: vi.fn(),
  settingsSet: vi.fn(),
  settingsReset: vi.fn(),
  audioListDevices: vi.fn(),
  hotkeysStatus: vi.fn(),
  hotkeysCapture: vi.fn(),
}));

vi.mock("@/bindings", () => ({
  SETTING_TOKEN_MAX_CHARS: 128,
  commands: mocks,
  // No Rust events in this test: the page is rendered without the invalidation bridge.
  events: {},
}));

const { default: DictionaryPage } = await import("./index");

let restoreLayout: () => void = () => undefined;

beforeEach(() => {
  restoreLayout = stubListLayout({ viewport: 600, row: 48 });
  mocks.settingsGetAll.mockResolvedValue({ status: "ok", data: values(TERMS) });
  mocks.settingsAvailability.mockResolvedValue({ status: "ok", data: AVAILABILITY });
  mocks.settingsSet.mockImplementation((input: SettingEntry) => Promise.resolve({ status: "ok", data: input }));
  mocks.audioListDevices.mockResolvedValue({ status: "ok", data: [] });
  mocks.hotkeysStatus.mockResolvedValue({ status: "ok", data: { paused: false, capturing: false } });
  mocks.hotkeysCapture.mockResolvedValue({ status: "ok", data: { paused: false, capturing: false } });
});

afterEach(() => {
  restoreLayout();
  vi.clearAllMocks();
});

function renderPage() {
  return render(
    <QueryClientProvider client={createEchoQueryClient()}>
      <RegistryContext value={REGISTRY}>
        <DictionaryPage nav={NAV} />
      </RegistryContext>
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

function savedTerms(): unknown {
  return mocks.settingsSet.mock.lastCall?.[0];
}

describe("DictionaryPage", () => {
  it("shows the switch and every saved term, and nothing Settings owns", async () => {
    renderPage();
    expect(await screen.findByText("TypeScript")).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 1, name: "Dictionary" })).toBeInTheDocument();
    expect(screen.getByText("Fix words Echo gets wrong.")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Apply dictionary to transcripts" })).toBeChecked();
    expect(within(row("BridgeMind")).getByText("bridge mind")).toBeInTheDocument();
    expect(screen.getByText("2")).toBeInTheDocument();
    expect(screen.queryByText("Remove filler words")).not.toBeInTheDocument();
  });

  it("saves the switch through settings_set", async () => {
    renderPage();
    fireEvent.click(await screen.findByRole("switch", { name: "Apply dictionary to transcripts" }));
    await waitFor(() => {
      expect(mocks.settingsSet).toHaveBeenCalledWith({ key: "dictionary.enabled", value: { kind: "bool", value: false } });
    });
  });

  it("adds a trimmed term at the end of the list", async () => {
    renderPage();
    await screen.findByText("TypeScript");
    fireEvent.click(screen.getByRole("button", { name: "Add term" }));
    const dialog = await screen.findByRole("dialog", { name: "Add term" });
    fireEvent.change(within(dialog).getByLabelText("Word"), { target: { value: "  cloud code " } });
    fireEvent.change(within(dialog).getByLabelText("Written as"), { target: { value: " Claude Code  " } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Add term" }));
    await waitFor(() => {
      expect(savedTerms()).toEqual({
        key: "dictionary.entries",
        value: { kind: "pairs", value: [...TERMS, { from: "cloud code", to: "Claude Code" }] },
      });
    });
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
  });

  it("refuses a word that is already listed and saves nothing", async () => {
    renderPage();
    await screen.findByText("TypeScript");
    fireEvent.click(screen.getByRole("button", { name: "Add term" }));
    const dialog = await screen.findByRole("dialog", { name: "Add term" });
    fireEvent.change(within(dialog).getByLabelText("Word"), { target: { value: "Type Script" } });
    fireEvent.change(within(dialog).getByLabelText("Written as"), { target: { value: "TS" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Add term" }));
    expect(await within(dialog).findByText('"Type Script" is listed more than once.')).toBeInTheDocument();
    expect(mocks.settingsSet).not.toHaveBeenCalled();
  });

  it("edits a term in place and deletes one", async () => {
    renderPage();
    await screen.findByText("TypeScript");
    fireEvent.click(within(row("TypeScript")).getByRole("button", { name: "Edit type script" }));
    const dialog = await screen.findByRole("dialog", { name: "Edit term" });
    expect(within(dialog).getByLabelText("Word")).toHaveValue("type script");
    fireEvent.change(within(dialog).getByLabelText("Written as"), { target: { value: "Typescript" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => {
      expect(savedTerms()).toEqual({
        key: "dictionary.entries",
        value: { kind: "pairs", value: [{ from: "type script", to: "Typescript" }, TERMS[1]] },
      });
    });

    await waitFor(() => {
      expect(within(row("BridgeMind")).getByRole("button", { name: "Delete bridge mind" })).toBeEnabled();
    });
    fireEvent.click(within(row("BridgeMind")).getByRole("button", { name: "Delete bridge mind" }));
    await waitFor(() => {
      expect(savedTerms()).toEqual({ key: "dictionary.entries", value: { kind: "pairs", value: [TERMS[0]] } });
    });
  });

  it("searches words and spellings, and says when nothing matches", async () => {
    renderPage();
    await screen.findByText("TypeScript");
    const field = screen.getByRole("searchbox", { name: "Search terms" });
    fireEvent.change(field, { target: { value: "BRIDGE" } });
    await waitFor(() => {
      expect(screen.queryByText("TypeScript")).not.toBeInTheDocument();
    });
    expect(screen.getByText("BridgeMind")).toBeInTheDocument();
    fireEvent.change(field, { target: { value: "zebra" } });
    expect(await screen.findByText("No terms match")).toBeInTheDocument();
  });

  it("invites the first term when the dictionary is empty", async () => {
    mocks.settingsGetAll.mockResolvedValue({ status: "ok", data: values([]) });
    renderPage();
    expect(await screen.findByText("No terms yet")).toBeInTheDocument();
    expect(screen.queryByRole("searchbox")).not.toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "Add term" })).toHaveLength(2);
  });
});

describe("dictionary term rules", () => {
  it("validates a term against the rest of the list with the registry's rules", () => {
    const schema = termFormSchema(PAIRS, TERMS);
    expect(schema.safeParse({ from: "cloud code", to: "Claude Code" }).success).toBe(true);
    expect(schema.safeParse({ from: "remove me", to: "" }).success).toBe(true);
    const blank = schema.safeParse({ from: "  ", to: "x" });
    expect(blank.error?.issues.map((issue) => [issue.path, issue.message])).toEqual([
      [["from"], "Every entry needs a word to replace."],
    ]);
    const long = schema.safeParse({ from: "x", to: "y".repeat(101) });
    expect(long.error?.issues.map((issue) => issue.path)).toEqual([["to"]]);
    const full = termFormSchema({ ...PAIRS, max_pairs: 2 }, TERMS).safeParse({ from: "new", to: "New" });
    expect(full.error?.issues.map((issue) => [issue.path, issue.message])).toEqual([
      [["from"], "Use at most 2 entries."],
    ]);
  });

  it("builds the list to save and filters it", () => {
    const added = withTerm(TERMS, { kind: "add" }, { from: " a ", to: " A " });
    expect(added.at(-1)).toEqual({ from: "a", to: "A" });
    expect(withTerm(TERMS, { kind: "edit", index: 0 }, { from: "ts", to: "TS" })).toEqual([
      { from: "ts", to: "TS" },
      TERMS[1],
    ]);
    expect(withoutTerm(TERMS, 0)).toEqual([TERMS[1]]);
    expect(matchesTerm({ from: "bridge mind", to: "BridgeMind" }, " mind")).toBe(true);
    expect(matchesTerm({ from: "bridge mind", to: "BridgeMind" }, "")).toBe(true);
    expect(matchesTerm({ from: "bridge mind", to: "BridgeMind" }, "script")).toBe(false);
  });
});
