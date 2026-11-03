use fastnote_core::{parse_task_date, parse_task_time, Datelike, NaiveDateTime, Timelike};

use crate::Date;

/// 选择器预填，仅为显示草稿，不代表新增排期。
pub struct TaskDateTimeSeed {
    /// 已有日期，未设置时为注入的本地当前日期。
    pub date: Date,
    /// 已有时分，旧全天/未设置时为当前小时。
    pub hour: i32,
    /// 分钟，0–59。
    pub minute: i32,
}

/// 日期/时分选择器使用当前草稿预填，非法草稿也不改写，只提供可重新选择的起点。
pub fn task_datetime_seed(date: &str, time: &str, now: NaiveDateTime) -> TaskDateTimeSeed {
    let date = parse_task_date(date).ok().flatten().unwrap_or(now.date());
    let time = parse_task_time(time).ok().flatten().unwrap_or(now.time());
    TaskDateTimeSeed {
        date: Date {
            year: date.year(),
            month: date.month() as i32,
            day: date.day() as i32,
        },
        hour: time.hour() as i32,
        minute: time.minute() as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_existing_schedule_seeds_its_own_date_and_time_without_using_utc() {
        let now = "2026-11-03T09:05:00".parse().unwrap();
        let seed = task_datetime_seed("2026-11-04", "15:31", now);
        assert_eq!(
            (seed.date.year, seed.date.month, seed.date.day),
            (2026, 11, 4)
        );
        assert_eq!((seed.hour, seed.minute), (15, 31));
        let empty = task_datetime_seed("", "", now);
        assert_eq!((empty.date.day, empty.hour, empty.minute), (3, 9, 5));
        let legacy = task_datetime_seed("2026-11-02", "", now);
        assert_eq!((legacy.date.day, legacy.hour, legacy.minute), (2, 9, 5));
        let invalid = task_datetime_seed("invalid-date", "25:70", now);
        assert_eq!((invalid.date.day, invalid.hour, invalid.minute), (3, 9, 5));
    }
}
