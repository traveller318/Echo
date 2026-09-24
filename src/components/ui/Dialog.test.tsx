/**
 * SOURCE OF TRUTH KEYWORDS: Dialog test, Sheet test, modal glass test, dialog close test
 * WHAT:  Verifies Dialog and Sheet open as modal GlassSurfaces with an accessible title, a "Close" button that
 *        closes them, and the Sheet's edge radius override resolved against the modal variant.
 * WHY:   Both reuse GlassSurface and the shared overlay and close button; a broken composition would lose the
 *        glass recipe or the only mouse way out of a modal.
 * WHERE: Runs in the `web` Vitest project (jsdom).
 */
import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "./Dialog";
import { Sheet, SheetContent, SheetTitle } from "./Sheet";

describe("Dialog", () => {
  it("opens as a modal glass surface and closes from its corner button", () => {
    render(
      <Dialog defaultOpen>
        <DialogContent>
          <DialogTitle>Delete this take?</DialogTitle>
          <DialogDescription>Its audio and text are removed.</DialogDescription>
        </DialogContent>
      </Dialog>,
    );
    const dialog = screen.getByRole("dialog", { name: "Delete this take?" });
    expect(dialog).toHaveAttribute("data-variant", "modal");
    expect(dialog).toHaveAccessibleDescription("Its audio and text are removed.");
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: "Close" }));
    });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("can hide the corner close button", () => {
    render(
      <Dialog defaultOpen>
        <DialogContent showClose={false} aria-describedby={undefined}>
          <DialogTitle>Working</DialogTitle>
        </DialogContent>
      </Dialog>,
    );
    expect(screen.queryByRole("button", { name: "Close" })).not.toBeInTheDocument();
  });
});

describe("Sheet", () => {
  it("opens as a modal glass drawer with only its inner edge rounded", () => {
    render(
      <Sheet defaultOpen>
        <SheetContent aria-describedby={undefined}>
          <SheetTitle>Take details</SheetTitle>
        </SheetContent>
      </Sheet>,
    );
    const sheet = screen.getByRole("dialog", { name: "Take details" });
    expect(sheet).toHaveAttribute("data-variant", "modal");
    expect(sheet.className).toContain("rounded-l-card");
    expect(sheet.className).not.toMatch(/(^|\s)rounded-card(\s|$)/);
    expect(screen.getByRole("button", { name: "Close" })).toBeInTheDocument();
  });
});
