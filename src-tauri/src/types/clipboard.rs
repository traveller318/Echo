/*!
 * SOURCE OF TRUTH KEYWORDS: ClipboardHistory, clipboard history exclusion, Win+V history, cloud clipboard, clipboard write policy
 * WHAT:  ClipboardHistory: whether a clipboard write may be archived by Windows (Win+V history, cloud clipboard,
 *        clipboard monitors) or must be excluded.
 * WHY:   Transcripts are private and Echo's History is already their archive, so delivery excludes them (05 W5);
 *        a named policy reads better at the call site than a bare bool and leaves room for a future policy.
 * WHERE: `Clipboard::write_text` (ports/clipboard.rs); chosen by pipeline/delivery.rs.
 */

/// Whether Windows may keep a copy of a clipboard write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClipboardHistory {
    /// Mark the data so Win+V history, cloud clipboard and clipboard monitors skip it.
    Exclude,
    /// A normal write.
    Include,
}
