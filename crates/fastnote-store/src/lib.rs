//! SQLite storage adapters for FastNote.

mod backup;
mod database;
mod error;
mod journal;
mod migrations;
mod notes;
mod settings;
mod tasks;

pub use error::StoreError;
pub use journal::JournalStoreError;
pub use notes::NoteStoreError;
pub use settings::SqliteSettings;
pub use tasks::TaskStoreError;
