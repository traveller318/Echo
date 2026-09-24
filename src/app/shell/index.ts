/**
 * SOURCE OF TRUTH KEYWORDS: shell barrel, window chrome, ShellFrame, ShellLayout, Titlebar, Sidebar, Toaster, RouteError
 * WHAT:  Barrel for src/app/shell: the main window's chrome (frame, titlebar, sidebar, toaster, page error).
 * WHY:   The router and the app gate import the chrome from one path; files inside can move freely (03 §3).
 * WHERE: app/router.tsx, app/routes.tsx.
 */
export { RouteError } from "./RouteError";
export { ShellFrame, type ShellFrameProps } from "./ShellFrame";
export { ShellLayout } from "./ShellLayout";
export { Sidebar } from "./Sidebar";
export { Titlebar } from "./Titlebar";
export { Toaster } from "./Toaster";
export { useAppErrorAction } from "./use-app-error-action";
