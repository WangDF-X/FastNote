mod error;
pub(crate) mod names;
mod read;
mod tree;
mod undo;
mod write;

use fastnote_core::{
    DateTime, Note, NoteDetail, NoteId, NoteListPage, NoteListQuery, NoteRepository, TaskClock, Utc,
};

use crate::SqliteSettings;

pub use error::NoteStoreError;

impl NoteRepository for SqliteSettings {
    type Error = NoteStoreError;

    fn note_file_tree(&self) -> Result<Vec<fastnote_core::NoteFileEntry>, Self::Error> {
        SqliteSettings::note_file_tree(self)
    }

    fn list_notes(
        &self,
        query: &NoteListQuery,
        clock: &impl TaskClock,
    ) -> Result<NoteListPage, Self::Error> {
        read::list(&self.connection, query, clock)
    }
    fn note_detail(&self, id: NoteId) -> Result<Option<NoteDetail>, Self::Error> {
        read::detail(&self.connection, id)
    }
    fn insert_note(&mut self, note: &Note) -> Result<(), Self::Error> {
        write::insert(&mut self.connection, note)
    }
    fn insert_named_note(&mut self, note: &Note, name: &str) -> Result<(), Self::Error> {
        write::insert_named(&mut self.connection, note, name)
    }
    fn insert_note_in_folder(
        &mut self,
        note: &Note,
        parent: Option<&str>,
        name: &str,
    ) -> Result<(), Self::Error> {
        self.create_note_file(note, parent, name)
    }
    fn insert_note_folder(
        &mut self,
        id: &str,
        parent: Option<&str>,
        name: &str,
        now: DateTime<Utc>,
    ) -> Result<(), Self::Error> {
        self.create_note_folder(id, parent, name, now)
    }
    fn relocate_note_entry(
        &mut self,
        id: &str,
        parent: Option<&str>,
        name: &str,
        now: DateTime<Utc>,
    ) -> Result<(), Self::Error> {
        SqliteSettings::relocate_note_entry(self, id, parent, name, now)
    }
    fn replace_note(&mut self, expected: &Note, next: &Note) -> Result<(), Self::Error> {
        write::replace(&mut self.connection, expected, next)
    }
    fn delete_notes(&mut self, id: Option<NoteId>) -> Result<u32, Self::Error> {
        undo::delete(&mut self.connection, id)
    }
    fn delete_note_folder(&mut self, id: &str) -> Result<u32, Self::Error> {
        undo::delete_folder(&mut self.connection, id)
    }
    fn restore_notes(&mut self) -> Result<u32, Self::Error> {
        undo::restore(&mut self.connection)
    }
    fn discard_note_undo(&mut self) -> Result<(), Self::Error> {
        undo::discard(&self.connection)
    }
}
