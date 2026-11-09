#[path = "note_cases/undo.rs"]
mod undo;

use fastnote_core::{
    Clock, DateTime, NaiveDate, NaiveDateTime, Note, NoteCommand, NoteCommandResult, NoteError,
    NoteId, NoteListQuery, NoteRepository, NoteService, NoteUndoState, TaskClock, Utc,
};
use fastnote_store::{NoteStoreError, SqliteSettings};
use rusqlite::Connection;

fn now() -> DateTime<Utc> {
    "2026-11-09T08:00:00Z".parse().unwrap()
}
fn id(number: usize) -> NoteId {
    format!("019a0fff-aaaa-7000-8000-{number:012x}")
        .parse()
        .unwrap()
}

#[test]
fn test_file_tree_commands_use_service_and_preserve_tree_on_rejection() {
    let mut f = Fixture::new();
    let mut undo = NoteUndoState::default();
    let folder = id(31).to_string();
    let file = id(32);
    f.execute(
        &mut undo,
        NoteCommand::CreateFolder {
            id: folder.clone(),
            parent: None,
            name: "资料".into(),
        },
        now(),
    )
    .unwrap();
    let result = f
        .execute(
            &mut undo,
            NoteCommand::CreateFile {
                id: file,
                parent: Some(folder.clone()),
                name: "草稿".into(),
            },
            now(),
        )
        .unwrap();
    assert_eq!(result.detail.unwrap().note.content, "");
    assert_eq!(
        f.execute(
            &mut undo,
            NoteCommand::Relocate {
                id: folder.clone(),
                parent: Some(folder.clone()),
                name: "资料".into()
            },
            now()
        )
        .unwrap_err()
        .domain_error(),
        Some(NoteError::Conflict)
    );
    f.execute(
        &mut undo,
        NoteCommand::Relocate {
            id: file.to_string(),
            parent: None,
            name: "记录".into(),
        },
        now(),
    )
    .unwrap();
    let tree = NoteRepository::note_file_tree(&f.store).unwrap();
    let entry = tree
        .iter()
        .find(|entry| entry.id == file.to_string())
        .unwrap();
    assert_eq!(
        (entry.parent.as_deref(), entry.name.as_str()),
        (None, "记录")
    );
    assert_eq!(
        tree.iter().find(|entry| entry.id == folder).unwrap().parent,
        None
    );
}

#[test]
fn test_file_tree_create_move_and_name_conflicts_are_atomic() {
    let mut f = Fixture::new();
    let a = id(10).to_string();
    let b = id(11).to_string();
    let c = id(12).to_string();
    f.store.create_note_folder(&a, None, "目录", now()).unwrap();
    f.store
        .create_note_folder(&b, Some(&a), "子目录", now())
        .unwrap();
    let note = Note::new(id(13), String::new(), now()).unwrap();
    f.store.create_note_file(&note, Some(&b), "Report").unwrap();
    assert_eq!(
        f.store
            .create_note_folder(&c, Some(&b), "report", now())
            .unwrap_err()
            .domain_error(),
        Some(NoteError::NameExists)
    );
    assert_eq!(
        f.store
            .create_note_folder(&c, Some(&b), "新目录", now())
            .unwrap(),
        ()
    );
    assert_eq!(
        f.store
            .relocate_note_entry(&a, Some(&b), "目录", now())
            .unwrap_err()
            .domain_error(),
        Some(NoteError::Conflict)
    );
    assert_eq!(
        f.store
            .relocate_note_entry(&a, Some(&c), "目录", now())
            .unwrap_err()
            .domain_error(),
        Some(NoteError::Conflict)
    );
    f.store
        .relocate_note_entry(&note.id.to_string(), None, "新名称", now())
        .unwrap();
    let tree = f.store.note_file_tree().unwrap();
    let file = tree
        .iter()
        .find(|entry| entry.id == note.id.to_string())
        .unwrap();
    assert_eq!(
        (file.parent.as_deref(), file.name.as_str()),
        (None, "新名称")
    );
    assert_eq!(
        f.store.note_detail(note.id).unwrap().unwrap().note.content,
        ""
    );
    assert_eq!(
        tree.iter().find(|entry| entry.id == a).unwrap().parent,
        None
    );
}

