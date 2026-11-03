use chrono::{DateTime, Duration, Utc};

use crate::{Clock, Priority, Task, TaskContext, TaskId};

use super::*;

struct FixedClock {
    now: DateTime<Utc>,
    offset_hours: i64,
}

impl Clock for FixedClock {
    fn today(&self) -> NaiveDate {
        self.local_date(self.now)
    }
}
impl TaskClock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.now
    }
    fn local_datetime(&self, at: DateTime<Utc>) -> NaiveDateTime {
        (at + Duration::hours(self.offset_hours)).naive_utc()
    }
}

fn clock() -> FixedClock {
    FixedClock {
        now: "2026-11-03T12:00:00Z".parse().expect("fixed timestamp"),
        offset_hours: 0,
    }
}

fn item(number: usize, status: TaskStatus) -> TaskSummary {
    let id: TaskId = format!("019a0fff-aaaa-7000-8000-{number:012x}")
        .parse()
        .expect("fake UUIDv7");
    let clock = clock();
    let task = Task::new(id, format!("假任务 {number}"), clock.now())
        .expect("valid title")
        .with_status(status, clock.now())
        .expect("valid status");
    TaskSummary::from_task(&task, TaskContext::default())
}

fn view(items: Vec<TaskSummary>, preferences: TodayPreferences) -> TodayView {
    let clock = clock();
    build_today_view(items, clock.today(), &clock, preferences, [200; 8])
}

#[test]
fn test_eight_sections_have_stable_order_and_each_task_occurs_once() {
    let today = clock().today();
    let mut items: Vec<_> = [
        TaskStatus::Todo,
        TaskStatus::Todo,
        TaskStatus::Todo,
        TaskStatus::InProgress,
        TaskStatus::Waiting,
        TaskStatus::Todo,
        TaskStatus::Todo,
        TaskStatus::Done,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, status)| item(index + 1, status))
    .collect();
    items[0].due_date = today.pred_opt();
    items[1].due_date = Some(today);
    items[2].start_date = Some(today);
    items[6].due_date = today.succ_opt();
    let view = view(items, TodayPreferences::default());
    assert_eq!(
        view.sections
            .iter()
            .map(|section| section.kind)
            .collect::<Vec<_>>(),
        Kind::ALL
    );
    assert_eq!(view.open_count, 7);
    assert_eq!(view.overdue_count, 1);
    assert_eq!(view.completed_today_count, 1);
    assert_eq!(view.tomorrow_count, 1);
    assert_eq!(
        view.sections
            .iter()
            .map(|section| section.total)
            .sum::<u32>(),
        8
    );
    assert!(view.sections[6].collapsed_default && view.sections[7].collapsed_default);
}

#[test]
fn test_priority_grouping_wins_over_waiting_in_progress_and_tomorrow() {
    let clock = clock();
    let today = clock.today();
    let mut task = item(1, TaskStatus::Waiting);
    task.due_date = today.pred_opt();
    task.start_date = Some(today);
    assert_eq!(today_section(&task, today, &clock), Some(Kind::Overdue));
    task.due_date = Some(today);
    assert_eq!(today_section(&task, today, &clock), Some(Kind::DueToday));
    task.due_date = today.succ_opt();
    assert_eq!(today_section(&task, today, &clock), Some(Kind::StartsToday));
    task.start_date = None;
    assert_eq!(today_section(&task, today, &clock), Some(Kind::Waiting));
    task.status = Some(TaskStatus::InProgress);
    assert_eq!(today_section(&task, today, &clock), Some(Kind::InProgress));
    task.start_date = today.succ_opt();
    assert_eq!(today_section(&task, today, &clock), Some(Kind::Tomorrow));
    task.start_date = today.succ_opt().and_then(|date| date.succ_opt());
    task.due_date = task.start_date;
    assert_eq!(today_section(&task, today, &clock), None);
}

