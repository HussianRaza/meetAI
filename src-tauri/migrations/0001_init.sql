PRAGMA journal_mode=WAL;
PRAGMA foreign_keys=ON;

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS meetings (
    id          TEXT PRIMARY KEY NOT NULL,  -- UUID v4
    title       TEXT NOT NULL DEFAULT 'Untitled Meeting',
    platform    TEXT,
    status      TEXT NOT NULL DEFAULT 'recording', -- recording | processing | done | error
    started_at  INTEGER NOT NULL,  -- unix ms
    ended_at    INTEGER,
    duration_ms INTEGER,
    notes       TEXT,
    audio_path  TEXT,
    tags        TEXT NOT NULL DEFAULT '[]'  -- JSON array
);

CREATE TABLE IF NOT EXISTS transcript_segments (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    meeting_id  TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    source      TEXT NOT NULL,  -- 'you' | 'speaker'
    speaker_id  TEXT,
    speaker_name TEXT,
    text        TEXT NOT NULL,
    start_ms    INTEGER NOT NULL,
    end_ms      INTEGER NOT NULL,
    is_final    INTEGER NOT NULL DEFAULT 1,
    created_at  INTEGER NOT NULL DEFAULT (unixepoch() * 1000)
);

CREATE INDEX IF NOT EXISTS idx_segments_meeting ON transcript_segments(meeting_id);

CREATE VIRTUAL TABLE IF NOT EXISTS segments_fts USING fts5(
    text,
    content='transcript_segments',
    content_rowid='id'
);

CREATE TABLE IF NOT EXISTS summaries (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    meeting_id  TEXT NOT NULL UNIQUE REFERENCES meetings(id) ON DELETE CASCADE,
    overview    TEXT,
    decisions   TEXT NOT NULL DEFAULT '[]',  -- JSON array
    topics      TEXT NOT NULL DEFAULT '[]',  -- JSON array
    created_at  INTEGER NOT NULL DEFAULT (unixepoch() * 1000)
);

CREATE TABLE IF NOT EXISTS action_items (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    meeting_id  TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    text        TEXT NOT NULL,
    assignee    TEXT,
    due_date    TEXT,
    done        INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL DEFAULT (unixepoch() * 1000)
);

CREATE INDEX IF NOT EXISTS idx_action_items_meeting ON action_items(meeting_id);

CREATE TABLE IF NOT EXISTS kb_files (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    path        TEXT NOT NULL UNIQUE,
    sha256      TEXT NOT NULL,
    indexed_at  INTEGER NOT NULL DEFAULT (unixepoch() * 1000)
);

CREATE TABLE IF NOT EXISTS kb_chunks (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id     INTEGER NOT NULL REFERENCES kb_files(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    text        TEXT NOT NULL,
    breadcrumb  TEXT,
    embedding   BLOB  -- 384-dim f32 little-endian
);

CREATE INDEX IF NOT EXISTS idx_kb_chunks_file ON kb_chunks(file_id);

-- vec0 virtual table for KB chunk ANN search (loaded after sqlite-vec extension)
-- Created separately after extension is loaded at runtime.

CREATE TABLE IF NOT EXISTS meeting_chunks (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    meeting_id  TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    text        TEXT NOT NULL,
    start_ms    INTEGER,
    end_ms      INTEGER,
    embedding   BLOB  -- 384-dim f32 little-endian
);

CREATE INDEX IF NOT EXISTS idx_meeting_chunks_meeting ON meeting_chunks(meeting_id);

-- vec0 virtual tables are created at runtime after sqlite-vec extension loads.

CREATE TABLE IF NOT EXISTS jobs (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    meeting_id  TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL,  -- Embed | Diarize | Summarize | RetranscribeParakeet
    status      TEXT NOT NULL DEFAULT 'pending',  -- pending | running | done | error
    error       TEXT,
    started_at  INTEGER,
    finished_at INTEGER,
    created_at  INTEGER NOT NULL DEFAULT (unixepoch() * 1000)
);

CREATE TABLE IF NOT EXISTS pre_context_chunks (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    meeting_id  TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    text        TEXT NOT NULL,
    embedding   BLOB
);

CREATE VIRTUAL TABLE IF NOT EXISTS meetings_fts USING fts5(
    title,
    content='meetings',
    content_rowid='rowid'
);