#[derive(Clone)]
struct FixedClock(DateTime<Utc>, i64);
impl Clock for FixedClock {
    fn today(&self) -> NaiveDate {
        self.local_date(self.0)
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

struct Fixture {
    store: SqliteSettings,
    path: std::path::PathBuf,
    _directory: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fake-notes.db");
        let store = SqliteSettings::open(&path, now()).unwrap();
        Self {
            store,
            path,
            _directory: directory,
        }
    }
    fn external(&self) -> Connection {
        let connection = Connection::open(&self.path).unwrap();
        connection.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        connection
    }
    fn add(&mut self, number: usize, content: &str) -> Note {
        let note = Note::new(id(number), content.to_owned(), now()).unwrap();
        self.store.insert_note(&note).unwrap();
        note
    }
    fn execute(
        &mut self,
        undo: &mut NoteUndoState,
        command: NoteCommand,
        at: DateTime<Utc>,
    ) -> Result<NoteCommandResult, NoteStoreError> {
        NoteService::new(&mut self.store, FixedClock(at, 0), undo).execute(command)
    }
}

#[test]
fn test_note_creation_roundtrips_without_storing_title_and_duplicate_is_atomic() {
    let mut fixture = Fixture::new();
    let note = fixture.add(1, "\n 假正文 \n第二行\n");
    let error = fixture.store.insert_note(&note).unwrap_err();
    assert_eq!(error.domain_error(), Some(NoteError::Conflict));
    assert!(error.storage_error().is_some());
    assert_eq!(
        fixture.store.note_detail(note.id).unwrap().unwrap().note,
        note
    );
    drop(fixture.store);
    let store = SqliteSettings::open(&fixture.path, now()).unwrap();
    assert_eq!(store.note_detail(note.id).unwrap().unwrap().note, note);
}

#[test]
fn test_missing_note_read_replace_delete_and_service_update_never_insert() {
    let mut fixture = Fixture::new();
    let note = Note::new(id(1), "假正文".to_owned(), now()).unwrap();
    assert!(fixture.store.note_detail(note.id).unwrap().is_none());
    assert_eq!(
        fixture
            .store
            .replace_note(&note, &note)
            .unwrap_err()
            .domain_error(),
        Some(NoteError::NotFound)
    );
    assert_eq!(
        fixture
            .store
            .delete_notes(Some(note.id))
            .unwrap_err()
            .domain_error(),
        Some(NoteError::NotFound)
    );
    let mut undo = NoteUndoState::default();
    assert_eq!(
        fixture
            .execute(
                &mut undo,
                NoteCommand::Update {
                    id: note.id,
                    content: note.content,
                    expected_updated_at: now()
                },
                now()
            )
            .unwrap_err()
            .domain_error(),
        Some(NoteError::NotFound)
    );
    assert_eq!(fixture.store.delete_notes(None).unwrap(), 0);
}

#[test]
fn test_save_validates_whitespace_unicode_limit_stale_version_and_no_op() {
    let mut fixture = Fixture::new();
    let note = fixture.add(1, "原假正文");
    let mut undo = NoteUndoState::default();
    let command = |content: String| NoteCommand::Update {
        id: note.id,
        content,
        expected_updated_at: note.updated_at,
    };
    let result = fixture
        .execute(&mut undo, command(note.content.clone()), now())
        .unwrap();
    assert!(result.events.is_empty());
    {
        let (content, expected) = ("字".repeat(20_001), NoteError::TooLong);
        assert_eq!(
            fixture
                .execute(&mut undo, command(content), now())
                .unwrap_err()
                .domain_error(),
            Some(expected)
        );
    }
    let result = fixture
        .execute(&mut undo, command("字".repeat(20_000)), now())
        .unwrap();
    assert!(result.detail.unwrap().note.updated_at > note.updated_at);
    assert_eq!(
        fixture
            .execute(&mut undo, command("旧版本".to_owned()), now())
            .unwrap_err()
            .domain_error(),
        Some(NoteError::Conflict)
    );
    assert_eq!(
        fixture
            .store
            .note_detail(note.id)
            .unwrap()
            .unwrap()
            .note
            .content
            .chars()
            .count(),
        20_000
    );
}

#[test]
fn test_replace_rejects_changed_creation_id_and_rolls_back_sql_failure() {
    let mut fixture = Fixture::new();
    let note = fixture.add(1, "原假正文");
    for changed in 0..2 {
        let mut next = note.clone();
        match changed {
            0 => next.id = id(2),
            _ => next.created_at += chrono::Duration::seconds(1),
        }
        assert_eq!(
            fixture
                .store
                .replace_note(&note, &next)
                .unwrap_err()
                .domain_error(),
            Some(NoteError::Conflict)
        );
    }
    fixture
        .external()
        .execute_batch(
            "CREATE TRIGGER fail_note_update BEFORE UPDATE ON notes
        BEGIN SELECT RAISE(ABORT,'fake failure'); END;",
        )
        .unwrap();
    let next = note.with_content("新假正文".to_owned(), now()).unwrap();
    assert!(fixture.store.replace_note(&note, &next).is_err());
    assert_eq!(
        fixture.store.note_detail(note.id).unwrap().unwrap().note,
        note
    );
}

#[test]
fn test_list_uses_local_dates_twenty_item_pages_and_bounded_excerpts_not_bodies() {
    let mut fixture = Fixture::new();
    for number in 1..=45 {
        let mut note = Note::new(
            id(number),
            format!(
                "{}{}\n第二行\n第三行",
                "\u{3000}\n".repeat(500),
                "字".repeat(200)
            ),
            now(),
        )
        .unwrap();
        note.updated_at = "2026-11-08T23:00:00Z".parse().unwrap();
        fixture.store.insert_note(&note).unwrap();
    }
    let clock = FixedClock(now(), 8);
    let first = fixture
        .store
        .list_notes(&NoteListQuery::default(), &clock)
        .unwrap();
    assert_eq!(
        (first.total, first.group_total, first.groups.len()),
        (45, 1, 1)
    );
    let group = &first.groups[0];
    assert_eq!(group.date.to_string(), "2026-11-09");
    assert_eq!((group.total, group.items.len()), (45, 20));
    assert_eq!(group.items[0].id, id(45));
    assert_eq!(group.items[0].title, format!("{} (45)", "字".repeat(80)));
    assert_eq!(group.items[0].preview.chars().count(), 120);
    let second = fixture
        .store
        .list_notes(
            &NoteListQuery {
                offsets: vec![(group.date, 20)],
                ..Default::default()
            },
            &clock,
        )
        .unwrap();
    assert_eq!(second.groups[0].items[0].id, id(25));
    let last = fixture
        .store
        .list_notes(
            &NoteListQuery {
                offsets: vec![(group.date, 40)],
                ..Default::default()
            },
            &clock,
        )
        .unwrap();
    assert_eq!(last.groups[0].items.len(), 5);
    let empty = fixture
        .store
        .list_notes(
            &NoteListQuery {
                group_offset: 1,
                ..Default::default()
            },
            &clock,
        )
        .unwrap();
    assert!(empty.groups.is_empty());
    assert_eq!(empty.total, 45);
}

#[test]
fn test_date_group_pagination_returns_ten_days_and_preserves_complete_counts() {
    let mut fixture = Fixture::new();
    for number in 1..=25 {
        let at = now() - chrono::Duration::days(number as i64);
        fixture
            .store
            .insert_note(&Note::new(id(number), "假正文".to_owned(), at).unwrap())
            .unwrap();
    }
    let clock = FixedClock(now(), 0);
    let first = fixture
        .store
        .list_notes(&NoteListQuery::default(), &clock)
        .unwrap();
    assert_eq!(
        (first.total, first.group_total, first.groups.len()),
        (25, 25, 10)
    );
    let next = fixture
        .store
        .list_notes(
            &NoteListQuery {
                group_offset: 20,
                ..Default::default()
            },
            &clock,
        )
        .unwrap();
    assert_eq!(next.groups.len(), 5);
    assert_eq!(next.groups[0].items[0].id, id(21));
}

#[test]
fn test_note_writes_recheck_schema_and_never_mutate_existing_data() {
    let mut fixture = Fixture::new();
    let note = fixture.add(1, "原假正文");
    fixture
        .external()
        .execute_batch("PRAGMA user_version=99;")
        .unwrap();
    let next = note.with_content("新假正文".to_owned(), now()).unwrap();
    assert!(fixture
        .store
        .insert_note(&Note::new(id(2), "新小记".to_owned(), now()).unwrap())
        .is_err());
    assert!(fixture.store.replace_note(&note, &next).is_err());
    assert!(fixture.store.delete_notes(Some(note.id)).is_err());
    assert_eq!(
        fixture.store.note_detail(note.id).unwrap().unwrap().note,
        note
    );
}
