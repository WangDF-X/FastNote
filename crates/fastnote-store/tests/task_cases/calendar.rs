use fastnote_core::{
    Command, Patch, Query, QueryResult, TaskPatch, TaskRepository, TaskService, TaskStatus,
};

use crate::task_support::{id, now, FixedClock, Fixture, PLAN};

#[test]
fn test_calendar_excludes_archived_plan_tasks() {
    let mut fixture = Fixture::new();
    fixture.plan();
    let task = fixture.add(1);
    fixture
        .external()
        .execute(
            "UPDATE tasks SET plan_id=?1,start_date='2026-11-05' WHERE id=?2",
            rusqlite::params![PLAN, task.id.to_string()],
        )
        .unwrap();
    fixture
        .external()
        .execute(
            "UPDATE plans SET status='archived', archived_at='2026-11-03T12:00:00Z' WHERE id=?1",
            [PLAN],
        )
        .unwrap();
    let result = fixture
        .store
        .query_tasks(
            Query::Calendar {
                start: "2026-11-05".parse().unwrap(),
                end: "2026-11-05".parse().unwrap(),
            },
            FixedClock(now(), 0),
        )
        .unwrap();
    let QueryResult::Calendar(days) = result else {
        panic!("calendar result expected")
    };
    assert!(days[0].1.start.is_empty());
}

#[test]
fn test_calendar_creation_atomically_sets_start_without_due() {
    let mut fixture = Fixture::new();
    let date = "2026-11-10".parse().unwrap();
    let result = TaskService::new(&mut fixture.store, FixedClock(now(), 0))
        .execute(Command::CreateCalendarTask {
            id: id(1),
            title: "假任务".into(),
            date,
        })
        .unwrap();
    assert_eq!(result.affected_tasks, vec![id(1)]);
    let task = fixture.store.task_detail(id(1)).unwrap().unwrap().task;
    assert_eq!(task.start_date, Some(date));
    assert_eq!(task.due_date, None);
}

#[test]
fn test_calendar_queries_future_dates_ongoing_and_local_completion() {
    let mut fixture = Fixture::new();
    let first = fixture.add(1);
    let start = first
        .patched(
            &TaskPatch {
                start_date: Patch::Set("2026-11-05".parse().unwrap()),
                ..Default::default()
            },
            now(),
        )
        .unwrap();
    fixture.store.replace_task(&first, &start).unwrap();

    let second = fixture.add(2);
    let ongoing = second
        .patched(
            &TaskPatch {
                start_date: Patch::Set("2026-11-01".parse().unwrap()),
                due_date: Patch::Set("2026-11-07".parse().unwrap()),
                ..Default::default()
            },
            now(),
        )
        .unwrap()
        .with_status(TaskStatus::InProgress, now())
        .unwrap();
    fixture.store.replace_task(&second, &ongoing).unwrap();

    let third = fixture.add(3);
    let completed = third
        .with_status(TaskStatus::Done, "2026-11-04T18:00:00Z".parse().unwrap())
        .unwrap();
    fixture.store.replace_task(&third, &completed).unwrap();
    fixture.add(4); // 无日期任务不进入日历。

    let result = fixture
        .store
        .query_tasks(
            Query::Calendar {
                start: "2026-11-05".parse().unwrap(),
                end: "2026-11-06".parse().unwrap(),
            },
            FixedClock(now(), 8),
        )
        .unwrap();
    let QueryResult::Calendar(days) = result else {
        panic!("calendar result expected")
    };
    assert_eq!(days.len(), 2);
    assert_eq!(days[0].1.start[0].id, first.id);
    assert_eq!(days[0].1.ongoing[0].id, second.id);
    assert_eq!(days[0].1.completed[0].id, third.id);
    assert!(days[1].1.start.is_empty());
    assert_eq!(days[1].1.ongoing[0].id, second.id);
    assert!(days[1].1.completed.is_empty());
}

#[test]
fn test_calendar_rejects_reversed_and_excessive_ranges() {
    let fixture = Fixture::new();
    for (start, end) in [("2026-11-06", "2026-11-05"), ("2026-01-01", "2026-03-04")] {
        let result = fixture.store.query_tasks(
            Query::Calendar {
                start: start.parse().unwrap(),
                end: end.parse().unwrap(),
            },
            FixedClock(now(), 0),
        );
        assert!(result.is_err());
    }
}
