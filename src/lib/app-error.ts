/**
 * SOURCE OF TRUTH KEYWORDS: AppError copy, describeAppError, toAppError, isAppError, CommandError, AppErrorAction, error message map, calm copy
 * WHAT:  The single map from an AppError (generated from Rust) to what the user reads and can do about it:
 *        a title, a body and an optional action. Plus `toAppError` to normalize any rejected value, and
 *        CommandError, the Error that carries an AppError through code that must throw (TanStack Query).
 * WHY:   AppError is the only error that crosses IPC (02 §4.2), so the UI has one error surface and this is its one
 *        copy table. The table is a mapped type over `AppError["code"]`, so a new Rust variant fails `tsc` until
 *        it has copy here. Copy follows 04 §1 "Calm copy": sentence case, short, no exclamation marks, no blame.
 *        Actions are ids, not handlers: the shell decides how to open a route or a Windows settings page.
 * WHERE: Imported through `@/lib` by any UI that shows an AppError (toasts, the pill error state, form and query
 *        errors); types come from the generated `@/bindings`.
 */
import type { AppError, Permission, ResourceKind } from "@/bindings";

export type AppErrorCode = AppError["code"];

/** The AppError variant with the given code. */
export type AppErrorOf<C extends AppErrorCode> = Extract<AppError, { code: C }>;

/** Something the user can do about an error; the app shell maps each id to navigation or a command. */
export type AppErrorActionId = "open_history" | "open_logs" | "open_mic_privacy" | "open_models" | "open_settings";

export interface AppErrorAction {
  readonly id: AppErrorActionId;
  readonly label: string;
}

export interface AppErrorCopy {
  readonly title: string;
  readonly body: string;
  readonly action: AppErrorAction | null;
}

const ACTIONS = {
  open_history: { id: "open_history", label: "Open History" },
  open_logs: { id: "open_logs", label: "Open logs folder" },
  open_mic_privacy: { id: "open_mic_privacy", label: "Open privacy settings" },
  open_models: { id: "open_models", label: "Open Models" },
  open_settings: { id: "open_settings", label: "Open Settings" },
} as const satisfies { readonly [Id in AppErrorActionId]: AppErrorAction & { readonly id: Id } };

const PERMISSION_COPY: Readonly<Record<Permission, AppErrorCopy>> = {
  microphone: {
    title: "Microphone access is off",
    body: "Turn on microphone access for desktop apps in Windows privacy settings.",
    action: ACTIONS.open_mic_privacy,
  },
  network: {
    title: "Offline mode is on",
    body: "Turn it off in Settings to download.",
    action: ACTIONS.open_settings,
  },
  clipboard: {
    title: "Couldn't use the clipboard",
    body: "Another app may be holding it. Try again in a moment.",
    action: null,
  },
  input_injection: {
    title: "Couldn't type into that window",
    body: "The text is on the clipboard. Press Ctrl+V to paste it.",
    action: null,
  },
};

const NOT_FOUND_COPY: Readonly<Record<ResourceKind, AppErrorCopy>> = {
  transcript: {
    title: "That take no longer exists",
    body: "It may have been deleted.",
    action: null,
  },
  model: {
    title: "That model isn't available",
    body: "Pick one from the Models page.",
    action: ACTIONS.open_models,
  },
  engine: {
    title: "That engine isn't available",
    body: "Pick another one in Settings.",
    action: ACTIONS.open_settings,
  },
  setting: {
    title: "That setting doesn't exist",
    body: "Restart Echo, then try again.",
    action: null,
  },
  audio_device: {
    title: "That microphone isn't connected",
    body: "Plug it back in or pick another one in Settings.",
    action: ACTIONS.open_settings,
  },
  update: {
    title: "No update to install",
    body: "This is the latest version of Echo.",
    action: null,
  },
};

/**
 * SOURCE OF TRUTH KEYWORDS: APP_ERROR_COPY, exhaustive error copy table, per-code copy builder
 * WHAT:  One copy builder per AppError code, each receiving its own narrowed variant.
 * WHY:   The mapped type makes the table exhaustive (a missing or unknown code fails tsc) and lets a builder use
 *        its variant's fields without any cast.
 * WHERE: describeAppError, isAppErrorCode.
 */
