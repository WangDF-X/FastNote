CREATE TABLE reminders (
    id TEXT PRIMARY KEY,
    task_id TEXT REFERENCES tasks(id) ON DELETE CASCADE,
    note_id TEXT REFERENCES notes(id) ON DELETE CASCADE,
    schedule_kind TEXT NOT NULL CHECK(schedule_kind IN ('once', 'daily', 'weekly')),
    local_date TEXT,
    local_time TEXT NOT NULL CHECK(length(local_time) = 5),
    weekday INTEGER,
    next_fire_at TEXT,
    last_handled_at TEXT,
    state TEXT NOT NULL CHECK(state IN ('active', 'paused', 'finished')),
    revision INTEGER NOT NULL DEFAULT 1 CHECK(revision >= 1),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK ((task_id IS NOT NULL AND note_id IS NULL)
        OR (task_id IS NULL AND note_id IS NOT NULL)),
    CHECK ((schedule_kind = 'once' AND local_date IS NOT NULL AND weekday IS NULL)
        OR (schedule_kind = 'daily' AND local_date IS NULL AND weekday IS NULL)
        OR (schedule_kind = 'weekly' AND local_date IS NULL AND weekday IS NOT NULL
            AND weekday BETWEEN 1 AND 7)),
    CHECK(state = 'active' OR next_fire_at IS NULL)
);
CREATE UNIQUE INDEX ux_reminders_task ON reminders(task_id) WHERE task_id IS NOT NULL;
CREATE UNIQUE INDEX ux_reminders_note ON reminders(note_id) WHERE note_id IS NOT NULL;
CREATE INDEX idx_reminders_next ON reminders(next_fire_at) WHERE state = 'active';
CREATE TABLE reminder_deliveries (
    id TEXT PRIMARY KEY,
    reminder_id TEXT NOT NULL REFERENCES reminders(id) ON DELETE CASCADE,
    rule_revision INTEGER NOT NULL CHECK(rule_revision >= 1),
    scheduled_for TEXT NOT NULL,
    outcome TEXT NOT NULL CHECK(outcome IN ('claimed', 'submitted', 'failed', 'missed', 'suppressed')),
    handled_at TEXT NOT NULL,
    UNIQUE(reminder_id, rule_revision, scheduled_for)
);
