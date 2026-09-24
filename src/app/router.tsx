/**
 * SOURCE OF TRUTH KEYWORDS: AppRouter, app gate, registry gate, hash router, RouterProvider, registry load error, RegistryContext provider
 * WHAT:  `AppRouter` loads the registry, then renders the routed app from it: while it loads, the bare ShellFrame
 *        (titlebar only); if it fails, the frame with the calm error copy and "Try again"; once loaded, the
 *        RegistryContext and a hash router built by buildAppRoutes.
 * WHY:   Routes and the sidebar come from the registry (02 §3.3), so nothing routed can render before it exists;
 *        the titlebar is there in every state so the frameless window can always be moved and closed. The load is
 *        local IPC and takes milliseconds, so the loading state shows no indicator (04 §1 "Nothing waits"). A hash
 *        router works under both the Vite dev server and Tauri's production origin with no server rewrites, and
 *        keeps the page across a webview reload. The registry never changes while Echo runs, so the router is built
 *        once per window.
 * WHERE: Rendered by src/main.tsx inside app/providers.tsx.
 */
import { RotateCcwIcon, TriangleAlertIcon } from "lucide-react";
import { useState } from "react";
import { createHashRouter } from "react-router";
import { RouterProvider } from "react-router/dom";
import type { AppError, NavItem } from "@/bindings";
import { EmptyState } from "@/components/global";
import { Button } from "@/components/ui";
import { RegistryContext, useRegistryQuery } from "@/hooks/use-registry";
import { describeAppError, toAppError } from "@/lib/app-error";
import { buildAppRoutes } from "./routes";
import { ShellFrame } from "./shell";

function NavRouter({ nav }: { readonly nav: readonly NavItem[] }) {
  const [router] = useState(() => createHashRouter(buildAppRoutes(nav)));
  return <RouterProvider router={router} />;
}

function RegistryUnavailable({
  error,
  retrying,
  onRetry,
}: {
  readonly error: AppError;
  readonly retrying: boolean;
  readonly onRetry: () => void;
}) {
  const copy = describeAppError(error);
  return (
    <div role="alert" className="p-content-pad">
      <EmptyState
        icon={<TriangleAlertIcon />}
        title={copy.title}
        body={copy.body}
        action={
          <Button variant="primary" disabled={retrying} onClick={onRetry}>
            <RotateCcwIcon aria-hidden="true" />
            Try again
          </Button>
        }
      />
    </div>
  );
}

export function AppRouter() {
  const registry = useRegistryQuery();
  if (registry.isPending) {
    return <ShellFrame />;
  }
  if (registry.isError) {
    return (
      <ShellFrame>
        <RegistryUnavailable
          error={toAppError(registry.error)}
          retrying={registry.isFetching}
          onRetry={() => {
            void registry.refetch();
          }}
        />
      </ShellFrame>
    );
  }
  return (
    <RegistryContext value={registry.data}>
      <NavRouter nav={registry.data.nav} />
    </RegistryContext>
  );
}
