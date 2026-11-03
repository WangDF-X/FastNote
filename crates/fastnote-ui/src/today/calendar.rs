use fastnote_core::{Datelike, NaiveDate};

use crate::{CalendarDayData, Date};

/// 选择器范围不追溯修改领域允许的既有日期。
pub(super) const PICKER_FIRST_YEAR: i32 = 2000;
/// 允许选择的最后一个年份（含该年）。
pub(super) const PICKER_LAST_YEAR: i32 = 2100;

fn checked_date(value: &Date) -> Option<NaiveDate> {
    if !(1900..=9999).contains(&value.year) {
        return None;
    }
    NaiveDate::from_ymd_opt(
        value.year,
        u32::try_from(value.month).ok()?,
        u32::try_from(value.day).ok()?,
    )
}

fn ui_date(value: NaiveDate) -> Date {
    Date {
        year: value.year(),
        month: value.month() as i32,
        day: value.day() as i32,
    }
}

pub(super) fn month_days(year: i32, month: i32, today: Date) -> Vec<CalendarDayData> {
    let Some(mut start) = checked_date(&Date {
        year,
        month,
        day: 1,
    }) else {
        return Vec::new();
    };
    for _ in 0..start.weekday().num_days_from_monday() {
        let Some(previous) = start.pred_opt() else {
            return Vec::new();
        };
        start = previous;
    }
    std::iter::successors(Some(start), |date| date.succ_opt())
        .take(42)
        .map(|date| {
            let value = ui_date(date);
            CalendarDayData {
                in_month: value.year == year && value.month == month,
                today: value.year == today.year
                    && value.month == today.month
                    && value.day == today.day,
                enabled: (PICKER_FIRST_YEAR..=PICKER_LAST_YEAR).contains(&value.year),
                date: value,
            }
        })
        .collect()
}

pub(super) fn move_day(value: Date, offset: i32) -> Date {
    let Some(mut date) = checked_date(&value) else {
        return value;
    };
    if offset.unsigned_abs() > 366 {
        return value;
    }
    for _ in 0..offset.unsigned_abs() {
        let next = if offset < 0 {
            date.pred_opt()
        } else {
            date.succ_opt()
        };
        let Some(next) = next else {
            return value;
        };
        date = next;
    }
    if (PICKER_FIRST_YEAR..=PICKER_LAST_YEAR).contains(&date.year()) {
        ui_date(date)
    } else {
        value
    }
}

pub(super) fn move_month(value: Date, offset: i32) -> Date {
    let Some(date) = checked_date(&value) else {
        return value;
    };
    if offset.unsigned_abs() > 120 {
        return value;
    }
    let index = date.year() * 12 + date.month0() as i32 + offset;
    let year = index.div_euclid(12);
    let month = index.rem_euclid(12) + 1;
    select_month(value, year, month)
}

