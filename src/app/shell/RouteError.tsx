/**
 * SOURCE OF TRUTH KEYWORDS: RouteError, route error boundary, page crash, errorElement, reload, useRouteError
 * WHAT:  What a page shows when it throws while loading or rendering: the calm `Internal` copy, a "Reload" button
 *        and the copy's own action (open the logs folder). The sidebar and titlebar stay usable around it.
 * WHY:   A failing page must not blank the whole window (04 §1 calm copy, 02 §12 errors go to the log). The error
 *        is logged to the console, which the dev tools and the webview log show; the copy stays the one from
 *        lib/app-error.ts so every surface says the same thing. Reload re-creates the page from scratch, which is
 *        the only honest recovery for an unknown render failure.
 * WHERE: `errorElement` of the page routes in app/routes.tsx.
 */
import { TriangleAlertIcon } from "lucide-react";
import { useEffect } from "react";
import { useRouteError } from "react-router";
import { EmptyState } from "@/components/global";
import { Button } from "@/components/ui";
import { describeAppError } from "@/lib/app-error";
import { useAppErrorAction } from "./use-app-error-action";

const COPY = describeAppError({ code: "Internal" });

export function RouteError() {
  const error = useRouteError();
  const runAppErrorAction = useAppErrorAction();
  const { action } = COPY;

  useEffect(() => {
    console.error("An Echo page failed", error);
  }, [error]);

  return (
    <div role="alert" className="p-content-pad">
      <EmptyState
        icon={<TriangleAlertIcon />}
        title={COPY.title}
        body={COPY.body}
        action={
          <>
            <Button
              variant="primary"
              onClick={() => {
                window.location.reload();
              }}
            >
              Reload
            </Button>
            {action === null ? null : (
              <Button
                onClick={() => {
                  runAppErrorAction(action.id);
                }}
              >
                {action.label}
              </Button>
            )}
          </>
        }
      />
    </div>
  );
}
