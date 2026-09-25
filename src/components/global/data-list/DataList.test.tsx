/**
 * SOURCE OF TRUTH KEYWORDS: DataList test, keyboard navigation test, slots test, virtualization test, search validation test, onEndReached test
 * WHAT:  Verifies DataList: the row, actions and empty slots render with their row state; only rows near the
 *        viewport exist; arrows, Home and End move focus (scrolling a far row in); Enter on a row activates it but
 *        not from an action button; inactive rows' actions are inert; the search reports valid text, shows an
 *        inline error for text over the limit and hands focus to the list; the end of the list asks for more.
 * WHY:   04 §6: DataList owns search, virtualization, keyboard navigation and focus for every long list, so these
 *        behaviours are tested once here. jsdom has no layout, so test/layout-stubs.ts gives each test a 400 px
 *        viewport and 40 px rows.
 * WHERE: Runs in the `web` Vitest project.
 */
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { stubListLayout } from "@/test/layout-stubs";
import { DataList, type DataListProps } from "./DataList";

interface Item {
  readonly id: string;
  readonly name: string;
}

const VIEWPORT_HEIGHT = 400;
const ROW_HEIGHT = 40;

function items(count: number): Item[] {
  return Array.from({ length: count }, (_, index) => ({ id: `id-${String(index)}`, name: `Item ${String(index)}` }));
}

let restoreLayout: () => void = () => undefined;

beforeEach(() => {
  restoreLayout = stubListLayout({ viewport: VIEWPORT_HEIGHT, row: ROW_HEIGHT });
});

afterEach(() => {
  restoreLayout();
});

function renderList(overrides: Partial<DataListProps<Item>> = {}) {
  const onActivate = vi.fn<(item: Item) => void>();
  const props: DataListProps<Item> = {
    items: items(100),
    getKey: (item) => item.id,
    label: "Things",
    row: (item, state) => (
      <span data-active={state.active}>
        {item.name} #{state.index}
      </span>
    ),
    actions: (item) => <button type="button">Edit {item.name}</button>,
    empty: <p>Nothing here</p>,
    onActivate,
    ...overrides,
  };
  const view = render(<DataList {...props} />);
  return { ...view, onActivate };
}

function rowElement(name: string): HTMLElement {
  const text = screen.getByText(new RegExp(`^${name} #`));
  const row = text.closest<HTMLElement>("[data-slot='data-list-row']");
  if (row === null) {
    throw new Error(`no row for ${name}`);
  }
  return row;
}