/// 年月直接选择保持当前日；月末向下校正，时分由弹层独立保持。
pub(super) fn select_month(value: Date, year: i32, month: i32) -> Date {
    let Some(date) = checked_date(&value) else {
        return value;
    };
    if !(PICKER_FIRST_YEAR..=PICKER_LAST_YEAR).contains(&year) || !(1..=12).contains(&month) {
        return value;
    }
    (1..=date.day())
        .rev()
        .find_map(|day| NaiveDate::from_ymd_opt(year, month as u32, day))
        .map(ui_date)
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i32, month: i32, day: i32) -> Date {
        Date { year, month, day }
    }

    #[test]
    fn test_month_grid_is_six_weeks_monday_first_and_marks_today_once() {
        let days = month_days(2026, 11, date(2026, 11, 3));
        assert_eq!(days.len(), 42);
        assert_eq!((days[0].date.month, days[0].date.day), (10, 26));
        assert_eq!((days[6].date.month, days[6].date.day), (11, 1));
        assert_eq!(days.iter().filter(|day| day.in_month).count(), 30);
        assert_eq!(days.iter().filter(|day| day.today).count(), 1);
        assert!(days[8].today);
    }

    #[test]
    fn test_month_grid_handles_leap_years_and_disables_out_of_range_spillover() {
        let leap = month_days(2024, 2, date(2024, 2, 29));
        assert_eq!(leap.iter().filter(|day| day.in_month).count(), 29);
        let upper = month_days(9999, 12, date(9999, 12, 31));
        assert!(upper
            .iter()
            .any(|day| !day.enabled && day.date.year == 10000));
        assert!(month_days(1899, 12, date(1900, 1, 1)).is_empty());
        assert!(month_days(2026, 13, date(2026, 1, 1)).is_empty());
    }

    #[test]
    fn test_month_grids_are_continuous_and_align_each_cell_to_its_weekday() {
        for year in [1900, 2000, 2024, 2026, 9999] {
            for month in 1..=12 {
                let days = month_days(year, month, date(year, month, 1));
                assert_eq!(days.len(), 42);
                let mut previous = None;
                for (index, day) in days.iter().enumerate() {
                    let date = NaiveDate::from_ymd_opt(
                        day.date.year,
                        day.date.month as u32,
                        day.date.day as u32,
                    )
                    .unwrap();
                    assert_eq!(date.weekday().num_days_from_monday() as usize, index % 7);
                    if let Some(previous) = previous {
                        assert_eq!(date.pred_opt(), Some(previous));
                    }
                    assert_eq!(
                        day.in_month,
                        date.year() == year && date.month() == month as u32
                    );
                    assert_eq!(
                        day.enabled,
                        (PICKER_FIRST_YEAR..=PICKER_LAST_YEAR).contains(&date.year())
                    );
                    previous = Some(date);
                }
                for day in 1..=31 {
                    let expected =
                        usize::from(NaiveDate::from_ymd_opt(year, month as u32, day).is_some());
                    assert_eq!(
                        days.iter()
                            .filter(|cell| cell.in_month && cell.date.day == day as i32)
                            .count(),
                        expected
                    );
                }
            }
        }
    }

    #[test]
    fn test_day_navigation_is_bounded_and_crosses_year_and_leap_boundaries() {
        assert_eq!(move_day(date(2026, 12, 31), 1), date(2027, 1, 1));
        assert_eq!(move_day(date(2024, 3, 1), -1), date(2024, 2, 29));
        assert_eq!(move_day(date(2026, 11, 3), 7), date(2026, 11, 10));
        for value in [date(2000, 1, 1), date(2100, 12, 31)] {
            assert_eq!(
                move_day(value.clone(), if value.year == 2000 { -1 } else { 1 }),
                value
            );
        }
        assert_eq!(move_day(date(2026, 11, 3), i32::MIN), date(2026, 11, 3));
        assert_eq!(move_day(date(2026, 2, 30), 1), date(2026, 2, 30));
    }

    #[test]
    fn test_month_navigation_clamps_end_of_month_without_changing_business_data() {
        assert_eq!(move_month(date(2026, 1, 31), 1), date(2026, 2, 28));
        assert_eq!(move_month(date(2024, 3, 31), -1), date(2024, 2, 29));
        assert_eq!(move_month(date(2026, 12, 3), 1), date(2027, 1, 3));
        assert_eq!(move_month(date(2000, 1, 1), -1), date(2000, 1, 1));
        assert_eq!(move_month(date(2100, 12, 31), 1), date(2100, 12, 31));
        assert_eq!(move_month(date(2026, 11, 3), i32::MAX), date(2026, 11, 3));
    }

    #[test]
    fn test_calendar_navigation_rejects_invalid_dates_and_excessive_offsets() {
        for value in [
            date(1899, 12, 31),
            date(10000, 1, 1),
            date(2026, -1, 1),
            date(2026, 1, -1),
            date(2026, 2, 30),
        ] {
            assert_eq!(move_day(value.clone(), 1), value);
            assert_eq!(move_month(value.clone(), 1), value);
        }
        let value = date(2026, 11, 3);
        assert_eq!(move_day(value.clone(), 0), value);
        assert_eq!(move_month(value.clone(), 0), value);
        assert_eq!(move_day(value.clone(), 366), date(2027, 11, 4));
        assert_eq!(move_month(value.clone(), -120), date(2016, 11, 3));
        for offset in [-367, 367] {
            assert_eq!(move_day(value.clone(), offset), value);
        }
        for offset in [-121, 121] {
            assert_eq!(move_month(value.clone(), offset), value);
        }
    }

    #[test]
    fn test_year_and_month_selection_changes_the_date_and_clamps_month_end() {
        assert_eq!(select_month(date(2026, 1, 31), 2026, 2), date(2026, 2, 28));
        assert_eq!(select_month(date(2024, 2, 29), 2025, 2), date(2025, 2, 28));
        assert_eq!(select_month(date(2026, 1, 31), 2000, 2), date(2000, 2, 29));
        assert_eq!(select_month(date(2026, 1, 31), 2100, 2), date(2100, 2, 28));
        assert_eq!(select_month(date(2026, 11, 3), 2099, 12), date(2099, 12, 3));
        let original = date(2026, 11, 3);
        for (year, month) in [(1999, 12), (2101, 1), (2026, 0), (2026, 13)] {
            assert_eq!(select_month(original.clone(), year, month), original);
        }
        assert_eq!(select_month(date(2026, 2, 30), 2027, 3), date(2026, 2, 30));
    }

    #[test]
    fn test_picker_range_disables_adjacent_dates_without_rewriting_legacy_dates() {
        let lower = month_days(2000, 1, date(2000, 1, 1));
        assert!(lower
            .iter()
            .any(|day| day.date.year == 1999 && !day.enabled));
        let upper = month_days(2100, 12, date(2100, 12, 31));
        assert!(upper
            .iter()
            .any(|day| day.date.year == 2101 && !day.enabled));
        let old = date(1990, 1, 31);
        assert_eq!(move_day(old.clone(), 1), old);
        assert_eq!(move_month(old.clone(), 1), old);
        assert_eq!(select_month(old, 2000, 2), date(2000, 2, 29));
    }
}
