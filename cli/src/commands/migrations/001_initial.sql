CREATE TABLE IF NOT EXISTS cas_journal (
    path            TEXT    NOT NULL,
    session_id      TEXT    NOT NULL,
    head_blob       TEXT,
    last_seen       TEXT,
    last_written    TEXT,
    updated_at      INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    PRIMARY KEY (path, session_id)
);

CREATE INDEX IF NOT EXISTS idx_cas_journal_session
    ON cas_journal(session_id);

CREATE INDEX IF NOT EXISTS idx_cas_journal_updated
    ON cas_journal(updated_at);
