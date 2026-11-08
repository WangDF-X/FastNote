-- D-TODAY-09：调用方必须在事务开始前关闭外键，并在提交前执行 foreign_key_check。
-- 只转换等待状态；所有其他字段，包括状态时间和归档任务，原样保留。
CREATE TABLE tasks_new (
    id TEXT PRIMARY KEY,
    plan_id TEXT REFERENCES plans(id) ON DELETE CASCADE,
    parent_id TEXT REFERENCES tasks(id) ON DELETE CASCADE,
    kind TEXT NOT NULL DEFAULT 'task' CHECK (kind IN ('task', 'stage')),
    title TEXT NOT NULL CHECK (length(trim(title)) BETWEEN 1 AND 200),
    description TEXT NOT NULL DEFAULT '',
    status TEXT CHECK (status IN ('todo', 'in_progress', 'done')),
    priority INTEGER NOT NULL DEFAULT 0 CHECK (priority BETWEEN 0 AND 3),
    start_date TEXT CHECK (start_date IS NULL OR length(start_date) = 10),
    due_date TEXT CHECK (due_date IS NULL OR length(due_date) = 10),
    completed_at TEXT,
    status_changed_at TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    start_time TEXT CHECK (start_time IS NULL OR (
        kind = 'task' AND start_date IS NOT NULL AND length(start_time) = 5
        AND start_time GLOB '[0-2][0-9]:[0-5][0-9]'
        AND substr(start_time, 1, 2) <= '23'
    )),
    due_time TEXT CHECK (due_time IS NULL OR (
        kind = 'task' AND due_date IS NOT NULL AND length(due_time) = 5
        AND due_time GLOB '[0-2][0-9]:[0-5][0-9]'
        AND substr(due_time, 1, 2) <= '23'
        AND (start_time IS NULL OR start_date <> due_date OR start_time <= due_time)
    )),
    CHECK (start_date IS NULL OR due_date IS NULL OR start_date <= due_date),
    CHECK ((kind = 'task' AND status IS NOT NULL) OR (kind = 'stage' AND status IS NULL)),
    CHECK ((kind = 'stage') OR (status = 'done' AND completed_at IS NOT NULL)
        OR (status <> 'done' AND completed_at IS NULL)),
    CHECK (kind = 'task' OR (plan_id IS NOT NULL AND parent_id IS NULL AND priority = 0))
);
INSERT INTO tasks_new (id, plan_id, parent_id, kind, title, description, status, priority,
    start_date, due_date, completed_at, status_changed_at, sort_order, created_at, updated_at,
    start_time, due_time)
SELECT id, plan_id, parent_id, kind, title, description,
    CASE status WHEN 'waiting' THEN 'todo' ELSE status END, priority,
    start_date, due_date, completed_at, status_changed_at, sort_order, created_at, updated_at,
    start_time, due_time FROM tasks;
DROP TABLE tasks;
ALTER TABLE tasks_new RENAME TO tasks;
CREATE INDEX idx_tasks_plan ON tasks(plan_id);
CREATE INDEX idx_tasks_parent ON tasks(parent_id);
CREATE INDEX idx_tasks_status ON tasks(status);
CREATE INDEX idx_tasks_due ON tasks(due_date) WHERE status <> 'done';
CREATE INDEX idx_tasks_start ON tasks(start_date) WHERE status <> 'done';
CREATE INDEX idx_tasks_updated ON tasks(updated_at);
CREATE INDEX idx_tasks_sort ON tasks(parent_id, sort_order);

INSERT INTO app_settings (key, value, updated_at)
SELECT 'today.later_collapsed', value, updated_at FROM app_settings
WHERE key = 'today.tomorrow_collapsed'
ON CONFLICT(key) DO NOTHING;
DELETE FROM app_settings WHERE key IN (
    'today.show_in_progress', 'today.show_waiting', 'today.tomorrow_collapsed'
);
