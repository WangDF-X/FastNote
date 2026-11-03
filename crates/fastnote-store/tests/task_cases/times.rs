use fastnote_core::{
    parse_task_time, Command, Patch, Query, QueryResult, TaskError, TaskPatch, TaskRepository,
    TaskService, TaskStatus,
};
use rusqlite::params;

use crate::task_support::{now, FixedClock, Fixture};

fn patch() -> TaskPatch {
    TaskPatch {
        start_date: Patch::Set("2026-11-03".parse().unwrap()),
        due_date: Patch::Set("2026-11-03".parse().unwrap()),
        start_time: Patch::Set(parse_task_time("09:00").unwrap().unwrap()),
        due_time: Patch::Set(parse_task_time("15:30").unwrap().unwrap()),
        ..Default::default()
    }
}

#[test]
fn test_datetime_roundtrips_in_details_and_today_and_clear_removes_both_parts() {
    let mut fixture = Fixture::new();
    let original = fixture.add(1);
    let next = original.patched(&patch(), now()).unwrap();
    fixture.store.replace_task(&original, &next).unwrap();
    assert_eq!(
        fixture
            .store
            .task_detail(original.id)
            .unwrap()
            .unwrap()
            .task,
        next
    );
    let summary = fixture
        .store
        .today_tasks(now().date_naive())
        .unwrap()
        .remove(0);
    assert_eq!(summary.start_time, next.start_time);
    assert_eq!(summary.due_time, next.due_time);
    let values: (String, String) = fixture
        .external()
        .query_row("SELECT start_time,due_time FROM tasks", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    assert_eq!(values, ("09:00".to_owned(), "15:30".to_owned()));
    let clear = next
        .patched(
            &TaskPatch {
                start_date: Patch::Clear,
                due_date: Patch::Clear,
                ..Default::default()
            },
            now(),
        )
        .unwrap();
    fixture.store.replace_task(&next, &clear).unwrap();
    let saved = fixture
        .store
        .task_detail(original.id)
        .unwrap()
        .unwrap()
        .task;
    assert!(
        saved.start_date.is_none()
            && saved.start_time.is_none()
            && saved.due_date.is_none()
            && saved.due_time.is_none()
    );
}

#[test]
fn test_sql_constraints_reject_invalid_or_orphan_times_and_same_day_inversion() {
    let mut fixture = Fixture::new();
    let original = fixture.add(1);
    let connection = fixture.external();
    assert!(connection
        .execute("UPDATE tasks SET start_time='09:00'", [])
        .is_err());
    for invalid in ["24:00", "09:60", "9:00", "09:00:00", "", "xx:yy"] {
        assert!(connection
            .execute(
                "UPDATE tasks SET due_date='2026-11-03',due_time=?1",
                [invalid]
            )
            .is_err());
    }
    assert!(connection
        .execute(
            "UPDATE tasks SET start_date='2026-11-03',due_date='2026-11-03',
        start_time='16:00',due_time='15:30'",
            []
        )
        .is_err());
    assert_eq!(
        fixture
            .store
            .task_detail(original.id)
            .unwrap()
            .unwrap()
            .task,
        original
    );
    let timed = original.patched(&patch(), now()).unwrap();
    fixture.store.replace_task(&original, &timed).unwrap();
    assert!(connection
        .execute("UPDATE tasks SET start_time='16:00'", [])
        .is_err());
    assert!(connection
        .execute("UPDATE tasks SET due_date=NULL", [])
        .is_err());
    assert_eq!(
        fixture
            .store
            .task_detail(original.id)
            .unwrap()
            .unwrap()
            .task,
        timed
    );
}

#[test]
fn test_status_transition_preserves_times_and_stale_time_patch_is_rejected() {
    let mut fixture = Fixture::new();
    let original = fixture.add(1);
    let timed = original.patched(&patch(), now()).unwrap();
    fixture.store.replace_task(&original, &timed).unwrap();
    fixture
        .external()
        .execute(
            "UPDATE tasks SET due_time='16:00' WHERE id=?1",
            [original.id.to_string()],
        )
        .unwrap();
    assert_eq!(
        fixture
            .store
            .replace_task(&timed, &timed)
            .unwrap_err()
            .domain_error(),
        Some(TaskError::Conflict)
    );
    let mut service = TaskService::new(&mut fixture.store, FixedClock(now(), 0));
    service
        .execute(Command::SetTaskStatus {
            id: original.id,
            status: TaskStatus::Done,
            confirmed: false,
        })
        .unwrap();
    let QueryResult::TaskDetail(detail) = service.query(Query::TaskDetail(original.id)).unwrap()
    else {
        panic!("detail expected")
    };
    assert_eq!(detail.task.start_time, timed.start_time);
    assert_eq!(detail.task.due_time, parse_task_time("16:00").unwrap());
}

#[test]
fn test_failed_timed_task_write_rolls_back_all_fields() {
    let mut fixture = Fixture::new();
    let original = fixture.add(1);
    fixture
        .external()
        .execute_batch(
            "CREATE TRIGGER reject_fake_timed_task AFTER UPDATE ON tasks
        BEGIN SELECT RAISE(ABORT,'fake failure'); END;",
        )
        .unwrap();
    let next = original.patched(&patch(), now()).unwrap();
    assert!(fixture.store.replace_task(&original, &next).is_err());
    assert_eq!(
        fixture
            .store
            .task_detail(original.id)
            .unwrap()
            .unwrap()
            .task,
        original
    );
}

#[test]
fn test_repository_accepts_minute_boundaries_but_rejects_orphan_create() {
    let mut fixture = Fixture::new();
    let original = fixture.add(1);
    let mut next = original.clone();
    next.due_time = parse_task_time("00:00").unwrap();
    assert_eq!(
        fixture
            .store
            .replace_task(&original, &next)
            .unwrap_err()
            .domain_error(),
        Some(TaskError::DateRange)
    );
    let missing = crate::task_support::id(2);
    next.id = missing;
    assert_eq!(
        fixture.store.insert_task(&next).unwrap_err().domain_error(),
        Some(TaskError::DateRange)
    );
    let mut valid = patch();
    valid.start_time = Patch::Set(parse_task_time("00:00").unwrap().unwrap());
    valid.due_time = Patch::Set(parse_task_time("23:59").unwrap().unwrap());
    let next = original.patched(&valid, now()).unwrap();
    fixture.store.replace_task(&original, &next).unwrap();
    assert_eq!(
        fixture
            .external()
            .query_row(
                "SELECT due_time FROM tasks WHERE id=?1",
                params![original.id.to_string()],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "23:59"
    );
}

#[test]
fn test_service_projects_the_same_local_overdue_flag_in_today_and_detail() {
    let mut fixture = Fixture::new();
    let original = fixture.add(1);
    let timed = original.patched(&patch(), now()).unwrap();
    fixture.store.replace_task(&original, &timed).unwrap();
    let service = TaskService::new(
        &mut fixture.store,
        FixedClock("2026-11-03T07:31:00Z".parse().unwrap(), 8),
    );
    let QueryResult::Today(view) = service
        .query(Query::Today {
            date: "2026-11-03".parse().unwrap(),
            limits: [200; 8],
        })
        .unwrap()
    else {
        panic!("today expected")
    };
    let QueryResult::TaskDetail(detail) = service.query(Query::TaskDetail(original.id)).unwrap()
    else {
        panic!("detail expected")
    };
    assert_eq!(view.overdue_count, 1);
    assert!(view.sections[0].items[0].is_overdue && detail.is_overdue);
    assert_eq!(view.sections[0].items[0].due_time, detail.task.due_time);
}
