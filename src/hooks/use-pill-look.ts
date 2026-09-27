/**
 * SOURCE OF TRUTH KEYWORDS: usePillLook, PillLook, pill_get_look, PillLookChanged, pill visibility, pill style, movable pill
 * WHAT:  `usePillLook()` returns the pill settings (when the pill shows, its style, whether it can be dragged) as Rust
 *        last published them, or null until the first answer: one `pill_get_look` read, then every PillLookChanged.
 * WHY:   Rust owns settings and pushes the pill's whole look (root CLAUDE.md §7), so the pill page never reads setting
 *        keys or keeps a copy; until it knows, the pill draws its default style and shows only during takes.
 * WHERE: src/pill/Pill.tsx.
 */
import { commands, type AppError, type PillLook } from "@/bindings";
import { usePushedView } from "./use-pushed-view";

function reportToConsole(error: AppError): void {
  console.error("Echo could not read the pill settings", error);
}

export function usePillLook(): PillLook | null {
  return usePushedView("pillLookChanged", commands.pillGetLook, reportToConsole);
}
