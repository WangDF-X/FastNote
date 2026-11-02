use chrono::NaiveDate;

/// 提供设备本地的今日日期（ARCH-03 §6）。
///
/// 应用层注入真实实现；测试使用固定日期，领域层不读取系统时钟。
pub trait Clock {
    /// 返回本地自然日，不包含时区或时间信息。
    fn today(&self) -> NaiveDate;
}
