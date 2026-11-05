use std::path::Path;

use fastnote_core::{Command, CommandResult, Query, QueryResult, TaskService};
use fastnote_store::{SqliteSettings, StoreError, TaskStoreError};

use crate::{clock::SystemClock, logging::storage_error};

fn unavailable() -> TaskStoreError {
    StoreError::from(std::io::Error::new(
        std::io::ErrorKind::NotConnected,
        "database worker unavailable",
    ))
    .into()
}

pub(crate) fn execute(
    store: Option<&mut SqliteSettings>,
    path: Option<&Path>,
    command: Command,
) -> Result<CommandResult, TaskStoreError> {
    let result = store
        .ok_or_else(unavailable)
        .and_then(|store| TaskService::new(store, SystemClock).execute(command));
    diagnose(path, "task_command", &result);
    result
}

pub(crate) fn query(
    store: Option<&mut SqliteSettings>,
    path: Option<&Path>,
    query: Query,
) -> Result<QueryResult, TaskStoreError> {
    let result = store
        .ok_or_else(unavailable)
        .and_then(|store| store.query_tasks(query, SystemClock));
    diagnose(path, "task_query", &result);
    result
}

fn diagnose<T>(path: Option<&Path>, phase: &'static str, result: &Result<T, TaskStoreError>) {
    if let Err(error) = result {
        if let Some(error) = error.storage_error() {
            storage_error(path, phase, error);
        } else if let Some(error) = error.domain_error() {
            tracing::debug!(phase, error_code = %error, "task validation failed");
        }
    }
}
