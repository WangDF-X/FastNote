//! Domain types and rules shared across FastNote crates.

pub mod desktop;
pub mod domain;
pub mod ports;

pub use chrono::{DateTime, NaiveDate, Utc};
pub use desktop::{
    is_valid_hotkey, restore_ball, restore_main, snap_ball, BallPosition, CloseBehavior,
    DesktopSetting, DesktopSettings, EntryMode, LogicalRect, Monitor, ResolvedGeometry,
    WindowGeometry,
};
pub use domain::{IdError, NoteId, PlanId, TaskId};
pub use ports::{Clock, SettingsRepository, StorageErrorCode};
