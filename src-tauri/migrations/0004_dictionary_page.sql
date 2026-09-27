-- SOURCE OF TRUTH KEYWORDS: migration 0004, dictionary.entries, polish.dictionary rename, dictionary page, settings key move
-- WHAT:  Moves the saved dictionary from the `polish.dictionary` settings row to `dictionary.entries`.
-- WHY:   The dictionary left Settings → Cleanup for its own sidebar page, and a setting key is always prefixed by
--        its section (registry/settings tests), so the key changed with the section. Renaming the stored row keeps
--        every term the user saved; nothing else reads the old key. `dictionary.enabled` needs no row: an unset
--        setting takes its registry default (on).
-- WHERE: Embedded by services/db.rs (`MIGRATIONS`); the row is read through registry/settings/reads.rs `dictionary`.

UPDATE settings SET key = 'dictionary.entries' WHERE key = 'polish.dictionary';
