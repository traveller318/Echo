/*!
 * SOURCE OF TRUTH KEYWORDS: dialog adapters, FolderPicker implementations, TauriFolderPicker, system dialogs
 * WHAT:  Adapters behind the FolderPicker port.
 * WHY:   System dialogs are Windows UI and stay behind their port (root CLAUDE.md §3).
 * WHERE: Constructed by app/bootstrap; used only through `dyn FolderPicker`.
 */

mod tauri_folder_picker;

pub use tauri_folder_picker::TauriFolderPicker;