#[test]
fn test_archived_stages_old_completed_and_unmatched_tasks_are_excluded() {
    let clock = clock();
    let today = clock.today();
    let mut task = item(1, TaskStatus::Todo);
    task.context.read_only = true;
    task.due_date = today.pred_opt();
    assert!(!is_overdue(&task, clock.local_datetime(clock.now())));
    assert_eq!(today_section(&task, today, &clock), None);
    task.context.read_only = false;
    task.kind = TaskKind::Stage;
    assert!(!is_overdue(&task, clock.local_datetime(clock.now())));
    assert_eq!(today_section(&task, today, &clock), None);
    task.kind = TaskKind::Task;
    task.status = None;
    assert_eq!(today_section(&task, today, &clock), None);
    task.status = Some(TaskStatus::Done);
    task.completed_at = Some(clock.now - Duration::days(1));
    assert_eq!(today_section(&task, today, &clock), None);
    task.completed_at = None;
    assert_eq!(today_section(&task, today, &clock), None);
    task.status = Some(TaskStatus::Todo);
    task.due_date = None;
    task.start_date = today.pred_opt();
    assert!(!is_overdue(&task, clock.local_datetime(clock.now())));
    assert_eq!(today_section(&task, today, &clock), None);
}

#[test]
fn test_hiding_groups_does_not_reclassify_or_count_hidden_open_items() {
    let mut progress = item(1, TaskStatus::InProgress);
    progress.due_date = clock().today().succ_opt();
    let view = view(
        vec![
            progress,
            item(2, TaskStatus::Waiting),
            item(3, TaskStatus::Todo),
            item(4, TaskStatus::Done),
        ],
        TodayPreferences {
            show_in_progress: false,
            show_waiting: false,
            show_unscheduled: false,
            show_completed: false,
            ..Default::default()
        },
    );
    assert!(view.sections.is_empty());
    assert_eq!(view.open_count, 0);
    assert_eq!(view.tomorrow_count, 0);
    assert_eq!(view.completed_today_count, 1);
}

#[test]
fn test_local_dates_control_completed_today_and_negative_waiting_is_clamped() {
    let clock = FixedClock {
        now: "2026-11-03T01:00:00Z".parse().unwrap(),
        offset_hours: 8,
    };
    let mut completed = item(1, TaskStatus::Done);
    completed.completed_at = Some("2026-11-02T17:00:00Z".parse().unwrap());
    assert_eq!(
        today_section(&completed, clock.today(), &clock),
        Some(Kind::CompletedToday)
    );
    completed.context.read_only = true;
    assert_eq!(today_section(&completed, clock.today(), &clock), None);
    let mut waiting = item(2, TaskStatus::Waiting);
    waiting.status_changed_at = clock.now + Duration::days(1);
    waiting.context.blocking_count = 2;
    derive_task_summary(&mut waiting, clock.today(), &clock);
    assert_eq!(waiting.waiting_days, Some(0));
    assert!(waiting.is_blocked);
    waiting.status_changed_at = clock.now - Duration::days(3);
    derive_task_summary(&mut waiting, clock.today(), &clock);
    assert_eq!(waiting.waiting_days, Some(3));
}

#[test]
fn test_date_sorts_use_null_last_priority_and_stable_id_tiebreaks() {
    let today = clock().today();
    let mut early = item(1, TaskStatus::Todo);
    early.start_date = Some(today);
    early.due_date = today.succ_opt();
    early.priority = Priority::Low;
    let mut high = item(2, TaskStatus::Todo);
    high.start_date = Some(today);
    high.priority = Priority::High;
    let input = vec![early.clone(), high.clone()];
    let priority = view(input.clone(), TodayPreferences::default());
    assert_eq!(priority.sections[0].items[0].id, high.id);
    let due = view(
        input,
        TodayPreferences {
            sort: TodaySort::DueDate,
            ..Default::default()
        },
    );
    assert_eq!(due.sections[0].items[0].id, early.id);
    let mut tie = high.clone();
    tie.id = item(3, TaskStatus::Todo).id;
    let stable = view(vec![tie, high.clone()], TodayPreferences::default());
    assert_eq!(stable.sections[0].items[0].id, high.id);
}

