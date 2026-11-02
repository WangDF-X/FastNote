//! SQLite storage adapters for FastNote.

mod backup;
mod database;
mod error;
mod migrations;
mod settings;

pub use error::StoreError;
pub use settings::SqliteSettings;
