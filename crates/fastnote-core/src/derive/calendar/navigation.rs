//! 日历网格日期计算，不依赖设置持久化或界面状态。

use chrono::{Datelike, Duration, NaiveDate};

/// 日历一周的第一天。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WeekStart {
    /// 周一开始。
    #[default]
    Monday,
    /// 周日开始。
    Sunday,
}

impl WeekStart {
    /// 返回数据库中使用的稳定值。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Monday => "monday",
            Self::Sunday => "sunday",
        }
    }

    /// 严格解析已登记的配置值。
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "monday" => Some(Self::Monday),
            "sunday" => Some(Self::Sunday),
            _ => None,
        }
    }
}

fn offset(date: NaiveDate, start: WeekStart) -> i64 {
    i64::from(match start {
        WeekStart::Monday => date.weekday().num_days_from_monday(),
        WeekStart::Sunday => date.weekday().num_days_from_sunday(),
    })
}

/// 返回包含指定日期的完整七日；超出日期表示范围时返回 `None`。
pub fn calendar_week_days(date: NaiveDate, start: WeekStart) -> Option<Vec<NaiveDate>> {
    let first = date.checked_sub_signed(Duration::days(offset(date, start)))?;
    (0..7)
        .map(|day| first.checked_add_signed(Duration::days(day)))
        .collect()
}

/// 返回六周月网格，包括前后月的补位日期。
/// 若补位日期超出日期表示范围，返回 `None`。
pub fn calendar_month_days(date: NaiveDate, start: WeekStart) -> Option<Vec<NaiveDate>> {
    let first = date.with_day(1)?;
    let grid_start = first.checked_sub_signed(Duration::days(offset(first, start)))?;
    (0..42)
        .map(|day| grid_start.checked_add_signed(Duration::days(day)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).expect("valid test date")
    }

    #[test]
    fn test_month_grid_respects_week_start_and_crosses_year() {
        let monday = calendar_month_days(date(2026, 1, 15), WeekStart::Monday).expect("grid");
        assert_eq!(monday.first(), Some(&date(2025, 12, 29)));
        assert_eq!(monday.last(), Some(&date(2026, 2, 8)));
        let sunday = calendar_month_days(date(2026, 1, 15), WeekStart::Sunday).expect("grid");
        assert_eq!(sunday.first(), Some(&date(2025, 12, 28)));
        assert_eq!(sunday.last(), Some(&date(2026, 2, 7)));
    }

    #[test]
    fn test_week_grid_contains_leap_day() {
        let days = calendar_week_days(date(2024, 2, 29), WeekStart::Monday).expect("week");
        assert_eq!(days.len(), 7);
        assert_eq!(days[0], date(2024, 2, 26));
        assert_eq!(days[6], date(2024, 3, 3));
    }

    #[test]
    fn test_grid_handles_representation_boundaries() {
        assert!(calendar_month_days(NaiveDate::MAX, WeekStart::Monday).is_none());
        assert!(calendar_week_days(NaiveDate::MAX, WeekStart::Monday).is_none());
    }
}