const APP_ERROR_COPY: { readonly [C in AppErrorCode]: (error: AppErrorOf<C>) => AppErrorCopy } = {
  Validation: (error) => ({ title: "Check that value", body: error.message, action: null }),
  PermissionDenied: (error) => PERMISSION_COPY[error.permission],
  Busy: () => ({
    title: "Already in progress",
    body: "Wait for it to finish, then try again.",
    action: null,
  }),
  NotFound: (error) => NOT_FOUND_COPY[error.resource],
  ModelMissing: () => ({
    title: "Speech model not installed",
    body: "Set it up on the Models page.",
    action: ACTIONS.open_models,
  }),
  ModelCorrupt: () => ({
    title: "Speech model is damaged",
    body: "Download it again from the Models page.",
    action: ACTIONS.open_models,
  }),
  AudioDevice: () => ({
    title: "Couldn't use the microphone",
    body: "Check that it's connected, or pick another one in Settings.",
    action: ACTIONS.open_settings,
  }),
  Asr: () => ({
    title: "Couldn't transcribe that take",
    body: "The audio is saved. Retry it from History.",
    action: ACTIONS.open_history,
  }),
  Polish: () => ({
    title: "Cleanup didn't finish",
    body: "The text was kept as it was heard.",
    action: null,
  }),
  Storage: () => ({
    title: "Couldn't save to disk",
    body: "Check that the drive has free space, then try again.",
    action: ACTIONS.open_logs,
  }),
  Offline: () => PERMISSION_COPY.network,
  Network: () => ({
    title: "Couldn't reach the download server",
    body: "Check your connection, then try again.",
    action: null,
  }),
  Hotkey: (error) =>
    error.reason === "conflict"
      ? {
          title: "That shortcut is taken",
          body: "Another Echo shortcut already uses it. Pick a different one.",
          action: null,
        }
      : {
          title: "That shortcut can't be used",
          body: "Use a key with modifiers, like Ctrl+Alt+Space, or two modifiers, like Ctrl+Alt.",
          action: null,
        },
  Internal: () => ({
    title: "Something went wrong",
    body: "Details are in the log.",
    action: ACTIONS.open_logs,
  }),
};

function copyFor<C extends AppErrorCode>(code: C, error: AppErrorOf<C>): AppErrorCopy {
  return APP_ERROR_COPY[code](error);
}

/** What to show the user for an error. */
export function describeAppError(error: AppError): AppErrorCopy {
  return copyFor(error.code, error);
}

export function isAppErrorCode(code: string): code is AppErrorCode {
  return Object.hasOwn(APP_ERROR_COPY, code);
}

/** True when a value has the AppError wire shape (an object whose `code` is a known AppError code). */
export function isAppError(value: unknown): value is AppError {
  if (typeof value !== "object" || value === null || !("code" in value)) {
    return false;
  }
  return typeof value.code === "string" && isAppErrorCode(value.code);
}

/**
 * SOURCE OF TRUTH KEYWORDS: CommandError, thrown AppError, query error, mutation error, Error subclass
 * WHAT:  An Error whose `appError` is the AppError a command returned (or `Internal` for a failed call).
 * WHY:   TanStack Query learns about a failure only through a thrown value, and the lint (and good practice) only
 *        allows throwing Error objects; wrapping keeps the one error shape reachable through `toAppError`.
 * WHERE: Thrown by `runCommand` (lib/command.ts); unwrapped by `toAppError` wherever a query or mutation error is shown.
 */
export class CommandError extends Error {
  readonly appError: AppError;

  constructor(appError: AppError) {
    super(`Echo command failed with ${appError.code}`);
    this.name = "CommandError";
    this.appError = appError;
  }
}

/**
 * SOURCE OF TRUTH KEYWORDS: toAppError, normalize rejection, unknown error, Internal fallback
 * WHAT:  Returns the value itself when it is an AppError, the carried AppError of a CommandError, otherwise
 *        `{ code: "Internal" }`.
 * WHY:   A promise can reject with something that is not an AppError (a webview failure, a thrown Error); the UI
 *        still gets exactly one error shape to render.
 * WHERE: Query/mutation error handlers, event subscriptions and window-control handlers that catch.
 */
export function toAppError(value: unknown): AppError {
  if (value instanceof CommandError) {
    return value.appError;
  }
  return isAppError(value) ? value : { code: "Internal" };
}
