use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, Utc};

use crate::{Clock, Task, TaskContext};

use super::*;

struct FixedClock(DateTime<Utc>, i64);
impl Clock for FixedClock {
    fn today(&self) -> NaiveDate {
        self.local_date(self.0)
    }
}
impl TaskClock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
    fn local_datetime(&self, at: DateTime<Utc>) -> NaiveDateTime {
        (at + Duration::hours(self.1)).naive_utc()
    }
}

fn task(number: usize, due: &str) -> TaskSummary {
    let task = Task::new(
        format!("019a0fff-aaaa-7000-8000-{number:012x}")
            .parse()
            .unwrap(),
        "假任务".to_owned(),
        "2026-11-03T08:00:00Z".parse().unwrap(),
    )
    .unwrap();
    let mut summary = TaskSummary::from_task(&task, TaskContext::default());
    summary.due_date = Some("2026-11-03".parse().unwrap());
    summary.due_time = crate::parse_task_time(due).unwrap();
    summary
}

#[test]
fn test_due_time_sets_overdue_only_after_deadline_without_changing_focus_section() {
    let task = task(1, "15:30");
    for (now, expected) in [
        ("2026-11-03T15:29:59Z", false),
        ("2026-11-03T15:30:00Z", false),
        ("2026-11-03T15:30:01Z", true),
    ] {
        let clock = FixedClock(now.parse().unwrap(), 0);
        assert_eq!(
            today_section(&task, clock.today(), &clock),
            Some(Kind::Focus)
        );
        assert_eq!(
            is_overdue(&task, clock.local_datetime(clock.now())),
            expected
        );
    }
}

#[test]
fn test_all_day_legacy_task_stays_due_today_until_next_day() {
    let task = task(1, "");
    let clock = FixedClock("2026-11-03T23:59:59Z".parse().unwrap(), 0);
    assert_eq!(
        today_section(&task, clock.today(), &clock),
        Some(Kind::Focus)
    );
    assert!(!is_overdue(&task, clock.local_datetime(clock.now())));
    let clock = FixedClock(clock.0 + Duration::seconds(1), 0);
    assert_eq!(
        today_section(&task, clock.today(), &clock),
        Some(Kind::Focus)
    );
    assert!(is_overdue(&task, clock.local_datetime(clock.now())));
}

#[test]
fn test_deadline_is_compared_in_device_local_time_not_utc() {
    let task = task(1, "15:30");
    let clock = FixedClock("2026-11-03T07:31:00Z".parse().unwrap(), 8);
    assert_eq!(
        today_section(&task, clock.today(), &clock),
        Some(Kind::Focus)
    );
    assert!(is_overdue(&task, clock.local_datetime(clock.now())));
    let clock = FixedClock(clock.0, 0);
    assert_eq!(
        today_section(&task, clock.today(), &clock),
        Some(Kind::Focus)
    );
    assert!(!is_overdue(&task, clock.local_datetime(clock.now())));
}

#[test]
fn test_today_clock_boundary_covers_pagination_and_collapsed_later_section() {
    let clock = FixedClock("2026-11-03T08:00:00Z".parse().unwrap(), 0);
    let mut items: Vec<_> = (1..=201).map(|index| task(index, "16:00")).collect();
    items[200].due_time = crate::parse_task_time("09:00").unwrap();
    let view = build_today_view(
        items,
        clock.today(),
        &clock,
        TodayPreferences::default(),
        [200; 4],
    );
    assert_eq!(
        view.next_overdue_at,
        Some("2026-11-03T09:00:00".parse().unwrap())
    );
    assert_eq!(view.sections[0].total, 201);
    assert_eq!(view.sections[0].items.len(), 200);
    let mut later = task(1, "16:00");
    later.due_date = Some("2026-11-04".parse().unwrap());
    let view = build_today_view(
        vec![later],
        clock.today(),
        &clock,
        TodayPreferences::default(),
        [200; 4],
    );
    assert_eq!(view.sections[0].kind, Kind::Later);
    assert!(view.sections[0].collapsed_default);
    assert_eq!(view.open_count, 1);
    assert_eq!(
        view.next_overdue_at,
        Some("2026-11-04T16:00:00".parse().unwrap())
    );
}

#[test]
fn test_completed_and_archived_tasks_do_not_schedule_deadline_refresh() {
    let clock = FixedClock("2026-11-03T08:00:00Z".parse().unwrap(), 0);
    let mut completed = task(1, "09:00");
    completed.status = Some(TaskStatus::Done);
    completed.completed_at = Some(clock.0);
    let mut archived = task(2, "10:00");
    archived.context.read_only = true;
    let view = build_today_view(
        vec![completed, archived],
        clock.today(),
        &clock,
        TodayPreferences::default(),
        [200; 4],
    );
    assert!(view.next_overdue_at.is_none());
    assert_eq!(view.overdue_count, 0);
}

#[test]
fn test_same_date_sort_orders_times_and_keeps_all_day_due_last() {
    let clock = FixedClock("2026-11-03T08:00:00Z".parse().unwrap(), 0);
    let view = build_today_view(
        vec![task(1, ""), task(2, "16:00"), task(3, "09:00")],
        clock.today(),
        &clock,
        TodayPreferences::default(),
        [200; 4],
    );
    assert_eq!(
        view.sections[0]
            .items
            .iter()
            .map(|task| task.due_time)
            .collect::<Vec<_>>(),
        vec![
            crate::parse_task_time("09:00").unwrap(),
            crate::parse_task_time("16:00").unwrap(),
            None
        ]
    );
}

#[test]
fn test_same_date_start_sort_places_all_day_before_specific_start_times() {
    let clock = FixedClock("2026-11-03T08:00:00Z".parse().unwrap(), 0);
    let mut items: Vec<_> = (1..=3).map(|number| task(number, "")).collect();
    for (item, time) in items.iter_mut().zip(["10:00", "", "09:00"]) {
        item.start_date = item.due_date.take();
        item.start_time = crate::parse_task_time(time).unwrap();
    }
    let view = build_today_view(
        items,
        clock.today(),
        &clock,
        TodayPreferences::default(),
        [200; 4],
    );
    assert_eq!(view.sections[0].kind, Kind::Focus);
    assert_eq!(
        view.sections[0]
            .items
            .iter()
            .map(|task| task.start_time)
            .collect::<Vec<_>>(),
        vec![
            None,
            crate::parse_task_time("09:00").unwrap(),
            crate::parse_task_time("10:00").unwrap()
        ]
    );
}
