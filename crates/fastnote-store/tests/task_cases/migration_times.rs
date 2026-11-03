use std::fs;
use std::path::Path;

use fastnote_core::TaskRepository;
use fastnote_store::SqliteSettings;
use rusqlite::{params, Connection};

use crate::task_support::{id, now};

fn create_v2(path: &Path) {
    let connection = Connection::open(path).unwrap();
    connection
        .execute_batch(include_str!("../../migrations/m0001_init.sql"))
        .unwrap();
    connection
        .execute_batch(include_str!("../../migrations/m0002_reminders.sql"))
        .unwrap();
    connection
        .execute(
            "INSERT INTO app_meta(key,value) VALUES('app_id','fastnote')",
            [],
        )
        .unwrap();
    for version in [1, 2] {
        connection.execute("INSERT INTO schema_migrations(version,applied_at,app_version) VALUES(?1,?2,'0.0.1')",
            params![version, now().to_rfc3339()]).unwrap();
    }
    connection.pragma_update(None, "user_version", 2).unwrap();
    connection.execute("INSERT INTO tasks(id,kind,title,description,status,priority,start_date,due_date,
        status_changed_at,sort_order,created_at,updated_at) VALUES(?1,'task','旧假任务','旧假描述','todo',3,
        '2026-11-03','2026-11-04',?2,1024,?2,?2)", params![id(1).to_string(), now().to_rfc3339()]).unwrap();
    connection.execute("INSERT INTO reminders(id,task_id,schedule_kind,local_time,next_fire_at,state,created_at,updated_at)
        VALUES(?1,?2,'daily','12:00',?3,'active',?3,?3)", params![id(2).to_string(), id(1).to_string(), now().to_rfc3339()]).unwrap();
}

fn version(connection: &Connection) -> i64 {
    connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap()
}

#[test]
fn test_v2_upgrade_preserves_task_and_reminder_and_makes_complete_old_schema_backup() {
    let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let path = directory.path().join("fake-v2.db");
    create_v2(&path);
    let store = SqliteSettings::open(&path, now()).unwrap();
    let task = store.task_detail(id(1)).unwrap().unwrap().task;
    assert_eq!(task.title, "旧假任务");
    assert_eq!(task.description, "旧假描述");
    assert_eq!(task.start_date.unwrap().to_string(), "2026-11-03");
    assert_eq!(task.due_date.unwrap().to_string(), "2026-11-04");
    assert!(task.start_time.is_none() && task.due_time.is_none());
    let connection = Connection::open(&path).unwrap();
    assert_eq!(version(&connection), 3);
    assert_eq!(
        connection
            .query_row("SELECT state FROM reminders", [], |row| row
                .get::<_, String>(0))
            .unwrap(),
        "active"
    );
    let backup_path = fs::read_dir(directory.path().join("backups"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let backup = Connection::open(backup_path).unwrap();
    assert_eq!(version(&backup), 2);
    assert!(backup.prepare("SELECT start_time FROM tasks").is_err());
    assert_eq!(
        backup
            .query_row("SELECT title FROM tasks", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "旧假任务"
    );
    assert_eq!(
        backup
            .query_row("SELECT count(*) FROM reminders", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        backup
            .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}

#[test]
fn test_failed_v3_migration_rolls_back_first_new_column_and_keeps_v2_data() {
    let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let path = directory.path().join("fake-v2.db");
    create_v2(&path);
    Connection::open(&path)
        .unwrap()
        .execute_batch("ALTER TABLE tasks ADD COLUMN due_time TEXT;")
        .unwrap();
    assert!(SqliteSettings::open(&path, now()).is_err());
    let connection = Connection::open(&path).unwrap();
    assert_eq!(version(&connection), 2);
    assert!(connection.prepare("SELECT start_time FROM tasks").is_err());
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM schema_migrations", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        connection
            .query_row("SELECT title FROM tasks", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "旧假任务"
    );
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM reminders", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
