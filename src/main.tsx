/**
 * SOURCE OF TRUTH KEYWORDS: main window bootstrap, main.tsx, main entry, providers, router mount, appearance sync
 * WHAT:  Bootstraps the main window: loads global styles, starts mirroring Rust's appearance onto <html>, and
 *        mounts the app (Providers around the registry-built AppRouter).
 * WHY:   Kept to wiring only; providers and the registry-built router mount here (03 §1), never page logic.
 *        Appearance sync starts before the first render so the theme attributes land as early as the IPC round
 *        trip allows; until then the CSS follows Windows, which is also the setting's default.
 * WHERE: Entry script of index.html (main window).
 */
import "@/styles/globals.css";
import { Providers } from "@/app/providers";
import { AppRouter } from "@/app/router";
import { mountRoot, syncAppearance } from "@/lib";

void syncAppearance();
mountRoot(
  <Providers>
    <AppRouter />
  </Providers>,
);
