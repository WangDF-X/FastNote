//! 窗口/入口的纯数据与屏幕范围规则（SHELL-01 §5、ARCH-06 §10）。

mod ball_geometry;
mod geometry;
mod hotkey;
mod settings;
mod types;

pub use ball_geometry::{restore_ball, snap_ball};
pub use geometry::restore_main;
pub use hotkey::is_valid_hotkey;
pub use settings::{CloseBehavior, DesktopSetting, DesktopSettings, EntryMode};
pub use types::{BallPosition, LogicalRect, Monitor, ResolvedGeometry, WindowGeometry};
