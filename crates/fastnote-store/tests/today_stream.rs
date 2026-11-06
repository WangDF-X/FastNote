use fastnote_core::{
    Clock, DateTime, NaiveDate, NaiveDateTime, Priority, Query, QueryResult, Task, TaskClock,
    TaskId, TaskRepository, TaskService, Utc,
};
use fastnote_store::SqliteSettings;

fn now() -> DateTime<Utc> {
    "2026-11-04T12:00:00Z".parse().unwrap()
}
struct FixedClock(DateTime<Utc>, i64);
impl Clock for FixedClock {
    fn today(&self) -> NaiveDate {
        self.0.date_naive()
    }
}
impl TaskClock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
    fn local_datetime(&self, at: DateTime<Utc>) -> NaiveDateTime {
        (at + chrono::Duration::hours(self.1)).naive_utc()
    }
}

fn fixture() -> (tempfile::TempDir, SqliteSettings) {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteSettings::open(&directory.path().join("fake.db"), now()).unwrap();
    (directory, store)
}

#[test]
fn test_streamed_candidates_match_legacy_projection_with_real_sqlite() {
    let (_directory, mut store) = fixture();
    for number in 1..=5 {
        let id: TaskId = format!("019a0fff-aaaa-7000-8000-{number:012x}")
            .parse()
            .unwrap();
        let mut task = Task::new(id, "假任务".to_owned(), now()).unwrap();
        if number % 2 == 0 {
            task.priority = Priority::High;
        }
        store.insert_task(&task).unwrap();
    }
    let mut streamed = Vec::new();
    store
        .visit_today_tasks(now().date_naive(), &mut |task| streamed.push(task))
        .unwrap();
    let mut legacy = store.today_tasks(now().date_naive()).unwrap();
    // 仓储候选流不承诺顺序，最终排序由领域层完成。
    streamed.sort_by_key(|task| task.id);
    legacy.sort_by_key(|task| task.id);
    assert_eq!(streamed, legacy);
    let service = TaskService::new(&mut store, FixedClock(now(), 0));
    let QueryResult::Today(view) = service
        .query(Query::Today {
            date: now().date_naive(),
            limits: [200; 8],
        })
        .unwrap()
    else {
        panic!("today view")
    };
    assert_eq!(view.open_count, 5);
    assert_eq!(view.sections[0].items.len(), 5);
}

#[test]
fn test_missing_candidates_do_not_call_the_visitor_or_allocate_business_items() {
    let (_directory, store) = fixture();
    let mut calls = 0;
    store
        .visit_today_tasks(now().date_naive(), &mut |_| calls += 1)
        .unwrap();
    assert_eq!(calls, 0);
}

#[test]
fn test_invalid_stored_candidate_fails_without_returning_a_partially_loaded_view() {
    let (directory, mut store) = fixture();
    let id: TaskId = "019a0fff-aaaa-7000-8000-000000000001".parse().unwrap();
    store
        .insert_task(&Task::new(id, "假任务".to_owned(), now()).unwrap())
        .unwrap();
    let connection = rusqlite::Connection::open(directory.path().join("fake.db")).unwrap();
    connection
        .execute_batch("PRAGMA ignore_check_constraints=ON; UPDATE tasks SET status='invalid'")
        .unwrap();
    let service = TaskService::new(&mut store, FixedClock(now(), 0));
    assert!(service
        .query(Query::Today {
            date: now().date_naive(),
            limits: [200; 8]
        })
        .is_err());
    let status: String = connection
        .query_row("SELECT status FROM tasks", [], |row| row.get(0))
        .unwrap();
    assert_eq!(status, "invalid");
}

#[test]
fn test_real_service_uses_streaming_port_not_full_vec_loading() {
    let service = include_str!("../../fastnote-core/src/service/task.rs");
    assert!(service.contains("visit_today_tasks(date"));
    assert!(!service.contains("today_tasks(date)?"));
    let adapter = include_str!("../src/tasks/read.rs");
    let visitor = adapter
        .split_once("fn visit_candidates(")
        .unwrap()
        .1
        .split_once("fn detail(")
        .unwrap()
        .0;
    assert!(!visitor.contains(".collect"));
    assert!(!visitor.contains("description\""));
    // Keep the injected clock import exercised; no real time is used for domain rules.
    assert_eq!(FixedClock(now(), 0).now(), now());
}

