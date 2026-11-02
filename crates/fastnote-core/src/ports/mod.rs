//! 应用层注入的环境能力接口，core 不自行访问系统资源。

mod clock;
mod settings;

pub use clock::Clock;
pub use settings::{SettingsRepository, StorageErrorCode};
