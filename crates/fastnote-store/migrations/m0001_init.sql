CREATE TABLE app_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL,
    app_version TEXT NOT NULL
);
CREATE TABLE plans (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 100),
    description TEXT NOT NULL DEFAULT '',
    start_date TEXT CHECK (start_date IS NULL OR length(start_date) = 10),
    target_end_date TEXT CHECK (target_end_date IS NULL OR length(target_end_date) = 10),
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'archived')),
    sort_order INTEGER NOT NULL DEFAULT 0,
    archived_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (start_date IS NULL OR target_end_date IS NULL OR start_date <= target_end_date),
    CHECK ((status = 'active' AND archived_at IS NULL)
        OR (status = 'archived' AND archived_at IS NOT NULL))
);
CREATE TABLE tasks (
    id TEXT PRIMARY KEY,
    plan_id TEXT REFERENCES plans(id) ON DELETE CASCADE,
    parent_id TEXT REFERENCES tasks(id) ON DELETE CASCADE,
    kind TEXT NOT NULL DEFAULT 'task' CHECK (kind IN ('task', 'stage')),
    title TEXT NOT NULL CHECK (length(trim(title)) BETWEEN 1 AND 200),
    description TEXT NOT NULL DEFAULT '',
    status TEXT CHECK (status IN ('todo', 'in_progress', 'waiting', 'done')),
    priority INTEGER NOT NULL DEFAULT 0 CHECK (priority BETWEEN 0 AND 3),
    start_date TEXT CHECK (start_date IS NULL OR length(start_date) = 10),
    due_date TEXT CHECK (due_date IS NULL OR length(due_date) = 10),
    completed_at TEXT,
    status_changed_at TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (start_date IS NULL OR due_date IS NULL OR start_date <= due_date),
    CHECK ((kind = 'task' AND status IS NOT NULL) OR (kind = 'stage' AND status IS NULL)),
    CHECK ((kind = 'stage') OR (status = 'done' AND completed_at IS NOT NULL)
        OR (status <> 'done' AND completed_at IS NULL)),
    CHECK (kind = 'task' OR (plan_id IS NOT NULL AND parent_id IS NULL AND priority = 0))
);
CREATE TABLE task_dependencies (
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    depends_on_task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    PRIMARY KEY (task_id, depends_on_task_id),
    CHECK (task_id <> depends_on_task_id)
);
CREATE TABLE notes (
    id TEXT PRIMARY KEY,
    content TEXT NOT NULL CHECK (length(trim(content)) > 0),
    converted_task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE UNIQUE INDEX ux_notes_converted_task ON notes(converted_task_id)
    WHERE converted_task_id IS NOT NULL;
CREATE TABLE journal_entries (
    id TEXT PRIMARY KEY,
    entry_date TEXT NOT NULL UNIQUE CHECK (length(entry_date) = 10),
    content TEXT NOT NULL CHECK (length(trim(content)) > 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE TABLE app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX idx_tasks_plan ON tasks(plan_id);
CREATE INDEX idx_tasks_parent ON tasks(parent_id);
CREATE INDEX idx_tasks_status ON tasks(status);
CREATE INDEX idx_tasks_due ON tasks(due_date) WHERE status <> 'done';
CREATE INDEX idx_tasks_start ON tasks(start_date) WHERE status <> 'done';
CREATE INDEX idx_tasks_updated ON tasks(updated_at);
CREATE INDEX idx_tasks_sort ON tasks(parent_id, sort_order);
CREATE INDEX idx_dep_prereq ON task_dependencies(depends_on_task_id);
CREATE INDEX idx_notes_updated ON notes(updated_at DESC);
CREATE INDEX idx_journal_date ON journal_entries(entry_date DESC);
