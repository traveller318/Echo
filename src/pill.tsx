/**
 * SOURCE OF TRUTH KEYWORDS: pill window bootstrap, pill.tsx, pill entry, pill mount, appearance sync
 * WHAT:  Bootstraps the pill window: loads global styles, starts mirroring Rust's appearance onto <html>, and
 *        mounts the Pill.
 * WHY:   The pill has its own entry so its bundle stays minimal and it paints within the 50 ms budget (02 §6.2).
 *        It follows the same theme and transparency as the main window, so it syncs appearance too. The window is
 *        pre-created and hidden, so this runs at startup and the pill is ready before the first hotkey press.
 * WHERE: Entry script of pill.html (pill window).
 */
import "@/styles/globals.css";
import { mountRoot, syncAppearance } from "@/lib";
import { Pill } from "@/pill/Pill";

void syncAppearance();
mountRoot(<Pill />);
