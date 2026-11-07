use fastnote_core::{
    Command, NaiveTime, Patch, Query, QueryResult, TaskPatch, TaskRepository, TaskService,
    TaskStatus,
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
fn test_calendar_keeps_overdue_unfinished_task_on_its_due_date() {
    let mut fixture = Fixture::new();
    let task = fixture.add(1);
    let due_date = "2026-11-05".parse().unwrap();
    let scheduled = task
        .patched(
            &TaskPatch {
                due_date: Patch::Set(due_date),
                ..Default::default()
            },
            now(),
        )
        .unwrap();
    fixture.store.replace_task(&task, &scheduled).unwrap();

    let result = fixture
        .store
        .query_tasks(
            Query::Calendar {
                start: due_date,
                end: "2026-11-07".parse().unwrap(),
            },
            FixedClock("2026-11-07T12:00:00Z".parse().unwrap(), 8),
        )
        .unwrap();
    let QueryResult::Calendar(days) = result else {
        panic!("calendar result expected")
    };
    assert_eq!(days[0].0, due_date);
    assert_eq!(days[0].1.due.len(), 1);
    assert_eq!(days[0].1.due[0].id, task.id);
    assert_eq!(days[0].1.due[0].title, task.title);
    assert!(days[0].1.due[0].is_overdue);
    assert!(days.iter().all(|(_, agenda)| agenda.completed.is_empty()));
    assert!(days[1..].iter().all(|(_, agenda)| agenda.due.is_empty()));
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

#[test]
fn test_calendar_status_flags_match_today_and_detail_at_local_deadline() {
    let mut fixture = Fixture::new();
    let task = fixture.add(1);
    let scheduled = task
        .patched(
            &TaskPatch {
                start_date: Patch::Set("2026-11-01".parse().unwrap()),
                due_date: Patch::Set("2026-11-03".parse().unwrap()),
                due_time: Patch::Set(NaiveTime::parse_from_str("15:30", "%H:%M").unwrap()),
                ..Default::default()
            },
            now(),
        )
        .unwrap()
        .with_status(TaskStatus::Waiting, now())
        .unwrap();
    fixture.store.replace_task(&task, &scheduled).unwrap();

    for (instant, overdue) in [
        ("2026-11-03T07:30:00Z", false),
        ("2026-11-03T07:31:00Z", true),
    ] {
        let service = TaskService::new(&mut fixture.store, FixedClock(instant.parse().unwrap(), 8));
        let QueryResult::Calendar(days) = service
            .query(Query::Calendar {
                start: "2026-11-01".parse().unwrap(),
                end: "2026-11-03".parse().unwrap(),
            })
            .unwrap()
        else {
            panic!("calendar result expected")
        };
        let QueryResult::Today(today) = service
            .query(Query::Today {
                date: "2026-11-03".parse().unwrap(),
                limits: [200; 8],
            })
            .unwrap()
        else {
            panic!("today result expected")
        };
        let QueryResult::TaskDetail(detail) = service.query(Query::TaskDetail(task.id)).unwrap()
        else {
            panic!("detail result expected")
        };
        let today_task = today
            .sections
            .iter()
            .flat_map(|section| &section.items)
            .find(|item| item.id == task.id)
            .unwrap();
        for item in [&days[0].1.start[0], &days[2].1.due[0], today_task] {
            assert_eq!(item.is_overdue, overdue);
            assert_eq!(item.status, Some(TaskStatus::Waiting));
            assert_eq!(item.waiting_days, Some(0));
        }
        assert_eq!(detail.is_overdue, overdue);
    }
    assert_eq!(
        fixture.store.task_detail(task.id).unwrap().unwrap().task,
        scheduled
    );
}

#[test]
fn test_calendar_completed_task_with_past_due_date_is_not_overdue() {
    let mut fixture = Fixture::new();
    let task = fixture.add(1);
    let completed_at = "2026-11-07T12:00:00Z".parse().unwrap();
    let completed = task
        .patched(
            &TaskPatch {
                due_date: Patch::Set("2026-11-05".parse().unwrap()),
                ..Default::default()
            },
            now(),
        )
        .unwrap()
        .with_status(TaskStatus::Done, completed_at)
        .unwrap();
    fixture.store.replace_task(&task, &completed).unwrap();
    let QueryResult::Calendar(days) = fixture
        .store
        .query_tasks(
            Query::Calendar {
                start: "2026-11-05".parse().unwrap(),
                end: "2026-11-07".parse().unwrap(),
            },
            FixedClock(completed_at, 8),
        )
        .unwrap()
    else {
        panic!("calendar result expected")
    };
    assert!(days[0].1.due.is_empty());
    assert_eq!(days[2].1.completed[0].status, Some(TaskStatus::Done));
    assert!(!days[2].1.completed[0].is_overdue);
}
