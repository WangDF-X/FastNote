//! 应用层注入的环境能力接口，core 不自行访问系统资源。

mod clock;
mod settings;
mod task;

pub use clock::{Clock, TaskClock};
pub use settings::{SettingsRepository, StorageErrorCode};
pub use task::TaskRepository;
