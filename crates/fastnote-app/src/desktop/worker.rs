use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Instant, SystemTime};

use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};
use fastnote_core::{
    Command as TaskCommand, CommandResult, DateTime, DesktopSetting, DesktopSettings, Query,
    QueryResult, SettingsRepository, Utc,
};
use fastnote_store::{SqliteSettings, StoreError, TaskStoreError};

use crate::logging::storage_error;

pub(super) enum Command {
    Save {
        id: u64,
        changes: Vec<DesktopSetting>,
        checkpoint: bool,
    },
    Reload,
    Execute {
        id: u64,
        command: TaskCommand,
        source: &'static str,
    },
    Query {
        id: u64,
        query: Query,
    },
    Stop,
}

pub(super) enum Reply {
    Loaded(Result<DesktopSettings, StoreError>),
    Saved {
        id: u64,
        result: Result<(), StoreError>,
    },
    Executed {
        id: u64,
        result: Result<CommandResult, TaskStoreError>,
    },
    Queried {
        id: u64,
        result: Result<QueryResult, TaskStoreError>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SendError {
    Full,
    Stopped,
}

pub(super) struct DbWorker {
    commands: Option<Sender<Command>>,
    replies: Option<Receiver<Reply>>,
    thread: Option<JoinHandle<()>>,
}

impl DbWorker {
    pub(super) fn spawn(
        path: Option<PathBuf>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self, std::io::Error> {
        let (command_tx, command_rx) = bounded(256);
        let (reply_tx, reply_rx) = bounded(256);
        let thread = thread::Builder::new()
            .name("fastnote-db".to_owned())
            .spawn(move || {
                work(path, command_rx, reply_tx, wake);
            })?;
        Ok(Self {
            commands: Some(command_tx),
            replies: Some(reply_rx),
            thread: Some(thread),
        })
    }

    pub(super) fn save(
        &self,
        id: u64,
        changes: Vec<DesktopSetting>,
        checkpoint: bool,
    ) -> Result<(), SendError> {
        self.send(Command::Save {
            id,
            changes,
            checkpoint,
        })
    }

    pub(super) fn reload(&self) -> Result<(), SendError> {
        self.send(Command::Reload)
    }

    pub(super) fn execute(
        &self,
        id: u64,
        command: TaskCommand,
        source: &'static str,
    ) -> Result<(), SendError> {
        self.send(Command::Execute {
            id,
            command,
            source,
        })
    }

    pub(super) fn query(&self, id: u64, query: Query) -> Result<(), SendError> {
        self.send(Command::Query { id, query })
    }

    fn send(&self, command: Command) -> Result<(), SendError> {
        let Some(sender) = &self.commands else {
            return Err(SendError::Stopped);
        };
        sender.try_send(command).map_err(|error| match error {
            TrySendError::Full(_) => SendError::Full,
            TrySendError::Disconnected(_) => SendError::Stopped,
        })
    }

    pub(super) fn try_reply(&self) -> Option<Reply> {
        self.replies
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok())
    }
}

impl Drop for DbWorker {
    fn drop(&mut self) {
        // 先断开结果接收端，防止工作线程堵在满结果队列、UI 又堵在 join。
        self.replies.take();
        if let Some(sender) = self.commands.take() {
            let _ = sender.try_send(Command::Stop);
        }
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                tracing::error!("database thread panicked during shutdown");
            }
        }
    }
}

fn timestamp() -> DateTime<Utc> {
    SystemTime::now().into()
}

