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
    parse_task_date, parse_task_time, IdError, NoteId, Patch, PlanId, Priority, Task, TaskError,
    TaskId, TaskKind, TaskPatch, TaskStatus, WarningCode,
};
pub use dto::{
    TaskContext, TaskDetail, TaskSummary, TodayPreferences, TodaySection, TodaySectionKind,
    TodaySort, TodayView,
};
pub use event::{AppEvent, CommandResult, ReminderOwner, TaskScope};
pub use ports::{Clock, SettingsRepository, StorageErrorCode, TaskClock, TaskRepository};
pub use query::{Query, QueryResult};
pub use service::TaskService;
