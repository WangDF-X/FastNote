use super::*;

fn now() -> DateTime<Utc> {
    "2026-11-08T08:00:00Z".parse().unwrap()
}

fn legacy() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .pragma_update(None, "foreign_keys", true)
        .unwrap();
    for (index, script) in SCRIPTS[..3].iter().enumerate() {
        connection.execute_batch(script).unwrap();
        connection
            .execute(
                "INSERT INTO schema_migrations VALUES(?1,?2,'0.0.1')",
                params![(index + 1) as i64, now().to_rfc3339()],
            )
            .unwrap();
    }
    connection.pragma_update(None, "user_version", 3).unwrap();
    connection
        .execute_batch(
            "INSERT INTO tasks(id,title,status,status_changed_at,created_at,updated_at)
         VALUES('t','假任务','waiting','old-state','old-created','old-updated');
         INSERT INTO app_settings VALUES('today.tomorrow_collapsed','false','old-pref');
         INSERT INTO app_settings VALUES('today.show_waiting','false','old-pref');
         INSERT INTO app_settings VALUES('today.show_in_progress','false','old-pref');",
        )
        .unwrap();
    connection
}

fn foreign_keys(connection: &Connection) -> bool {
    connection
        .pragma_query_value(None, "foreign_keys", |row| row.get(0))
        .unwrap()
}

fn version(connection: &Connection) -> i64 {
    connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap()
}

#[test]
fn test_v4_success_restores_foreign_keys_and_preserves_old_times_and_setting_timestamp() {
    let mut connection = legacy();
    migrate(&mut connection, now()).unwrap();
    assert!(foreign_keys(&connection));
    assert_eq!(version(&connection), 6);
    let task: (String, String, String, String) = connection
        .query_row(
            "SELECT status,status_changed_at,created_at,updated_at FROM tasks",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        task,
        (
            "todo".into(),
            "old-state".into(),
            "old-created".into(),
            "old-updated".into()
        )
    );
    let setting: (String, String) = connection
        .query_row(
            "SELECT value,updated_at FROM app_settings WHERE key='today.later_collapsed'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(setting, ("false".into(), "old-pref".into()));
    check_integrity(&connection, true).unwrap();
}

#[test]
fn test_missing_old_collapse_key_uses_default_and_injected_migration_time() {
    let mut connection = legacy();
    connection
        .execute(
            "DELETE FROM app_settings WHERE key='today.tomorrow_collapsed'",
            [],
        )
        .unwrap();
    migrate(&mut connection, now()).unwrap();
    let setting: (String, String) = connection
        .query_row(
            "SELECT value,updated_at FROM app_settings WHERE key='today.later_collapsed'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(setting, ("true".into(), now().to_rfc3339()));
}

#[test]
fn test_v4_failures_restore_foreign_keys_and_roll_back_ddl_tasks_settings_and_audit() {
    for phase in 0..4 {
        let mut connection = legacy();
        match phase {
            0 => connection
                .execute_batch("CREATE TABLE tasks_new(fake TEXT);")
                .unwrap(),
            1 => connection
                .execute_batch(
                    "CREATE TRIGGER reject_setting BEFORE DELETE ON app_settings
                 WHEN OLD.key='today.show_waiting' BEGIN SELECT RAISE(ABORT,'fake failure'); END;",
                )
                .unwrap(),
            2 => connection
                .execute_batch(
                    "CREATE TRIGGER reject_audit BEFORE INSERT ON schema_migrations
                 WHEN NEW.version=4 BEGIN SELECT RAISE(ABORT,'fake failure'); END;",
                )
                .unwrap(),
            _ => {}
        }
        let script = if phase == 3 {
            format!(
                "{}\nINSERT INTO task_dependencies VALUES('missing','t','fake');",
                SCRIPTS[3]
            )
        } else {
            SCRIPTS[3].to_owned()
        };
        assert!(
            migrate_step(&mut connection, 4, &script, &now().to_rfc3339()).is_err(),
            "phase {phase}"
        );
        assert!(foreign_keys(&connection), "phase {phase}");
        assert_eq!(version(&connection), 3);
        let status: String = connection
            .query_row("SELECT status FROM tasks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(status, "waiting");
        let setting: (String, String) = connection
            .query_row(
                "SELECT value,updated_at FROM app_settings WHERE key='today.tomorrow_collapsed'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(setting, ("false".into(), "old-pref".into()));
        let count: i64 = connection
            .query_row(
                "SELECT count(*) FROM app_settings WHERE key='today.later_collapsed'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        let audit: i64 = connection
            .query_row("SELECT count(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(audit, 3);
        check_integrity(&connection, true).unwrap();
    }
}

#[test]
fn test_migration_rechecks_current_version_and_never_runs_sql_twice() {
    let mut connection = legacy();
    migrate(&mut connection, now()).unwrap();
    migrate_step(
        &mut connection,
        4,
        "invalid SQL must not execute",
        "different-time",
    )
    .unwrap();
    assert!(foreign_keys(&connection));
    let applied: String = connection
        .query_row(
            "SELECT applied_at FROM schema_migrations WHERE version=4",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(applied, now().to_rfc3339());
    connection.pragma_update(None, "user_version", 7).unwrap();
    assert!(matches!(
        migrate_step(&mut connection, 4, SCRIPTS[3], "different-time"),
        Err(StoreError::NewerSchema { found: 7, .. })
    ));
    assert!(foreign_keys(&connection));
    assert_eq!(version(&connection), 7);
}
