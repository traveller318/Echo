/**
 * SOURCE OF TRUTH KEYWORDS: GlassSurface test, glass variants test, asChild test, reduced transparency test
 * WHAT:  Verifies every GlassSurface variant renders its recipe (tint, blur, border, shadow, radius), that
 *        overrides resolve against the variant, that `asChild` moves the recipe onto the child, and that reduced
 *        transparency drops the backdrop filter.
 * WHY:   GlassSurface is the only place the glass recipe lives (docs/04 §3.2); a broken variant would restyle
 *        every card, popover, dialog and the pill at once.
 * WHERE: Runs in the `web` Vitest project (jsdom).
 */
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { GlassSurface } from "./GlassSurface";
import { GLASS_SURFACE_VARIANTS, type GlassSurfaceVariant } from "./glass-surface-variants";

const RECIPE: Readonly<Record<GlassSurfaceVariant, readonly string[]>> = {
  card: ["bg-(--glass-tint)", "backdrop-blur-md", "rounded-card", "border-(--glass-border)", "var(--shadow-e1)"],
  sidebar: ["bg-(--glass-tint)", "backdrop-blur-md"],
  popover: ["bg-(--glass-tint-strong)", "backdrop-blur-sm", "rounded-control", "border-(--glass-border)", "var(--shadow-e2)"],
  pill: ["bg-(--glass-tint-strong)", "backdrop-blur-md", "rounded-pill", "border-(--glass-border)", "var(--shadow-e3)"],
  modal: ["bg-(--glass-tint-strong)", "backdrop-blur-lg", "rounded-card", "border-(--glass-border)", "var(--shadow-e2)"],
};

describe("GlassSurface", () => {
  it.each(GLASS_SURFACE_VARIANTS)("renders the %s recipe from tokens only", (variant) => {
    render(<GlassSurface variant={variant}>content</GlassSurface>);
    const surface = screen.getByText("content");
    expect(surface).toHaveAttribute("data-slot", "glass-surface");
    expect(surface).toHaveAttribute("data-variant", variant);
    for (const part of RECIPE[variant]) {
      expect(surface.className).toContain(part);
    }
    expect(surface.className).toContain("backdrop-saturate-(--glass-saturate)");
    expect(surface.className).toContain("[:root[data-transparency=reduced]_&]:backdrop-filter-none");
  });

  it("keeps the sidebar tint-only: no border, no shadow, no radius", () => {
    render(<GlassSurface variant="sidebar">sidebar</GlassSurface>);
    const { className } = screen.getByText("sidebar");
    expect(className).not.toMatch(/\bborder-|\bshadow-|\brounded-/);
  });

  it("uses the card variant by default", () => {
    render(<GlassSurface>card</GlassSurface>);
    expect(screen.getByText("card")).toHaveAttribute("data-variant", "card");
  });

  it("resolves a className override against the variant instead of stacking both", () => {
    render(
      <GlassSurface variant="modal" className="rounded-none p-6">
        sheet
      </GlassSurface>,
    );
    const { className } = screen.getByText("sheet");
    expect(className).toContain("rounded-none");
    expect(className).not.toContain("rounded-card");
    expect(className).toContain("p-6");
  });

  it("puts the recipe on its child with asChild", () => {
    render(
      <GlassSurface variant="popover" asChild>
        <section aria-label="menu" className="p-1" />
      </GlassSurface>,
    );
    const section = screen.getByRole("region", { name: "menu" });
    expect(section.tagName).toBe("SECTION");
    expect(section).toHaveAttribute("data-variant", "popover");
    expect(section.className).toContain("rounded-control");
    expect(section.className).toContain("p-1");
  });

  it("passes DOM props through", () => {
    render(
      <GlassSurface role="dialog" aria-label="Glass" id="glass">
        body
      </GlassSurface>,
    );
    expect(screen.getByRole("dialog", { name: "Glass" })).toHaveAttribute("id", "glass");
  });
});
