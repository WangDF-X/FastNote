use std::path::Path;

use fastnote_core::{
    NoteCommand, NoteCommandResult, NoteDetail, NoteFileEntry, NoteId, NoteListPage, NoteListQuery,
    NoteRepository, NoteService, NoteUndoState,
};
use fastnote_store::{NoteStoreError, SqliteSettings, StoreError};

use crate::clock::SystemClock;
use crate::logging::storage_error;

pub(crate) enum NoteRead {
    List(NoteListQuery),
    Detail(NoteId),
    Tree,
}
pub(crate) enum NoteReadResult {
    List(NoteListPage),
    Detail(Option<Box<NoteDetail>>),
    Tree(Vec<NoteFileEntry>),
}

pub(crate) fn execute(
    store: Option<&mut SqliteSettings>,
    path: Option<&Path>,
    command: NoteCommand,
    undo: &mut NoteUndoState,
) -> Result<NoteCommandResult, NoteStoreError> {
    let result = store
        .ok_or_else(unavailable)
        .and_then(|store| NoteService::new(store, SystemClock, undo).execute(command));
    diagnose(path, "note_command", &result);
    result
}

pub(crate) fn query(
    store: Option<&mut SqliteSettings>,
    path: Option<&Path>,
    query: NoteRead,
) -> Result<NoteReadResult, NoteStoreError> {
    let result = store.ok_or_else(unavailable).and_then(|store| match query {
        NoteRead::List(query) => store
            .list_notes(&query, &SystemClock)
            .map(NoteReadResult::List),
        NoteRead::Detail(id) => store
            .note_detail(id)
            .map(|detail| NoteReadResult::Detail(detail.map(Box::new))),
        NoteRead::Tree => store.note_file_tree().map(NoteReadResult::Tree),
    });
    diagnose(path, "note_query", &result);
    result
}

pub(crate) fn expire(
    store: Option<&mut SqliteSettings>,
    path: Option<&Path>,
    undo: &mut NoteUndoState,
) {
    let Some(store) = store else {
        undo.clear();
        return;
    };
    let result = NoteService::new(store, SystemClock, undo).expire_undo();
    diagnose(path, "note_undo_expire", &result);
}

fn unavailable() -> NoteStoreError {
    StoreError::from(std::io::Error::new(
        std::io::ErrorKind::NotConnected,
        "notes store unavailable",
    ))
    .into()
}

fn diagnose<T>(path: Option<&Path>, operation: &'static str, result: &Result<T, NoteStoreError>) {
    if let Err(error) = result {
        if let Some(error) = error.storage_error() {
            storage_error(path, operation, error);
        } else if let Some(error) = error.domain_error() {
            tracing::warn!(operation, error_code = ?error, "note operation rejected");
        }
    }
}
