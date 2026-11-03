use fastnote_core::{
    AppEvent, Command, Query, QueryResult, TaskError, TaskRepository, TaskScope, TaskService,
    TaskStatus, WarningCode,
};
use rusqlite::params;

use crate::task_support::{id, now, FixedClock, Fixture, PLAN};

#[test]
fn test_completed_task_and_reminder_pause_commit_together_and_reopen_does_not_resume() {
    let mut fixture = Fixture::new();
    let task = fixture.add(1);
    let connection = fixture.external();
    connection.execute("INSERT INTO reminders(id,task_id,schedule_kind,local_time,next_fire_at,state,created_at,updated_at)
        VALUES(?1,?2,'daily','12:00',?3,'active',?3,?3)", params![id(2).to_string(), task.id.to_string(), now().to_rfc3339()]).unwrap();
    let mut service = TaskService::new(
        &mut fixture.store,
        FixedClock(now() + chrono::Duration::seconds(1), 0),
    );
    let result = service
        .execute(Command::SetTaskStatus {
            id: task.id,
            status: TaskStatus::Done,
            confirmed: false,
        })
        .unwrap();
    assert!(result
        .events
        .iter()
        .any(|event| matches!(event, AppEvent::RemindersChanged { .. })));
    for scope in [TaskScope::Today, TaskScope::Calendar, TaskScope::All] {
        assert!(result.events.iter().any(|event| matches!(event, AppEvent::TasksChanged { scope: actual, .. } if *actual == scope)));
    }
    let QueryResult::TaskDetail(detail) = service.query(Query::TaskDetail(task.id)).unwrap() else {
        panic!("detail expected")
    };
    assert_eq!(detail.task.status, Some(TaskStatus::Done));
    assert!(detail.task.completed_at.is_some());
    let reminder: (String, Option<String>, i64) = connection
        .query_row(
            "SELECT state,next_fire_at,revision FROM reminders",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(reminder, ("paused".to_owned(), None, 2));
    service
        .execute(Command::SetTaskStatus {
            id: task.id,
            status: TaskStatus::Todo,
            confirmed: false,
        })
        .unwrap();
    let QueryResult::TaskDetail(detail) = service.query(Query::TaskDetail(task.id)).unwrap() else {
        panic!("detail expected")
    };
    assert!(detail.task.completed_at.is_none());
    assert_eq!(
        connection
            .query_row("SELECT state FROM reminders", [], |row| row
                .get::<_, String>(0))
            .unwrap(),
        "paused"
    );
    assert!(service
        .execute(Command::SetTaskStatus {
            id: task.id,
            status: TaskStatus::Todo,
            confirmed: false
        })
        .unwrap()
        .events
        .is_empty());
}

#[test]
fn test_failed_reminder_pause_rolls_back_task_status_and_timestamps() {
    let mut fixture = Fixture::new();
    let task = fixture.add(1);
    let connection = fixture.external();
    connection.execute("INSERT INTO reminders(id,task_id,schedule_kind,local_time,next_fire_at,state,created_at,updated_at)
        VALUES(?1,?2,'daily','12:00',?3,'active',?3,?3)", params![id(2).to_string(), task.id.to_string(), now().to_rfc3339()]).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER fake_pause_failure BEFORE UPDATE ON reminders
        BEGIN SELECT RAISE(ABORT,'fake failure'); END;",
        )
        .unwrap();
    let completed = task
        .with_status(TaskStatus::Done, now() + chrono::Duration::seconds(1))
        .unwrap();
    assert!(fixture.store.replace_task(&task, &completed).is_err());
    assert_eq!(
        fixture.store.task_detail(task.id).unwrap().unwrap().task,
        task
    );
    assert_eq!(
        connection
            .query_row("SELECT state FROM reminders", [], |row| row
                .get::<_, String>(0))
            .unwrap(),
        "active"
    );
}

#[test]
fn test_parent_and_dependency_warnings_require_confirmation_without_locking_execution() {
    let mut fixture = Fixture::new();
    let parent = fixture.add(1);
    let child = fixture.add(2);
    let predecessor = fixture.add(3);
    fixture.plan();
    let connection = fixture.external();
    connection
        .execute("UPDATE tasks SET plan_id=?1", [PLAN])
        .unwrap();
    connection
        .execute(
            "UPDATE tasks SET parent_id=?2 WHERE id=?1",
            params![child.id.to_string(), parent.id.to_string()],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO task_dependencies(task_id,depends_on_task_id,created_at) VALUES(?1,?2,?3)",
            params![
                parent.id.to_string(),
                predecessor.id.to_string(),
                now().to_rfc3339()
            ],
        )
        .unwrap();
    let mut service = TaskService::new(&mut fixture.store, FixedClock(now(), 0));
    let warning = service
        .execute(Command::SetTaskStatus {
            id: parent.id,
            status: TaskStatus::Done,
            confirmed: false,
        })
        .unwrap();
    assert!(
        warning.needs_confirmation
            && warning.events.is_empty()
            && warning.affected_tasks.is_empty()
    );
    assert!(warning.warnings.contains(&WarningCode::ParentIncomplete));
    assert!(warning
        .warnings
        .contains(&WarningCode::DependencyIncomplete));
    let QueryResult::TaskDetail(detail) = service.query(Query::TaskDetail(parent.id)).unwrap()
    else {
        panic!("detail expected")
    };
    assert_eq!(detail.task.status, Some(TaskStatus::Todo));
    let committed = service
        .execute(Command::SetTaskStatus {
            id: parent.id,
            status: TaskStatus::Done,
            confirmed: true,
        })
        .unwrap();
    assert!(!committed.needs_confirmation);
    assert!(committed.events.iter().any(|event| matches!(
        event,
        AppEvent::TasksChanged {
            scope: TaskScope::Plan(_),
            ..
        }
    )));
    let QueryResult::TaskDetail(child) = service.query(Query::TaskDetail(child.id)).unwrap() else {
        panic!("detail expected")
    };
    assert_eq!(child.task.status, Some(TaskStatus::Todo));
}

#[test]
fn test_delete_requires_cascade_and_preserves_notes_while_removing_children_dependencies_and_reminders(
) {
    let mut fixture = Fixture::new();
    let parent = fixture.add(1);
    let child = fixture.add(2);
    let grandchild = fixture.add(3);
    let predecessor = fixture.add(6);
    fixture.plan();
    let connection = fixture.external();
    connection
        .execute("UPDATE tasks SET plan_id=?1", [PLAN])
        .unwrap();
    connection
        .execute(
            "UPDATE tasks SET parent_id=?2 WHERE id=?1",
            params![child.id.to_string(), parent.id.to_string()],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE tasks SET parent_id=?2 WHERE id=?1",
            params![grandchild.id.to_string(), child.id.to_string()],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO task_dependencies(task_id,depends_on_task_id,created_at) VALUES(?1,?2,?3)",
            params![
                child.id.to_string(),
                predecessor.id.to_string(),
                now().to_rfc3339()
            ],
        )
        .unwrap();
    connection.execute("INSERT INTO notes(id,content,converted_task_id,created_at,updated_at) VALUES(?1,'假小记',?2,?3,?3)",
        params![id(4).to_string(), child.id.to_string(), now().to_rfc3339()]).unwrap();
    connection
        .execute(
            "INSERT INTO reminders(id,task_id,schedule_kind,local_time,state,created_at,updated_at)
        VALUES(?1,?2,'daily','12:00','paused',?3,?3)",
            params![id(5).to_string(), child.id.to_string(), now().to_rfc3339()],
        )
        .unwrap();
    assert_eq!(
        fixture
            .store
            .task_detail(parent.id)
            .unwrap()
            .unwrap()
            .context
            .descendant_count,
        2
    );
    assert_eq!(
        fixture
            .store
            .delete_task(parent.id, false)
            .unwrap_err()
            .domain_error(),
        Some(TaskError::HasChildren)
    );
    assert_eq!(
        fixture.store.today_tasks(now().date_naive()).unwrap().len(),
        4
    );
    assert_eq!(fixture.store.delete_task(parent.id, true).unwrap().len(), 3);
    assert_eq!(
        fixture.store.today_tasks(now().date_naive()).unwrap().len(),
        1
    );
    assert!(fixture.store.task_detail(predecessor.id).unwrap().is_some());
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM task_dependencies", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM reminders", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(connection
        .query_row("SELECT converted_task_id FROM notes", [], |row| row
            .get::<_, Option<String>>(0))
        .unwrap()
        .is_none());
    assert_eq!(
        connection
            .query_row("SELECT content FROM notes", [], |row| row
                .get::<_, String>(0))
            .unwrap(),
        "假小记"
    );
}

#[test]
fn test_failed_cascade_delete_rolls_back_entire_subtree() {
    let mut fixture = Fixture::new();
    let parent = fixture.add(1);
    let child = fixture.add(2);
    let connection = fixture.external();
    connection
        .execute(
            "UPDATE tasks SET parent_id=?2 WHERE id=?1",
            params![child.id.to_string(), parent.id.to_string()],
        )
        .unwrap();
    connection
        .execute_batch(&format!(
            "CREATE TRIGGER fake_delete_failure BEFORE DELETE ON tasks
        WHEN OLD.id='{}' BEGIN SELECT RAISE(ABORT,'fake failure'); END;",
            child.id
        ))
        .unwrap();
    assert!(fixture.store.delete_task(parent.id, true).is_err());
    assert!(fixture.store.task_detail(parent.id).unwrap().is_some());
    assert!(fixture.store.task_detail(child.id).unwrap().is_some());
}
