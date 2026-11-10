-- NOTE-03: 重建小记表以允许空正文；旧任务关联解除，任务及提醒保留。
CREATE TABLE note_folders (
    id TEXT PRIMARY KEY,
    parent_id TEXT REFERENCES note_folders(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE TABLE notes_new (
    id TEXT PRIMARY KEY,
    content TEXT NOT NULL CHECK(length(content) <= 20000),
    converted_task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    folder_id TEXT REFERENCES note_folders(id) ON DELETE CASCADE
);
INSERT INTO notes_new(id, content, converted_task_id, created_at, updated_at, name)
SELECT id, content, NULL, created_at, updated_at, '' FROM notes;
DROP TABLE notes;
ALTER TABLE notes_new RENAME TO notes;
CREATE INDEX idx_notes_updated ON notes(updated_at DESC);
CREATE UNIQUE INDEX ux_notes_converted_task ON notes(converted_task_id)
    WHERE converted_task_id IS NOT NULL;
CREATE INDEX idx_notes_folder ON notes(folder_id);
CREATE INDEX idx_note_folders_parent ON note_folders(parent_id);
