use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};

/// 提供设备本地的今日日期（ARCH-03 §6）。
///
/// 应用层注入真实实现；测试使用固定日期，领域层不读取系统时钟。
pub trait Clock {
    /// 返回本地自然日，不包含时区或时间信息。
    fn today(&self) -> NaiveDate;
}

/// 任务服务所需的 UTC 时钟与设备本地日期转换；core 不读取系统时钟。
pub trait TaskClock: Clock {
    /// 当前 UTC 时间，供写入时间戳使用。
    fn now(&self) -> DateTime<Utc>;
    /// 时间戳换算为设备当前时区的墙上时间；排期判断统一使用此转换。
    fn local_datetime(&self, instant: DateTime<Utc>) -> NaiveDateTime;
    /// 历史时间戳换算为设备当前时区的自然日。
    fn local_date(&self, instant: DateTime<Utc>) -> NaiveDate {
        self.local_datetime(instant).date()
    }
}
