#[path = "task_cases/migration_times.rs"]
mod migration_times;
#[path = "task_cases/relationships.rs"]
mod relationships;
mod task_support;
#[path = "task_cases/times.rs"]
mod times;

use fastnote_core::{
    Command, Patch, Priority, Query, QueryResult, Task, TaskError, TaskPatch, TaskRepository,
    TaskService, TaskStatus, TodaySort,
};
use fastnote_store::{SqliteSettings, StoreError, TaskStoreError};
use rusqlite::params;

use task_support::{id, now, FixedClock, Fixture, PLAN};

#[test]
fn test_task_creation_roundtrips_and_persists_with_allocated_sort_order() {
    let mut fixture = Fixture::new();
    let first = fixture.add(1);
    let second = fixture.add(2);
    assert_eq!(first.sort_order, 1024);
    assert_eq!(second.sort_order, 2048);
    drop(fixture.store);
    let store = SqliteSettings::open(&fixture.path, now()).unwrap();
    assert_eq!(store.task_detail(first.id).unwrap().unwrap().task, first);
    assert_eq!(store.today_tasks(now().date_naive()).unwrap().len(), 2);
}

#[test]
fn test_missing_task_read_update_and_delete_are_explicit_and_do_not_insert() {
    let mut fixture = Fixture::new();
    let task = Task::new(id(1), "假任务".to_owned(), now()).unwrap();
    assert!(fixture.store.task_detail(task.id).unwrap().is_none());
    assert!(fixture
        .store
        .today_tasks(now().date_naive())
        .unwrap()
        .is_empty());
    assert_eq!(
        fixture
            .store
            .replace_task(&task, &task)
            .unwrap_err()
            .domain_error(),
        Some(TaskError::NotFound)
    );
    assert_eq!(
        fixture
            .store
            .delete_task(task.id, true)
            .unwrap_err()
            .domain_error(),
        Some(TaskError::NotFound)
    );
}

#[test]
fn test_duplicate_id_preserves_existing_task_and_retains_error_source() {
    let mut fixture = Fixture::new();
    let first = fixture.add(1);
    let error = fixture.store.insert_task(&first).unwrap_err();
    assert_eq!(error.domain_error(), Some(TaskError::Conflict));
    assert!(error.storage_error().is_some());
    assert_eq!(
        fixture.store.task_detail(first.id).unwrap().unwrap().task,
        first
    );
}

#[test]
fn test_replace_roundtrips_fields_and_clear_dates_without_changing_status() {
    let mut fixture = Fixture::new();
    let first = fixture.add(1);
    let dated = first
        .patched(
            &TaskPatch {
                title: Some("  修改后的假任务  ".to_owned()),
                description: Some("\n纯文本\n".to_owned()),
                priority: Some(Priority::High),
                start_date: Patch::Set("2026-11-03".parse().unwrap()),
                due_date: Patch::Set("2026-11-04".parse().unwrap()),
                ..Default::default()
            },
            now() + chrono::Duration::seconds(1),
        )
        .unwrap();
    fixture.store.replace_task(&first, &dated).unwrap();
    assert_eq!(
        fixture.store.task_detail(first.id).unwrap().unwrap().task,
        dated
    );
    let clear = dated
        .patched(
            &TaskPatch {
                start_date: Patch::Clear,
                due_date: Patch::Clear,
                ..Default::default()
            },
            now() + chrono::Duration::seconds(2),
        )
        .unwrap();
    fixture.store.replace_task(&dated, &clear).unwrap();
    assert_eq!(
        fixture.store.task_detail(first.id).unwrap().unwrap().task,
        clear
    );
    assert_eq!(clear.status, Some(TaskStatus::Todo));
}

#[test]
fn test_invalid_replace_and_stale_snapshot_never_overwrite_other_content() {
    let mut fixture = Fixture::new();
    let first = fixture.add(1);
    let mut invalid = first.clone();
    invalid.start_date = Some("2026-11-04".parse().unwrap());
    invalid.due_date = Some("2026-11-03".parse().unwrap());
    assert_eq!(
        fixture
            .store
            .replace_task(&first, &invalid)
            .unwrap_err()
            .domain_error(),
        Some(TaskError::DateRange)
    );
    fixture
        .external()
        .execute(
            "UPDATE tasks SET title='另一份假变更' WHERE id=?1",
            [first.id.to_string()],
        )
        .unwrap();
    let replacement = first
        .patched(
            &TaskPatch {
                description: Some("新假描述".to_owned()),
                ..Default::default()
            },
            now(),
        )
        .unwrap();
    assert_eq!(
        fixture
            .store
            .replace_task(&first, &replacement)
            .unwrap_err()
            .domain_error(),
        Some(TaskError::Conflict)
    );
    assert_eq!(
        fixture
            .store
            .task_detail(first.id)
            .unwrap()
            .unwrap()
            .task
            .title,
        "另一份假变更"
    );
}

