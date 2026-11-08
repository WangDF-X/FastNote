use rusqlite::Connection;

#[test]
fn test_v4_script_preserves_relationships_times_and_preferences() {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(include_str!("../migrations/m0001_init.sql"))
        .unwrap();
    db.execute_batch(include_str!("../migrations/m0002_reminders.sql"))
        .unwrap();
    db.execute_batch(include_str!("../migrations/m0003_task_times.sql"))
        .unwrap();
    db.execute_batch(
        "INSERT INTO plans(id,name,status,archived_at,created_at,updated_at)
             VALUES('p','假计划','archived','t','t','t');
         INSERT INTO tasks(id,plan_id,title,status,status_changed_at,created_at,updated_at,
             start_date,due_date,start_time,due_time)
             VALUES('a','p','假任务','waiting','old','created','updated',
                 '2026-11-01','2026-11-02','09:00','10:00');
         INSERT INTO tasks(id,parent_id,title,status,status_changed_at,created_at,updated_at)
             VALUES('b','a','子任务','todo','t','t','t');
         INSERT INTO task_dependencies VALUES('b','a','t');
         INSERT INTO notes(id,content,converted_task_id,created_at,updated_at)
             VALUES('n','假小记','a','t','t');
         INSERT INTO app_settings VALUES('today.tomorrow_collapsed','false','old');
         INSERT INTO app_settings VALUES('today.show_waiting','true','old');",
    )
    .unwrap();
    db.execute_batch("PRAGMA foreign_keys=OFF; BEGIN IMMEDIATE;")
        .unwrap();
    db.execute_batch(include_str!("../migrations/m0004_task_workflow.sql"))
        .unwrap();
    let violations: i64 = db
        .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(violations, 0);
    db.execute_batch("COMMIT; PRAGMA foreign_keys=ON;").unwrap();
    let task: (String, String, String, String, String) = db
        .query_row(
            "SELECT status,status_changed_at,updated_at,start_time,due_time FROM tasks WHERE id='a'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .unwrap();
    assert_eq!(
        task,
        (
            "todo".into(),
            "old".into(),
            "updated".into(),
            "09:00".into(),
            "10:00".into()
        )
    );
    let preference: (String, String) = db
        .query_row(
            "SELECT value,updated_at FROM app_settings WHERE key='today.later_collapsed'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(preference, ("false".into(), "old".into()));
    assert!(db
        .execute("UPDATE tasks SET status='waiting' WHERE id='a'", [])
        .is_err());
    assert_eq!(
        db.query_row("SELECT count(*) FROM task_dependencies", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT converted_task_id FROM notes WHERE id='n'",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "a"
    );
}
