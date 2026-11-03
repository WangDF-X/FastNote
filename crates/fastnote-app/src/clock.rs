//! 本地时钟适配：只有应用层读取系统日期。

use chrono::Local;
use fastnote_core::{Clock, DateTime, NaiveDate, NaiveDateTime, TaskClock, Utc};

pub(crate) struct SystemClock;

impl Clock for SystemClock {
    fn today(&self) -> NaiveDate {
        Local::now().date_naive()
    }
}

impl TaskClock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
    fn local_datetime(&self, instant: DateTime<Utc>) -> NaiveDateTime {
        instant.with_timezone(&Local).naive_local()
    }
}
