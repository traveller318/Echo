/**
 * SOURCE OF TRUTH KEYWORDS: main window bootstrap, main.tsx, main entry, providers, router mount
 * WHAT:  Bootstraps the main window: loads global styles and mounts the React root.
 * WHY:   Kept to wiring only; providers and the registry-built router mount here (03 §1), never page logic.
 * WHERE: Entry script of index.html (main window).
 */
import "@/styles/globals.css";
import { mountRoot } from "@/lib";

mountRoot(null);
