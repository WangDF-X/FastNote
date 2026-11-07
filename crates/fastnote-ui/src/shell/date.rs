use fastnote_core::{Datelike, NaiveDate};

use crate::MainWindow;

/// 日期和星期从同一个注入日期生成，不读取系统时钟或当前业务路由。
fn labels(date: NaiveDate) -> (String, &'static str) {
    let weekday = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
        [date.weekday().num_days_from_monday() as usize];
    (
        format!("{}年{}月{}日", date.year(), date.month(), date.day()),
        weekday,
    )
}

pub(super) fn apply_date(window: &MainWindow, date: NaiveDate) {
    let (date_label, weekday) = labels(date);
    // 复用现有时钟观察；相同日期不重复写入控件，也不移动用户焦点。
    if window.get_sidebar_date().as_str() != date_label {
        window.set_sidebar_date(date_label.into());
    }
    if window.get_sidebar_weekday().as_str() != weekday {
        window.set_sidebar_weekday(weekday.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_date_and_weekday_are_formatted_from_the_same_injected_date() {
        for (value, label, weekday) in [
            ("2026-11-02", "2026年11月2日", "周一"),
            ("2026-11-03", "2026年11月3日", "周二"),
            ("2026-11-04", "2026年11月4日", "周三"),
            ("2026-11-05", "2026年11月5日", "周四"),
            ("2026-11-06", "2026年11月6日", "周五"),
            ("2026-11-07", "2026年11月7日", "周六"),
            ("2026-11-08", "2026年11月8日", "周日"),
        ] {
            let (actual_label, actual_weekday) = labels(value.parse().unwrap());
            assert_eq!(actual_label, label);
            assert_eq!(actual_weekday, weekday);
        }
    }

    #[test]
    fn test_date_labels_handle_leap_day_year_boundary_and_longest_supported_date() {
        for (value, label, weekday) in [
            ("2024-02-29", "2024年2月29日", "周四"),
            ("2026-12-31", "2026年12月31日", "周四"),
            ("2027-01-01", "2027年1月1日", "周五"),
            ("9999-12-31", "9999年12月31日", "周五"),
        ] {
            assert_eq!(labels(value.parse().unwrap()), (label.to_owned(), weekday));
        }
    }
}
