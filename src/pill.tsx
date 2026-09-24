/**
 * SOURCE OF TRUTH KEYWORDS: pill window bootstrap, pill.tsx, pill entry, pill mount
 * WHAT:  Bootstraps the pill window: loads global styles and mounts the React root.
 * WHY:   The pill has its own entry so its bundle stays minimal and it paints within the 50 ms budget (02 §6.2).
 * WHERE: Entry script of pill.html (pill window).
 */
import "@/styles/globals.css";
import { mountRoot } from "@/lib";

mountRoot(null);