fn open(path: &mut Option<PathBuf>) -> Result<SqliteSettings, StoreError> {
    let resolved = match path {
        Some(path) => path.clone(),
        None => std::env::var_os("LOCALAPPDATA")
            .filter(|value| !value.is_empty())
            .map(|base| PathBuf::from(base).join("FastNote").join("fastnote.db"))
            .ok_or_else(|| {
                StoreError::from(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "LOCALAPPDATA unavailable",
                ))
            })
            .inspect_err(|error| storage_error(None, "resolve_database_path", error))?,
    };
    let result = SqliteSettings::open(&resolved, timestamp())
        .inspect_err(|error| storage_error(Some(&resolved), "open_database", error));
    // 后续保存/重试使用同一有效路径，错误终端输出不能丢失路径上下文。
    *path = Some(resolved);
    result
}

fn load(
    path: &mut Option<PathBuf>,
) -> (Option<SqliteSettings>, Result<DesktopSettings, StoreError>) {
    match open(path) {
        Ok(store) => match store
            .load_desktop()
            .inspect_err(|error| storage_error(path.as_deref(), "load_settings", error))
        {
            Ok(settings) => (Some(store), Ok(settings)),
            Err(error) => (None, Err(error)),
        },
        Err(error) => (None, Err(error)),
    }
}

fn save(
    store: &mut Option<SqliteSettings>,
    path: Option<&Path>,
    changes: &[DesktopSetting],
    checkpoint: bool,
) -> Result<(), StoreError> {
    let store = store
        .as_mut()
        .ok_or_else(|| {
            StoreError::from(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "settings store unavailable",
            ))
        })
        .inspect_err(|error| storage_error(path, "save_settings", error))?;
    store
        .save_desktop(changes, timestamp())
        .inspect_err(|error| storage_error(path, "save_settings", error))?;
    if checkpoint {
        store
            .checkpoint()
            .inspect_err(|error| storage_error(path, "checkpoint", error))?;
    }
    Ok(())
}

fn report(sender: &Sender<Reply>, wake: &Arc<dyn Fn() + Send + Sync>, reply: Reply) -> bool {
    if sender.send(reply).is_err() {
        return false;
    }
    wake();
    true
}

fn work(
    mut path: Option<PathBuf>,
    commands: Receiver<Command>,
    replies: Sender<Reply>,
    wake: Arc<dyn Fn() + Send + Sync>,
) {
    let (mut store, result) = load(&mut path);
    if !report(&replies, &wake, Reply::Loaded(result)) {
        return;
    }
    while let Ok(command) = commands.recv() {
        let reply = match command {
            Command::Stop => break,
            Command::Execute {
                id,
                command,
                source,
            } => {
                let started = Instant::now();
                let result = crate::tasks::execute(store.as_mut(), path.as_deref(), command);
                if started.elapsed().as_millis() > 50 {
                    tracing::warn!(
                        request_id = id,
                        source,
                        elapsed_ms = started.elapsed().as_millis() as u64,
                        "task command slow"
                    );
                }
                Reply::Executed { id, result }
            }
            Command::Query { id, query } => {
                let query_scope = match &query {
                    Query::Today { .. } => "today",
                    Query::Calendar { .. } => "calendar",
                    Query::TaskDetail(_) => "task_detail",
                };
                let started = Instant::now();
                let result = crate::tasks::query(store.as_mut(), path.as_deref(), query);
                tracing::info!(
                    phase = "db_query_completed",
                    request_id = id,
                    query_scope,
                    elapsed_us = started.elapsed().as_micros() as u64
                );
                Reply::Queried { id, result }
            }
            Command::Reload => {
                store.take();
                let (new_store, result) = load(&mut path);
                store = new_store;
                Reply::Loaded(result)
            }
            Command::Save {
                id,
                changes,
                checkpoint,
            } => {
                let started = Instant::now();
                let result = save(&mut store, path.as_deref(), &changes, checkpoint);
                let elapsed = started.elapsed().as_millis();
                if elapsed > 50 {
                    tracing::warn!(elapsed_ms = elapsed as u64, "settings command slow");
                }
                Reply::Saved { id, result }
            }
        };
        if !report(&replies, &wake, reply) {
            break;
        }
    }
}

#[cfg(test)]
mod tests;
