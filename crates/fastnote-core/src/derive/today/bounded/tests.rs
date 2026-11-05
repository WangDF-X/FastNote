use std::cell::Cell;

use chrono::{DateTime, Duration, Utc};

use crate::{Clock, Priority, Task, TaskContext, TaskId};

use super::*;

struct FixedClock(Cell<usize>);
impl Clock for FixedClock {
    fn today(&self) -> NaiveDate {
        "2026-11-04".parse().unwrap()
    }
}
impl TaskClock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0.set(self.0.get() + 1);
        "2026-11-04T12:00:00Z".parse().unwrap()
    }
    fn local_datetime(&self, at: DateTime<Utc>) -> NaiveDateTime {
        at.naive_utc()
    }
}

fn item(number: usize) -> TaskSummary {
    let id: TaskId = format!("019a0fff-aaaa-7000-8000-{number:012x}")
        .parse()
        .unwrap();
    let at: DateTime<Utc> = "2026-11-04T12:00:00Z".parse().unwrap();
    let task = Task::new(
        id,
        format!("假任务 {number}"),
        at - Duration::seconds(number as i64),
    )
    .unwrap();
    let mut summary = TaskSummary::from_task(&task, TaskContext::default());
    summary.priority = [
        Priority::None,
        Priority::Low,
        Priority::Medium,
        Priority::High,
    ][number % 4];
    let today = at.date_naive();
    match number % 8 {
        0 => {
            summary.due_date = today.pred_opt();
        }
        1 => {
            summary.due_date = Some(today);
        }
        2 => {
            summary.start_date = Some(today);
        }
        3 => {
            summary.status = Some(TaskStatus::InProgress);
        }
        4 => {
            summary.status = Some(TaskStatus::Waiting);
            summary.status_changed_at -= Duration::days((number % 9) as i64);
        }
        5 => {}
        6 => {
            summary.start_date = today.succ_opt();
        }
        _ => {
            summary.status = Some(TaskStatus::Done);
            summary.completed_at = Some(at - Duration::seconds((number % 90) as i64));
        }
    }
    summary
}

#[test]
fn test_streaming_twenty_thousand_rows_keeps_only_bounded_prefixes_and_full_counts() {
    let clock = FixedClock(Cell::new(0));
    let mut collector =
        TodayCollector::new(clock.today(), &clock, TodayPreferences::default(), [200; 8]);
    for number in 1..=20_000 {
        collector.push(item(number));
        assert!(collector
            .groups
            .iter()
            .all(|group| group.len() <= 200 && group.capacity() <= 200));
    }
    assert_eq!(clock.0.get(), 1);
    let view = collector.finish();
    assert_eq!(view.open_count, 17_500);
    assert_eq!(view.completed_today_count, 2_500);
    assert!(view
        .sections
        .iter()
        .all(|section| section.total == 2_500 && section.items.len() == 200 && section.has_more));
}

#[test]
fn test_bounded_heaps_match_reference_sort_for_every_group_and_sort_mode() {
    let clock = FixedClock(Cell::new(0));
    for sort in [
        TodaySort::Priority,
        TodaySort::DueDate,
        TodaySort::CreatedDescending,
    ] {
        let input: Vec<_> = (1..=4_800).rev().map(item).collect();
        let mut collector = TodayCollector::new(
            clock.today(),
            &clock,
            TodayPreferences {
                sort,
                ..Default::default()
            },
            [200; 8],
        );
        for task in &input {
            collector.push(task.clone());
        }
        let view = collector.finish();
        for section in view.sections {
            let mut expected: Vec<_> = input
                .iter()
                .filter(|task| section_at(task, view.evaluated_at, &clock) == Some(section.kind))
                .cloned()
                .collect();
            expected
                .sort_by(|a, b| compare(a, b, section.kind, sort).then_with(|| a.id.cmp(&b.id)));
            let expected: Vec<_> = expected.into_iter().take(200).map(|task| task.id).collect();
            assert_eq!(
                section.items.iter().map(|task| task.id).collect::<Vec<_>>(),
                expected,
                "{sort:?} {:?}",
                section.kind
            );
        }
    }
}

#[test]
fn test_hidden_groups_allocate_nothing_and_keep_complete_counts_and_future_deadline() {
    let clock = FixedClock(Cell::new(0));
    let mut collector = TodayCollector::new(
        clock.today(),
        &clock,
        TodayPreferences {
            show_waiting: false,
            ..Default::default()
        },
        [1000; 8],
    );
    let mut task = item(4);
    task.due_date = clock.today().succ_opt();
    task.due_time = Some("00:01".parse().unwrap());
    collector.push(task);
    assert_eq!(collector.groups[Kind::Waiting as usize].capacity(), 0);
    let view = collector.finish();
    assert!(view.sections.is_empty());
    assert_eq!(view.open_count, 0);
    assert_eq!(
        view.next_overdue_at,
        Some("2026-11-05T00:01:00".parse().unwrap())
    );
}

#[test]
fn test_rank_order_is_total_even_across_different_group_or_sort_configurations() {
    let a = RankedTask {
        task: item(1),
        kind: Kind::DueToday,
        sort: TodaySort::Priority,
    };
    let b = RankedTask {
        task: item(2),
        kind: Kind::StartsToday,
        sort: TodaySort::DueDate,
    };
    assert_eq!(a.cmp(&b), b.cmp(&a).reverse());
    assert_eq!(a.partial_cmp(&a), Some(Ordering::Equal));
    assert!(a == a);
    assert!(a != b);
}