#[test]
fn test_archived_plan_is_excluded_and_rejects_update_delete_and_service_status() {
    let mut fixture = Fixture::new();
    let task = fixture.add(1);
    fixture.plan();
    fixture
        .external()
        .execute(
            "UPDATE tasks SET plan_id=?2 WHERE id=?1",
            params![task.id.to_string(), PLAN],
        )
        .unwrap();
    fixture
        .external()
        .execute(
            "UPDATE plans SET status='archived',archived_at=?2 WHERE id=?1",
            params![PLAN, now().to_rfc3339()],
        )
        .unwrap();
    let detail = fixture.store.task_detail(task.id).unwrap().unwrap();
    assert!(detail.context.read_only);
    assert!(fixture
        .store
        .today_tasks(now().date_naive())
        .unwrap()
        .is_empty());
    assert_eq!(
        fixture
            .store
            .replace_task(&detail.task, &detail.task)
            .unwrap_err()
            .domain_error(),
        Some(TaskError::PlanArchived)
    );
    assert_eq!(
        fixture
            .store
            .delete_task(task.id, true)
            .unwrap_err()
            .domain_error(),
        Some(TaskError::PlanArchived)
    );
    let mut service = TaskService::new(&mut fixture.store, FixedClock(now(), 0));
    assert_eq!(
        service
            .execute(Command::SetTaskStatus {
                id: task.id,
                status: TaskStatus::Done,
                confirmed: false
            })
            .unwrap_err()
            .domain_error(),
        Some(TaskError::PlanArchived)
    );
}

#[test]
fn test_today_preferences_use_canonical_keys_and_invalid_values_fall_back() {
    let fixture = Fixture::new();
    assert_eq!(
        fixture.store.today_preferences().unwrap(),
        Default::default()
    );
    let connection = fixture.external();
    for (key, value) in [
        ("tasks.default_sort", "due_date"),
        ("today.show_waiting", "false"),
        ("today.show_completed", "false"),
        ("today.tomorrow_collapsed", "false"),
        ("general.default_sort", "created_desc"),
    ] {
        connection
            .execute(
                "INSERT INTO app_settings(key,value,updated_at) VALUES(?1,?2,?3)
            ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at",
                params![key, value, now().to_rfc3339()],
            )
            .unwrap();
    }
    let prefs = fixture.store.today_preferences().unwrap();
    assert_eq!(prefs.sort, TodaySort::DueDate);
    assert!(!prefs.show_waiting && !prefs.show_completed && !prefs.tomorrow_collapsed);
    connection
        .execute(
            "UPDATE app_settings SET value=?1 WHERE key='tasks.default_sort'",
            ["x".repeat(5000)],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE app_settings SET value='invalid' WHERE key='today.show_waiting'",
            [],
        )
        .unwrap();
    let prefs = fixture.store.today_preferences().unwrap();
    assert_eq!(prefs.sort, TodaySort::Priority);
    assert!(prefs.show_waiting);
}

#[test]
fn test_local_completed_today_is_not_filtered_by_utc_midnight() {
    let mut fixture = Fixture::new();
    let at = "2026-11-02T17:00:00Z".parse().unwrap();
    let task = Task::new(id(1), "跨时区假任务".to_owned(), at)
        .unwrap()
        .with_status(TaskStatus::Done, at)
        .unwrap();
    fixture.store.insert_task(&task).unwrap();
    let service = TaskService::new(
        &mut fixture.store,
        FixedClock("2026-11-03T01:00:00Z".parse().unwrap(), 8),
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
    assert_eq!(view.completed_today_count, 1);
    assert_eq!(view.sections[0].items[0].id, task.id);
}

#[test]
fn test_service_rejects_invalid_and_stale_patches_without_events_or_data_changes() {
    let mut fixture = Fixture::new();
    let task = fixture.add(1);
    let mut service = TaskService::new(&mut fixture.store, FixedClock(now(), 0));
    let invalid = service
        .execute(Command::UpdateTask {
            id: task.id,
            patch: TaskPatch {
                title: Some(" ".to_owned()),
                ..Default::default()
            },
            expected_updated_at: task.updated_at,
        })
        .unwrap_err();
    assert_eq!(invalid.domain_error(), Some(TaskError::TitleRequired));
    let stale = service
        .execute(Command::UpdateTask {
            id: task.id,
            patch: TaskPatch {
                title: Some("不能覆盖的假变更".to_owned()),
                ..Default::default()
            },
            expected_updated_at: task.updated_at - chrono::Duration::seconds(1),
        })
        .unwrap_err();
    assert_eq!(stale.domain_error(), Some(TaskError::Conflict));
    let QueryResult::TaskDetail(detail) = service.query(Query::TaskDetail(task.id)).unwrap() else {
        panic!("detail expected")
    };
    assert_eq!(detail.task, task);
}

#[test]
fn test_task_writes_recheck_schema_version_before_changing_data() {
    let mut fixture = Fixture::new();
    let task = fixture.add(1);
    fixture
        .external()
        .pragma_update(None, "user_version", 4)
        .unwrap();
    let error = fixture.store.delete_task(task.id, true).unwrap_err();
    assert!(matches!(
        error,
        TaskStoreError::Storage(StoreError::NewerSchema { .. })
    ));
    assert!(fixture.store.task_detail(task.id).unwrap().is_some());
}

#[test]
fn test_sort_order_overflow_rejects_creation_without_partial_insert() {
    let mut fixture = Fixture::new();
    let first = fixture.add(1);
    fixture
        .external()
        .execute(
            "UPDATE tasks SET sort_order=?2 WHERE id=?1",
            params![first.id.to_string(), i64::MAX],
        )
        .unwrap();
    let second = Task::new(id(2), "第二个假任务".to_owned(), now()).unwrap();
    assert_eq!(
        fixture
            .store
            .insert_task(&second)
            .unwrap_err()
            .domain_error(),
        Some(TaskError::Conflict)
    );
    assert!(fixture.store.task_detail(second.id).unwrap().is_none());
    assert_eq!(
        fixture.store.today_tasks(now().date_naive()).unwrap().len(),
        1
    );
}
