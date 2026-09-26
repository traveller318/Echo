/**
 * SOURCE OF TRUTH KEYWORDS: pill commands, stopTake, cancelTake, openPage from pill, openOnboarding, reportHitAreas, reportExited, pill error action
 * WHAT:  The pill's calls into Rust: stop the take, cancel it or undo the cancel (as Esc), report its button areas, report the end of its exit animation,
 *        bring the main window forward on a page or on onboarding, and perform an AppError action.
 * WHY:   The pill has no router, no toasts and no QueryClient (02 §6.2: a minimal bundle), so each call is a plain
 *        command whose failure is logged to the console: a pill that could not tell Rust something costs a click,
 *        never the take (Rust's fallbacks hide the pill and keep it click-through). Page actions go through
 *        `app_open_page`, which shows the main window and makes it navigate (lib/app-error-actions decides what each
 *        error action means).
 * WHERE: src/pill/Pill.tsx and its _components.
 */
import { commands, type AppError, type NavId, type OverlayRect } from "@/bindings";
import { toAppError, type AppErrorActionId } from "@/lib/app-error";
import { performAppErrorAction } from "@/lib/app-error-actions";
import { runCommand, type CommandResult } from "@/lib/command";

function logFailure(error: AppError): void {
  console.error("The pill could not reach Echo", error);
}

function send(call: () => Promise<CommandResult<null>>): void {
  runCommand(call).catch((error: unknown) => {
    logFailure(toAppError(error));
  });
}

/** The stop button: the same input as pressing the record hotkey while recording. */
export function stopTake(): void {
  send(() => commands.sessionInput("stop"));
}

/** The ✕ and undo buttons: the same input as pressing Esc (cancel while recording, undo while cancelling). */
export function cancelTake(): void {
  send(() => commands.sessionInput("cancel"));
}

export function reportHitAreas(areas: OverlayRect[]): void {
  send(() => commands.pillSetHitAreas({ areas }));
}

export function reportExited(): void {
  send(() => commands.pillExited());
}

/** Shows the main window on `page`. */
export function openPage(page: NavId): void {
  send(() => commands.appOpenPage({ page }));
}

/** Shows the main window on onboarding ("Model not installed · Set up"). */
export function openOnboarding(): void {
  send(() => commands.onboardingOpen());
}

/** Performs an AppError action from the pill (page actions open the main window). */
export function performPillAction(id: AppErrorActionId): void {
  performAppErrorAction(id, { openPage, onError: logFailure });
}
