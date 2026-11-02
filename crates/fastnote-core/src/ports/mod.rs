//! 应用层注入的环境能力接口，core 不自行访问系统资源。

mod clock;

pub use clock::Clock;
