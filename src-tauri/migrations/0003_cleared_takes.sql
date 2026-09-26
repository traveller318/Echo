-- SOURCE OF TRUTH KEYWORDS: migration 0003, cleared_at, clear history, hidden take, keep metrics, history_clear
-- WHAT:  Adds `transcripts.cleared_at` (unix ms, NULL = still in History): when the user cleared History, the
--        take's text, audio and app were erased and the row left History, but its measurements stay.
-- WHY:   Dashboard metrics and the streak are SQL aggregates over `transcripts` (02 §7.4), so deleting the rows
--        would wipe them. A cleared row keeps only what the aggregates read (status, time, counts, durations,
--        latency); its text is NULL, so the FTS update trigger drops it from the search index too.
-- WHERE: Embedded by services/db.rs (`MIGRATIONS`); written by services/transcripts/update (ClearedAt) and read
--        by services/transcripts/selector (`cleared`).

ALTER TABLE transcripts ADD COLUMN cleared_at INTEGER;
