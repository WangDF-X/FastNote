-- schema v6：允许正文为空（有图片时），并新增日记图片附件两表。
CREATE TABLE journal_entries_v6 (
    id         TEXT PRIMARY KEY,
    entry_date TEXT NOT NULL UNIQUE CHECK (length(entry_date) = 10),
    content    TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

INSERT INTO journal_entries_v6(id, entry_date, content, created_at, updated_at)
    SELECT id, entry_date, content, created_at, updated_at FROM journal_entries;

DROP TABLE journal_entries;
ALTER TABLE journal_entries_v6 RENAME TO journal_entries;

CREATE INDEX idx_journal_date ON journal_entries(entry_date DESC);

CREATE TABLE journal_attachments (
    id             TEXT PRIMARY KEY,
    entry_date     TEXT NOT NULL REFERENCES journal_entries(entry_date) ON DELETE CASCADE,
    position       INTEGER NOT NULL,
    media_type     TEXT NOT NULL CHECK (media_type IN ('image/png', 'image/jpeg')),
    width          INTEGER NOT NULL CHECK (width > 0),
    height         INTEGER NOT NULL CHECK (height > 0),
    byte_size      INTEGER NOT NULL CHECK (byte_size > 0),
    thumbnail_size INTEGER NOT NULL CHECK (thumbnail_size > 0),
    created_at     TEXT NOT NULL
);

CREATE TABLE journal_attachment_data (
    attachment_id TEXT PRIMARY KEY REFERENCES journal_attachments(id) ON DELETE CASCADE,
    bytes         BLOB NOT NULL,
    thumbnail     BLOB NOT NULL
);

CREATE INDEX idx_journal_attachments_date
    ON journal_attachments(entry_date, position);
