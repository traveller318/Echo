/**
 * SOURCE OF TRUTH KEYWORDS: Providers, app providers, QueryClientProvider, ToastProvider, query invalidation bridge, theme
 * WHAT:  Wraps a window's React tree in what every screen needs: the TanStack QueryClient (with the event-driven
 *        invalidation bridge running for its lifetime) and the Radix ToastProvider (the toaster's timing and live
 *        region; the Toaster itself renders inside the routed shell).
 * WHY:   One place owns the data layer and notification context, so routes and components only use hooks. The
 *        client is created once per window (or injected by tests) and the bridge is tied to it in an effect, so
 *        StrictMode's double mount and unmount leave no listener behind. Theme is not a React provider: Rust owns
 *        appearance and src/lib/appearance.ts mirrors it onto <html> before React mounts (main.tsx), which paints
 *        the right theme earlier than any provider could; tooltips bring their own Radix provider.
 * WHERE: src/main.tsx (main window); app tests.
 */
import { QueryClientProvider, type QueryClient } from "@tanstack/react-query";
import { useEffect, useState, type ReactNode } from "react";
import { ToastProvider } from "@/components/ui";
import { createEchoQueryClient } from "@/lib/query-client";
import { startQueryInvalidation } from "@/lib/query-invalidation";

export interface ProvidersProps {
  readonly children: ReactNode;
  /** A ready client (tests); by default the window creates its own. */
  readonly queryClient?: QueryClient;
}

export function Providers({ children, queryClient }: ProvidersProps) {
  const [client] = useState(() => queryClient ?? createEchoQueryClient());
  useEffect(() => startQueryInvalidation(client), [client]);
  return (
    <QueryClientProvider client={client}>
      <ToastProvider>{children}</ToastProvider>
    </QueryClientProvider>
  );
}
