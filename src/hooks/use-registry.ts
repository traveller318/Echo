/**
 * SOURCE OF TRUTH KEYWORDS: useRegistry, REGISTRY_QUERY, RegistryContext, useRegistryView, useNavItems, registry_get, RegistryView, MissingRegistryError
 * WHAT:  REGISTRY_QUERY (the `registry_get` read), `useRegistryQuery()` to load it, RegistryContext to hand the
 *        loaded RegistryView to everything below the app gate, and `useRegistryView()` / `useNavItems()` (nav in
 *        sidebar order) to read it synchronously.
 * WHY:   The registry is what the app *has* (02 §3.3) and the UI renders from it, never from its own lists. It is
 *        compiled into Rust, so it never changes while Echo runs: no invalidating event, read once per window.
 *        The app gate (app/router.tsx) renders nothing registry-driven until it has loaded, then provides it
 *        through context, so the shell and pages read it without handling a loading state they can never be in.
 *        The context value is the query's own data, not a copy. Reading outside the gate is a programming error
 *        and throws MissingRegistryError at once instead of rendering an empty app.
 * WHERE: app/router.tsx (load + provide); app/shell (sidebar, error actions); later Settings (setting specs),
 *        Dashboard (metric specs), Models (engine specs).
 */
import { createContext, use, useMemo } from "react";
import { commands, type NavItem, type RegistryView } from "@/bindings";
import { useEchoQuery, type EchoQuery } from "./use-echo-query";

export const REGISTRY_QUERY: EchoQuery<RegistryView> = {
  queryKey: ["registry"],
  command: commands.registryGet,
};

export function useRegistryQuery() {
  return useEchoQuery(REGISTRY_QUERY);
}

export const RegistryContext = createContext<RegistryView | null>(null);

export class MissingRegistryError extends Error {
  constructor() {
    super("The registry is read outside the app gate (app/router.tsx provides RegistryContext).");
    this.name = "MissingRegistryError";
  }
}

export function useRegistryView(): RegistryView {
  const registry = use(RegistryContext);
  if (registry === null) {
    throw new MissingRegistryError();
  }
  return registry;
}

/** Sidebar entries in registry order. */
export function sortNavItems(nav: readonly NavItem[]): NavItem[] {
  return [...nav].sort((left, right) => left.order - right.order);
}

export function useNavItems(): NavItem[] {
  const { nav } = useRegistryView();
  return useMemo(() => sortNavItems(nav), [nav]);
}
