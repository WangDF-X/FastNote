use fastnote_core::{Task, TaskContext, TaskDetail, TaskId};

pub(super) fn id(number: usize) -> TaskId {
    format!("019a0fff-aaaa-7000-8000-{number:012x}")
        .parse()
        .expect("fake UUIDv7")
}

pub(super) fn detail(number: usize) -> TaskDetail {
    TaskDetail {
        task: Task::new(
            id(number),
            "假任务".to_owned(),
            "2026-11-03T12:00:00Z".parse().expect("fixed timestamp"),
        )
        .expect("valid fake task"),
        context: TaskContext::default(),
        is_overdue: false,
        is_blocked: false,
    }
}
