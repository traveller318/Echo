-- SOURCE OF TRUTH KEYWORDS: migration 0001, init schema, transcripts table, transcripts_fts, settings table, FTS sync triggers
-- WHAT:  The first schema version (02 §7.2): the `transcripts` history table with its created_at index, the
--        external-content FTS5 index over final_text with the triggers that keep it in sync, and the `settings`
--        key/value table.
-- WHY:   External-content FTS never updates itself (05 decision log), so every insert, final_text update and
--        delete on `transcripts` writes the matching 'delete' + insert rows into `transcripts_fts`. The index is
--        keyed by the implicit rowid, which VACUUM may renumber on a table without an INTEGER PRIMARY KEY
--        (05 W29): the database is never vacuumed and backups use the online backup API. Migrations are forward
--        only; a shipped file is never edited, a change is a new numbered file.
-- WHERE: Embedded by services/db.rs (`MIGRATIONS`) and applied on every open, in the app and in service tests.

CREATE TABLE transcripts (
  id TEXT PRIMARY KEY,
  created_at INTEGER NOT NULL,
  status TEXT NOT NULL,
  raw_text TEXT,
  final_text TEXT,
  audio_path TEXT,
  duration_ms INTEGER,
  speech_ms INTEGER,
  word_count INTEGER,
  engine_id TEXT,
  polisher_ids TEXT,
  language TEXT,
  latency_ms INTEGER,
  app_name TEXT,
  error_code TEXT
);

CREATE INDEX transcripts_created ON transcripts(created_at DESC);

CREATE VIRTUAL TABLE transcripts_fts USING fts5(
  final_text,
  content='transcripts',
  content_rowid='rowid'
);

CREATE TRIGGER transcripts_fts_insert AFTER INSERT ON transcripts BEGIN
  INSERT INTO transcripts_fts(rowid, final_text) VALUES (new.rowid, new.final_text);
END;

CREATE TRIGGER transcripts_fts_update AFTER UPDATE OF final_text ON transcripts BEGIN
  INSERT INTO transcripts_fts(transcripts_fts, rowid, final_text) VALUES ('delete', old.rowid, old.final_text);
  INSERT INTO transcripts_fts(rowid, final_text) VALUES (new.rowid, new.final_text);
END;

CREATE TRIGGER transcripts_fts_delete AFTER DELETE ON transcripts BEGIN
  INSERT INTO transcripts_fts(transcripts_fts, rowid, final_text) VALUES ('delete', old.rowid, old.final_text);
END;

CREATE TABLE settings (
  key TEXT PRIMARY KEY,
  value_json TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);