#[test]
fn test_waiting_unscheduled_completed_and_tomorrow_ignore_date_sort_override() {
    let mut earlier = item(1, TaskStatus::Waiting);
    earlier.status_changed_at -= Duration::days(2);
    earlier.priority = Priority::Low;
    let mut newer = item(2, TaskStatus::Waiting);
    newer.priority = Priority::High;
    let waiting = view(
        vec![newer, earlier.clone()],
        TodayPreferences {
            sort: TodaySort::CreatedDescending,
            ..Default::default()
        },
    );
    assert_eq!(waiting.sections[0].items[0].id, earlier.id);
    let mut newer = item(2, TaskStatus::Todo);
    newer.created_at += Duration::seconds(1);
    let unscheduled = view(
        vec![item(1, TaskStatus::Todo), newer.clone()],
        TodayPreferences::default(),
    );
    assert_eq!(unscheduled.sections[0].items[0].id, newer.id);
    let mut completed = item(2, TaskStatus::Done);
    completed.completed_at = Some(clock().now + Duration::seconds(1));
    let done = view(
        vec![item(1, TaskStatus::Done), completed.clone()],
        TodayPreferences::default(),
    );
    assert_eq!(done.sections[0].items[0].id, completed.id);
    let tomorrow = clock().today().succ_opt();
    earlier.status = Some(TaskStatus::Todo);
    earlier.start_date = tomorrow;
    newer.start_date = tomorrow;
    newer.created_at = earlier.created_at - Duration::seconds(1);
    let expected = earlier.id;
    let tomorrow = view(
        vec![earlier, newer.clone()],
        TodayPreferences {
            sort: TodaySort::CreatedDescending,
            tomorrow_collapsed: false,
            ..Default::default()
        },
    );
    assert_eq!(tomorrow.sections[0].items[0].id, expected);
    assert!(!tomorrow.sections[0].collapsed_default);
}

#[test]
fn test_created_descending_applies_to_date_groups() {
    let mut first = item(1, TaskStatus::Todo);
    first.due_date = Some(clock().today());
    let mut second = item(2, TaskStatus::Todo);
    second.due_date = first.due_date;
    second.created_at += Duration::seconds(1);
    let view = view(
        vec![first, second.clone()],
        TodayPreferences {
            sort: TodaySort::CreatedDescending,
            ..Default::default()
        },
    );
    assert_eq!(view.sections[0].items[0].id, second.id);
}

#[test]
fn test_pagination_preserves_full_counts_and_caps_each_group_at_one_thousand() {
    let items: Vec<_> = (1..=1001)
        .map(|index| item(index, TaskStatus::Todo))
        .collect();
    let clock = clock();
    for (requested, loaded) in [(0, 200), (200, 200), (400, 400), (5000, 1000)] {
        let view = build_today_view(
            items.clone(),
            clock.today(),
            &clock,
            TodayPreferences::default(),
            [requested; 8],
        );
        assert_eq!(view.open_count, 1001);
        assert_eq!(view.sections[0].total, 1001);
        assert_eq!(view.sections[0].items.len(), loaded);
        assert!(view.sections[0].has_more);
    }
    let exact = build_today_view(
        items[..200].to_vec(),
        clock.today(),
        &clock,
        TodayPreferences::default(),
        [200; 8],
    );
    assert!(!exact.sections[0].has_more);
}

#[test]
fn test_weekends_month_and_year_boundaries_use_natural_next_day() {
    for (now, tomorrow) in [
        ("2026-11-01T12:00:00Z", "2026-11-02"),
        ("2026-12-31T12:00:00Z", "2027-01-01"),
        ("2024-02-28T12:00:00Z", "2024-02-29"),
    ] {
        let clock = FixedClock {
            now: now.parse().unwrap(),
            offset_hours: 0,
        };
        let mut task = item(1, TaskStatus::Todo);
        task.due_date = Some(tomorrow.parse().unwrap());
        assert_eq!(
            today_section(&task, clock.today(), &clock),
            Some(Kind::Tomorrow)
        );
    }
    let clock = clock();
    let mut task = item(1, TaskStatus::Todo);
    task.start_date = Some(NaiveDate::MAX);
    assert_eq!(
        today_section(&task, NaiveDate::MAX, &clock),
        Some(Kind::StartsToday)
    );
    task.start_date = NaiveDate::MAX.pred_opt();
    assert_eq!(today_section(&task, NaiveDate::MAX, &clock), None);
}