#[test]
fn test_bounded_projection_restores_plan_children_and_dependency_flags_without_body() {
    let (directory, mut store) = fixture();
    let ids: Vec<TaskId> = (1..=3)
        .map(|number| {
            format!("019a0fff-aaaa-7000-8000-{number:012x}")
                .parse()
                .unwrap()
        })
        .collect();
    for id in &ids {
        store
            .insert_task(&Task::new(*id, "假任务".to_owned(), now()).unwrap())
            .unwrap();
    }
    let connection = rusqlite::Connection::open(directory.path().join("fake.db")).unwrap();
    let plan = "019a0fff-aaaa-7000-8000-0000000000ff";
    connection
        .execute(
            "INSERT INTO plans(id,name,created_at,updated_at) VALUES(?1,'假计划',?2,?2)",
            rusqlite::params![plan, now().to_rfc3339()],
        )
        .unwrap();
    connection
        .execute("UPDATE tasks SET plan_id=?1", [plan])
        .unwrap();
    connection
        .execute(
            "UPDATE tasks SET parent_id=?1 WHERE id=?2",
            rusqlite::params![ids[0].to_string(), ids[1].to_string()],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO task_dependencies(task_id,depends_on_task_id,created_at) VALUES(?1,?2,?3)",
            rusqlite::params![ids[2].to_string(), ids[1].to_string(), now().to_rfc3339()],
        )
        .unwrap();
    let QueryResult::Today(view) = store
        .query_tasks(
            Query::Today {
                date: now().date_naive(),
                limits: [200; 8],
            },
            FixedClock(now(), 0),
        )
        .unwrap()
    else {
        panic!("today")
    };
    let items = &view.sections[0].items;
    let parent = items.iter().find(|task| task.id == ids[0]).unwrap();
    assert_eq!(parent.context.child_total, 1);
    assert_eq!(parent.context.plan_name.as_deref(), Some("假计划"));
    let blocked = items.iter().find(|task| task.id == ids[2]).unwrap();
    assert_eq!(blocked.context.dependency_count, 1);
    assert!(blocked.is_blocked);
}

#[test]
fn test_projection_rejects_missing_or_changed_entities_without_modifying_the_database() {
    for missing in [false, true] {
        let (_directory, mut store) = fixture();
        let id: TaskId = "019a0fff-aaaa-7000-8000-000000000001".parse().unwrap();
        store
            .insert_task(&Task::new(id, "假任务".to_owned(), now()).unwrap())
            .unwrap();
        let QueryResult::Today(mut view) = store
            .query_tasks(
                Query::Today {
                    date: now().date_naive(),
                    limits: [200; 8],
                },
                FixedClock(now(), 0),
            )
            .unwrap()
        else {
            panic!("today")
        };
        if missing {
            store.delete_task(id, true).unwrap();
        } else {
            let previous = store.task_detail(id).unwrap().unwrap().task;
            let mut updated = previous.clone();
            updated.updated_at += chrono::Duration::seconds(1);
            store.replace_task(&previous, &updated).unwrap();
        }
        assert!(store.complete_today_projection(&mut view).is_err());
        assert_eq!(store.task_detail(id).unwrap().is_none(), missing);
    }
}

#[test]
fn test_failed_read_snapshot_rolls_back_and_does_not_block_a_later_query() {
    let (_directory, mut store) = fixture();
    let id: TaskId = "019a0fff-aaaa-7000-8000-000000000001".parse().unwrap();
    assert!(store
        .query_tasks(Query::TaskDetail(id), FixedClock(now(), 0))
        .is_err());
    store
        .insert_task(&Task::new(id, "假任务".to_owned(), now()).unwrap())
        .unwrap();
    assert!(matches!(
        store
            .query_tasks(Query::TaskDetail(id), FixedClock(now(), 0))
            .unwrap(),
        QueryResult::TaskDetail(_)
    ));
}
