-- D-TODAY-02：旧任务仍为全天；只追加可空时分，不改日期/正文/提醒。
ALTER TABLE tasks ADD COLUMN start_time TEXT CHECK (
    start_time IS NULL OR (
        kind = 'task' AND start_date IS NOT NULL
        AND length(start_time) = 5
        AND start_time GLOB '[0-2][0-9]:[0-5][0-9]'
        AND substr(start_time, 1, 2) <= '23'
    )
);

ALTER TABLE tasks ADD COLUMN due_time TEXT CHECK (
    due_time IS NULL OR (
        kind = 'task' AND due_date IS NOT NULL
        AND length(due_time) = 5
        AND due_time GLOB '[0-2][0-9]:[0-5][0-9]'
        AND substr(due_time, 1, 2) <= '23'
        AND (start_time IS NULL OR start_date <> due_date OR start_time <= due_time)
    )
);
