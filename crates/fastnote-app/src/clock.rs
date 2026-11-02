//! 本地时钟适配：只有应用层读取系统日期。

use chrono::Local;
use fastnote_core::{Clock, NaiveDate};

pub(crate) struct SystemClock;

impl Clock for SystemClock {
    fn today(&self) -> NaiveDate {
        Local::now().date_naive()
    }
}
