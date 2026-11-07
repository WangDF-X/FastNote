use chrono::{DateTime, Duration, NaiveDateTime, Utc};

use crate::{Clock, Task, TaskContext, TaskId};

use super::*;

struct FixedClock;

impl Clock for FixedClock {
    fn today(&self) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 7).expect("fixed date")
    }
}

impl TaskClock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        "2026-11-07T12:00:00Z".parse().expect("fixed timestamp")
    }

    fn local_datetime(&self, instant: DateTime<Utc>) -> NaiveDateTime {
        (instant + Duration::hours(8)).naive_utc()
    }
}

fn item(number: usize, status: TaskStatus) -> TaskSummary {
    let id: TaskId = format!("019a0fff-aaaa-7000-8000-{number:012x}")
        .parse()
        .expect("fake UUID");
    let task = Task::new(id, format!("测试任务 {number}"), FixedClock.now())
        .expect("title")
        .with_status(status, FixedClock.now())
        .expect("status");
    TaskSummary::from_task(&task, TaskContext::default())
}

#[test]
fn test_agenda_groups_deduplicates_and_filters_archived() {
    let date = FixedClock.today();
    let mut due = item(1, TaskStatus::Todo);
    due.due_date = Some(date);
    due.start_date = Some(date);
    let mut start = item(2, TaskStatus::Waiting);
    start.start_date = Some(date);
    let mut ongoing = item(3, TaskStatus::InProgress);
    ongoing.start_date = date.pred_opt();
    let mut completed = item(4, TaskStatus::Done);
    completed.completed_at = Some("2026-11-06T17:00:00Z".parse().expect("timestamp"));
    let mut archived = due.clone();
    archived.context.read_only = true;
    let unscheduled = item(5, TaskStatus::Todo);
    let agenda = build_day_agenda(
        date,
        [due, start, ongoing, completed, archived, unscheduled],
        &FixedClock,
    );
    assert_eq!(agenda.due.len(), 1);
    assert_eq!(agenda.start.len(), 1);
    assert_eq!(agenda.ongoing.len(), 1);
    assert_eq!(agenda.completed.len(), 1);
    assert!(!agenda.overload);
}

#[test]
fn test_overload_counts_only_active_tasks() {
    let date = FixedClock.today();
    let tasks = (0..9).map(|n| {
        let mut task = item(n, TaskStatus::Todo);
        task.due_date = Some(date);
        task
    });
    assert!(build_day_agenda(date, tasks, &FixedClock).overload);
}
