//! Domain types and rules shared across FastNote crates.

pub mod command;
pub mod derive;
pub mod desktop;
pub mod domain;
pub mod dto;
pub mod event;
pub mod ports;
pub mod query;
pub mod service;

pub use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Utc};
pub use command::Command;
pub use desktop::{
    is_valid_hotkey, restore_ball, restore_main, snap_ball, BallPosition, CloseBehavior,
    DesktopSetting, DesktopSettings, EntryMode, LogicalRect, Monitor, ResolvedGeometry, ThemeMode,
    WindowGeometry,
};
pub use domain::{
    journal_excerpt, note_excerpt, parse_task_date, parse_task_time, validate_attachment,
    validate_journal_content, validate_note_content, AttachmentError, IdError, ImageMediaType,
    JournalAttachment, JournalEntry, JournalError, Note, NoteError, NoteId, Patch, PlanId,
    Priority, Task, TaskError, TaskId, TaskKind, TaskPatch, TaskStatus, WarningCode,
    JOURNAL_ATTACHMENT_THUMB_EDGE, JOURNAL_ATTACHMENT_VIEW_EDGE, JOURNAL_EXCERPT_LENGTH,
    MAX_JOURNAL_ATTACHMENTS, MAX_JOURNAL_ATTACHMENT_BYTES, MAX_JOURNAL_ATTACHMENT_PIXELS,
    MAX_JOURNAL_LENGTH, MAX_NOTE_LENGTH,
};
pub use dto::{
    JournalAttachmentPayload, JournalImageUsage, JournalSummary, NoteDateGroup, NoteDetail,
    NoteFileEntry, NoteListPage, NoteListQuery, NoteSummary, TaskContext, TaskDetail, TaskSummary,
    TodayPreferences, TodaySection, TodaySectionKind, TodaySort, TodayView,
};
pub use event::{AppEvent, CommandResult, ReminderOwner, TaskScope};
pub use ports::{
    Clock, JournalRepository, NoteRepository, SettingsRepository, StorageErrorCode, TaskClock,
    TaskRepository,
};
pub use query::{Query, QueryResult};
pub use service::{
    JournalCommand, JournalCommandResult, JournalService, NoteCommand, NoteCommandResult,
    NoteService, NoteUndoState, NoteUndoTicket, TaskService,
};
