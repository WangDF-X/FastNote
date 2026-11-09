use super::*;
use rusqlite::params;

#[test]
fn test_restore_legacy_link_keeps_task_and_does_not_restore_association() {
    let mut fixture = Fixture::new();
    let note = fixture.add(1, "正文");
    let db = fixture.external();
    db.execute("INSERT INTO tasks(id,title,status,status_changed_at,created_at,updated_at) VALUES(?1,'旧任务','todo',?2,?2,?2)", params![id(2).to_string(), now().to_rfc3339()]).unwrap();
    db.execute(
        "UPDATE notes SET converted_task_id=?2 WHERE id=?1",
        params![note.id.to_string(), id(2).to_string()],
    )
    .unwrap();
    fixture.store.delete_notes(Some(note.id)).unwrap();
    fixture.store.restore_notes().unwrap();
    let link: Option<String> = db
        .query_row(
            "SELECT converted_task_id FROM notes WHERE id=?1",
            [note.id.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(link, None);
    assert_eq!(
        db.query_row::<i64, _, _>(
            "SELECT count(*) FROM tasks WHERE id=?1",
            [id(2).to_string()],
            |row| row.get(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        fixture
            .store
            .note_detail(note.id)
            .unwrap()
            .unwrap()
            .note
            .content,
        "正文"
    );
}

#[test]
fn test_folder_delete_undo_restores_tree_and_renames_conflicting_root() {
    let mut f = Fixture::new();
    let root = id(20).to_string();
    let child = id(21).to_string();
    f.store
        .create_note_folder(&root, None, "项目", now())
        .unwrap();
    f.store
        .create_note_folder(&child, Some(&root), "资料", now())
        .unwrap();
    let note = Note::new(id(22), "正文".into(), now()).unwrap();
    f.store
        .create_note_file(&note, Some(&child), "记录")
        .unwrap();
    let external = f.external();
    external.execute("INSERT INTO reminders(id,note_id,schedule_kind,local_date,local_time,next_fire_at,state,created_at,updated_at) VALUES(?1,?2,'once','2026-11-10','09:00',?3,'active',?3,?3)", params![id(24).to_string(), note.id.to_string(), now().to_rfc3339()]).unwrap();
    external.execute("INSERT INTO reminder_deliveries(id,reminder_id,rule_revision,scheduled_for,outcome,handled_at) VALUES(?1,?2,1,?3,'submitted',?3)", params![id(25).to_string(), id(24).to_string(), now().to_rfc3339()]).unwrap();
    let mut undo = NoteUndoState::default();
    let deleted = f
        .execute(
            &mut undo,
            NoteCommand::DeleteFolder {
                id: root.clone(),
                token: 7,
            },
            now(),
        )
        .unwrap();
    assert_eq!(deleted.count, 3);
    assert!(f.store.note_file_tree().unwrap().is_empty());
    assert_eq!(
        external
            .query_row::<i64, _, _>("SELECT count(*) FROM reminders", [], |r| r.get(0))
            .unwrap(),
        0
    );
    f.store
        .create_note_folder(&id(23).to_string(), None, "项目", now())
        .unwrap();
    let restored = f
        .execute(&mut undo, NoteCommand::Undo { token: 7 }, now())
        .unwrap();
    assert_eq!(restored.count, 3);
    let tree = f.store.note_file_tree().unwrap();
    assert_eq!(tree.iter().find(|e| e.id == root).unwrap().name, "项目 (2)");
    assert_eq!(
        tree.iter()
            .find(|e| e.id == child)
            .unwrap()
            .parent
            .as_deref(),
        Some(root.as_str())
    );
    assert_eq!(
        tree.iter()
            .find(|e| e.id == note.id.to_string())
            .unwrap()
            .parent
            .as_deref(),
        Some(child.as_str())
    );
    assert_eq!(
        f.store.note_detail(note.id).unwrap().unwrap().note.content,
        "正文"
    );
    assert_eq!(
        external
            .query_row::<i64, _, _>(
                "SELECT count(*) FROM reminder_deliveries WHERE reminder_id=?1",
                [id(24).to_string()],
                |r| r.get(0)
            )
            .unwrap(),
        1
    );
}

#[test]
fn test_folder_delete_failure_preserves_tree_and_previous_undo() {
    let mut f = Fixture::new();
    let mut undo = NoteUndoState::default();
    f.add(1, "旧批次");
    f.execute(
        &mut undo,
        NoteCommand::Delete {
            id: Some(id(1)),
            token: 1,
        },
        now(),
    )
    .unwrap();
    let root = id(2).to_string();
    f.store
        .create_note_folder(&root, None, "目录", now())
        .unwrap();
    let note = Note::new(id(3), "正文".into(), now()).unwrap();
    f.store
        .create_note_file(&note, Some(&root), "文件")
        .unwrap();
    let external = f.external();
    external.execute_batch("CREATE TRIGGER fail_folder_note_delete BEFORE DELETE ON notes BEGIN SELECT RAISE(ABORT,'fake failure'); END;").unwrap();
    assert!(f
        .execute(
            &mut undo,
            NoteCommand::DeleteFolder {
                id: root.clone(),
                token: 2
            },
            now()
        )
        .is_err());
    assert_eq!(f.store.note_file_tree().unwrap().len(), 2);
    external
        .execute_batch("DROP TRIGGER fail_folder_note_delete")
        .unwrap();
    f.execute(&mut undo, NoteCommand::Undo { token: 1 }, now())
        .unwrap();
    assert!(f.store.note_detail(id(1)).unwrap().is_some());
}

#[test]
fn test_restore_renames_case_insensitive_conflict_without_losing_content() {
    let mut fixture = Fixture::new();
    let original = Note::new(id(1), "原正文".into(), now()).unwrap();
    fixture
        .store
        .insert_named_note(&original, "Report")
        .unwrap();
    fixture.store.delete_notes(Some(id(1))).unwrap();
    let replacement = Note::new(id(2), "新正文".into(), now()).unwrap();
    fixture
        .store
        .insert_named_note(&replacement, "report")
        .unwrap();
    fixture.store.restore_notes().unwrap();
    let db = fixture.external();
    let name: String = db
        .query_row(
            "SELECT name FROM notes WHERE id=?1",
            [id(1).to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(name, "Report (2)");
    assert_eq!(
        fixture
            .store
            .note_detail(id(1))
            .unwrap()
            .unwrap()
            .note
            .content,
        "原正文"
    );
    assert_eq!(
        fixture
            .store
            .note_detail(id(2))
            .unwrap()
            .unwrap()
            .note
            .content,
        "新正文"
    );
}

#[test]
fn test_latest_delete_batch_only_and_expiry_cleanup_is_exact_without_wall_clock_sleep() {
    let mut fixture = Fixture::new();
    fixture.add(1, "第一条假正文");
    fixture.add(2, "第二条假正文");
    let mut undo = NoteUndoState::default();
    fixture
        .execute(
            &mut undo,
            NoteCommand::Delete {
                id: Some(id(1)),
                token: 10,
            },
            now(),
        )
        .unwrap();
    let result = fixture
        .execute(
            &mut undo,
            NoteCommand::Delete {
                id: Some(id(2)),
                token: 11,
            },
            now(),
        )
        .unwrap();
    assert_eq!(result.count, 1);
    assert_eq!(
        fixture
            .execute(&mut undo, NoteCommand::Undo { token: 10 }, now())
            .unwrap_err()
            .domain_error(),
        Some(NoteError::UndoExpired)
    );
    fixture
        .execute(
            &mut undo,
            NoteCommand::Undo { token: 11 },
            now() + chrono::Duration::milliseconds(1999),
        )
        .unwrap();
    assert!(fixture.store.note_detail(id(1)).unwrap().is_none());
    assert!(fixture.store.note_detail(id(2)).unwrap().is_some());
    fixture
        .execute(
            &mut undo,
            NoteCommand::Delete {
                id: Some(id(2)),
                token: 12,
            },
            now(),
        )
        .unwrap();
    assert_eq!(
        fixture
            .execute(
                &mut undo,
                NoteCommand::Undo { token: 12 },
                now() + chrono::Duration::seconds(2)
            )
            .unwrap_err()
            .domain_error(),
        Some(NoteError::UndoExpired)
    );
    assert!(undo.ticket().is_none());
    assert_eq!(
        fixture.store.restore_notes().unwrap_err().domain_error(),
        Some(NoteError::UndoExpired)
    );
}

#[test]
fn test_clear_all_undo_restores_content_reminders_and_delivery_history() {
    let mut fixture = Fixture::new();
    let first = fixture.add(1, "\n 原假正文 ");
    let second = fixture.add(2, "第二条假正文");
    let external = fixture.external();
    external.execute("INSERT INTO reminders(id,note_id,schedule_kind,local_date,local_time,next_fire_at,state,created_at,updated_at)
        VALUES(?1,?2,'once','2026-11-10','09:00',?3,'active',?3,?3)", params![id(4).to_string(), first.id.to_string(), now().to_rfc3339()]).unwrap();
    external.execute("INSERT INTO reminder_deliveries(id,reminder_id,rule_revision,scheduled_for,outcome,handled_at)
        VALUES(?1,?2,1,?3,'submitted',?3)", params![id(5).to_string(), id(4).to_string(), now().to_rfc3339()]).unwrap();
    let mut undo = NoteUndoState::default();
    let result = fixture
        .execute(
            &mut undo,
            NoteCommand::Delete {
                id: None,
                token: 10,
            },
            now(),
        )
        .unwrap();
    assert_eq!(result.count, 2);
    assert_eq!(
        external
            .query_row("SELECT count(*) FROM reminders", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        fixture
            .execute(&mut undo, NoteCommand::Undo { token: 10 }, now())
            .unwrap()
            .count,
        2
    );
    assert_eq!(
        fixture.store.note_detail(first.id).unwrap().unwrap().note,
        first
    );
    assert_eq!(
        fixture.store.note_detail(second.id).unwrap().unwrap().note,
        second
    );
    assert_eq!(
        external
            .query_row("SELECT count(*) FROM reminders", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        external
            .query_row("SELECT count(*) FROM reminder_deliveries", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn test_failed_delete_preserves_notes_and_previous_undo_batch() {
    let mut fixture = Fixture::new();
    fixture.add(1, "第一条假正文");
    let second = fixture.add(2, "第二条假正文");
    let mut undo = NoteUndoState::default();
    fixture
        .execute(
            &mut undo,
            NoteCommand::Delete {
                id: Some(id(1)),
                token: 10,
            },
            now(),
        )
        .unwrap();
    assert_eq!(
        fixture
            .store
            .delete_notes(Some(id(99)))
            .unwrap_err()
            .domain_error(),
        Some(NoteError::NotFound)
    );
    fixture
        .external()
        .execute_batch(
            "CREATE TRIGGER fail_note_delete BEFORE DELETE ON notes
        BEGIN SELECT RAISE(ABORT,'fake failure'); END;",
        )
        .unwrap();
    assert!(fixture
        .execute(
            &mut undo,
            NoteCommand::Delete {
                id: None,
                token: 11
            },
            now()
        )
        .is_err());
    assert_eq!(
        fixture.store.note_detail(second.id).unwrap().unwrap().note,
        second
    );
    fixture
        .execute(&mut undo, NoteCommand::Undo { token: 10 }, now())
        .unwrap();
    assert!(fixture.store.note_detail(id(1)).unwrap().is_some());
}

#[test]
fn test_restore_id_collision_rolls_back_entire_batch_and_can_retry_before_expiry() {
    let mut fixture = Fixture::new();
    fixture.add(1, "原第一条");
    fixture.add(2, "原第二条");
    let mut undo = NoteUndoState::default();
    fixture
        .execute(
            &mut undo,
            NoteCommand::Delete {
                id: None,
                token: 10,
            },
            now(),
        )
        .unwrap();
    let collision = fixture.add(2, "新冲突假正文");
    assert_eq!(
        fixture
            .execute(&mut undo, NoteCommand::Undo { token: 10 }, now())
            .unwrap_err()
            .domain_error(),
        Some(NoteError::Conflict)
    );
    assert!(fixture.store.note_detail(id(1)).unwrap().is_none());
    assert_eq!(
        fixture.store.note_detail(id(2)).unwrap().unwrap().note,
        collision
    );
    fixture
        .external()
        .execute("DELETE FROM notes WHERE id=?1", [id(2).to_string()])
        .unwrap();
    fixture
        .execute(&mut undo, NoteCommand::Undo { token: 10 }, now())
        .unwrap();
    assert_eq!(
        fixture
            .store
            .note_detail(id(2))
            .unwrap()
            .unwrap()
            .note
            .content,
        "原第二条"
    );
}

#[test]
fn test_undo_does_not_persist_across_reopen() {
    let mut fixture = Fixture::new();
    let note = fixture.add(1, "假正文");
    fixture.store.delete_notes(Some(note.id)).unwrap();
    drop(fixture.store);
    let mut store = SqliteSettings::open(&fixture.path, now()).unwrap();
    assert_eq!(
        store.restore_notes().unwrap_err().domain_error(),
        Some(NoteError::UndoExpired)
    );
    assert!(store.note_detail(note.id).unwrap().is_none());
}