describe("DataList", () => {
  it("renders the row and actions slots with their state, and only rows near the viewport", () => {
    renderList();
    const list = screen.getByRole("list", { name: "Things" });
    const rendered = within(list).getAllByRole("listitem");
    expect(rendered.length).toBeGreaterThan(0);
    expect(rendered.length).toBeLessThan(40);
    expect(screen.getByText("Item 0 #0")).toHaveAttribute("data-active", "true");
    expect(screen.getByText("Item 1 #1")).toHaveAttribute("data-active", "false");
    expect(rendered[0]).toHaveAttribute("aria-posinset", "1");
    expect(rendered[0]).toHaveAttribute("aria-setsize", "100");
    expect(screen.getByText("Edit Item 0")).toBeInTheDocument();
    expect(screen.queryByText("Item 90 #90")).not.toBeInTheDocument();
  });

  it("shows the empty slot when there are no items", () => {
    renderList({ items: [] });
    expect(screen.getByText("Nothing here")).toBeInTheDocument();
    expect(screen.queryByRole("list")).not.toBeInTheDocument();
  });

  it("keeps one tabbable row and makes the other rows' actions inert", () => {
    renderList();
    expect(rowElement("Item 0")).toHaveAttribute("tabindex", "0");
    expect(rowElement("Item 1")).toHaveAttribute("tabindex", "-1");
    const actions = document.querySelectorAll("[data-slot='data-list-actions']");
    expect(actions[0]).not.toHaveAttribute("inert");
    expect(actions[1]).toHaveAttribute("inert");

    fireEvent.pointerEnter(rowElement("Item 2").parentElement ?? document.body);
    expect(rowElement("Item 2")).toHaveAttribute("tabindex", "0");
    expect(document.querySelectorAll("[data-slot='data-list-actions']")[2]).not.toHaveAttribute("inert");
  });

  it("moves focus with the arrows, Home and End, scrolling far rows into the DOM", async () => {
    renderList();
    const first = rowElement("Item 0");
    act(() => {
      first.focus();
    });
    fireEvent.keyDown(first, { key: "ArrowDown" });
    await waitFor(() => {
      expect(rowElement("Item 1")).toHaveFocus();
    });
    fireEvent.keyDown(rowElement("Item 1"), { key: "ArrowDown" });
    await waitFor(() => {
      expect(rowElement("Item 2")).toHaveFocus();
    });
    fireEvent.keyDown(rowElement("Item 2"), { key: "ArrowUp" });
    await waitFor(() => {
      expect(rowElement("Item 1")).toHaveFocus();
    });

    fireEvent.keyDown(rowElement("Item 1"), { key: "End" });
    await waitFor(() => {
      expect(rowElement("Item 99")).toHaveFocus();
    });
    expect(screen.queryByText("Item 0 #0")).not.toBeInTheDocument();

    fireEvent.keyDown(rowElement("Item 99"), { key: "Home" });
    await waitFor(() => {
      expect(rowElement("Item 0")).toHaveFocus();
    });
  });

  it("activates a row with Enter or a click, but not from its action button", () => {
    const { onActivate } = renderList();
    const first = rowElement("Item 0");
    fireEvent.keyDown(first, { key: "Enter" });
    expect(onActivate).toHaveBeenCalledWith({ id: "id-0", name: "Item 0" });

    fireEvent.keyDown(screen.getByText("Edit Item 0"), { key: "Enter" });
    expect(onActivate).toHaveBeenCalledTimes(1);

    fireEvent.click(rowElement("Item 3"));
    expect(onActivate).toHaveBeenLastCalledWith({ id: "id-3", name: "Item 3" });
  });

  it("asks for more near the end only while more exist and nothing is loading", async () => {
    const onEndReached = vi.fn();
    const { rerender } = renderList({ items: items(12), hasMore: true, onEndReached });
    await waitFor(() => {
      expect(onEndReached).toHaveBeenCalled();
    });
    onEndReached.mockClear();
    const props: DataListProps<Item> = {
      items: items(12),
      getKey: (item) => item.id,
      label: "Things",
      row: (item) => item.name,
      empty: null,
      hasMore: true,
      loadingMore: true,
      onEndReached,
    };
    rerender(<DataList {...props} />);
    rerender(<DataList {...props} hasMore={false} loadingMore={false} />);
    expect(onEndReached).not.toHaveBeenCalled();
  });
});

describe("DataList search", () => {
  function SearchableList({ onSearch }: { readonly onSearch: (value: string) => void }) {
    const [value, setValue] = useState("");
    return (
      <DataList<Item>
        items={items(5).filter((item) => item.name.includes(value))}
        getKey={(item) => item.id}
        label="Things"
        row={(item, state) => `${item.name} #${String(state.index)}`}
        empty={<p>No match</p>}
        search={{
          value,
          onChange: (next) => {
            onSearch(next);
            setValue(next);
          },
          label: "Search things",
          maxLength: 5,
        }}
      />
    );
  }

  it("reports valid text, shows an error over the limit, and clears on Escape", async () => {
    const onSearch = vi.fn<(value: string) => void>();
    render(<SearchableList onSearch={onSearch} />);
    const field = screen.getByRole("searchbox", { name: "Search things" });

    fireEvent.change(field, { target: { value: "3" } });
    await waitFor(() => {
      expect(onSearch).toHaveBeenLastCalledWith("3");
    });
    expect(screen.getByText("Item 3 #0")).toBeInTheDocument();

    onSearch.mockClear();
    fireEvent.change(field, { target: { value: "too long" } });
    expect(await screen.findByRole("alert")).toHaveTextContent("Keep the search under 5 characters.");
    expect(field).toHaveAttribute("aria-invalid", "true");
    expect(onSearch).not.toHaveBeenCalled();

    fireEvent.keyDown(field, { key: "Escape" });
    await waitFor(() => {
      expect(onSearch).toHaveBeenLastCalledWith("");
    });
    expect(field).toHaveValue("");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("hands focus to the active row on ArrowDown and shows the empty slot for no match", async () => {
    render(<SearchableList onSearch={vi.fn()} />);
    const field = screen.getByRole("searchbox", { name: "Search things" });
    fireEvent.keyDown(field, { key: "ArrowDown" });
    await waitFor(() => {
      expect(rowElement("Item 0")).toHaveFocus();
    });
    fireEvent.change(field, { target: { value: "zz" } });
    expect(await screen.findByText("No match")).toBeInTheDocument();
  });
});
